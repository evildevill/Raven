use std::collections::HashMap;
use std::fs;
use std::path::Path;

use fancy_regex::Regex;
use reqwest::Client;
use tracing::{info, warn};

use crate::error::RavenError;
use crate::types::*;

#[derive(Debug)]
pub struct Manifest {
    pub sites: Vec<SiteInfo>,
}

impl Manifest {
    pub fn from_local<P: AsRef<Path>>(path: P) -> Result<Self, RavenError> {
        let data = fs::read_to_string(path.as_ref())
            .map_err(|e| RavenError::Manifest(format!("Failed to read manifest file: {e}")))?;
        Self::parse_json(&data)
    }

    pub async fn from_remote(client: &Client, url: &str) -> Result<Self, RavenError> {
        info!("Fetching manifest from {url}");
        let response = client
            .get(url)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(RavenError::Manifest(format!(
                "Remote manifest returned HTTP {}",
                response.status()
            )));
        }

        let text = response.text().await?;
        Self::parse_json(&text)
    }

    pub async fn load_default(client: &Client, force_local: bool) -> Result<Self, RavenError> {
        if force_local {
            info!("Forcing local manifest");
            return Self::from_local(LOCAL_MANIFEST_PATH);
        }

        let bundled_path = Path::new(LOCAL_MANIFEST_PATH);
        if bundled_path.exists() {
            info!("Loading bundled manifest from {LOCAL_MANIFEST_PATH}");
            match Self::from_local(LOCAL_MANIFEST_PATH) {
                Ok(manifest) => return Ok(manifest),
                Err(e) => warn!("Failed to load bundled manifest: {e}. Falling back to remote."),
            }
        }

        match Self::from_remote(client, DEFAULT_MANIFEST_REMOTE_URL).await {
            Ok(manifest) => {
                info!("Loaded remote manifest successfully");
                Ok(manifest)
            }
            Err(e) => {
                if bundled_path.exists() {
                    warn!("Remote manifest fetch failed: {e}. Falling back to bundled.");
                    Self::from_local(LOCAL_MANIFEST_PATH)
                } else {
                    Err(RavenError::Manifest(format!(
                        "Failed to load manifest from any source: {e}"
                    )))
                }
            }
        }
    }

    pub async fn load_custom(client: &Client, source: &str) -> Result<Self, RavenError> {
        let text = if source.starts_with("http://") || source.starts_with("https://") {
            info!("Fetching custom manifest from {source}");
            let resp = client
                .get(source)
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?;
            if !resp.status().is_success() {
                return Err(RavenError::Manifest(format!(
                    "Custom manifest URL returned HTTP {}",
                    resp.status()
                )));
            }
            resp.text().await?
        } else if source.chars().all(|c| c.is_ascii_digit()) {
            let pull_url = format!(
                "https://api.github.com/repos/sherlock-project/sherlock/pulls/{source}"
            );
            info!("Fetching PR #{source} metadata from GitHub API");
            let resp = client
                .get(&pull_url)
                .header("User-Agent", "raven")
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?;
            if !resp.status().is_success() {
                return Err(RavenError::Manifest(format!(
                    "GitHub API returned HTTP {} for PR #{source}",
                    resp.status()
                )));
            }
            let pr_data: serde_json::Value = resp.json().await?;
            let sha = pr_data["head"]["sha"]
                .as_str()
                .ok_or_else(|| RavenError::Manifest("Could not determine PR head SHA".to_string()))?;
            let raw_url = format!(
                "https://raw.githubusercontent.com/sherlock-project/sherlock/{sha}/sherlock_project/resources/data.json"
            );
            info!("Fetching manifest from PR #{source} (sha: {sha})");
            let resp = client
                .get(&raw_url)
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?;
            if !resp.status().is_success() {
                return Err(RavenError::Manifest(format!(
                    "PR manifest URL returned HTTP {}",
                    resp.status()
                )));
            }
            resp.text().await?
        } else {
            info!("Loading custom manifest from file {source}");
            std::fs::read_to_string(source)
                .map_err(|e| RavenError::Manifest(format!("Failed to read custom manifest: {e}")))?
        };

        Self::parse_json(&text)
    }

    pub async fn update_manifest(client: &Client, url: &str) -> Result<(), RavenError> {
        info!("Downloading latest manifest from {url}");
        let response = client
            .get(url)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(RavenError::Manifest(format!(
                "Remote manifest returned HTTP {}",
                response.status()
            )));
        }

        let json_text = response.text().await?;
        let parsed: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|e| RavenError::Manifest(format!("Invalid JSON: {e}")))?;

        let pretty = serde_json::to_string_pretty(&parsed)
            .map_err(|e| RavenError::Manifest(format!("Failed to format JSON: {e}")))?;

        let path = Path::new(LOCAL_MANIFEST_PATH);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &pretty)?;

        info!("Manifest saved to {LOCAL_MANIFEST_PATH}");
        println!("✓ Manifest updated successfully at {LOCAL_MANIFEST_PATH}");
        Ok(())
    }

    fn parse_json(text: &str) -> Result<Self, RavenError> {
        let raw_map: HashMap<String, serde_json::Value> = serde_json::from_str(text)
            .map_err(|e| RavenError::Manifest(format!("Failed to parse manifest JSON: {e}")))?;

        let mut sites = Vec::with_capacity(raw_map.len());

        for (name, value) in raw_map {
            if name == "$schema" {
                continue;
            }

            let raw_site: RawSiteInfo = serde_json::from_value(value)
                .map_err(|e| {
                    RavenError::Manifest(format!("Failed to parse site '{name}': {e}"))
                })?;

            match process_raw_site(&name, &raw_site) {
                Ok(site) => sites.push(site),
                Err(e) => warn!("Skipping site '{name}': {e}"),
            }
        }

        sites.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        Ok(Manifest { sites })
    }

    pub fn len(&self) -> usize {
        self.sites.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    #[allow(dead_code)]
    pub fn site_names(&self) -> Vec<&str> {
        self.sites.iter().map(|s| s.name.as_str()).collect()
    }
}

