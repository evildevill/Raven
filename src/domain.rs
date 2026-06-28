use std::net::ToSocketAddrs;
use std::time::Duration;

const COMMON_TLDS: &[&str] = &[
    "com", "dev", "io", "net", "org", "me", "app", "co", "xyz", "online",
    "tech", "site", "fun", "live", "pro", "name", "cc", "info", "in", "us",
    "uk", "ca", "de", "fr", "eu", "jp", "ru", "br", "au",
];

#[derive(Debug, Clone, Default)]
pub struct DomainInfo {
    pub domain: String,
    pub resolves: bool,
    pub a_records: Vec<String>,
    pub aaaa_records: Vec<String>,
    pub mx_records: Vec<String>,
    pub txt_records: Vec<String>,
    pub ns_records: Vec<String>,
    pub whois: Option<String>,
    pub homepage_title: Option<String>,
    pub homepage_description: Option<String>,
    pub error: Option<String>,
    pub pages: DomainPages,
}

pub async fn find_domain(username: &str, http_client: &reqwest::Client) -> Option<DomainInfo> {
    for tld in COMMON_TLDS {
        let domain = format!("{}.{}", username, tld);
        let domain_clone = domain.clone();
        let resolves = tokio::task::spawn_blocking(move || quick_resolve(&domain_clone))
            .await
            .unwrap_or(false);
        if resolves {
            let mut info = DomainInfo {
                domain: domain.clone(),
                resolves: true,
                ..Default::default()
            };
            resolve_dns_records(&domain, &mut info).await;
            info.whois = query_whois(&domain).await;
            scrape_homepage(&domain, http_client, &mut info).await;
            info.pages = scrape_domain_pages(&domain, http_client).await;
            return Some(info);
        }
    }
    None
}

fn quick_resolve(domain: &str) -> bool {
    format!("{}:80", domain)
        .to_socket_addrs()
        .ok()
        .map(|mut iter| iter.next().is_some())
        .unwrap_or(false)
}

async fn resolve_dns_records(domain: &str, info: &mut DomainInfo) {
    let domain_owned = domain.to_string();
    let a_records: Vec<String> = tokio::task::spawn_blocking(move || {
        let mut recs = Vec::new();
        if let Ok(addrs) = format!("{}:80", domain_owned).to_socket_addrs() {
            for addr in addrs {
                let ip = addr.ip().to_string();
                if !recs.contains(&ip) {
                    recs.push(ip);
                }
            }
        }
        recs
    }).await.unwrap_or_default();
    for ip in a_records {
        if ip.contains(':') {
            if !info.aaaa_records.contains(&ip) {
                info.aaaa_records.push(ip);
            }
        } else {
            if !info.a_records.contains(&ip) {
                info.a_records.push(ip);
            }
        }
    }

    // MX, TXT, NS via system dig
    let domain_mx = domain.to_string();
    if let Ok(mx) = tokio::task::spawn_blocking(move || {
        let mut recs = Vec::new();
        if let Ok(output) = std::process::Command::new("dig").args(["+short", "MX", &domain_mx]).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if !line.is_empty() { recs.push(line.to_string()); }
                }
            }
        }
        recs
    }).await {
        info.mx_records = mx;
    }

    let domain_txt = domain.to_string();
    if let Ok(txt) = tokio::task::spawn_blocking(move || {
        let mut recs = Vec::new();
        if let Ok(output) = std::process::Command::new("dig").args(["+short", "TXT", &domain_txt]).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim().trim_matches('"');
                    if !line.is_empty() { recs.push(line.to_string()); }
                }
            }
        }
        recs
    }).await {
        info.txt_records = txt;
    }

    let domain_ns = domain.to_string();
    if let Ok(ns) = tokio::task::spawn_blocking(move || {
        let mut recs = Vec::new();
        if let Ok(output) = std::process::Command::new("dig").args(["+short", "NS", &domain_ns]).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim().trim_end_matches('.');
                    if !line.is_empty() { recs.push(line.to_string()); }
                }
            }
        }
        recs
    }).await {
        info.ns_records = ns;
    }
}

async fn query_whois(domain: &str) -> Option<String> {
    let domain_owned = domain.to_string();
    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("whois").arg(&domain_owned).output().ok()?;
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            Some(summarize_whois(&text))
        } else {
            None
        }
    }).await.ok().flatten()
}

