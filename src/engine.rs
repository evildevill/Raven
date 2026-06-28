use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use colored::Colorize;
use futures::pin_mut;
use futures::stream::{self, StreamExt};
use reqwest::Client;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::detector::probe_site;
use crate::error::RavenError;
use crate::rate_limiter::RateLimiter;
use crate::types::*;

pub async fn search_username(
    username: &str,
    sites: &[SiteInfo],
    client: &Client,
    concurrency: usize,
    retry_count: usize,
    rate_limiter: Option<RateLimiter>,
    dump_response: bool,
    unique_tor: bool,
    shutdown: Arc<AtomicBool>,
    print_all: bool,
    verbose: bool,
    browse: bool,
) -> Result<SearchResults, RavenError> {
    let usernames = if has_wildcard(username) {
        expand_wildcard(username)
    } else {
        vec![username.to_string()]
    };

    let mut all_results = Vec::new();

    for uname in &usernames {
        if shutdown.load(Ordering::Relaxed) {
            info!("Shutdown requested, stopping search for '{username}' early");
            break;
        }
        let results = search_single(uname, sites, client, concurrency, retry_count, &rate_limiter, dump_response, unique_tor, &shutdown, print_all, verbose, browse).await?;
        all_results.extend(results);
    }

    Ok(SearchResults::new(username, all_results))
}

fn format_result_line(result: &QueryResult, verbose: bool) -> String {
    let time_str = result
        .query_time_ms
        .map(|ms| {
            if ms >= 1000 {
                format!("{:.1}s", ms as f64 / 1000.0)
            } else {
                format!("{ms}ms")
            }
        })
        .unwrap_or_default();

    let http_str = result
        .http_status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "--".to_string());

    match result.status {
        QueryStatus::Claimed => {
            let icon = "+".green();
            let mut line = format!(" {}  {}  {}", icon, result.site_name.green().bold(), result.site_url_user.cyan().underline());
            if verbose {
                line.push_str(&format!("\n     └─HTTP {}  {}", http_str, time_str));
            }
            line
        }
        QueryStatus::Available => {
            let icon = "-".yellow();
            format!(" {}  {}    {}", icon, result.site_name.yellow(), "Not Found".yellow())
        }
        QueryStatus::Unknown => {
            let icon = "?".red();
            let context = result.context.as_deref().unwrap_or("Unknown error");
            format!(" {}  {}  {}  {}", icon, result.site_name.red(), context.red(), time_str.red())
        }
        QueryStatus::Illegal => {
            let icon = "!".white();
            format!(" {}  {}    {}", icon, result.site_name.white(), "Invalid username format".white())
        }
        QueryStatus::Waf => {
            let icon = "!".red();
            format!(" {}  {}    {}", icon, result.site_name.red(), "WAF Blocked".red())
        }
    }
}

