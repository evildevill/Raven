use colored::Colorize;
use reqwest::Client;
use serde::Deserialize;
use tracing::debug;

const VERSION_CHECK_URL: &str = "https://api.github.com/repos/evildevill/Raven/releases/latest";

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

pub async fn check_for_update(client: &Client, no_color: bool) {
    match fetch_latest_release(client).await {
        Ok(release) => {
            let latest = release.tag_name.trim_start_matches('v');
            if is_newer(latest, env!("CARGO_PKG_VERSION")) {
                let msg = format!(
                    "Update available: {} → {} at {}",
                    env!("CARGO_PKG_VERSION"),
                    latest,
                    release.html_url,
                );
                if no_color {
                    eprintln!("{msg}");
                } else {
                    eprintln!("{}", msg.yellow());
                }
            }
        }
        Err(e) => {
            debug!("Version check failed: {e}");
        }
    }
}

async fn fetch_latest_release(client: &Client) -> Result<Release, Box<dyn std::error::Error>> {
    let resp = client
        .get(VERSION_CHECK_URL)
        .header("User-Agent", "raven")
        .header("Accept", "application/vnd.github.v3+json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned HTTP {}", resp.status()).into());
    }

    let release: Release = resp.json().await?;
    Ok(release)
}

fn parse_version(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split('.')
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let latest_parts = parse_version(latest);
    let current_parts = parse_version(current);

    for (l, c) in latest_parts.iter().zip(current_parts.iter()) {
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }

    latest_parts.len() > current_parts.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_major() {
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("2.0", "1.999"));
    }

    #[test]
    fn newer_minor() {
        assert!(is_newer("0.2.0", "0.1.99"));
        assert!(is_newer("1.2", "1.1"));
    }

    #[test]
    fn newer_patch() {
        assert!(is_newer("0.1.2", "0.1.1"));
        assert!(is_newer("0.1.10", "0.1.9"));
    }

    #[test]
    fn same_version() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("1.0", "1.0"));
    }

    #[test]
    fn current_is_newer() {
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("1.0", "2.0"));
    }

    #[test]
    fn different_lengths() {
        assert!(is_newer("0.1.0.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0.0"));
    }

    #[test]
    fn strip_v_prefix() {
        assert!(is_newer("v0.2.0", "0.1.0"));
    }

    #[test]
    fn parse_version_string() {
        assert_eq!(parse_version("0.1.0"), vec![0, 1, 0]);
        assert_eq!(parse_version("v1.2.3"), vec![1, 2, 3]);
        assert_eq!(parse_version("10.20"), vec![10, 20]);
    }
}
