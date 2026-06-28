mod avatar;
mod banner;
mod cli;
mod client;
mod config;
mod database;
mod detector;
mod domain;
mod engine;
mod enricher;
mod error;
mod filter;
mod graph;
mod identity;
mod manifest;
mod rate_limiter;
mod reporter;
mod scraper;
mod timeline;
mod tor_controller;
mod types;
mod update_check;
mod variants;
mod web;

use std::collections::HashSet;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use clap::CommandFactory;
use clap_complete::{generate, Shell};
use colored::Colorize;
use tokio::signal;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use avatar::find_avatar_matches;
use cli::Cli;
use config::Config;
use database::ScanDb;
use domain::{find_domain, DomainInfo};
use enricher::enrich_profile;
use error::RavenError;
use filter::{filter_sites, load_exclusions};
use graph::{build_account_graph, to_dot};
use identity::{build_identity_cluster, IdentityCluster};
use manifest::Manifest;
use rate_limiter::RateLimiter;
use reporter::*;
use scraper::scrape_profile;
use timeline::build_timeline;
use types::*;
use variants::generate_variants;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), RavenError> {
    let config = Config::load();
    let cli = Cli::new_with_config(config);

    if std::env::var("RUST_LOG").is_err() {
        if cli.verbose {
            std::env::set_var("RUST_LOG", "debug");
        } else {
            std::env::set_var("RUST_LOG", "info");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    debug!("CLI args: {cli:#?}");

    if !cli.update_manifest && cli.completions.is_none() && cli.history.is_none() {
        banner::print_banner(cli.no_color);
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        signal::ctrl_c().await.ok();
        eprintln!("\nReceived Ctrl+C, shutting down gracefully...");
        shutdown_signal.store(true, Ordering::Relaxed);
    });

    if let Some(shell) = &cli.completions {
        generate_completions(shell)?;
        return Ok(());
    }

    if cli.serve {
        let addr = format!("{}:{}", cli.host, cli.port);
        info!("Starting web UI server on http://{addr}");
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, web::router()).await?;
        return Ok(());
    }

    if let Some(filter) = cli.history.as_ref() {
        let db = ScanDb::open()?;
        let username = if filter.is_empty() { None } else { Some(filter.as_str()) };
        let scans = db.list_scans(username)?;
        if scans.is_empty() {
            println!("No scan history found.");
            return Ok(());
        }
        for scan in &scans {
            let completed = scan.completed_at.as_deref().unwrap_or("-");
            println!(
                "  #{:<4} {:<20} {}  total={:<4} claimed={:<4} avail={:<4} unknown={:<4} illegal={:<4} waf={:<4}",
                scan.id,
                scan.username,
                completed,
                scan.total_sites,
                scan.claimed,
                scan.available,
                scan.unknown,
                scan.illegal,
                scan.waf,
            );
        }
        return Ok(());
    }

    let proxy_url = resolve_proxy(&cli);
    let http_client = client::create_http_client(cli.timeout, proxy_url.as_deref())?;

    update_check::check_for_update(&http_client, cli.no_color).await;

    if cli.update_manifest {
        Manifest::update_manifest(&http_client, types::DEFAULT_MANIFEST_REMOTE_URL).await?;
        return Ok(());
    }

    let manifest = if let Some(ref json_source) = cli.json {
        info!("Loading custom manifest from '{json_source}'");
        Manifest::load_custom(&http_client, json_source).await?
    } else {
        Manifest::load_default(&http_client, cli.local).await?
    };
    info!("Loaded {} sites from manifest", manifest.len());

    let exclusions: HashSet<String> = if cli.ignore_exclusions {
        debug!("Exclusions disabled by --ignore-exclusions");
        HashSet::new()
    } else {
        load_exclusions(&http_client).await
    };

    let sites = filter_sites(
        manifest.sites,
        cli.nsfw,
        &cli.site_list,
        &exclusions,
        &cli.tag,
    );

    if sites.is_empty() {
        return Err(RavenError::Cli(
            "No sites to search after filtering".to_string(),
        ));
    }

    info!(
        "Searching {} sites{}",
        sites.len(),
        if cli.nsfw { " (incl. NSFW)" } else { "" }
    );

    let concurrency = cli.effective_concurrency();
    let retry_count = cli.retry;
    let rate_limiter = cli.rate_limit.map(RateLimiter::new);

    debug!("Using concurrency of {concurrency}, retry={retry_count}");

    let usernames = resolve_usernames(&cli)?;
    let total_start = Instant::now();
    let mut all_results: Vec<types::SearchResults> = Vec::new();

    if let Some(ref cron_expr) = cli.schedule {
        let schedule = parse_cron(cron_expr)?;
        info!("Cron schedule: {cron_expr} — will re-scan and report new findings");
        let db = ScanDb::open()?;
        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            info!("Starting scheduled scan...");
            for username in &usernames {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                let prev = db.get_last_scan(username)?;
                let prev_results = match prev {
                    Some(ref record) => db.get_last_scan_results(record.id).ok().unwrap_or_default(),
                    None => vec![],
                };

                let results = engine::search_username(
                    username,
                    &sites,
                    &http_client,
                    concurrency,
                    retry_count,
                    rate_limiter.clone(),
                    cli.dump_response,
                    cli.unique_tor,
                    shutdown.clone(),
                    cli.print_all,
                    cli.verbose,
                    cli.browse,
                )
                .await?;

                write_reports(&cli, username, &results)?;
                let scan_id = db.save_scan(username, &results)?;
                info!("Scan #{scan_id} saved for '{username}'");

                let new_sites = database::find_new_results(&prev_results, &results);
                if new_sites.is_empty() {
                    println!("  [{}] No new findings.", username.green());
                } else {
                    println!("  [{}] {} new site(s) found!", username.green(), new_sites.len());
                    for r in &new_sites {
                        let status_color = match r.status {
                            types::QueryStatus::Claimed => "✓".green(),
                            types::QueryStatus::Available => "◻".yellow(),
                            types::QueryStatus::Unknown => "?".dimmed(),
                            types::QueryStatus::Illegal => "✗".red(),
                            types::QueryStatus::Waf => "⚠".red(),
                        };
                        println!("    {status_color} {} — {}", r.site_name.white().bold(), r.site_url_user.dimmed());
                    }
                }
                all_results.push(results);
            }
            print_performance_summary(&all_results, total_start.elapsed().as_millis() as u64);

            let next = schedule.upcoming(chrono::Utc).next();
            match next {
                Some(t) => {
                    let delay = t
                        .signed_duration_since(chrono::Utc::now())
                        .to_std()
                        .unwrap_or(std::time::Duration::from_secs(60));
                    let mins = delay.as_secs() / 60;
                    info!("Next scan at {t} ({mins} min from now)");
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {},
                        _ = tokio::signal::ctrl_c() => {
                            eprintln!("\nReceived Ctrl+C, exiting scheduler.");
                            break;
                        },
                    }
                }
                None => {
                    info!("No future schedule time, exiting.");
                    break;
                }
            }
        }
        return Ok(());
    }

    for username in &usernames {
        if shutdown.load(Ordering::Relaxed) {
            info!("Shutdown requested, skipping remaining usernames");
            break;
        }

        let results = engine::search_username(
            username,
            &sites,
            &http_client,
            concurrency,
            retry_count,
            rate_limiter.clone(),
            cli.dump_response,
            cli.unique_tor,
            shutdown.clone(),
            cli.print_all,
            cli.verbose,
            cli.browse,
        )
        .await?;

        // Handle --variants: generate and search variants, merge results
        let results = if cli.variants {
            let variants = generate_variants(username);
            if !variants.is_empty() {
                info!("Searching {} username variants", variants.len());
                let mut all_results = results;
                for variant in &variants {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    let var_results = engine::search_username(
                        variant,
                        &sites,
                        &http_client,
                        concurrency,
                        retry_count,
                        rate_limiter.clone(),
                        cli.dump_response,
                        cli.unique_tor,
                        shutdown.clone(),
                        cli.print_all,
                        cli.verbose,
                        cli.browse,
                    )
                    .await?;
                    // Merge variant results into main results
                    for r in var_results.results {
                        if !all_results.results.iter().any(|existing| existing.site_name == r.site_name && existing.username == r.username) {
                            all_results.results.push(r);
                        }
                    }
                    // Update counts
                    all_results.total_sites = all_results.results.len();
                    all_results.claimed_count = all_results.results.iter().filter(|r| r.status == QueryStatus::Claimed).count();
                    all_results.available_count = all_results.results.iter().filter(|r| r.status == QueryStatus::Available).count();
                    all_results.unknown_count = all_results.results.iter().filter(|r| r.status == QueryStatus::Unknown).count();
                    all_results.illegal_count = all_results.results.iter().filter(|r| r.status == QueryStatus::Illegal).count();
                    all_results.waf_count = all_results.results.iter().filter(|r| r.status == QueryStatus::Waf).count();
                }
                all_results
            } else {
                results
            }
        } else {
            results
        };

        write_reports(&cli, username, &results)?;

        // Intelligence pipeline
        let mut claimed_profiles: Vec<ClaimedProfile>;
        if cli.profile || cli.deep || cli.avatar_match || cli.graph || cli.report_html {
            // Collect claimed results into ClaimedProfile list
            let claimed_results: Vec<&types::QueryResult> = results.results.iter()
                .filter(|r| r.status == QueryStatus::Claimed)
                .collect();

            // --- Live: Scraping profiles ---
            if cli.profile && !claimed_results.is_empty() {
                eprint!("  {} Scraping profiles from {} sites…", "●".cyan(), claimed_results.len());
                std::io::Write::flush(&mut std::io::stderr()).ok();
            }

            let scrape_futures: Vec<_> = claimed_results.iter().map(|result| {
                let client = http_client.clone();
                let site = sites.iter().find(|s| s.name == result.site_name).cloned();
                let url = result.site_url_user.clone();
                let name = result.site_name.clone();
                let uname = username.to_string();
                async move {
                    let selectors = site.as_ref().and_then(|s| s.scrape.as_ref());
                    let scraped = if cli.profile {
                        scrape_profile(&client, &url, selectors).await.unwrap_or_default()
                    } else {
                        ProfileDetails::default()
                    };
                    ClaimedProfile {
                        site_name: name,
                        site_url: url,
                        username: uname,
                        details: scraped,
                        avatar_phash: None,
                    }
                }
            }).collect();
            claimed_profiles = futures::future::join_all(scrape_futures).await;

            if cli.profile && !claimed_results.is_empty() {
                eprintln!(" done");
            }

            // --- Live: API enrichment ---
            if cli.deep {
                eprint!("  {} Enriching via API…", "●".cyan());
                std::io::Write::flush(&mut std::io::stderr()).ok();

                let enrich_futures: Vec<_> = claimed_profiles.iter_mut().map(|p| {
                    let client = &http_client;
                    let uname = username.to_string();
                    let site_name = p.site_name.clone();
                    async move {
                        enrich_profile(client, &site_name, &uname, &mut p.details).await;
                    }
                }).collect();
                futures::future::join_all(enrich_futures).await;
                eprintln!(" done");
            }

            // --- Live: Domain check ---
            eprint!("  {} Checking domain registration…", "●".cyan());
            std::io::Write::flush(&mut std::io::stderr()).ok();
            let domain_info = find_domain(username, &http_client).await;
            if let Some(ref di) = domain_info {
                if di.resolves {
                    eprintln!(" found {}", di.domain.white().bold());
                } else {
                    eprintln!(" none");
                }
            } else {
                eprintln!(" none");
            }

            // Avatar matching
            if cli.avatar_match {
                find_avatar_matches(&http_client, &mut claimed_profiles).await;
            }

            // Build identity cluster and timeline
            let cluster = if !claimed_profiles.is_empty() {
                Some(build_identity_cluster(claimed_profiles.clone()))
            } else {
                None
            };

            let timeline = if !claimed_profiles.is_empty() {
                Some(build_timeline(&claimed_profiles))
            } else {
                None
            };

            // Print terminal investigative report
            if cli.verbose || cli.profile || cli.deep || cli.avatar_match {
                print_investigative_report(
                    cluster.as_ref(),
                    timeline.as_ref(),
                    &claimed_profiles,
                    username,
                );
            }

            // Print domain info in terminal
            if let Some(ref di) = domain_info {
                if di.resolves {
                    print_domain_report(di);
                }
            }

            // Graph output
            if cli.graph && !claimed_profiles.is_empty() {
                let graph = build_account_graph(&claimed_profiles);
                println!("{}", to_dot(&graph));
            }

            // HTML report
            if cli.report_html {
                let path = match cli.report_html_path.as_ref() {
                    Some(p) => PathBuf::from(p),
                    None => PathBuf::from(format!("{}_report.html", username)),
                };
                let mut html_reporter = HtmlReporter::new(path);
                html_reporter.set_profiles(claimed_profiles.clone());
                if let Some(ref cl) = cluster {
                    html_reporter.set_cluster(cl.clone());
                }
                if let Some(ref tl) = timeline {
                    html_reporter.set_timeline(tl.clone());
                }
                if cli.graph || claimed_profiles.len() >= 2 {
                    let g = build_account_graph(&claimed_profiles);
                    html_reporter.set_graph(g);
                }
                if let Some(ref di) = domain_info {
                    html_reporter.set_domain_info(di.clone());
                }
                html_reporter.write_search_start(username)?;
                for result in &results.results {
                    html_reporter.write_result(result)?;
                }
                html_reporter.write_search_complete(&results)?;
                html_reporter.finish()?;
            }
        }

        // Save to scan history database
        if let Ok(db) = ScanDb::open() {
            if let Ok(scan_id) = db.save_scan(username, &results) {
                debug!("Scan #{scan_id} saved to history for '{username}'");
            }
        }

        all_results.push(results);
    }

    let total_elapsed = total_start.elapsed().as_millis() as u64;

    let total_claimed: usize = all_results.iter().map(|r| r.claimed_count).sum();
    let total_searched: usize = all_results.iter().map(|r| r.total_sites).sum();

    print_performance_summary(&all_results, total_elapsed);

    info!(
        "Done. Searched {} users across {} sites. Total claimed: {total_claimed}",
        usernames.len(),
        total_searched
    );

    Ok(())
}

