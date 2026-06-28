# Raven — OSINT Username Search Engine

**Raven** is a high-performance Rust-based OSINT tool for hunting down social media accounts by username across **400+ social networks**. It searches, detects, scrapes public profile data, correlates identities across platforms, and generates comprehensive investigative reports — all from publicly available information.

```
██████╗  █████╗ ██╗   ██╗███████╗███╗   ██╗
██╔══██╗██╔══██╗██║   ██║██╔════╝████╗  ██║
██████╔╝███████║██║   ██║█████╗  ██╔██╗ ██║
██╔══██╗██╔══██║╚██╗ ██╔╝██╔══╝  ██║╚██╗██║
██║  ██║██║  ██║ ╚████╔╝ ███████╗██║ ╚████║
╚═╝  ╚═╝╚═╝  ╚═╝  ╚═══╝  ╚══════╝╚═╝  ╚═══╝

Modern Rust OSINT for Username Reconnaissance

    Author: Waseem Akram (hackerwasii)
```

<p align="center">
  <a href="https://crates.io/crates/raven-osint"><img src="https://img.shields.io/crates/v/raven-osint.svg?style=flat-square&logo=rust" alt="Crates.io"></a>
  <a href="https://crates.io/crates/raven-osint"><img src="https://img.shields.io/crates/d/raven-osint.svg?style=flat-square" alt="Downloads"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MPL--2.0-blue.svg?style=flat-square" alt="License"></a>
  <a href="https://github.com/evildevill/Raven/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/evildevill/Raven/ci.yml?style=flat-square&logo=github" alt="CI"></a>
  <a href="https://rust-lang.org"><img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg?style=flat-square&logo=rust" alt="Rust"></a>
  <a href="https://hub.docker.com/r/hackerwasii/raven"><img src="https://img.shields.io/docker/pulls/hackerwasii/raven.svg?style=flat-square&logo=docker" alt="Docker Pulls"></a>
  <a href="https://www.patreon.com/hackerwasii"><img src="https://img.shields.io/badge/donate-patreon-F96854.svg?style=flat-square&logo=patreon" alt="Patreon"></a>
</p>

> **Legal Notice:** Raven only collects publicly available data. Use responsibly and in accordance with applicable laws and platform terms of service. The authors are not responsible for misuse.

## Table of Contents

