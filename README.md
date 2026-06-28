# Raven — OSINT Username Search Engine

**Raven** is a high-performance Rust-based OSINT tool for hunting down social media accounts by username across **400+ social networks**. It searches, detects, and reports on username presence with async concurrency, automatic retries, rate limiting, multi-format export, and a real-time web UI.

```
██████╗  █████╗ ██╗   ██╗███████╗███╗   ██╗
██╔══██╗██╔══██╗██║   ██║██╔════╝████╗  ██║
██████╔╝███████║██║   ██║█████╗  ██╔██╗ ██║
██╔══██╗██╔══██║╚██╗ ██╔╝██╔══╝  ██║╚██╗██║
██║  ██║██║  ██║ ╚████╔╝ ███████╗██║ ╚████║
╚═╝  ╚═╝╚═╝  ╚═╝  ╚═══╝  ╚══════╝╚═╝  ╚═══╝

Modern Rust OSINT for Username Reconnaissance
```

## Raven vs Sherlock

| Feature | Raven | Sherlock |
|---|---|---|
| **Runtime** | Compiled Rust binary — no dependencies | Requires Python 3 + pip packages |
| **Performance** | Async `buffer_unordered` concurrency (default 200) with per-attempt 15s timeout; HTTP/2 enabled | Sequential-ish with thread pools; no per-attempt hard timeout |
| **Installation** | `cargo install raven` or single binary download | `pip install sherlock` (requires Python venv) |
| **Web UI** | Built-in — Axum + SSE real-time streaming + Chart.js + export | None (CLI only) |
| **Multi-user batch** | CLI + web UI (comma/newline separated textarea) | CLI only (space-separated args) |
| **Interactive charts** | Doughnut + bar chart auto-rendered after search | None |
| **Result filtering/sorting** | Status checkboxes, column sort, live text search in web UI | None |
| **Detail expansion** | Click-to-expand probe URL, HTTP status, response time, error context | None |
| **Export formats** | CSV, XLSX, JSON, TXT via CLI + web UI endpoints | CSV only |
| **WAF detection** | Cloudflare, PerimeterX, AWS WAF fingerprinting | Basic Cloudflare detection |
| **Manifest hosting** | Self-hosted via GitHub raw URL (your infra, not Sherlock's) | Relies on Sherlock's `data.sherlockproject.xyz` |
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
- 🌐 **Web UI** — real-time browser interface with SSE streaming, interactive charts, detail expansion, batch search, filtering, and sorting
- 📊 **Interactive charts** — doughnut chart for status distribution + horizontal bar chart for response times (Chart.js)
- 🔎 **Detail expansion** — click any result to reveal probe URL, HTTP status, response time, error context
- 👥 **Batch / multi-user search** — search multiple usernames simultaneously (comma or newline separated)
- 🔍 **Filtering + sorting** — filter by status with checkboxes, sort by name/response time/status, live text search
- ⚙️ **HTTP/2** — enabled by default for faster connections
- 🧹 **Clean output** — results stream directly to stdout without ANSI cursor noise
- 🏷️ **Probe URL tracking** — each result stores the exact URL that was probed for deeper analysis
- 📜 **Scan history** — every search is saved to a local SQLite database at `~/.local/share/raven/scans.db`
- 📋 **History browser** — `raven --history` lists past scans; `raven --history <username>` filters by user
- 🔄 **Cron mode** — `raven --schedule "0 */6 * * *" johndoe` runs periodic re-scans with change detection alerts ("NEW:" on new findings)
- 🔔 **Change detection** — cron mode compares each scan against the previous one and reports newly found sites

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
cargo install raven
```

### Package Managers

| Distribution | Command | Maintainer |
|---|---|---|
| **Fedora / RHEL** | `dnf install raven` | Community |
| **Debian / Ubuntu** | `apt install raven` | Community |
| **Kali Linux** | `apt install raven` | Community |
| **Homebrew (macOS/Linux)** | `brew install raven` | Community |
| **Arch Linux (AUR)** | `yay -S raven` | Community |

> Not yet packaged? Help us add your platform! See [Community Packaging](#community-packaging).

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

## License

MIT
