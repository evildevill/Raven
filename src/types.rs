use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use fancy_regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProbeMethod {
    Get,
    Head,
    Post,
    Put,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    StatusCode,
    Message,
    ResponseUrl,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum QueryStatus {
    Claimed,
    Available,
    Unknown,
    Illegal,
    Waf,
}

impl fmt::Display for QueryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryStatus::Claimed => write!(f, "Claimed"),
            QueryStatus::Available => write!(f, "Available"),
            QueryStatus::Unknown => write!(f, "Unknown"),
            QueryStatus::Illegal => write!(f, "Illegal"),
            QueryStatus::Waf => write!(f, "WAF"),
        }
    }
}

impl FromStr for QueryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Claimed" => Ok(QueryStatus::Claimed),
            "Available" => Ok(QueryStatus::Available),
            "Unknown" => Ok(QueryStatus::Unknown),
            "Illegal" => Ok(QueryStatus::Illegal),
            "WAF" => Ok(QueryStatus::Waf),
            _ => Err(format!("Unknown query status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ErrorTypeOrList {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MsgOrList {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CodeOrList {
    Single(i64),
    Multiple(Vec<i64>),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RawSiteInfo {
    pub url: String,
    #[serde(rename = "urlMain")]
    pub url_main: String,
    #[serde(rename = "urlProbe")]
    pub url_probe: Option<String>,
    #[serde(rename = "username_claimed")]
    pub username_claimed: String,
    #[serde(rename = "regexCheck")]
    pub regex_check: Option<String>,
    #[serde(rename = "isNSFW", default)]
    pub is_nsfw: bool,
    pub headers: Option<HashMap<String, String>>,
    #[serde(rename = "request_method")]
    pub request_method: Option<ProbeMethod>,
    #[serde(rename = "request_payload")]
    pub request_payload: Option<serde_json::Value>,
    #[serde(rename = "errorType")]
    pub error_type: ErrorTypeOrList,
    #[serde(rename = "errorMsg")]
    pub error_msg: Option<MsgOrList>,
    #[serde(rename = "errorCode")]
    pub error_code: Option<CodeOrList>,
    #[serde(rename = "errorUrl")]
    pub error_url: Option<String>,
    #[serde(rename = "__comment__")]
    pub comment: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub scrape: Option<ScrapeConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ScrapeConfig {
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub bio: Option<String>,
    pub location: Option<String>,
    pub website: Option<String>,
    pub followers: Option<String>,
    pub following: Option<String>,
    pub joined_date: Option<String>,
    pub post_count: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SiteInfo {
    pub name: String,
    pub url: String,
    pub url_main: String,
    pub url_probe: Option<String>,
    pub username_claimed: String,
    pub regex_check: Option<Regex>,
    pub is_nsfw: bool,
    pub headers: Option<HashMap<String, String>>,
    pub request_method: Option<ProbeMethod>,
    pub request_payload: Option<serde_json::Value>,
    pub error_types: Vec<ErrorType>,
    pub error_msgs: Vec<String>,
    pub error_codes: Vec<i64>,
    pub error_url: Option<String>,
    pub tags: Vec<String>,
    pub scrape: Option<ScrapeConfig>,
}

impl SiteInfo {
    pub fn url_for_username(&self, username: &str) -> String {
        self.url.replace("{}", username)
    }

    pub fn probe_url_for_username(&self, username: &str) -> String {
        match &self.url_probe {
            Some(probe_url) => probe_url.replace("{}", username),
            None => self.url_for_username(username),
        }
    }

    pub fn is_username_valid(&self, username: &str) -> bool {
        match &self.regex_check {
            Some(re) => re.is_match(username).unwrap_or(false),
            None => true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    pub username: String,
    pub site_name: String,
    pub site_url_user: String,
    pub probe_url: String,
    pub status: QueryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub username: String,
    pub timestamp: String,
    pub total_sites: usize,
    pub claimed_count: usize,
    pub available_count: usize,
    pub unknown_count: usize,
    pub illegal_count: usize,
    pub waf_count: usize,
    pub results: Vec<QueryResult>,
}

impl SearchResults {
    pub fn new(username: &str, results: Vec<QueryResult>) -> Self {
        let total_sites = results.len();
        let claimed_count = results.iter().filter(|r| r.status == QueryStatus::Claimed).count();
        let available_count = results.iter().filter(|r| r.status == QueryStatus::Available).count();
        let unknown_count = results.iter().filter(|r| r.status == QueryStatus::Unknown).count();
        let illegal_count = results.iter().filter(|r| r.status == QueryStatus::Illegal).count();
        let waf_count = results.iter().filter(|r| r.status == QueryStatus::Waf).count();
        let timestamp = chrono::Utc::now().to_rfc3339();

        SearchResults {
            username: username.to_string(),
            timestamp,
            total_sites,
            claimed_count,
            available_count,
            unknown_count,
            illegal_count,
            waf_count,
            results,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileDetails {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub location: Option<String>,
    pub website: Option<String>,
    pub followers: Option<String>,
    pub following: Option<String>,
    pub joined_date: Option<String>,
    pub post_count: Option<String>,
    pub verified: Option<bool>,
    pub emails: Vec<String>,
    pub phone_numbers: Vec<String>,
    pub linked_urls: Vec<String>,
    pub extra: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedProfile {
    pub site_name: String,
    pub site_url: String,
    pub username: String,
    pub details: ProfileDetails,
    pub avatar_phash: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceSummary {
    pub total_time_ms: u64,
    pub avg_response_ms: f64,
    pub slowest_site: Option<String>,
    pub slowest_time_ms: u64,
}

pub static WAF_FINGERPRINTS: &[&str] = &[
    ".loading-spinner{visibility:hidden}body.no-js .challenge-running{display:none}body.dark{background-color:#222;color:#d9d9d9}body.dark a{color:#fff}body.dark a:hover{color:#ee730a;text-decoration:underline}body.dark .lds-ring div{border-color:#999 transparent transparent}body.dark .font-red{color:#b20f03}body.dark",
    r#"<span id="challenge-error-text">"#,
    "AwsWafIntegration.forceRefreshToken",
    "{return l.onPageView}}),Object.defineProperty(r,\"perimeterxIdentifiers\",{enumerable:",
];

pub const CHECK_SYMBOLS: &[char] = &['_', '-', '.'];

pub fn has_wildcard(username: &str) -> bool {
    username.contains("{?}")
}

pub fn expand_wildcard(username: &str) -> Vec<String> {
    CHECK_SYMBOLS
        .iter()
        .map(|&c| username.replace("{?}", &c.to_string()))
        .collect()
}

pub const DEFAULT_MANIFEST_REMOTE_URL: &str =
    "https://raw.githubusercontent.com/evildevill/Raven/main/resources/data.json";
pub const DEFAULT_EXCLUSIONS_URL: &str =
    "https://raw.githubusercontent.com/evildevill/Raven/main/resources/false_positive_exclusions.txt";

pub const LOCAL_MANIFEST_PATH: &str = "resources/data.json";

pub fn interpolate_payload(payload: &serde_json::Value, username: &str) -> serde_json::Value {
    match payload {
        serde_json::Value::String(s) => {
            serde_json::Value::String(s.replace("{}", username))
        }
        serde_json::Value::Object(map) => {
            let new_map: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), interpolate_payload(v, username)))
                .collect();
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            let new_arr: Vec<serde_json::Value> = arr
                .iter()
                .map(|v| interpolate_payload(v, username))
                .collect();
            serde_json::Value::Array(new_arr)
        }
            other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_wildcard_true() {
        assert!(has_wildcard("{?}"));
        assert!(has_wildcard("test{?}user"));
    }

    #[test]
    fn test_has_wildcard_false() {
        assert!(!has_wildcard("simple"));
        assert!(!has_wildcard("john_doe"));
    }

    #[test]
    fn test_expand_wildcard() {
        let variants = expand_wildcard("a{?}b");
        assert_eq!(variants.len(), 3);
        assert!(variants.contains(&"a_b".to_string()));
        assert!(variants.contains(&"a-b".to_string()));
        assert!(variants.contains(&"a.b".to_string()));
    }

    #[test]
    fn test_expand_wildcard_no_placeholder() {
        let variants = expand_wildcard("nope");
        assert_eq!(variants.len(), 3);
        assert_eq!(variants[0], "nope");
    }

    #[test]
    fn test_url_for_username() {
        let site = SiteInfo {
            name: "T".to_string(),
            url: "https://t.com/{}".to_string(),
            url_main: "https://t.com".to_string(),
            url_probe: None,
            username_claimed: "u".to_string(),
            regex_check: None,
            is_nsfw: false,
            headers: None,
            request_method: None,
            request_payload: None,
            error_types: vec![ErrorType::StatusCode],
            error_msgs: vec![],
            error_codes: vec![404],
            error_url: None,
            tags: vec![],
            scrape: None,
        };
        assert_eq!(site.url_for_username("joe"), "https://t.com/joe");
        assert_eq!(site.probe_url_for_username("joe"), "https://t.com/joe");
    }

    #[test]
    fn test_probe_url_custom() {
        let site = SiteInfo {
            url_probe: Some("https://api.t.com/{}".to_string()),
            ..SiteInfo {
                name: "T".to_string(),
                url: "https://t.com/{}".to_string(),
                url_main: "https://t.com".to_string(),
                url_probe: None,
                username_claimed: "u".to_string(),
                regex_check: None,
                is_nsfw: false,
                headers: None,
                request_method: None,
                request_payload: None,
                error_types: vec![ErrorType::StatusCode],
                error_msgs: vec![],
                error_codes: vec![404],
                error_url: None,
                tags: vec![],
                scrape: None,
            }
        };
        assert_eq!(site.probe_url_for_username("joe"), "https://api.t.com/joe");
    }

    #[test]
    fn test_username_valid_no_regex() {
        let site = SiteInfo {
            name: "T".to_string(),
            url: "https://t.com/{}".to_string(),
            url_main: "https://t.com".to_string(),
            url_probe: None,
            username_claimed: "u".to_string(),
            regex_check: None,
            is_nsfw: false,
            headers: None,
            request_method: None,
            request_payload: None,
            error_types: vec![ErrorType::StatusCode],
            error_msgs: vec![],
            error_codes: vec![404],
            error_url: None,
            tags: vec![],
            scrape: None,
        };
        assert!(site.is_username_valid("anything!!"));
    }

    #[test]
    fn test_username_valid_with_regex() {
        let re = fancy_regex::Regex::new(r"^\w+$").unwrap();
        let site = SiteInfo {
            regex_check: Some(re),
            ..SiteInfo {
                name: "T".to_string(),
                url: "https://t.com/{}".to_string(),
                url_main: "https://t.com".to_string(),
                url_probe: None,
                username_claimed: "u".to_string(),
                regex_check: None,
                is_nsfw: false,
                headers: None,
                request_method: None,
                request_payload: None,
                error_types: vec![ErrorType::StatusCode],
                error_msgs: vec![],
                error_codes: vec![404],
                error_url: None,
                tags: vec![],
                scrape: None,
            }
        };
        assert!(site.is_username_valid("normal_user"));
        assert!(!site.is_username_valid("dash-user"));
    }

    #[test]
    fn test_search_results_counts() {
        let results = vec![
            QueryResult {
                username: "joe".to_string(),
                site_name: "A".to_string(),
                site_url_user: "https://a.com/joe".to_string(),
                probe_url: String::new(),
                status: QueryStatus::Claimed,
                query_time_ms: Some(100),
                http_status: Some(200),
                context: None,
            },
            QueryResult {
                username: "joe".to_string(),
                site_name: "B".to_string(),
                site_url_user: "https://b.com/joe".to_string(),
                probe_url: String::new(),
                status: QueryStatus::Available,
                query_time_ms: Some(50),
                http_status: Some(404),
                context: None,
            },
            QueryResult {
                username: "joe".to_string(),
                site_name: "C".to_string(),
                site_url_user: "https://c.com/joe".to_string(),
                probe_url: String::new(),
                status: QueryStatus::Unknown,
                query_time_ms: None,
                http_status: None,
                context: None,
            },
        ];
        let sr = SearchResults::new("joe", results);
        assert_eq!(sr.total_sites, 3);
        assert_eq!(sr.claimed_count, 1);
        assert_eq!(sr.available_count, 1);
        assert_eq!(sr.unknown_count, 1);
        assert_eq!(sr.illegal_count, 0);
        assert_eq!(sr.waf_count, 0);
    }

    #[test]
    fn test_search_results_empty() {
        let sr = SearchResults::new("nobody", vec![]);
        assert_eq!(sr.total_sites, 0);
        assert_eq!(sr.claimed_count, 0);
    }

    #[test]
    fn test_interpolate_string() {
        let v = serde_json::json!("hello {}");
        assert_eq!(interpolate_payload(&v, "world"), serde_json::json!("hello world"));
    }

    #[test]
    fn test_interpolate_nested() {
        let v = serde_json::json!({"user": "{}", "nested": {"name": "pre_{}_suf"}});
        let r = interpolate_payload(&v, "x");
        assert_eq!(r["user"], "x");
        assert_eq!(r["nested"]["name"], "pre_x_suf");
    }

    #[test]
    fn test_interpolate_array() {
        let v = serde_json::json!(["{}", "static", {"k": "{}"}]);
        let r = interpolate_payload(&v, "y");
        assert_eq!(r[0], "y");
        assert_eq!(r[1], "static");
        assert_eq!(r[2]["k"], "y");
    }

    #[test]
    fn test_waf_fingerprints_nonempty() {
        for fp in WAF_FINGERPRINTS {
            assert!(!fp.is_empty());
        }
        assert!(WAF_FINGERPRINTS[0].contains(".loading-spinner"));
    }

    #[test]
    fn test_query_status_display() {
        assert_eq!(QueryStatus::Claimed.to_string(), "Claimed");
        assert_eq!(QueryStatus::Available.to_string(), "Available");
        assert_eq!(QueryStatus::Unknown.to_string(), "Unknown");
        assert_eq!(QueryStatus::Illegal.to_string(), "Illegal");
        assert_eq!(QueryStatus::Waf.to_string(), "WAF");
    }
}