- [Raven vs Sherlock](#raven-vs-sherlock)
- [Features](#features)
  - [Core](#core)
  - [Scan History & Automation](#scan-history--automation)
  - [Web UI](#web-ui)
  - [Intelligence Pipeline (v0.2.0)](#intelligence-pipeline-v020)
  - [Domain Intelligence (v0.2.0)](#domain-intelligence-v020)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Web UI](#web-ui-1)
- [CLI Reference](#cli-reference)
- [Configuration File](#configuration-file)
- [Output Formats](#output-formats)
- [Detection Methods](#detection-methods)
- [Performance Tips](#performance-tips)
- [Architecture](#architecture)
- [Community Packaging](#community-packaging)
- [License](#license)

## Raven vs Sherlock

| Feature | Raven | Sherlock |
|---|---|---|---|
| **Runtime** | Compiled Rust binary — no dependencies | Requires Python 3 + pip packages |
| **Performance** | Async `buffer_unordered` concurrency (default 200) with per-attempt 15s timeout; HTTP/2 enabled | Sequential-ish with thread pools; no per-attempt hard timeout |
| **Installation** | `cargo install raven-osint` or single binary download | `pip install sherlock` (requires Python venv) |
| **Web UI** | Built-in — Axum + SSE real-time streaming + Chart.js + export | None (CLI only) |
| **Multi-user batch** | CLI + web UI (comma/newline separated textarea) | CLI only (space-separated args) |
| **Interactive charts** | Doughnut + bar chart auto-rendered after search | None |
| **Result filtering/sorting** | Status checkboxes, column sort, live text search in web UI | None |
| **Detail expansion** | Click-to-expand probe URL, HTTP status, response time, error context | None |
| **Export formats** | CSV, XLSX, JSON, TXT via CLI + web UI endpoints | CSV only |
| **WAF detection** | Cloudflare, PerimeterX, AWS WAF fingerprinting | Basic Cloudflare detection |
| **Manifest hosting** | Self-hosted via GitHub raw URL (your infra, not Sherlock's) | Relies on Sherlock's `data.sherlockproject.xyz` |
| **Profile scraping** | OG tags + CSS selectors per site — extracts name, avatar, bio, location, emails, phones, URLs | None |
| **API enrichment** | GitHub, Reddit, HackerNews, Dev.to, Keybase APIs (overrides scraped data) | None |
| **Identity clustering** | Cross-references by shared name, bio similarity, avatar pHash, cross-links, emails, location with 0–100 confidence | None |
| **Avatar matching** | Perceptual hash (pHash) comparison across platforms | None |
| **Account graph** | Force-directed graph (DOT + D3.js JSON export) | None |
| **Timeline** | Digital footprint timeline from account creation dates | None |
| **Domain intelligence** | DNS (A/AAAA/MX/TXT/NS), WHOIS, homepage scrape, + /about /contact /resume /blog page scraping with email/phone/social extraction | None |
| **HTML report** | Self-contained offline HTML with statistics, identity, timeline, graph, domain intelligence, profile cards | None |
| **Scan history + cron** | SQLite persistent storage + cron-based re-scan with change detection | None |
| **HTTP client** | `reqwest` with `tcp_nodelay`, `pool_max_idle_per_host=100`, connect timeout | `requests` library (blocking) |
| **Per-attempt timeout** | 15s hard timeout per probe attempt (prevents hanging) | No per-attempt timeout — single request can hang indefinitely |
| **Config file** | `~/.config/raven/config.toml` (persistent defaults) | CLI flags only |
| **Retry logic** | Configurable retry count + per-attempt timeout before retry | Limited retry support |
| **Graceful shutdown** | Ctrl+C cancels in-flight requests cleanly | Abrupt termination on Ctrl+C |
| **Shell completions** | bash, zsh, fish, powershell, elvish | None |
| **Docker** | Multi-stage Dockerfile (18 MB slim image) | Community images only |
| **Site coverage** | 400+ sites (same Sherlock manifest format) | 400+ sites |
| **Detection methods** | Status code, error message, response URL + WAF | Status code, error message, response URL |

> Raven's site definitions and detection logic are built on top of [Sherlock](https://github.com/sherlock-project/sherlock)'s manifest format. Huge credit and thanks to the Sherlock team for maintaining the excellent site database that makes this tool possible.

## Features

## Core

- 🔍 **400+ sites** — searches across the largest collection of social networks (powered by Sherlock's manifest format)
- ⚡ **Async concurrency** — configurable parallel requests (default: 200)
- 🎯 **Smart detection** — 3 detection methods: status codes, error messages, response URLs
- 🛡️ **WAF detection** — identifies Cloudflare, PerimeterX, and AWS WAF blocks
- 🔄 **Auto-retry** — automatic retry on timeout/connect failures (default: 1 retry)
- ⏱ **Per-attempt timeout** — each probe attempt has a 15-second hard limit (prevents hanging)
- 🔁 **Rate limiting** — requests-per-second throttle for polite scanning
- 📋 **Multi-format export** — CSV, XLSX, JSON, TXT (plain URL list)
- 🔧 **Config file** — persistent defaults at `~/.config/raven/config.toml`
- 🏷️ **Tag filtering** — filter sites by category tags
- 🌐 **Proxy support** — HTTP and SOCKS5 proxies
- 🖥️ **Shell completions** — bash, zsh, fish, powershell, elvish
- ✋ **Graceful shutdown** — Ctrl+C cancels in-flight requests cleanly
- ⚙️ **HTTP/2** — enabled by default for faster connections
- 🧹 **Clean output** — results stream directly to stdout without ANSI cursor noise
- 🏷️ **Probe URL tracking** — each result stores the exact URL that was probed for deeper analysis

## Scan History & Automation

- 📜 **Scan history** — every search is saved to a local SQLite database at `~/.local/share/raven/scans.db`
- 📋 **History browser** — `raven --history` lists past scans; `raven --history <username>` filters by user
- 🔄 **Cron mode** — `raven --schedule "0 */6 * * *" johndoe` runs periodic re-scans with change detection alerts ("NEW:" on new findings)
- 🔔 **Change detection** — cron mode compares each scan against the previous one and reports newly found sites

## Web UI

- 🌐 **Real-time browser interface** — Axum + SSE streaming with interactive charts, detail expansion, batch search, filtering, and sorting
- 📊 **Interactive charts** — doughnut chart for status distribution + horizontal bar chart for response times (Chart.js)
- 🔎 **Detail expansion** — click any result to reveal probe URL, HTTP status, response time, error context
- 👥 **Batch / multi-user search** — search multiple usernames simultaneously (comma or newline separated)
- 🔍 **Filtering + sorting** — filter by status with checkboxes, sort by name/response time/status, live text search

## Intelligence Pipeline (v0.2.0)

- 🧠 **Profile scraping** (`--profile`) — extracts display name, avatar, bio, location, emails, phones, and URLs from claimed profile pages via OG tags and CSS selectors
- 🔬 **API enrichment** (`--deep`) — overrides scraped data with verified API responses from GitHub, Reddit, HackerNews, Dev.to, and Keybase
- 🆔 **Identity clustering** — cross-references profiles by shared name, bio similarity, avatar matching, cross-links, co-location, same website, and same email to build a unified identity with 0–100 confidence scoring
- 👤 **Avatar matching** (`--avatar-match`) — computes perceptual hashes (pHash) of profile avatars and detects identical or near-identical images across platforms
- 🔗 **Account link graph** (`--graph`) — builds a force-directed graph of interconnected accounts and exports in DOT or D3.js JSON format
- 📅 **Timeline reconstruction** — reconstructs a digital footprint timeline from account creation dates, showing platforms active per year
- 🔀 **Username variants** (`--variants`) — generates and searches separator, suffix, and leet-speak variants of the original username
- 📄 **HTML report** (`--report-html`) — single-file dark-themed self-contained HTML report with statistics, profile cards, identity summary, bio similarity matrix, timeline, domain intelligence, and interactive force-directed graph (no external dependencies, works offline)

## Domain Intelligence (v0.2.0)

- 🌐 **Domain detection** — automatically detects if the username matches a registered domain (e.g. `hackerwasii.com` → resolves) by checking 27 common TLDs
- 📡 **DNS reconnaissance** — resolves A, AAAA, MX, TXT, and NS records
- 📋 **WHOIS lookup** — extracts registrar, creation/expiry dates, nameservers, registrant info
- 🏠 **Homepage scraping** — fetches `<title>` and `<meta description>` from the domain
- 📄 **Page scraping** — concurrently fetches `/about`, `/contact`, `/resume`, `/cv`, `/blog`, `/projects`, `/portfolio`, `/about-me`, `/bio`, `/links` and extracts emails, phone numbers, social profile links, and text previews from each page

## Installation

### From Source

```bash
git clone https://github.com/evildevill/Raven.git
cd raven
cargo build --release
./target/release/raven --help
```

### Docker

```bash
# Pull from Docker Hub
docker pull hackerwasii/raven

# Search a single username
docker run --rm hackerwasii/raven johndoe

# Start the web UI
docker run --rm -p 8080:8080 hackerwasii/raven --serve --host 0.0.0.0

# Build locally
docker build -t raven .
```

### Docker Compose

```yaml
services:
  raven:
    image: hackerwasii/raven:latest
    command: ["--serve", "--host", "0.0.0.0"]
    ports:
      - "8080:8080"
    volumes:
      - raven_data:/root/.local/share/raven
    restart: unless-stopped

volumes:
  raven_data:
```

### Pre-built Binary

Download the latest release for your platform from the [releases page](https://github.com/evildevill/Raven/releases).

### Cargo (from crates.io)

```bash
cargo install raven-osint
```

> Installs the `raven` binary. v0.2.0 adds the full intelligence pipeline — profile scraping, API enrichment, identity clustering, graph analysis, domain reconnaissance, HTML reports, and more.

<!-- ### Package Managers

| Distribution | Command | Maintainer |
|---|---|---|
| **Fedora / RHEL** | `dnf install raven` | Community |
| **Debian / Ubuntu** | `apt install raven` | Community |
| **Kali Linux** | `apt install raven` | Community |
| **Homebrew (macOS/Linux)** | `brew install raven` | Community |
| **Arch Linux (AUR)** | `yay -S raven` | Community |

> Not yet packaged? Help us add your platform! See [Community Packaging](#community-packaging). -->

## Quick Start

```bash
# Search a single username across all sites
raven johndoe

# Search multiple usernames
raven johndoe janedoe

# Start the web UI
raven --serve

# Web UI on a custom port
raven --serve --port 3000 --host 0.0.0.0

# Search specific sites only
raven --site GitHub --site GitLab johndoe

# Export results as CSV and JSON
raven --csv --json-report johndoe

# Export with custom file paths
raven --csv results.csv --xlsx results.xlsx johndoe

# Full scan with rate limiting and retries
raven --retry 2 --rate-limit 30 --csv johndoe

# Load usernames from a file
raven --usernames-file targets.txt

# Use a SOCKS5 proxy
raven --proxy socks5://127.0.0.1:9050 johndoe

# Generate shell completions
raven --completions bash > /etc/bash_completion.d/raven

# Intelligence pipeline: scrape profiles, enrich via API, generate HTML report
raven --profile --deep --report-html johndoe

# Username variants scanning
raven --variants johndoe

# Avatar matching across platforms
raven --profile --avatar-match johndoe

# Full OSINT: profile scraping + API enrichment + avatar match + graph + HTML report
raven --profile --deep --avatar-match --graph --report-html johndoe

# Generate email report (no terminal noise)
raven --profile --deep --report-html johndoe
```

## Web UI

Raven includes an optional web interface powered by **Axum** + **HTMX** + **Tailwind CSS** with real-time result streaming via **Server-Sent Events** (SSE).

### Starting the Web UI

```bash
# Default: http://127.0.0.1:8080
raven --serve

# Custom host and port
raven --serve --host 0.0.0.0 --port 3000
```

### How It Works

1. Open `http://127.0.0.1:8080` in your browser
2. Enter a username (or multiple, comma/newline-separated) in the search bar
3. Expand **Advanced options** to configure concurrency, timeout, retries, rate limiting, NSFW toggle, Tor routing, and exclusion preferences
4. Click **Search** — results appear in real-time as probes complete
5. **Click any row** to expand and see full probe details (probe URL, HTTP status, response time, error context)
6. **Charts** auto-render after search completes:
   - Doughnut chart: claimed vs available vs unknown vs WAF vs illegal
   - Horizontal bar chart: top 20 slowest sites with response times
7. **Filter results** by status using checkboxes, sort by site name/response time/HTTP status, or type to search
8. **Export** results as CSV, JSON, or plain URL list — all respecting the "Claimed only" filter

### Multi-User Search

Enter multiple usernames separated by commas or newlines:

```
johndoe, janedoe
bobsmith
alice
```

Results from all users appear in the same stream, tagged by username. Charts aggregate across all searched users.

### API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | HTML dashboard |
| `POST` | `/search` | Launch a search (JSON body) |
| `GET` | `/stream/:id` | SSE stream of search results |
| `GET` | `/export/:id/:format` | Download results as csv/json/txt |
| `GET` | `/results/:id` | Full results as JSON (polling) |

#### POST /search

```json
{
  "usernames": "johndoe",
  "concurrency": 200,
  "timeout": 20,
  "retry": 1,
  "nsfw": false,
  "tor": false,
  "unique_tor": false,
  "rate_limit": null,
  "proxy": null,
  "ignore_exclusions": false
}
```

For batch search, separate usernames with commas or newlines:
```json
{
  "usernames": "johndoe\njanedoe\nbobsmith",
  "concurrency": 200,
  "timeout": 20,
  "retry": 1,
  "nsfw": false
}
```

Response:
```json
{ "session_id": "550e8400-e29b-41d4-a716-446655440000" }
```

#### SSE Events

| Event | Data |
|-------|------|
| `result` | `{ "site_name": "GitHub", "site_url_user": "...", "status": "Claimed", ... }` |
| `progress` | `{ "completed": 50, "total": 400 }` |
| `complete` | `{ "total": 400, "claimed": 12, "available": 3, "unknown": 380, "illegal": 2, "waf": 3 }` |
| `error` | plain text error message |

## CLI Reference

### Basic Options

| Flag | Description |
|------|-------------|
| `<USERNAMES>` | Username(s) to search for (one or more) |
| `-v, --verbose` | Display detailed metrics and debug output |
| `-o, --output <PATH>` | Save result to a specific file (auto-names by username) |
| `-F, --folderoutput <DIR>` | Save results for multiple users to a directory |

### Site Filtering

| Flag | Description |
|------|-------------|
| `-s, --site <NAME>` | Limit to specific site(s). Repeatable: `-s GitHub -s GitLab` |
| `--tag <TAG>` | Filter by site tags. Repeatable: `--tag social --tag video` |
| `--nsfw` | Include NSFW sites in the search |
| `--ignore-exclusions` | Skip upstream false-positive exclusion list |

### Network Options

| Flag | Default | Description |
|------|---------|-------------|
| `-p, --proxy <URL>` | — | HTTP or SOCKS5 proxy (e.g. `socks5://127.0.0.1:9050`) |
| `--timeout <SECS>` | `20` | Per-request timeout in seconds |
| `--concurrency <N>` | `200` | Max concurrent HTTP requests (1–10000) |
| `--rate-limit <N>` | — | Max requests per second |
| `--retry <N>` | `1` | Retry count on timeout/connect failures |

### Output Control

| Flag | Description |
|------|-------------|
| `--print-all` | Display all sites (found and not found) |
| `--print-found` | Display only found sites (default) |
| `--no-color` | Disable ANSI colored output |
| `-b, --browse` | Open found profile URLs in default browser |

### Export Formats

| Flag | Description |
|------|-------------|
| `--csv [PATH]` | Export as CSV. Omitting PATH auto-names as `<username>.csv` |
| `--xlsx [PATH]` | Export as Excel XLSX. Omitting PATH auto-names |
| `--json-report [PATH]` | Export full results as JSON. Omitting PATH auto-names |
| `--txt [PATH]` | Export found URLs as plain text. Omitting PATH auto-names |

### Utilities

| Flag | Description |
|------|-------------|
| `--update-manifest` | Download the latest site list and exit |
| `--local` | Force use of the bundled manifest (skip remote) |
| `--completions <SHELL>` | Generate shell completions (bash, zsh, fish, powershell, elvish) |
| `--usernames-file <PATH>` | Read usernames from file (one per line) |

### Web Server

| Flag | Default | Description |
|------|---------|-------------|
| `--serve` | — | Start the web UI server |
| `--port <PORT>` | `8080` | Port for the web UI server |
| `--host <HOST>` | `127.0.0.1` | Host for the web UI server |

### Intelligence Options (v0.2.0)

| Flag | Description |
|------|-------------|
| `--profile` | Scrape display name, avatar, bio, location, emails, phones, URLs from claimed profile pages |
| `--deep` | Enrich profiles via GitHub, Reddit, HackerNews, Dev.to, Keybase APIs (overrides scraped data) |
| `--avatar-match` | Compare profile avatars across platforms using perceptual hashing |
| `--variants` | Generate and search username variants (separators, suffixes, leet speak) |
| `--graph` | Build and print an account link graph (DOT format) |
| `--report-html` | Generate a self-contained HTML report with charts, timeline, graph, identity summary, and domain intelligence |
| `--report-html-path <PATH>` | Specify output path for the HTML report (default: `<username>_report.html`) |

### Advanced Options

| Flag | Description |
|------|-------------|
| `--tor, -t` | Route all requests through Tor (default SOCKS port 9050) |
| `--unique-tor, -u` | Route through Tor with a new circuit per request (requires ControlPort 9051) |
| `--dump-response` | Print full HTTP response body and metadata to stdout for debugging |
| `--json <PATH>, -j <PATH>` | Load site manifest from a JSON file, URL, or GitHub PR number |
| `--schedule <CRON>` | Run on a cron schedule with change detection (e.g. `"0 */6 * * *"`) |
| `--history [<USERNAME>]` | Browse past scan history. Optionally filter by username. |
| `{?}` | Wildcard in usernames — expands to `_`, `-`, `.` variants (e.g. `john{?}doe`) |
| `-h, --help` | Print help information |
| `-V, --version` | Print version |

## Configuration File

Raven loads configuration from `~/.config/raven/config.toml`. CLI flags override config file values.

### Example Config

```toml
# ~/.config/raven/config.toml
timeout = 30
concurrency = 50
retry = 2
rate_limit = 30.0
proxy = "socks5://127.0.0.1:9050"
no_color = false
nsfw = false
ignore_exclusions = false
```

| Key | Type | Description |
|-----|------|-------------|
| `timeout` | float | Request timeout in seconds |
| `concurrency` | int | Max concurrent requests |
| `retry` | int | Number of retries on failure |
| `rate_limit` | float | Requests per second |
| `proxy` | string | Proxy URL |
| `no_color` | bool | Disable colors |
| `nsfw` | bool | Include NSFW sites |
| `ignore_exclusions` | bool | Skip exclusion list |
| `tor` | bool | Route through Tor |
| `unique_tor` | bool | New Tor circuit per request |
| `dump_response` | bool | Dump HTTP responses |
| `profile` | bool | Enable profile scraping by default |
| `deep` | bool | Enable API enrichment by default |
| `avatar_match` | bool | Enable avatar matching by default |

## Output Formats

### Terminal

```
  ◆ johndoe
  ────────────────────────────────────────────────────────────
 +  GitHub  https://www.github.com/johndoe
 -  GitLab    Not Found

  ────────────────────────────────────────────────────────────
  Found  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 100%
  Breakdown: 1 found, 1 available
  Total:  2 sites checked

  ────────────────────────────────────────────────────────────
  Total time:     15.0s  Avg response:    1936ms
  Fastest:     550ms (GitLab)
  Slowest:    3322ms (GitHub)
  ────────────────────────────────────────────────────────────
```

### CSV

```csv
username,site_name,url_user,status,http_status,response_time_ms
johndoe,GitHub,https://github.com/johndoe,Claimed,200,1234
johndoe,GitLab,https://gitlab.com/johndoe,Claimed,200,567
```

### JSON

```json
{
  "results": [
    {
      "username": "johndoe",
      "timestamp": "2026-06-27T12:00:00Z",
      "total_sites": 2,
      "claimed_count": 2,
      "results": [
        {
          "username": "johndoe",
          "site_name": "GitHub",
          "site_url_user": "https://github.com/johndoe",
          "status": "Claimed",
          "query_time_ms": 1234,
          "http_status": 200
        }
      ]
    }
  ]
}
```

## Detection Methods

Raven uses three detection strategies, configurable per site in the manifest:

1. **Status Code** — HTTP 4xx/5xx responses indicate the username is available; 2xx/3xx indicate claimed
2. **Error Message** — Scans response body for site-specific "not found" text
3. **Response URL** — Redirects to a specific error URL indicate availability

Combined with WAF fingerprinting to detect Cloudflare/PerimeterX/AWS WAF blocks.

Each probe attempt has a **15-second hard timeout** — if a site doesn't respond within that window, the attempt is cancelled and retried (if retries are configured). This prevents a single unresponsive site from blocking the entire scan.

## Performance Tips

- **Increase concurrency** for faster scans: `--concurrency 200` (default is already 200)
- **Add rate limiting** to avoid IP blocks: `--rate-limit 30`
- **Use a proxy** for blocked sites: `--proxy socks5://127.0.0.1:9050`
- **Use Tor** to rotate IPs: `--tor` or `--unique-tor`
- **Filter specific sites** for targeted searches: `--site GitHub --site GitLab`
- **Use retries** for unreliable networks: `--retry 2` (default is 1)
- **Update the manifest** regularly: `--update-manifest`
- **Use the web UI** for interactive exploration: `raven --serve`

## Architecture

```
                    ┌─────────────────────┐
                    │   CLI / Web UI      │
                    │  (clap / Axum +     │
                    │   HTMX + SSE)       │
                    └──────┬──────────────┘
                           │
                    ┌──────▼──────────────┐
                    │      Config         │
                    │  (~/.config/raven/  │
                    │   config.toml)      │
                    └──────┬──────────────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
     ┌────────▼───┐ ┌─────▼─────┐ ┌────▼────────┐
     │  Manifest  │ │  Filter   │ │   Client    │
     │ (data.json)│ │ (sites,   │ │ (HTTP/2,    │
     │ remote or  │ │  tags,    │ │  tcp_nodelay,│
     │  local)    │ │  NSFW,    │ │  proxy, Tor)│
     └────────────┘ │exclusions)│ └────┬────────┘
                    └───────────┘      │
                                       │
                              ┌────────▼────────┐
                              │ Search Engine   │
                              │ (buffer_unordered│
                              │  N concurrency, │
                              │  retry × N,     │
                              │  15s per-attempt│
                              │  timeout, WAF   │
                              │  detection)     │
                              └────────┬────────┘
                                       │
                    ┌──────────────────┼──────────────┐
                    │                  │              │
          ┌─────────▼────┐   ┌────────▼──────┐ ┌─────▼──────┐
          │   Reporters  │   │   Stdout      │ │  Web UI    │
          │ (CSV/XLSX/   │   │  (clean, no   │ │ (SSE       │
          │  JSON/TXT)   │   │  ANSI noise)  │ │  stream)   │
          └──────────────┘   └───────────────┘ └────────────┘
```

## Community Packaging

Raven is available for multiple platforms. Package maintainers, please update this table:

| Platform | Repository | Install Command | Status |
|---|---|---|---|
| **Fedora / RHEL** | Fedora Rawhide | `dnf install raven` | ⏳ Planned |
| **Debian / Ubuntu** | Debian Sid | `apt install raven` | ⏳ Planned |
| **Kali Linux** | Kali Rolling | `apt install raven` | ⏳ Planned |
| **Homebrew** | Homebrew Core | `brew install raven` | ⏳ Planned |
| **Arch Linux** | AUR | `yay -S raven` | ⏳ Planned |
| **Docker** | [Docker Hub](https://hub.docker.com/r/hackerwasii/raven) | `docker pull hackerwasii/raven` | ✅ Published |
| **Apify** | Apify Store | — | ✅ Configured |

### Adding a New Platform

1. Fork the repository
2. Add packaging scripts to `packaging/<platform>/`
3. Update this table in a PR
4. Or open a [feature request](https://github.com/evildevill/Raven/issues/new?template=feature-request.yml)

### Apify Actor

Raven includes Apify Actor configuration in `.actor/`. To publish:

```bash
cd .actor
apify push
```

See the [Apify Actor README](./.actor/README.md) for details.


## Author

**Waseem Akram** — [@hackerwasii](https://linkedin.com/in/hackerwasii) — [GitHub](https://github.com/evildevill) — [Website](https://hackerwasii.com)

## Funding

Support Raven development on Patreon:

[![Patreon](https://img.shields.io/badge/Patreon-Support-F96854?style=for-the-badge&logo=patreon)](https://www.patreon.com/hackerwasii)

## License

Raven is licensed under the **MPL-2.0**. See [LICENSE](./LICENSE) for details.