fn summarize_whois(raw: &str) -> String {
    let interesting_keys = [
        "Domain Name:", "Registry Domain ID:", "Registrar:", "Registrar URL:",
        "Creation Date:", "Registry Expiry Date:", "Updated Date:",
        "Registrant Name:", "Registrant Organization:", "Registrant Email:",
        "Admin Email:", "Tech Email:", "Name Server:", "DNSSEC:",
        "Status:", "Registrar IANA ID:",
    ];
    let mut lines: Vec<String> = Vec::new();
    let lower = raw.to_lowercase();
    for key in &interesting_keys {
        let lower_key = key.to_lowercase().trim_end_matches(':').to_string();
        if let Some(pos) = lower.find(&lower_key) {
            let start = pos;
            let end = raw[start..].find('\n').map(|e| start + e).unwrap_or(raw.len());
            let line = raw[start..end].trim().to_string();
            if !line.is_empty() && !lines.contains(&line) {
                lines.push(line);
            }
        }
    }
    if lines.is_empty() {
        let first_200 = raw.lines().take(15).collect::<Vec<_>>().join("\n");
        return first_200.chars().take(500).collect();
    }
    lines.join("\n")
}

async fn scrape_homepage(domain: &str, client: &reqwest::Client, info: &mut DomainInfo) {
    let url = format!("https://{}", domain);
    let result = client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .header("User-Agent", "Mozilla/5.0 (compatible; RavenOSINT/1.0)")
        .send()
        .await;
    match result {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            extract_page_metadata(&body, &mut info.homepage_title, &mut info.homepage_description);
        }
        Err(e) => {
            info.error = Some(format!("Failed to fetch homepage: {e}"));
        }
    }
}

// ── Domain page scraping ──────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct DomainPage {
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub social_links: Vec<String>,
    pub text_preview: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DomainPages {
    pub pages: Vec<DomainPage>,
}

const COMMON_PATHS: &[&str] = &[
    "/about", "/contact", "/resume", "/cv", "/blog",
    "/projects", "/portfolio", "/about-me", "/bio", "/links",
];

/// Scrape common pages from a detected domain concurrently.
pub async fn scrape_domain_pages(domain: &str, client: &reqwest::Client) -> DomainPages {
    let base_url = format!("https://{}", domain);
    let mut pages = DomainPages::default();

    let mut handles = Vec::new();
    for path in COMMON_PATHS {
        let url = format!("{}{}", base_url, path);
        let cl = client.clone();
        handles.push(tokio::spawn(async move {
            let path_str = path.to_string();
            fetch_page(&path_str, &url, &cl).await
        }));
    }

    for handle in handles {
        if let Ok(Some(page)) = handle.await {
            pages.pages.push(page);
        }
    }

    pages
}

async fn fetch_page(path: &str, url: &str, client: &reqwest::Client) -> Option<DomainPage> {
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(10))
        .header("User-Agent", "Mozilla/5.0 (compatible; RavenOSINT/1.0)")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body = resp.text().await.ok()?;
    if body.len() < 50 {
        return None;
    }

    let mut page = DomainPage {
        path: path.to_string(),
        ..Default::default()
    };

    extract_page_metadata(&body, &mut page.title, &mut page.description);
    page.emails = extract_emails(&body);
    page.phones = extract_phones(&body);
    page.social_links = extract_social_links(&body);
    page.text_preview = extract_text_preview(&body);

    Some(page)
}

fn extract_page_metadata(html: &str, title: &mut Option<String>, description: &mut Option<String>) {
    // <title>
    if let Some(start) = html.find("<title") {
        if let Some(tag_end) = html[start..].find('>') {
            let content_start = start + tag_end + 1;
            if let Some(title_end) = html[content_start..].find("</title>") {
                let t = html[content_start..content_start + title_end].trim().to_string();
                if !t.is_empty() {
                    *title = Some(t);
                }
            }
        }
    }
    // <meta name="description" content="...">
    let meta_patterns = [
        r#"name="description""#,
        r#"name='description'"#,
        r#"name=description"#,
    ];
    for pattern in &meta_patterns {
        if let Some(p) = html.find(pattern) {
            let before = &html[..p];
            if let Some(cp) = before.rfind("content=\"") {
                let val_start = cp + 9;
                let val_end = html[val_start..].find('"').unwrap_or(0);
                let d = html[val_start..val_start + val_end].to_string();
                if !d.is_empty() {
                    *description = Some(d);
                    break;
                }
            }
        }
    }
}

fn extract_emails(html: &str) -> Vec<String> {
    let mut emails = Vec::new();
    let re = fancy_regex::Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").ok();
    if let Some(re) = re {
        for m in re.find_iter(html).flatten() {
            let email = m.as_str().to_lowercase();
            if !emails.contains(&email) {
                emails.push(email);
            }
        }
    }
    emails
}