fn print_investigative_report(
    cluster: Option<&IdentityCluster>,
    timeline: Option<&timeline::ActivityTimeline>,
    profiles: &[ClaimedProfile],
    username: &str,
) {
    let sep = "═".repeat(62);
    println!("\n  {sep}");
    println!("  {}  ── {}", "OSINT REPORT".white().bold(), username.white().bold());
    println!("  {sep}");

    if let Some(cl) = cluster {
        let name = cl.inferred_name.as_deref().unwrap_or("Unknown");
        let loc = cl.inferred_location.as_deref().unwrap_or("Unknown");
        let emails = if cl.emails_found.is_empty() {
            "—".to_string()
        } else {
            cl.emails_found.join(", ")
        };
        let phones = if cl.phones_found.is_empty() {
            "—".to_string()
        } else {
            cl.phones_found.join(", ")
        };

        let confidence = cl.confidence as u32;
        let bar_len: usize = 20;
        let filled = ((confidence as f32 / 100.0) * bar_len as f32).round() as usize;
        let bar = format!("{}{}", "▓".repeat(filled).green(), "░".repeat(bar_len.saturating_sub(filled)));

        println!("\n  {}  {}", "IDENTITY".cyan().bold(), "─".repeat(50));
        println!("  ├─ {}  {}  (seen on {}/{} platforms)", "Inferred Name:".dimmed(), name.white().bold(), cl.accounts.len(), profiles.len().max(1));
        println!("  ├─ {}  {}", "Inferred Location:".dimmed(), loc);
        println!("  ├─ {}  {}", "Emails:".dimmed(), emails);
        println!("  ├─ {}  {}", "Phones:".dimmed(), phones);

        if let Some(ref tl) = timeline {
            if let Some(footprint) = tl.digital_footprint_years {
                let earliest = tl.earliest_account.as_ref()
                    .map(|e| format!("({})", e.date))
                    .unwrap_or_default();
                println!("  └─ {}  {} years {}", "Digital Footprint:".dimmed(), footprint, earliest);
            }
        }

        println!("\n  {}  {} / 100  [{}] {}",
            "CONFIDENCE SCORE:".cyan().bold(),
            confidence.to_string().white().bold(),
            bar,
            if confidence >= 80 { "HIGH".green().bold() }
            else if confidence >= 60 { "MEDIUM".yellow().bold() }
            else if confidence >= 40 { "LOW".yellow().bold() }
            else { "VERY LOW".red().bold() }
        );

        if !cl.shared_signals.is_empty() {
            println!("\n  {}  {}", "SIGNALS DETECTED".cyan().bold(), "─".repeat(46));
            for signal in &cl.shared_signals {
                match signal {
                    identity::Signal::SameName(name) => {
                        println!("  ├─ {} {}", "Same name:".dimmed(), name.white());
                    }
                    identity::Signal::SameAvatar { site_a, site_b, hash_distance } => {
                        println!("  ├─ {}  {} ↔ {}  (distance: {})", "Same avatar:".dimmed(), site_a.cyan(), site_b.cyan(), hash_distance);
                    }
                    identity::Signal::CrossLinked { from_site, to_url } => {
                        println!("  ├─ {}  {} → {}", "Cross-link:".dimmed(), from_site.cyan(), to_url.dimmed());
                    }
                    identity::Signal::SameLocation(loc) => {
                        println!("  ├─ {}  {}", "Same location:".dimmed(), loc.white());
                    }
                    identity::Signal::SameWebsite(site) => {
                        println!("  ├─ {}  {}", "Same website:".dimmed(), site.white());
                    }
                    identity::Signal::SameEmail(email) => {
                        println!("  ├─ {}  {}", "Same email:".dimmed(), email.white());
                    }
                    identity::Signal::SameBio { similarity } => {
                        println!("  ├─ {}  {:.0}%", "Bio similarity:".dimmed(), similarity * 100.0);
                    }
                }
            }
        }
    }

    if let Some(ref tl) = timeline {
        println!("\n  {}  {}", "TIMELINE".cyan().bold(), "─".repeat(52));
        for (year, platforms) in &tl.platforms_by_year {
            println!("  ├─ {}  {}", year.white().bold(), platforms.join(", "));
        }
        if let Some(footprint) = tl.digital_footprint_years {
            println!("  └─ {}  {} years", "Digital footprint:".dimmed(), footprint);
        }
    }

    if !profiles.is_empty() && cluster.is_none() {
        println!("\n  {}  {}", "PROFILES".cyan().bold(), "─".repeat(52));
        for p in profiles {
            let name = p.details.display_name.as_deref().unwrap_or("?");
            let bio = p.details.bio.as_deref().unwrap_or("");
            println!("  ├─ {}  {}  ({})", p.site_name.cyan(), name, p.site_url.dimmed());
            if !bio.is_empty() {
                println!("  │   Bio: {}", bio.dimmed());
            }
        }
    }

    println!("  {sep}\n");
}

