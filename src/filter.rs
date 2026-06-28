use std::collections::HashSet;

use reqwest::Client;
use tracing::{debug, info, warn};
use crate::types::{SiteInfo, DEFAULT_EXCLUSIONS_URL};

pub fn filter_sites(
    sites: Vec<SiteInfo>,
    include_nsfw: bool,
    site_filter: &[String],
    exclusions: &HashSet<String>,
    tags: &[String],
) -> Vec<SiteInfo> {
    let mut filtered: Vec<SiteInfo> = sites;

    if !include_nsfw {
        let before = filtered.len();
        filtered.retain(|s| !s.is_nsfw);
        let removed = before - filtered.len();
        if removed > 0 {
            debug!("Removed {removed} NSFW sites");
        }
    }

    if !exclusions.is_empty() {
        let before = filtered.len();
        filtered.retain(|s| !exclusions.contains(&s.name));
        let removed = before - filtered.len();
        if removed > 0 {
            info!("Excluded {removed} sites via exclusion list");
        }
    }

    if !tags.is_empty() {
        let before = filtered.len();
        let sites_with_any_tag = filtered.iter().filter(|s| !s.tags.is_empty()).count();
        if sites_with_any_tag == 0 {
            warn!(
                "No sites in manifest have any tags. --tag filter will match 0 sites. \
                 Available tags will appear when the upstream manifest adds tag data."
            );
        }
        let tag_set: HashSet<&str> = tags.iter().map(|s| s.as_str()).collect();
        filtered.retain(|s| {
            s.tags.iter().any(|t| tag_set.contains(t.as_str()))
        });
        let removed = before - filtered.len();
        if removed > 0 {
            info!("Filtered {removed} sites by tags ({} remain)", filtered.len());
        }
    }

    if !site_filter.is_empty() {
        let target_set: HashSet<&str> = site_filter.iter().map(|s| s.as_str()).collect();
        filtered.retain(|s| {
            target_set
                .iter()
                .any(|&target| s.name.eq_ignore_ascii_case(target))
        });
        if filtered.is_empty() {
            let unknown: Vec<&str> = target_set
                .iter()
                .filter(|&&target| {
                    !filtered.iter().any(|s| s.name.eq_ignore_ascii_case(target))
                })
                .copied()
                .collect();
            warn!(
                "No matching sites found for filter(s): {:?}",
                unknown
            );
        }
    }

    filtered
}

pub async fn load_exclusions(client: &Client) -> HashSet<String> {
    match client
        .get(DEFAULT_EXCLUSIONS_URL)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            match response.text().await {
                Ok(text) => {
                    let exclusions: HashSet<String> = text
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect();
                    debug!("Loaded {} exclusions", exclusions.len());
                    exclusions
                }
                Err(e) => {
                    warn!("Failed to read exclusions response: {e}");
                    HashSet::new()
                }
            }
        }
        Ok(resp) => {
            warn!(
                "Exclusions endpoint returned HTTP {}, continuing without exclusions",
                resp.status()
            );
            HashSet::new()
        }
        Err(e) => {
            warn!("Failed to load exclusions: {e}. Continuing without exclusions.");
            HashSet::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_site(name: &str, nsfw: bool, tags: Vec<&str>) -> SiteInfo {
        SiteInfo {
            name: name.to_string(),
            url: format!("https://{}.com/{{}}", name.to_lowercase()),
            url_main: format!("https://{}.com", name.to_lowercase()),
            url_probe: None,
            username_claimed: "u".to_string(),
            regex_check: None,
            is_nsfw: nsfw,
            headers: None,
            request_method: None,
            request_payload: None,
            error_types: vec![],
            error_msgs: vec![],
            error_codes: vec![],
            error_url: None,
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            scrape: None,
        }
    }

    fn sample() -> Vec<SiteInfo> {
        vec![
            make_site("GitHub", false, vec!["social", "code"]),
            make_site("GitLab", false, vec!["social", "code"]),
            make_site("Pornhub", true, vec!["adult"]),
            make_site("Reddit", false, vec!["social"]),
            make_site("YouTube", false, vec!["video"]),
        ]
    }

    #[test]
    fn filter_noop() {
        let r = filter_sites(sample(), false, &[], &HashSet::new(), &[]);
        assert_eq!(r.len(), 4);
        assert!(!r.iter().any(|s| s.name == "Pornhub"));
    }

    #[test]
    fn filter_with_nsfw() {
        let r = filter_sites(sample(), true, &[], &HashSet::new(), &[]);
        assert_eq!(r.len(), 5);
    }

    #[test]
    fn filter_by_site_name() {
        let r = filter_sites(sample(), false, &["GitHub".to_string()], &HashSet::new(), &[]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "GitHub");
    }

    #[test]
    fn filter_by_site_case_insensitive() {
        let r = filter_sites(sample(), false, &["github".to_string()], &HashSet::new(), &[]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "GitHub");
    }

    #[test]
    fn filter_multiple_sites() {
        let r = filter_sites(sample(), false, &["GitHub".to_string(), "Reddit".to_string()], &HashSet::new(), &[]);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn filter_by_tag() {
        let r = filter_sites(sample(), false, &[], &HashSet::new(), &["video".to_string()]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "YouTube");
    }

    #[test]
    fn filter_tag_no_match() {
        let r = filter_sites(sample(), false, &[], &HashSet::new(), &["nonexistent".to_string()]);
        assert!(r.is_empty());
    }

    #[test]
    fn filter_exclusions() {
        let mut excl = HashSet::new();
        excl.insert("Reddit".to_string());
        let r = filter_sites(sample(), false, &[], &excl, &[]);
        assert!(!r.iter().any(|s| s.name == "Reddit"));
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn filter_combined() {
        let mut excl = HashSet::new();
        excl.insert("GitLab".to_string());
        let r = filter_sites(
            sample(), false, &[], &excl,
            &["social".to_string()],
        );
        assert_eq!(r.len(), 2);
        assert!(r.iter().any(|s| s.name == "GitHub"));
        assert!(r.iter().any(|s| s.name == "Reddit"));
    }

    #[test]
    fn filter_no_sites_remain() {
        let r = filter_sites(sample(), false, &["NonExistent".to_string()], &HashSet::new(), &[]);
        assert!(r.is_empty());
    }
}