fn process_raw_site(name: &str, raw: &RawSiteInfo) -> Result<SiteInfo, RavenError> {
    let regex_check = match &raw.regex_check {
        Some(pattern) => {
            let re = Regex::new(pattern)?;
            Some(re)
        }
        None => None,
    };

    let error_types = parse_error_types(&raw.error_type);
    let error_msgs = parse_msgs(&raw.error_msg);
    let error_codes = parse_codes(&raw.error_code);
    let tags = raw.tags.clone().unwrap_or_default();

    Ok(SiteInfo {
        name: name.to_string(),
        url: raw.url.clone(),
        url_main: raw.url_main.clone(),
        url_probe: raw.url_probe.clone(),
        username_claimed: raw.username_claimed.clone(),
        regex_check,
        is_nsfw: raw.is_nsfw,
        headers: raw.headers.clone(),
        request_method: raw.request_method.clone(),
        request_payload: raw.request_payload.clone(),
        error_types,
        error_msgs,
        error_codes,
        error_url: raw.error_url.clone(),
        tags,
        scrape: raw.scrape.clone(),
    })
}

fn parse_error_types(et: &ErrorTypeOrList) -> Vec<ErrorType> {
    match et {
        ErrorTypeOrList::Single(s) => {
            vec![match s.as_str() {
                "status_code" => ErrorType::StatusCode,
                "message" => ErrorType::Message,
                "response_url" => ErrorType::ResponseUrl,
                other => {
                    warn!("Unknown error type '{other}', defaulting to status_code");
                    ErrorType::StatusCode
                }
            }]
        }
        ErrorTypeOrList::Multiple(list) => {
            list.iter()
                .map(|s| match s.as_str() {
                    "status_code" => ErrorType::StatusCode,
                    "message" => ErrorType::Message,
                    "response_url" => ErrorType::ResponseUrl,
                    other => {
                        warn!("Unknown error type '{other}' in list, defaulting to status_code");
                        ErrorType::StatusCode
                    }
                })
                .collect()
        }
    }
}