fn print_domain_report(info: &DomainInfo) {
    let sep = "═".repeat(62);
    println!("\n  {sep}");
    println!("  {}  ── {}", "DOMAIN".cyan().bold(), info.domain.white().bold());
    println!("  {sep}");

    if !info.a_records.is_empty() {
        println!("  ├─ {}  {}", "A Records:".dimmed(), info.a_records.join(", "));
    }
    if !info.aaaa_records.is_empty() {
        println!("  ├─ {}  {}", "AAAA Records:".dimmed(), info.aaaa_records.join(", "));
    }
    if !info.mx_records.is_empty() {
        println!("  ├─ {}  {}", "MX Records:".dimmed(), info.mx_records.join(", "));
    }
    if !info.txt_records.is_empty() {
        println!("  ├─ {}  {}", "TXT Records:".dimmed(), info.txt_records.join(", "));
    }
    if !info.ns_records.is_empty() {
        println!("  ├─ {}  {}", "NS Records:".dimmed(), info.ns_records.join(", "));
    }
    if let Some(ref title) = info.homepage_title {
        println!("  ├─ {}  {}", "Homepage Title:".dimmed(), title.white());
    }
    if let Some(ref desc) = info.homepage_description {
        let truncated: String = desc.chars().take(120).collect();
        println!("  ├─ {}  {}", "Description:".dimmed(), truncated.dimmed());
    }
    if let Some(ref whois) = info.whois {
        println!("  ├─ {}", "WHOIS:".dimmed());
        for line in whois.lines() {
            println!("  │   {line}");
        }
    }
    if let Some(ref err) = info.error {
        println!("  └─ {}  {}", "Error:".dimmed(), err.red());
    }

    // Domain pages
    if !info.pages.pages.is_empty() {
        println!("  ├─ {}", "PAGES SCRAPED:".dimmed());
        for page in &info.pages.pages {
            let title = page.title.as_deref().unwrap_or("?");
            let emails = if page.emails.is_empty() { String::new() } else {
                format!("  \u{2514} emails: {}", page.emails.join(", "))
            };
            let social = if page.social_links.is_empty() { String::new() } else {
                let first_few: Vec<&str> = page.social_links.iter().take(3).map(|s| {
                    s.split(':').next().unwrap_or(s)
                }).collect();
                format!("  \u{2514} social: {}", first_few.join(", "))
            };
            println!("  \u{2502}   {}  {}", page.path.cyan(), title.white().bold());
            if !emails.is_empty() {
                println!("  \u{2502}   {emails}");
            }
            if !social.is_empty() {
                println!("  \u{2502}   {social}");
            }
        }
    }

    println!("  {sep}\n");
}

