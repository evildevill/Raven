use clap::Parser;
use tracing::debug;

use crate::config::Config;

#[derive(Parser, Debug)]
#[command(
    name = "raven",
    version,
    about = "Hunt down social media accounts by username across social networks",
    long_about = "Raven is a Rust-based OSINT tool that searches for usernames across 400+ social networks.\n\n\
                   Examples:\n  raven johndoe\n  raven johndoe janedoe\n  raven --csv --xlsx johndoe\n  \
                   raven --site GitHub --site GitLab johndoe\n  raven --proxy socks5://127.0.0.1:9050 johndoe\n  \
                   raven --serve\n  raven --serve --port 3000\n  \
                   raven --schedule \"0 */6 * * *\" johndoe\n  raven --history",
    after_help = "Use --update-manifest to download the latest site list from the remote manifest server.\n\
                  Configuration file: ~/.config/raven/config.toml\n\
                  Scan history database: ~/.local/share/raven/scans.db\n\
                  Use --completions <shell> to generate shell completion scripts.\n\
                  Use --serve to start the web UI server.\n\
                  Use --schedule to run periodic scans with change detection alerts.\n\
                  Use --history to browse past scan records."
)]
pub struct Cli {
    #[arg(
        required_unless_present_any = ["update_manifest", "completions", "usernames_file", "serve", "history", "schedule"],
        help = "Username(s) to search for across social networks",
        num_args = 1..,
    )]
    pub usernames: Vec<String>,

    #[arg(
        short = 'v',
        long = "verbose",
        alias = "debug",
        help = "Display extra debugging information and metrics",
        global = true
    )]
    pub verbose: bool,

    #[arg(
        short = 'o',
        long = "output",
        help = "Save result for single username to this file (overrides auto-naming)"
    )]
    pub output: Option<String>,

    #[arg(
        short = 'F',
        long = "folderoutput",
        help = "If using multiple usernames, save results to this folder"
    )]
    pub folderoutput: Option<String>,

    #[arg(
        long = "csv",
        num_args = 0..=1,
        default_missing_value = "",
        help = "Export results as CSV file. Optionally specify path (e.g. --csv results.csv)"
    )]
    pub csv: Option<String>,

    #[arg(
        long = "xlsx",
        num_args = 0..=1,
        default_missing_value = "",
        help = "Export results as Excel (.xlsx) file. Optionally specify path (e.g. --xlsx results.xlsx)"
    )]
    pub xlsx: Option<String>,

    #[arg(
        long = "txt",
        num_args = 0..=1,
        default_missing_value = "",
        help = "Export found URLs as TXT file. Optionally specify path (e.g. --txt results.txt)"
    )]
    pub txt: Option<String>,

    #[arg(
        long = "json-report",
        num_args = 0..=1,
        default_missing_value = "",
        help = "Export full results as JSON file. Optionally specify path (e.g. --json-report results.json)"
    )]
    pub json_report: Option<String>,

    #[arg(
        short = 's',
        long = "site",
        help = "Limit analysis to specific site(s). Repeatable."
    )]
    pub site_list: Vec<String>,

    #[arg(
        long = "tag",
        help = "Filter sites by tag(s). Repeatable (e.g. --tag social --tag video)"
    )]
    pub tag: Vec<String>,

    #[arg(
        short = 'p',
        long = "proxy",
        help = "Make requests over a proxy. e.g. socks5://127.0.0.1:1080 or http://proxy:8080"
    )]
    pub proxy: Option<String>,

    #[arg(
        long = "timeout",
        default_value = "20",
        help = "Time in seconds to wait for each request"
    )]
    pub timeout: f64,

    #[arg(
        long = "rate-limit",
        help = "Maximum number of HTTP requests per second (e.g. --rate-limit 30)"
    )]
    pub rate_limit: Option<f64>,

    #[arg(
        long = "retry",
        default_value = "1",
        help = "Number of automatic retries on timeout/connect failures"
    )]
    pub retry: usize,

    #[arg(
        long = "usernames-file",
        help = "Read usernames from a file (one per line)"
    )]
    pub usernames_file: Option<String>,

    #[arg(
        long = "print-all",
        help = "Output all sites, including those where username was not found"
    )]
    pub print_all: bool,

    #[arg(
        long = "print-found",
        help = "Output only sites where username was found (default)",
        conflicts_with = "print_all"
    )]
    pub print_found: bool,

    #[arg(long = "no-color", help = "Disable colored terminal output")]
    pub no_color: bool,

    #[arg(long = "nsfw", help = "Include NSFW sites in search")]
    pub nsfw: bool,

    #[arg(
        short = 'l',
        long = "local",
        help = "Force use of local manifest file instead of remote"
    )]
    pub local: bool,

    #[arg(
        short = 'b',
        long = "browse",
        help = "Open found profile URLs in default web browser"
    )]
    pub browse: bool,

    #[arg(
        long = "ignore-exclusions",
        help = "Skip upstream false-positive exclusions (may increase false positives)"
    )]
    pub ignore_exclusions: bool,

    #[arg(
        long = "concurrency",
        default_value = "200",
        help = "Maximum number of concurrent HTTP requests"
    )]
    pub concurrency: usize,

    #[arg(
        long = "completions",
        help = "Generate shell completion script. Values: bash, zsh, fish, powershell, elvish"
    )]
    pub completions: Option<String>,

    #[arg(
        long = "update-manifest",
        help = "Download the latest site manifest and exit"
    )]
    pub update_manifest: bool,

    #[arg(
        short = 't',
        long = "tor",
        help = "Route all requests through Tor (requires Tor running on localhost:9050)"
    )]
    pub tor: bool,

    #[arg(
        short = 'u',
        long = "unique-tor",
        help = "Route through Tor with a new circuit per request (requires Tor with ControlPort on 9051)"
    )]
    pub unique_tor: bool,

    #[arg(
        long = "dump-response",
        help = "Dump full HTTP response body and metadata to stdout for debugging"
    )]
    pub dump_response: bool,

    #[arg(
        short = 'j',
        long = "json",
        help = "Load site manifest from a JSON file or URL. Also accepts GitHub PR numbers."
    )]
    pub json: Option<String>,

    #[arg(
        long = "serve",
        help = "Start web UI server instead of CLI search"
    )]
    pub serve: bool,

    #[arg(
        long = "port",
        default_value = "8080",
        help = "Port for web UI server"
    )]
    pub port: u16,

    #[arg(
        long = "host",
        default_value = "127.0.0.1",
        help = "Host for web UI server"
    )]
    pub host: String,

    #[arg(
        long = "schedule",
        help = "Run on a cron schedule (e.g. \"0 */6 * * *\") with change detection. Re-scans periodically and reports new findings."
    )]
    pub schedule: Option<String>,

    #[arg(
        long = "history",
        num_args = 0..=1,
        default_missing_value = "",
        help = "Show past scan history. Optionally filter by username: --history <username>"
    )]
    pub history: Option<String>,

    #[arg(
        long = "profile",
        help = "Enable profile scraping on claimed accounts (OG tags + CSS selectors)"
    )]
    pub profile: bool,

    #[arg(
        long = "deep",
        help = "Enable API enrichment (GitHub, Reddit, HackerNews, Dev.to, Keybase)"
    )]
    pub deep: bool,

    #[arg(
        long = "avatar-match",
        help = "Download avatars and compare via perceptual hashing (pHash)"
    )]
    pub avatar_match: bool,

    #[arg(
        long = "variants",
        help = "Auto-scan username variations (separators, suffixes, leet speak, etc.)"
    )]
    pub variants: bool,

    #[arg(
        long = "graph",
        help = "Build and export account link graph (DOT format to stdout)"
    )]
    pub graph: bool,

    #[arg(
        long = "report-html",
        help = "Generate full HTML investigative report (auto-named based on username)"
    )]
    pub report_html: bool,

    #[arg(
        long = "report-html-path",
        help = "Path for the HTML report (requires --report-html)"
    )]
    pub report_html_path: Option<String>,
}