async fn search_single(
    username: &str,
    sites: &[SiteInfo],
    client: &Client,
    concurrency: usize,
    retry_count: usize,
    rate_limiter: &Option<RateLimiter>,
    dump_response: bool,
    unique_tor: bool,
    shutdown: &AtomicBool,
    print_all: bool,
    verbose: bool,
    browse: bool,
) -> Result<Vec<QueryResult>, RavenError> {
    let total = sites.len();
    info!("Searching '{username}' across {total} sites");

    let header = format!("  {} {}", "◆".cyan().bold(), username.white().bold());
    let sep = format!("  {}", "─".repeat(60));
    println!("{header}");
    println!("{sep}");

    let client = Arc::new(client.clone());
    let rate_limiter = rate_limiter.clone();
    let shutdown_signal = shutdown;

    let tasks: Vec<_> = sites
        .iter()
        .map(|site| {
            let client = client.clone();
            let site = site.clone();
            let username = username.to_string();
            let rate_limiter = rate_limiter.clone();

            async move {
                if unique_tor {
                    if let Err(e) = crate::tor_controller::new_tor_circuit().await {
                        warn!("Failed to rotate Tor circuit: {e}");
                    }
                }
                if let Some(ref rl) = rate_limiter {
                    rl.acquire().await;
                }
                let result = probe_site(&site, &client, &username, retry_count, dump_response).await;
                (result, site.name.clone())
            }
        })
        .collect();

    let stream = stream::iter(tasks).buffer_unordered(concurrency.max(1));
    pin_mut!(stream);

    let mut results: Vec<QueryResult> = Vec::new();

    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    Some((result, site_name)) => {
                        let qr = match result {
                            Ok(r) => r,
                            Err(e) => {
                                error!("Error probing '{site_name}': {e}");
                                QueryResult {
                                    username: username.to_string(),
                                    site_name,
                                    site_url_user: String::new(),
                                    probe_url: String::new(),
                                    status: QueryStatus::Unknown,
                                    query_time_ms: None,
                                    http_status: None,
                                    context: Some(format!("{e}")),
                                }
                            }
                        };

                        // Print result line
                        let show = matches!(qr.status, QueryStatus::Claimed) || print_all;
                        if show {
                            let line = format_result_line(&qr, verbose);
                            println!("{line}");
                        }

                        let is_claimed = matches!(qr.status, QueryStatus::Claimed);
                        let url = qr.site_url_user.clone();
                        results.push(qr);

                        if show && is_claimed && browse && !url.is_empty() {
                            if let Err(e) = webbrowser::open(&url) {
                                info!("Failed to open browser: {e}");
                            }
                        }
                    }
                    None => break,
                }
            }
            _ = wait_for_shutdown(shutdown_signal) => {
                debug!("Shutdown detected, dropping remaining in-flight requests");
                drop(stream);
                break;
            }
        }
    }
    let total_results = results.len();
    let claimed_count = results.iter().filter(|r| r.status == QueryStatus::Claimed).count();

    print_search_summary(total_results, claimed_count, results.iter().filter(|r| r.status == QueryStatus::Available).count(), results.iter().filter(|r| r.status == QueryStatus::Unknown).count(), results.iter().filter(|r| r.status == QueryStatus::Illegal).count(), results.iter().filter(|r| r.status == QueryStatus::Waf).count(), verbose, print_all);

    info!(
        "Completed '{username}': {}/{} claimed",
        claimed_count,
        total_results
    );

    Ok(results)
}

fn print_search_summary(total: usize, claimed: usize, available: usize, unknown: usize, illegal: usize, waf: usize, verbose: bool, print_all: bool) {
    let line = "─".repeat(60);
    println!("  {}", line.dimmed());

    if total > 0 {
        let claimed_pct = claimed as f64 / total as f64 * 100.0;
        let bar_len: usize = 30;
        let filled = (claimed_pct / 100.0 * bar_len as f64).round() as usize;
        let bar = format!("{}{}", "▓".repeat(filled).green(), "░".repeat(bar_len.saturating_sub(filled)));
        println!("  {}  {} {:>3.0}%", "Found".green().bold(), bar, claimed_pct);
    }

    if verbose || print_all {
        let mut parts: Vec<String> = Vec::new();
        if claimed > 0 {
            parts.push(format!("{} {}", claimed.to_string().green().bold(), "found".green()));
        }
        if available > 0 {
            parts.push(format!("{} {}", available.to_string().yellow().bold(), "available".yellow()));
        }
        if unknown > 0 {
            parts.push(format!("{} {}", unknown.to_string().red().bold(), "unknown".red()));
        }
        if illegal > 0 {
            parts.push(format!("{} {}", illegal.to_string().white().bold(), "invalid".white()));
        }
        if waf > 0 {
            parts.push(format!("{} {}", waf.to_string().red().bold(), "WAF".red()));
        }
        if !parts.is_empty() {
            println!("  Breakdown: {}", parts.join(", "));
        }
    }

    println!("  {}  {}", "Total:".dimmed(), total.to_string().white().bold());
    println!();
}