fn print_performance_summary(results: &[types::SearchResults], total_time_ms: u64) {
    if results.is_empty() {
        return;
    }

    let all_results: Vec<&types::QueryResult> = results.iter()
        .flat_map(|r| r.results.iter())
        .filter(|r| r.query_time_ms.is_some())
        .collect();

    if all_results.is_empty() {
        return;
    }

    let total_resp: u64 = all_results.iter().filter_map(|r| r.query_time_ms).sum();
    let avg = total_resp as f64 / all_results.len() as f64;

    let slowest = all_results.iter()
        .filter_map(|r| r.query_time_ms.map(|t| (t, r.site_name.clone())))
        .max_by_key(|(t, _)| *t);

    let fastest = all_results.iter()
        .filter_map(|r| r.query_time_ms.map(|t| (t, r.site_name.clone())))
        .min_by_key(|(t, _)| *t);

    println!("  {}", "─".repeat(60).dimmed());
    println!(
        "  {} {:>9}  {} {:>7.0}ms",
        "Total time:".dimmed(),
        if total_time_ms >= 1000 {
            format!("{:.1}s", total_time_ms as f64 / 1000.0)
        } else {
            format!("{total_time_ms}ms")
        }.white().bold(),
        "Avg response:".dimmed(),
        avg,
    );
    if let Some((t, ref name)) = fastest {
        println!(
            "  {} {:>7}ms ({})",
            "Fastest:".dimmed(),
            t.to_string().green(),
            name.green(),
        );
    }
    if let Some((t, ref name)) = slowest {
        println!(
            "  {} {:>7}ms ({})",
            "Slowest:".dimmed(),
            t.to_string().red(),
            name.red(),
        );
    }
    println!("  {}", "─".repeat(60).dimmed());
    println!();
}