impl Cli {
    pub fn new_with_config(config: Config) -> Self {
        let mut cli = Cli::parse();

        if cli.proxy.is_none() {
            cli.proxy = config.proxy;
        }
        if let Some(t) = config.timeout {
            if cli.timeout == 20.0 {
                cli.timeout = t;
            }
        }
        if let Some(c) = config.concurrency {
            if cli.concurrency == 200 {
                cli.concurrency = c;
            }
        }
        if cli.rate_limit.is_none() {
            cli.rate_limit = config.rate_limit;
        }
        if cli.retry == 1 {
            if let Some(r) = config.retry {
                cli.retry = r;
            }
        }
        if !cli.nsfw {
            cli.nsfw = config.nsfw.unwrap_or(false);
        }
        if !cli.no_color {
            cli.no_color = config.no_color.unwrap_or(false);
        }
        if !cli.ignore_exclusions {
            cli.ignore_exclusions = config.ignore_exclusions.unwrap_or(false);
        }
        if !cli.tor {
            cli.tor = config.tor.unwrap_or(false);
        }
        if !cli.unique_tor {
            cli.unique_tor = config.unique_tor.unwrap_or(false);
        }
        if !cli.dump_response {
            cli.dump_response = config.dump_response.unwrap_or(false);
        }
        if !cli.profile {
            cli.profile = config.profile.unwrap_or(false);
        }
        if !cli.deep {
            cli.deep = config.deep.unwrap_or(false);
        }
        if !cli.avatar_match {
            cli.avatar_match = config.avatar_match.unwrap_or(false);
        }

        debug!("CLI after config merge: {cli:#?}");
        cli
    }

    pub fn effective_concurrency(&self) -> usize {
        self.concurrency.max(1).min(10000)
    }

    #[allow(dead_code)]
    pub fn has_output_format(&self) -> bool {
        self.csv.is_some() || self.xlsx.is_some() || self.txt.is_some() || self.json_report.is_some()
    }
}