fn parse_msgs(msgs: &Option<MsgOrList>) -> Vec<String> {
    match msgs {
        Some(MsgOrList::Single(s)) => vec![s.clone()],
        Some(MsgOrList::Multiple(list)) => list.clone(),
        None => vec![],
    }
}

fn parse_codes(codes: &Option<CodeOrList>) -> Vec<i64> {
    match codes {
        Some(CodeOrList::Single(c)) => vec![*c],
        Some(CodeOrList::Multiple(list)) => list.clone(),
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bundled_manifest() {
        let m = Manifest::from_local("resources/data.json").unwrap();
        assert!(m.len() > 100, "expected >= 100 sites, got {}", m.len());
    }

    #[test]
    fn manifest_contains_github() {
        let m = Manifest::from_local("resources/data.json").unwrap();
        assert!(m.site_names().contains(&"GitHub"));
    }

    #[test]
    fn manifest_contains_gitlab() {
        let m = Manifest::from_local("resources/data.json").unwrap();
        assert!(m.site_names().contains(&"GitLab"));
    }

    #[test]
    fn manifest_all_urls_have_placeholder() {
        let m = Manifest::from_local("resources/data.json").unwrap();
        for site in &m.sites {
            let has_url_placeholder = site.url.contains("{}");
            let has_probe_placeholder = site.url_probe.as_deref().map_or(false, |u| u.contains("{}"));
            let has_payload_placeholder = site.request_payload.as_ref().map_or(false, |p| {
                let s = serde_json::to_string(p).unwrap_or_default();
                s.contains("{}")
            });
            assert!(
                has_url_placeholder || has_probe_placeholder || has_payload_placeholder,
                "Site '{}' has no placeholder in url, urlProbe, or request_payload",
                site.name
            );
        }
    }

    #[test]
    fn manifest_invalid_json() {
        let r = Manifest::parse_json("{invalid");
        assert!(r.is_err());
    }

    #[test]
    fn manifest_empty() {
        let m = Manifest::parse_json("{}").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn manifest_skips_dollar_schema() {
        let data = r#"{"$schema": "s.json", "S1": {"url": "https://s1.com/{}", "urlMain": "https://s1.com", "username_claimed": "u", "errorType": "status_code"}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m.site_names(), vec!["S1"]);
    }

    #[test]
    fn manifest_single_errotype() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": "status_code"}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert_eq!(m.sites[0].error_types, vec![ErrorType::StatusCode]);
    }

    #[test]
    fn manifest_list_errotype() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": ["message", "status_code"], "errorMsg": "not found"}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert_eq!(m.sites[0].error_types.len(), 2);
    }

    #[test]
    fn manifest_single_error_msg() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": "message", "errorMsg": "not found"}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert_eq!(m.sites[0].error_msgs, vec!["not found"]);
    }

    #[test]
    fn manifest_list_error_msg() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": "message", "errorMsg": ["a", "b"]}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert_eq!(m.sites[0].error_msgs, vec!["a", "b"]);
    }

    #[test]
    fn manifest_error_code_single() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": "status_code", "errorCode": 404}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert_eq!(m.sites[0].error_codes, vec![404]);
    }

    #[test]
    fn manifest_error_code_list() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": "status_code", "errorCode": [404, 410]}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert_eq!(m.sites[0].error_codes, vec![404, 410]);
    }

    #[test]
    fn manifest_with_nsfw() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": "status_code", "isNSFW": true}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert!(m.sites[0].is_nsfw);
    }

    #[test]
    fn manifest_with_regex_check() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": "status_code", "regexCheck": "\\w+"}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert!(m.sites[0].regex_check.is_some());
    }

    #[test]
    fn manifest_with_tags() {
        let data = r#"{"S": {"url": "https://s.com/{}", "urlMain": "https://s.com", "username_claimed": "u", "errorType": "status_code", "tags": ["social", "code"]}}"#;
        let m = Manifest::parse_json(data).unwrap();
        assert_eq!(m.sites[0].tags, vec!["social", "code"]);
    }
}