fn resolve_usernames(cli: &Cli) -> Result<Vec<String>, RavenError> {
    let mut usernames = cli.usernames.clone();

    if let Some(ref path) = cli.usernames_file {
        let file = std::fs::File::open(path)
            .map_err(|e| RavenError::Cli(format!("Failed to open usernames file '{path}': {e}")))?;
        let reader = std::io::BufReader::new(file);
        for line in reader.lines() {
            let line = line.map_err(|e| {
                RavenError::Cli(format!("Failed to read usernames file: {e}"))
            })?;
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() {
                usernames.push(trimmed);
            }
        }
        if usernames.is_empty() {
            return Err(RavenError::Cli(
                "No usernames found in file or command line".to_string(),
            ));
        }
        info!(
            "Loaded {} usernames from file + {} from CLI",
            cli.usernames_file.as_ref().map_or(0, |_| {
                usernames.len() - cli.usernames.len()
            }),
            cli.usernames.len()
        );
    }

    if usernames.is_empty() {
        return Err(RavenError::Cli(
            "No usernames provided. Use --help for usage.".to_string(),
        ));
    }

    Ok(usernames)
}

fn resolve_proxy(cli: &Cli) -> Option<String> {
    if cli.unique_tor || cli.tor {
        let tor_proxy = Some("socks5://127.0.0.1:9050".to_string());
        if cli.proxy.is_some() && cli.proxy.as_deref() != Some("socks5://127.0.0.1:9050") {
            info!("--tor overrides proxy setting to socks5://127.0.0.1:9050");
        }
        return tor_proxy;
    }
    cli.proxy.clone()
}