fn extract_phones(html: &str) -> Vec<String> {
    let mut phones = Vec::new();
    // Match real phone patterns, not SVG/CSS numeric junk:
    //   +1 (226) 798-2360  |  +12267982360  |  (226) 798-2360  |  1-800-555-0199
    let re = fancy_regex::Regex::new(
        r"(?:\+?\d{1,3}[\s.-]?)?\(?\d{2,4}\)?[\s.-]?\d{3,4}[\s.-]?\d{3,4}(?:\s*(?:ext|x|#)\s*\d{1,5})?"
    ).ok();
    if let Some(re) = re {
        for m in re.find_iter(html).flatten() {
            let raw = m.as_str().trim().to_string();
            let digit_count = raw.chars().filter(|c| c.is_ascii_digit()).count();
            if digit_count >= 7 && digit_count <= 15 && !phones.contains(&raw) {
                // Skip if it contains alphabetic chars (filters SVG paths like "M10 20L5 9")
                let alpha = raw.chars().filter(|c| c.is_ascii_alphabetic()).count();
                if alpha == 0 || raw.starts_with('+') {
                    phones.push(raw);
                }
            }
        }
    }
    phones
}

fn extract_social_links(html: &str) -> Vec<String> {
    let platforms = [
        ("github.com", "GitHub"),
        ("linkedin.com", "LinkedIn"),
        ("twitter.com", "Twitter"),
        ("x.com", "X"),
        ("youtube.com", "YouTube"),
        ("facebook.com", "Facebook"),
        ("instagram.com", "Instagram"),
        ("tiktok.com", "TikTok"),
        ("medium.com", "Medium"),
        ("dev.to", "Dev.to"),
        ("hashnode.com", "Hashnode"),
        ("reddit.com", "Reddit"),
        ("stackoverflow.com", "Stack Overflow"),
        ("dribbble.com", "Dribbble"),
        ("behance.net", "Behance"),
        ("twitch.tv", "Twitch"),
        ("discord.com", "Discord"),
        ("telegram.org", "Telegram"),
        ("whatsapp.com", "WhatsApp"),
        ("patreon.com", "Patreon"),
        ("producthunt.com", "Product Hunt"),
        ("news.ycombinator.com", "Hacker News"),
        ("keybase.io", "Keybase"),
    ];

    let mut links = Vec::new();
    // Collect all href values first
    let href_re = fancy_regex::Regex::new(r#"href=["']([^"']+)["']"#).ok();
    let hrefs: Vec<String> = if let Some(re) = href_re {
        re.find_iter(html).flatten().map(|m| m.as_str().to_string()).collect()
    } else {
        return links;
    };

    for href_attr in &hrefs {
        let url = href_attr
            .trim_start_matches("href=\"")
            .trim_start_matches("href='")
            .trim_end_matches('"')
            .trim_end_matches('\'')
            .to_string();
        // Only take full URLs, not relative paths
        if !url.starts_with("http://") && !url.starts_with("https://") {
            continue;
        }
        for (domain, platform) in &platforms {
            if url.contains(domain) {
                let entry = format!("{}: {}", platform, url);
                if !links.contains(&entry) {
                    links.push(entry);
                }
            }
        }
    }
    links
}

fn extract_text_preview(html: &str) -> Option<String> {
    let mut cleaned = html.to_string();
    let tag_pairs = [
        ("<script", "</script>"), ("<style", "</style>"),
        ("<nav", "</nav>"), ("<footer", "</footer>"),
        ("<header", "</header>"), ("<svg", "</svg>"),
    ];
    for (open, close) in &tag_pairs {
        while let Some(s) = cleaned.find(open) {
            if let Some(e) = cleaned[s..].find(close) {
                cleaned.replace_range(s..s + e + close.len(), " ");
            } else {
                break;
            }
        }
    }
    let re = fancy_regex::Regex::new(r"<[^>]*>").ok()?;
    let text = re.replace_all(&cleaned, " ").to_string();
    let text = text.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&nbsp;", " ").replace("&#39;", "'").replace("&quot;", "\"");
    let re_ws = fancy_regex::Regex::new(r"\s+").ok()?;
    let text = re_ws.replace_all(&text, " ").to_string();
    let text = text.trim().to_string();
    if text.len() < 80 {
        return None;
    }
    // Find first substantial sentence cluster
    let sentences: Vec<&str> = text.split(|c| c == '.' || c == '!' || c == '?').collect();
    for window in sentences.windows(2) {
        let joined = window.join(". ").trim().to_string();
        if joined.len() > 60 && joined.len() < 1000 && !joined.contains("Search") {
            return Some(joined);
        }
    }
    // Fallback: first meaningful paragraph
    let text = text.trim_start_matches("Search");
    Some(text.chars().take(300).collect())
}