async fn wait_for_shutdown(shutdown: &AtomicBool) {
    while !shutdown.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[derive(Clone)]
pub enum SearchUpdate {
    Result(QueryResult),
    Progress { completed: usize, total: usize },
    Complete { total: usize, claimed: usize, available: usize, unknown: usize, illegal: usize, waf: usize },
    Intelligence { confidence: f32, profile_count: usize, graph_json: String, timeline_json: String, domain_json: String },
    Error(String),
}

pub async fn search_username_stream(
    username: &str,
    sites: &[SiteInfo],
    client: &Client,
    concurrency: usize,
    retry_count: usize,
    rate_limiter: Option<RateLimiter>,
    unique_tor: bool,
    shutdown: Arc<AtomicBool>,
    tx: broadcast::Sender<SearchUpdate>,
) {
    let usernames = if has_wildcard(username) {
        expand_wildcard(username)
    } else {
        vec![username.to_string()]
    };

    for uname in &usernames {
        if shutdown.load(Ordering::Relaxed) {
            tx.send(SearchUpdate::Error("Shutdown requested".to_string())).ok();
            return;
        }
        search_single_stream(uname, sites, client, concurrency, retry_count, &rate_limiter, unique_tor, &shutdown, &tx).await;
    }
}

async fn search_single_stream(
    username: &str,
    sites: &[SiteInfo],
    client: &Client,
    concurrency: usize,
    retry_count: usize,
    rate_limiter: &Option<RateLimiter>,
    unique_tor: bool,
    shutdown: &AtomicBool,
    tx: &broadcast::Sender<SearchUpdate>,
) {
    let total = sites.len();

    let client = Arc::new(client.clone());
    let rate_limiter = rate_limiter.clone();

    let tasks: Vec<_> = sites
        .iter()
        .map(|site| {
            let client = client.clone();
            let site = site.clone();
            let username = username.to_string();
            let rate_limiter = rate_limiter.clone();

            async move {
                if unique_tor {
                    if let Err(e) = crate::tor_controller::new_tor_circuit().await {
                        warn!("Failed to rotate Tor circuit: {e}");
                    }
                }
                if let Some(ref rl) = rate_limiter {
                    rl.acquire().await;
                }
                let result = probe_site(&site, &client, &username, retry_count, false).await;
                (result, site.name.clone())
            }
        })
        .collect();

    let stream = stream::iter(tasks).buffer_unordered(concurrency.max(1));
    pin_mut!(stream);

    let mut results: Vec<QueryResult> = Vec::new();

    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    Some((result, _site_name)) => {
                        let qr = match result {
                            Ok(r) => r,
                            Err(e) => {
                                error!("Stream error: {e}");
                                QueryResult {
                                    username: username.to_string(),
                                    site_name: "unknown".to_string(),
                                    site_url_user: String::new(),
                                    probe_url: String::new(),
                                    status: QueryStatus::Unknown,
                                    query_time_ms: None,
                                    http_status: None,
                                    context: Some(format!("{e}")),
                                }
                            }
                        };
                        results.push(qr.clone());
                        tx.send(SearchUpdate::Result(qr)).ok();
                        let p = results.len();
                        tx.send(SearchUpdate::Progress { completed: p, total }).ok();
                    }
                    None => break,
                }
            }
            _ = wait_for_shutdown(shutdown) => {
                break;
            }
        }
    }

    let claimed = results.iter().filter(|r| r.status == QueryStatus::Claimed).count();
    let available = results.iter().filter(|r| r.status == QueryStatus::Available).count();
    let unknown = results.iter().filter(|r| r.status == QueryStatus::Unknown).count();
    let illegal = results.iter().filter(|r| r.status == QueryStatus::Illegal).count();
    let waf = results.iter().filter(|r| r.status == QueryStatus::Waf).count();

    tx.send(SearchUpdate::Complete { total, claimed, available, unknown, illegal, waf }).ok();
}