fn generate_completions(shell_str: &str) -> Result<(), RavenError> {
    let shell = match shell_str.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        other => {
            return Err(RavenError::Cli(format!(
                "Unknown shell '{other}'. Supported: bash, zsh, fish, powershell, elvish"
            )));
        }
    };

    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

fn parse_cron(expr: &str) -> Result<cron::Schedule, RavenError> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    let full_expr = if parts.len() == 5 {
        format!("0 {expr}")
    } else if parts.len() == 6 {
        expr.to_string()
    } else {
        return Err(RavenError::Cli(
            "Cron expression must have 5 or 6 fields (e.g. \"0 */6 * * *\")".to_string(),
        ));
    };
    full_expr
        .parse::<cron::Schedule>()
        .map_err(|e| RavenError::Cli(format!("Invalid cron expression '{expr}': {e}")))
}

fn write_reports(cli: &Cli, username: &str, results: &types::SearchResults) -> Result<(), RavenError> {
    let mut reporters = Reporters::new();
    let result_path = resolve_output_path(cli, username);
    let has_export = cli.csv.is_some() || cli.xlsx.is_some() || cli.txt.is_some() || cli.json_report.is_some();

    if let Some(path_override) = &cli.csv {
        let path = if path_override.is_empty() {
            with_extension(&result_path, "csv")
        } else {
            PathBuf::from(path_override)
        };
        reporters.add(CsvReporter::new(path, cli.print_all));
    }

    if let Some(path_override) = &cli.xlsx {
        let path = if path_override.is_empty() {
            with_extension(&result_path, "xlsx")
        } else {
            PathBuf::from(path_override)
        };
        reporters.add(XlsxReporter::new(path, cli.print_all));
    }

    if let Some(path_override) = &cli.txt {
        let path = if path_override.is_empty() {
            with_extension(&result_path, "txt")
        } else {
            PathBuf::from(path_override)
        };
        reporters.add(TxtReporter::new(path));
    }

    if let Some(path_override) = &cli.json_report {
        let path = if path_override.is_empty() {
            with_extension(&result_path, "json")
        } else {
            PathBuf::from(path_override)
        };
        reporters.add(JsonReporter::new(path, cli.print_all));
    }

    if !has_export && (cli.output.is_some() || cli.folderoutput.is_some()) {
        reporters.add(TxtReporter::new(result_path));
    }

    reporters.write_search_start(username)?;

    for result in &results.results {
        reporters.write_result(result)?;
    }

    reporters.write_search_complete(results)?;
    reporters.finish()?;

    Ok(())
}

fn resolve_output_path(cli: &Cli, username: &str) -> PathBuf {
    if let Some(ref output) = cli.output {
        PathBuf::from(output)
    } else if let Some(ref folder) = cli.folderoutput {
        std::fs::create_dir_all(folder).ok();
        PathBuf::from(folder).join(username)
    } else {
        PathBuf::from(username)
    }
}

fn with_extension(path: &PathBuf, ext: &str) -> PathBuf {
    let mut p = path.clone();
    match p.extension() {
        Some(_) => {
            let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
            p.set_file_name(format!("{stem}.{ext}"));
        }
        None => {
            p.set_extension(ext);
        }
    }
    p
}
