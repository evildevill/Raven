use std::time::Instant;

use reqwest::Client;
use tracing::{debug, trace};

use crate::client::build_site_request;
use crate::error::RavenError;
use crate::types::*;

pub async fn probe_site(
    site: &SiteInfo,
    client: &Client,
    username: &str,
    retry_count: usize,
    dump_response: bool,
) -> Result<QueryResult, RavenError> {
    let _start = Instant::now();

    let url_user = site.url_for_username(username);
    let probe_url = site.probe_url_for_username(username);

    if !site.is_username_valid(username) {
        debug!("Username '{username}' is illegal for site '{}'", site.name);
        return Ok(QueryResult {
            username: username.to_string(),
            site_name: site.name.clone(),
            site_url_user: url_user,
            probe_url: probe_url.clone(),
            status: QueryStatus::Illegal,
            query_time_ms: Some(0),
            http_status: None,
            context: None,
        });
    }

    let max_attempts = retry_count + 1;

    for attempt in 1..=max_attempts {
        let attempt_start = Instant::now();
        let req = build_site_request(client, site, &probe_url, username);

        let req_fut = req.send();
        let attempt_timeout = Duration::from_secs(15);
        let result = tokio::time::timeout(attempt_timeout, req_fut).await;

        match result {
            Ok(Ok(resp)) => {
                let elapsed = attempt_start.elapsed().as_millis() as u64;
                let http_status = resp.status().as_u16();
                let response_url = resp.url().to_string();
                let response_text = resp.text().await.unwrap_or_default();

                trace!(
                    "Probed {} for '{}': HTTP {} in {}ms (attempt {})",
                    site.name,
                    username,
                    http_status,
                    elapsed,
                    attempt
                );

                if dump_response {
                    println!("+++++++++++++++++++++");
                    println!("TARGET NAME   : {}", site.name);
                    println!("USERNAME      : {username}");
                    println!("TARGET URL    : {probe_url}");
                    println!("HTTP STATUS   : {http_status}");
                    println!("RESPONSE TIME : {elapsed}ms");
                    println!(">>>>> BEGIN RESPONSE TEXT ({} bytes)", response_text.len());
                    println!("{response_text}");
                    println!("<<<<< END RESPONSE TEXT");
                    println!("+++++++++++++++++++++");
                }

                let (status, context) = classify_response(site, &response_text, http_status, &response_url);

                return Ok(QueryResult {
                    username: username.to_string(),
                    site_name: site.name.clone(),
                    site_url_user: url_user,
                    probe_url: probe_url.clone(),
                    status,
                    query_time_ms: Some(elapsed),
                    http_status: Some(http_status),
                    context,
                });
            }
            Ok(Err(e)) => {
                let elapsed = attempt_start.elapsed().as_millis() as u64;
                let is_retryable = e.is_timeout() || e.is_connect();

                if attempt < max_attempts && is_retryable {
                    let backoff = Duration::from_millis(500 * attempt as u64);
                    debug!(
                        "Attempt {}/{} for '{}' on '{}' failed: {}. Retrying in {}ms...",
                        attempt,
                        max_attempts,
                        username,
                        site.name,
                        classify_network_error(&e),
                        backoff.as_millis()
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                let error_context = classify_network_error(&e);
                return Ok(QueryResult {
                    username: username.to_string(),
                    site_name: site.name.clone(),
                    site_url_user: url_user,
                    probe_url: probe_url.clone(),
                    status: QueryStatus::Unknown,
                    query_time_ms: Some(elapsed),
                    http_status: None,
                    context: Some(error_context),
                });
            }
            Err(_) => {
                let elapsed = attempt_start.elapsed().as_millis() as u64;

                if attempt < max_attempts {
                    let backoff = Duration::from_millis(500 * attempt as u64);
                    debug!(
                        "Attempt {}/{} for '{}' on '{}' timed out (15s). Retrying in {}ms...",
                        attempt,
                        max_attempts,
                        username,
                        site.name,
                        backoff.as_millis()
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                return Ok(QueryResult {
                    username: username.to_string(),
                    site_name: site.name.clone(),
                    site_url_user: url_user,
                    probe_url: probe_url.clone(),
                    status: QueryStatus::Unknown,
                    query_time_ms: Some(elapsed),
                    http_status: None,
                    context: Some("Request timed out (15s)".to_string()),
                });
            }
        }
    }

    Err(RavenError::Other("Unreachable code in probe_site".to_string()))
}

use std::time::Duration;

fn classify_response(
    site: &SiteInfo,
    response_text: &str,
    http_status: u16,
    response_url: &str,
) -> (QueryStatus, Option<String>) {
    for fingerprint in WAF_FINGERPRINTS {
        if response_text.contains(fingerprint) {
            debug!("WAF detected for site '{}'", site.name);
            return (QueryStatus::Waf, None);
        }
    }

    let mut final_status = QueryStatus::Unknown;
    let mut final_context: Option<String> = None;

    for error_type in &site.error_types {
        match error_type {
            ErrorType::Message => {
                if site.error_msgs.is_empty() {
                    continue;
                }
                let error_found = site.error_msgs.iter().any(|msg| response_text.contains(msg));
                if error_found {
                    final_status = QueryStatus::Available;
                } else if final_status == QueryStatus::Unknown {
                    final_status = QueryStatus::Claimed;
                }
            }
            ErrorType::StatusCode => {
                if http_status >= 300 || http_status < 200 {
                    if !site.error_codes.is_empty() {
                        if site.error_codes.contains(&(http_status as i64)) {
                            final_status = QueryStatus::Available;
                        } else if final_status == QueryStatus::Unknown {
                            final_status = QueryStatus::Claimed;
                        }
                    } else {
                        final_status = QueryStatus::Available;
                    }
                } else if final_status == QueryStatus::Unknown {
                    final_status = QueryStatus::Claimed;
                }
            }
            ErrorType::ResponseUrl => {
                if (200..300).contains(&http_status) {
                    if let Some(error_url) = &site.error_url {
                        if response_url.starts_with(error_url) {
                            final_status = QueryStatus::Available;
                        } else if final_status == QueryStatus::Unknown {
                            final_status = QueryStatus::Claimed;
                        }
                    } else if final_status == QueryStatus::Unknown {
                        final_status = QueryStatus::Claimed;
                    }
                } else {
                    final_status = QueryStatus::Available;
                }
            }
        }
    }

    if let Some(err_url) = &site.error_url {
        if response_url.starts_with(err_url) {
            final_context = Some("Redirected to error URL".to_string());
        }
    }

    (final_status, final_context)
}

fn classify_network_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "Request timed out".to_string()
    } else if err.is_connect() {
        "Failed to connect".to_string()
    } else if err.is_status() {
        if let Some(status) = err.status() {
            format!("HTTP {}", status.as_u16())
        } else {
            "HTTP error".to_string()
        }
    } else {
        format!("Network error: {err}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_site(name: &str, url: &str, error_types: Vec<ErrorType>, error_msgs: Vec<String>, error_codes: Vec<i64>, error_url: Option<String>) -> SiteInfo {
        SiteInfo {
            name: name.to_string(),
            url: url.to_string(),
            url_main: format!("https://{}", url.split('/').next().unwrap_or("")),
            url_probe: None,
            username_claimed: "u".to_string(),
            regex_check: None,
            is_nsfw: false,
            headers: None,
            request_method: None,
            request_payload: None,
            error_types,
            error_msgs,
            error_codes,
            error_url,
            tags: vec![],
            scrape: None,
        }
    }

    fn msg_site(error_msgs: Vec<&str>) -> SiteInfo {
        make_site(
            "Msg",
            "https://msg.com/{}",
            vec![ErrorType::Message],
            error_msgs.into_iter().map(|s| s.to_string()).collect(),
            vec![],
            None,
        )
    }

    fn status_site(error_codes: Vec<i64>) -> SiteInfo {
        make_site(
            "Status",
            "https://status.com/{}",
            vec![ErrorType::StatusCode],
            vec![],
            error_codes,
            None,
        )
    }

    fn redirect_site(error_url: &str) -> SiteInfo {
        make_site(
            "Redir",
            "https://redir.com/{}",
            vec![ErrorType::ResponseUrl],
            vec![],
            vec![],
            Some(error_url.to_string()),
        )
    }

    #[test]
    fn classify_message_found() {
        let site = msg_site(vec!["not found"]);
        let (status, _) = classify_response(&site, "this page is not found", 200, "https://msg.com/user");
        assert_eq!(status, QueryStatus::Available);
    }

    #[test]
    fn classify_message_not_found() {
        let site = msg_site(vec!["not found"]);
        let (status, _) = classify_response(&site, "welcome to my profile", 200, "https://msg.com/user");
        assert_eq!(status, QueryStatus::Claimed);
    }

    #[test]
    fn classify_message_multiple_any_match() {
        let site = msg_site(vec!["not found", "does not exist"]);
        let (status, _) = classify_response(&site, "this page does not exist", 200, "");
        assert_eq!(status, QueryStatus::Available);
    }

    #[test]
    fn classify_status_code_404() {
        let site = status_site(vec![404]);
        let (status, _) = classify_response(&site, "", 404, "");
        assert_eq!(status, QueryStatus::Available);
    }

    #[test]
    fn classify_status_code_200() {
        let site = status_site(vec![404]);
        let (status, _) = classify_response(&site, "", 200, "");
        assert_eq!(status, QueryStatus::Claimed);
    }

    #[test]
    fn classify_status_code_no_codes_list() {
        let site = status_site(vec![]);
        let (status, _) = classify_response(&site, "", 404, "");
        assert_eq!(status, QueryStatus::Available);
        let (status, _) = classify_response(&site, "", 200, "");
        assert_eq!(status, QueryStatus::Claimed);
    }

    #[test]
    fn classify_status_code_3xx_no_codes() {
        let site = status_site(vec![]);
        let (status, _) = classify_response(&site, "", 302, "");
        assert_eq!(status, QueryStatus::Available);
    }

    #[test]
    fn classify_redirect_available() {
        let site = redirect_site("https://redir.com/404");
        let (status, _) = classify_response(&site, "", 200, "https://redir.com/404");
        assert_eq!(status, QueryStatus::Available);
    }

    #[test]
    fn classify_redirect_claimed() {
        let site = redirect_site("https://redir.com/404");
        let (status, _) = classify_response(&site, "", 200, "https://redir.com/user");
        assert_eq!(status, QueryStatus::Claimed);
    }

    #[test]
    fn classify_redirect_non_200() {
        let site = redirect_site("https://redir.com/404");
        let (status, _) = classify_response(&site, "", 500, "https://redir.com/500");
        assert_eq!(status, QueryStatus::Available);
    }

    #[test]
    fn classify_waf_cloudflare() {
        let site = status_site(vec![404]);
        let (status, _) = classify_response(&site, WAF_FINGERPRINTS[0], 200, "");
        assert_eq!(status, QueryStatus::Waf);
    }

    #[test]
    fn classify_combined_msg_and_status() {
        let site = SiteInfo {
            error_types: vec![ErrorType::Message, ErrorType::StatusCode],
            error_msgs: vec!["not found".to_string()],
            error_codes: vec![404],
            ..msg_site(vec![])
        };
        let (status, _) = classify_response(&site, "not found", 404, "");
        assert_eq!(status, QueryStatus::Available);
    }

    #[test]
    fn classify_redirect_context() {
        let site = redirect_site("https://redir.com/404");
        let (_, ctx) = classify_response(&site, "", 200, "https://redir.com/404");
        assert_eq!(ctx, Some("Redirected to error URL".to_string()));
    }
}
