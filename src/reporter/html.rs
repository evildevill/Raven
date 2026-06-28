use std::path::PathBuf;

use crate::domain::DomainInfo;
use crate::error::RavenError;
use crate::graph::AccountGraph;
use crate::identity::{find_similar_bios, IdentityCluster};
use crate::timeline::ActivityTimeline;
use crate::types::{ClaimedProfile, QueryResult, SearchResults};

pub struct HtmlReporter {
    path: PathBuf,
    username: String,
    profiles: Vec<ClaimedProfile>,
    cluster: Option<IdentityCluster>,
    timeline: Option<ActivityTimeline>,
    graph: Option<AccountGraph>,
    domain_info: Option<DomainInfo>,
}

impl HtmlReporter {
    pub fn new(path: PathBuf) -> Self {
        HtmlReporter {
            path,
            username: String::new(),
            profiles: Vec::new(),
            cluster: None,
            timeline: None,
            graph: None,
            domain_info: None,
        }
    }
}

impl super::Reporter for HtmlReporter {
    fn write_search_start(&mut self, username: &str) -> Result<(), RavenError> {
        self.username = username.to_string();
        Ok(())
    }

    fn write_result(&mut self, _result: &QueryResult) -> Result<(), RavenError> {
        Ok(())
    }

    fn write_search_complete(&mut self, results: &SearchResults) -> Result<(), RavenError> {
        let html = generate_html_report(
            &self.username,
            results,
            &self.profiles,
            self.cluster.as_ref(),
            self.timeline.as_ref(),
            self.graph.as_ref(),
            self.domain_info.as_ref(),
        )?;
        std::fs::write(&self.path, &html)
            .map_err(|e| RavenError::Io(e))?;
        println!("  ✓ HTML report saved to {}", self.path.display());
        Ok(())
    }

    fn finish(&mut self) -> Result<(), RavenError> {
        Ok(())
    }
}

impl HtmlReporter {
    pub fn set_profiles(&mut self, profiles: Vec<ClaimedProfile>) {
        self.profiles = profiles;
    }

    pub fn set_cluster(&mut self, cluster: IdentityCluster) {
        self.cluster = Some(cluster);
    }

    pub fn set_timeline(&mut self, timeline: ActivityTimeline) {
        self.timeline = Some(timeline);
    }

    pub fn set_graph(&mut self, graph: AccountGraph) {
        self.graph = Some(graph);
    }

    pub fn set_domain_info(&mut self, info: DomainInfo) {
        self.domain_info = Some(info);
    }
}

fn generate_html_report(
    username: &str,
    results: &SearchResults,
    profiles: &[ClaimedProfile],
    cluster: Option<&IdentityCluster>,
    timeline: Option<&ActivityTimeline>,
    graph: Option<&AccountGraph>,
    domain_info: Option<&DomainInfo>,
) -> Result<String, RavenError> {
    let confidence = cluster.map(|c| c.confidence).unwrap_or(0.0) as u32;
    let confidence_label = if confidence >= 80 {
        "Very High"
    } else if confidence >= 60 {
        "High"
    } else if confidence >= 40 {
        "Medium"
    } else if confidence >= 20 {
        "Low"
    } else {
        "Very Low"
    };

    let graph_json = graph.map(|g| crate::graph::to_d3_json(g).to_string()).unwrap_or_default();

    let mut claimed_rows = String::new();
    for result in &results.results {
        if result.status == crate::types::QueryStatus::Claimed {
            claimed_rows.push_str(&format!(
                r#"<a href="{}" target="_blank" class="table-row">
                    <span class="cell-site">{}</span>
                    <span class="cell-url">{}</span>
                    <span class="cell-status"><span class="status-badge claimed">Claimed</span></span>
                </a>"#,
                result.site_url_user,
                html_escape(&result.site_name),
                html_escape(&result.site_url_user),
            ));
        }
    }

    let mut profile_cards = String::new();
    for p in profiles {
        let avatar_html = match &p.details.avatar_url {
            Some(url) => format!(r#"<img src="{}" alt="Avatar" class="avatar" onerror="this.style.display='none'">"#, url),
            None => String::new(),
        };
        let name = p.details.display_name.as_deref().unwrap_or("-");
        let bio = p.details.bio.as_deref().unwrap_or("");
        let loc = p.details.location.as_deref().unwrap_or("");
        let emails = p.details.emails.join(", ");
        let joined = p.details.joined_date.as_deref().unwrap_or("");

        profile_cards.push_str(&format!(
            r#"<div class="outer-shell profile-card">
                <div class="inner-core">
                    <div class="profile-avatar-wrap">{avatar}</div>
                    <div class="profile-header">
                        <h3><a href="{url}" target="_blank">{sn}</a></h3>
                        <span class="profile-site">{site}</span>
                    </div>
                    <div class="profile-details">
                        <div class="detail-row"><span class="detail-label">Name</span><span class="detail-value">{dname}</span></div>
                        <div class="detail-row"><span class="detail-label">Bio</span><span class="detail-value">{bio_esc}</span></div>
                        <div class="detail-row"><span class="detail-label">Location</span><span class="detail-value">{loc_esc}</span></div>
                        <div class="detail-row"><span class="detail-label">Joined</span><span class="detail-value">{joined_esc}</span></div>
                        <div class="detail-row"><span class="detail-label">Emails</span><span class="detail-value">{emails_esc}</span></div>
                    </div>
                </div>
            </div>"#,
            avatar = avatar_html,
            url = html_escape(&p.site_url),
            sn = html_escape(&p.site_name),
            site = html_escape(&p.site_name),
            dname = html_escape(name),
            bio_esc = html_escape(bio),
            loc_esc = html_escape(loc),
            joined_esc = html_escape(joined),
            emails_esc = html_escape(&emails),
        ));
    }

    let mut bio_similarity_section = String::new();
    if let Some(cl) = cluster {
        let similar = find_similar_bios(&cl.accounts, 0.5);
        if !similar.is_empty() {
            bio_similarity_section.push_str(
                r#"<section class="report-section reveal">
                    <div class="outer-shell">
                        <div class="inner-core">
                            <h2 class="section-title">Bio Similarity Matches</h2>
                            <div class="similarity-grid">"#,
            );
            for (a, b, score) in &similar {
                bio_similarity_section.push_str(&format!(
                    r#"<div class="similarity-card">
                        <div class="similarity-pair">
                            <span class="similarity-name">{}</span>
                            <span class="similarity-connector">↔</span>
                            <span class="similarity-name">{}</span>
                        </div>
                        <div class="similarity-score">
                            <div class="score-bar"><div class="score-fill" style="width:{:.0}%"></div></div>
                            <span class="score-label">{:.0}%</span>
                        </div>
                    </div>"#,
                    html_escape(a),
                    html_escape(b),
                    score * 100.0,
                    score * 100.0,
                ));
            }
            bio_similarity_section.push_str("</div></div></div></section>");
        }
    }

    let mut timeline_section = String::new();
    if let Some(tl) = timeline {
        if !tl.platforms_by_year.is_empty() || tl.earliest_account.is_some() || tl.digital_footprint_years.is_some() {
            timeline_section.push_str(
                r#"<section class="report-section reveal">
                    <div class="outer-shell">
                        <div class="inner-core">
                            <h2 class="section-title">Activity Timeline</h2>
                            <div class="timeline">"#,
            );
            for (year, platforms) in &tl.platforms_by_year {
                timeline_section.push_str(&format!(
                    r#"<div class="timeline-item">
                        <div class="timeline-marker"><div class="timeline-dot"></div></div>
                        <div class="timeline-content">
                            <span class="timeline-year">{}</span>
                            <span class="timeline-platforms">{}</span>
                        </div>
                    </div>"#,
                    year,
                    html_escape(&platforms.join(", ")),
                ));
            }
            if let Some(ref earliest) = tl.earliest_account {
                timeline_section.push_str(&format!(
                    r#"<div class="timeline-item">
                        <div class="timeline-marker"><div class="timeline-dot earliest"></div></div>
                        <div class="timeline-content">
                            <span class="timeline-year">Earliest</span>
                            <span class="timeline-platforms">{} on {}</span>
                        </div>
                    </div>"#,
                    html_escape(&earliest.date),
                    html_escape(&earliest.site_name),
                ));
            }
            if let Some(years) = tl.digital_footprint_years {
                timeline_section.push_str(&format!(
                    r#"<div class="timeline-footprint">
                        <span class="footprint-label">Digital footprint</span>
                        <span class="footprint-value">{} years</span>
                    </div>"#,
                    years,
                ));
            }
            timeline_section.push_str("</div></div></div></section>");
        }
    }

    let cluster_section = match cluster {
        Some(cl) => {
            let name = cl.inferred_name.as_deref().unwrap_or("Unknown");
            let loc = cl.inferred_location.as_deref().unwrap_or("Unknown");
            let emails = cl.emails_found.join(", ");
            let phones = cl.phones_found.join(", ");
            let conf_class = if confidence >= 80 {
                "very-high"
            } else if confidence >= 60 {
                "high"
            } else if confidence >= 40 {
                "medium"
            } else if confidence >= 20 {
                "low"
            } else {
                "very-low"
            };
            format!(
                r#"<section class="report-section reveal">
                    <div class="outer-shell identity-shell">
                        <div class="inner-core">
                            <h2 class="section-title">Identity Summary</h2>
                            <div class="identity-grid">
                                <div class="identity-item">
                                    <span class="identity-label">Inferred Name</span>
                                    <span class="identity-value">{name_esc}</span>
                                </div>
                                <div class="identity-item">
                                    <span class="identity-label">Inferred Location</span>
                                    <span class="identity-value">{loc_esc}</span>
                                </div>
                                <div class="identity-item">
                                    <span class="identity-label">Emails</span>
                                    <span class="identity-value">{emails_esc}</span>
                                </div>
                                <div class="identity-item">
                                    <span class="identity-label">Phones</span>
                                    <span class="identity-value">{phones_esc}</span>
                                </div>
                                <div class="identity-item confidence">
                                    <span class="identity-label">Confidence</span>
                                    <span class="confidence-badge {conf_class}">{confidence}% — {confidence_label}</span>
                                </div>
                            </div>
                        </div>
                    </div>
                </section>"#,
                name_esc = html_escape(name),
                loc_esc = html_escape(loc),
                emails_esc = html_escape(&emails),
                phones_esc = html_escape(&phones),
                conf_class = conf_class,
                confidence = confidence,
                confidence_label = confidence_label,
            )
        }
        None => String::new(),
    };

    let claimed_section = if claimed_rows.is_empty() {
        String::new()
    } else {
        format!(
            r#"<section class="report-section reveal">
                <div class="outer-shell">
                    <div class="inner-core">
                        <h2 class="section-title">Claimed Accounts <span class="section-count">{count}</span></h2>
                        <div class="glass-table">
                            <div class="table-header">
                                <span>Site</span>
                                <span>URL</span>
                                <span>Status</span>
                            </div>
                            {rows}
                        </div>
                    </div>
                </div>
            </section>"#,
            count = results.claimed_count,
            rows = claimed_rows,
        )
    };

    let profile_section = if profile_cards.is_empty() {
        String::new()
    } else {
        format!(
            r#"<section class="report-section reveal">
                <h2 class="section-title">Profile Details</h2>
                <div class="profiles-grid">
                    {cards}
                </div>
            </section>"#,
            cards = profile_cards,
        )
    };

    let graph_section = if graph_json.is_empty() || graph_json == "null" {
        String::new()
    } else {
        String::from(
            r#"<section class="report-section reveal">
                <div class="outer-shell">
                    <div class="inner-core graph-core">
                        <h2 class="section-title">Account Link Graph</h2>
                        <svg id="graph" class="graph-svg"></svg>
                    </div>
                </div>
            </section>"#,
        )
    };

    let domain_section = match domain_info {
        Some(di) if di.resolves => {
            let a_records = if di.a_records.is_empty() { String::new() } else {
                format!(r#"<div class="dns-row"><span class="dns-type">A</span><span class="dns-value">{}</span></div>"#, html_escape(&di.a_records.join(", ")))
            };
            let aaaa_records = if di.aaaa_records.is_empty() { String::new() } else {
                format!(r#"<div class="dns-row"><span class="dns-type">AAAA</span><span class="dns-value">{}</span></div>"#, html_escape(&di.aaaa_records.join(", ")))
            };
            let mx_records = if di.mx_records.is_empty() { String::new() } else {
                format!(r#"<div class="dns-row"><span class="dns-type">MX</span><span class="dns-value">{}</span></div>"#, html_escape(&di.mx_records.join(", ")))
            };
            let txt_records = if di.txt_records.is_empty() { String::new() } else {
                format!(r#"<div class="dns-row"><span class="dns-type">TXT</span><span class="dns-value">{}</span></div>"#, html_escape(&di.txt_records.join(", ")))
            };
            let ns_records = if di.ns_records.is_empty() { String::new() } else {
                format!(r#"<div class="dns-row"><span class="dns-type">NS</span><span class="dns-value">{}</span></div>"#, html_escape(&di.ns_records.join(", ")))
            };
            let whois_html = di.whois.as_ref().map(|w| {
                let escaped = html_escape(w);
                format!(r#"<pre class="whois-block">{}</pre>"#, escaped)
            }).unwrap_or_default();
            let title_html = di.homepage_title.as_ref().map(|t| {
                format!(r#"<div class="dns-row"><span class="dns-type">Title</span><span class="dns-value">{}</span></div>"#, html_escape(t))
            }).unwrap_or_default();
            let desc_html = di.homepage_description.as_ref().map(|d| {
                let truncated: String = d.chars().take(200).collect();
                format!(r#"<div class="dns-row"><span class="dns-type">Description</span><span class="dns-value">{}</span></div>"#, html_escape(&truncated))
            }).unwrap_or_default();
            let error_html = di.error.as_ref().map(|e| {
                format!(r#"<div class="dns-row"><span class="dns-type">Error</span><span class="dns-value">{}</span></div>"#, html_escape(e))
            }).unwrap_or_default();

            let pages_html = if di.pages.pages.is_empty() {
                String::new()
            } else {
                let mut cards = String::new();
                for page in &di.pages.pages {
                    let page_title = page.title.as_deref().unwrap_or("Untitled");
                    let page_desc = page.description.as_deref().unwrap_or("");
                    let page_preview = page.text_preview.as_deref().unwrap_or("");
                    let page_emails = if page.emails.is_empty() { String::new() } else {
                        format!(r#"<div class="page-detail"><span class="detail-label">Emails</span><span class="detail-value">{}</span></div>"#, html_escape(&page.emails.join(", ")))
                    };
                    let page_phones = if page.phones.is_empty() { String::new() } else {
                        format!(r#"<div class="page-detail"><span class="detail-label">Phones</span><span class="detail-value">{}</span></div>"#, html_escape(&page.phones.join(", ")))
                    };
                    let page_social = if page.social_links.is_empty() { String::new() } else {
                        let lines: String = page.social_links.iter().map(|s| format!(r#"<div class="social-link">{}</div>"#, html_escape(s))).collect();
                        format!(r#"<div class="page-detail"><span class="detail-label">Social</span><span class="detail-value social-links">{}</span></div>"#, lines)
                    };
                    let page_preview_html = if page_preview.is_empty() { String::new() } else {
                        format!(r#"<div class="page-preview">{}</div>"#, html_escape(page_preview))
                    };
                    cards.push_str(&format!(
                        r#"<div class="page-card">
                            <div class="page-path">{}</div>
                            <div class="page-title">{}</div>
                            {}
                            {}
                            {}
                            {}
                            {}
                        </div>"#,
                        html_escape(&page.path),
                        html_escape(page_title),
                        if page_desc.is_empty() { String::new() } else { format!(r#"<div class="page-desc">{}</div>"#, html_escape(page_desc)) },
                        page_emails,
                        page_phones,
                        page_social,
                        page_preview_html,
                    ));
                }
                format!(
                    r#"<div class="domain-pages">
                        <h3 class="pages-heading">Scraped Pages</h3>
                        <div class="pages-grid">{}</div>
                    </div>"#,
                    cards,
                )
            };

            format!(
                r#"<section class="report-section reveal">
                    <div class="outer-shell">
                        <div class="inner-core">
                            <h2 class="section-title">Domain Profile <span class="section-count">{domain}</span></h2>
                            <div class="dns-table">
                                {a_records}
                                {aaaa_records}
                                {mx_records}
                                {txt_records}
                                {ns_records}
                                {title_html}
                                {desc_html}
                                {error_html}
                            </div>
                            {whois_html}
                            {pages_html}
                        </div>
                    </div>
                </section>"#,
                domain = html_escape(&di.domain),
                a_records = a_records,
                aaaa_records = aaaa_records,
                mx_records = mx_records,
                txt_records = txt_records,
                ns_records = ns_records,
                whois_html = whois_html,
                title_html = title_html,
                desc_html = desc_html,
                error_html = error_html,
                pages_html = pages_html,
            )
        }
        _ => String::new(),
    };

    let html = format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Raven OSINT Report — {username}</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@300;400;500;600;700;800&display=swap" rel="stylesheet">
<script src="https://unpkg.com/@phosphor-icons/web@2.1.1"></script>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}

  body {{
    font-family: 'Plus Jakarta Sans', -apple-system, sans-serif;
    background: #050505;
    color: rgba(255,255,255,0.85);
    min-height: 100dvh;
    line-height: 1.6;
    -webkit-font-smoothing: antialiased;
  }}

  body::before {{
    content: '';
    position: fixed;
    inset: 0;
    background:
      radial-gradient(ellipse at 20% 30%, rgba(56,189,248,0.08) 0%, transparent 50%),
      radial-gradient(ellipse at 80% 20%, rgba(168,85,247,0.06) 0%, transparent 50%),
      radial-gradient(ellipse at 50% 80%, rgba(52,211,153,0.04) 0%, transparent 50%);
    pointer-events: none;
    z-index: 0;
  }}

  .container {{
    max-width: 1200px;
    margin: 0 auto;
    padding: 48px 24px 96px;
    position: relative;
    z-index: 1;
  }}

  h1 {{
    font-size: 48px;
    font-weight: 700;
    letter-spacing: -0.03em;
    line-height: 1.1;
    margin: 8px 0 4px;
  }}

  h1 .accent {{ color: rgba(255,255,255,0.4); font-weight: 400; }}

  h2 {{
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.02em;
    margin-bottom: 20px;
    color: rgba(255,255,255,0.9);
  }}

  h3 {{
    font-size: 16px;
    font-weight: 500;
    margin-bottom: 4px;
  }}

  a {{
    color: #38bdf8;
    text-decoration: none;
    transition: color 0.3s cubic-bezier(0.32, 0.72, 0, 1);
  }}

  a:hover {{ color: #7dd3fc; }}

  .eyebrow {{
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 14px;
    border-radius: 999px;
    background: rgba(255,255,255,0.04);
    border: 1px solid rgba(255,255,255,0.07);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.2em;
    text-transform: uppercase;
    color: rgba(255,255,255,0.35);
    margin-bottom: 16px;
  }}

  .eyebrow i {{ font-size: 14px; }}

  .timestamp {{
    font-size: 13px;
    color: rgba(255,255,255,0.25);
    margin-top: 8px;
    font-weight: 400;
    letter-spacing: 0.02em;
  }}

  .report-header {{
    margin-bottom: 48px;
  }}

  .report-section {{
    margin-bottom: 48px;
  }}

  .section-title {{
    display: flex;
    align-items: center;
    gap: 10px;
  }}

  .section-count {{
    font-size: 12px;
    font-weight: 500;
    padding: 2px 10px;
    border-radius: 999px;
    background: rgba(255,255,255,0.05);
    color: rgba(255,255,255,0.35);
    border: 1px solid rgba(255,255,255,0.06);
  }}

  .stats-bento {{
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    margin-bottom: 56px;
  }}

  .stat-card {{
    position: relative;
    background: rgba(255,255,255,0.03);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 20px;
    padding: 24px;
    transition: all 0.6s cubic-bezier(0.32, 0.72, 0, 1);
    overflow: hidden;
  }}

  .stat-card:hover {{
    background: rgba(255,255,255,0.05);
    border-color: rgba(255,255,255,0.12);
    transform: scale(1.02);
  }}

  .stat-card.claimed {{
    background: rgba(56,189,248,0.05);
    border-color: rgba(56,189,248,0.12);
  }}

  .stat-card.claimed:hover {{
    background: rgba(56,189,248,0.08);
    border-color: rgba(56,189,248,0.2);
  }}

  .stat-icon {{
    font-size: 22px;
    margin-bottom: 14px;
    color: rgba(255,255,255,0.2);
  }}

  .stat-card.claimed .stat-icon {{ color: #38bdf8; }}

  .stat-number {{
    font-size: 38px;
    font-weight: 700;
    letter-spacing: -0.03em;
    line-height: 1;
    margin-bottom: 6px;
  }}

  .stat-label {{
    font-size: 11px;
    font-weight: 600;
    color: rgba(255,255,255,0.3);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }}

  .outer-shell {{
    background: rgba(255,255,255,0.025);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 24px;
    padding: 1.5px;
    transition: all 0.6s cubic-bezier(0.32, 0.72, 0, 1);
  }}

  .outer-shell:hover {{
    border-color: rgba(255,255,255,0.12);
  }}

  .inner-core {{
    background: rgba(255,255,255,0.035);
    border-radius: calc(24px - 1.5px);
    padding: 24px;
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.08);
  }}

  .identity-grid {{
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
  }}

  .identity-item {{
    display: flex;
    flex-direction: column;
    gap: 4px;
  }}

  .identity-item.confidence {{ grid-column: 1 / -1; }}

  .identity-label {{
    font-size: 10px;
    font-weight: 600;
    color: rgba(255,255,255,0.25);
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }}

  .identity-value {{
    font-size: 14px;
    color: rgba(255,255,255,0.7);
    word-break: break-word;
  }}

  .confidence-badge {{
    display: inline-flex;
    align-items: center;
    padding: 4px 16px;
    border-radius: 999px;
    font-size: 13px;
    font-weight: 600;
    width: fit-content;
  }}

  .confidence-badge.very-high {{ background: rgba(52,211,153,0.1); color: #34d399; border: 1px solid rgba(52,211,153,0.2); }}
  .confidence-badge.high {{ background: rgba(56,189,248,0.1); color: #38bdf8; border: 1px solid rgba(56,189,248,0.2); }}
  .confidence-badge.medium {{ background: rgba(251,191,36,0.1); color: #fbbf24; border: 1px solid rgba(251,191,36,0.2); }}
  .confidence-badge.low {{ background: rgba(251,146,60,0.1); color: #fb923c; border: 1px solid rgba(251,146,60,0.2); }}
  .confidence-badge.very-low {{ background: rgba(248,113,113,0.1); color: #f87171; border: 1px solid rgba(248,113,113,0.2); }}

  .similarity-grid {{
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
  }}

  .similarity-card {{
    padding: 16px;
    background: rgba(255,255,255,0.02);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 16px;
  }}

  .similarity-pair {{
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }}

  .similarity-name {{
    font-size: 13px;
    font-weight: 500;
    color: rgba(255,255,255,0.65);
  }}

  .similarity-connector {{
    color: rgba(255,255,255,0.15);
    font-size: 16px;
  }}

  .similarity-score {{
    display: flex;
    align-items: center;
    gap: 10px;
  }}

  .score-bar {{
    flex: 1;
    height: 4px;
    background: rgba(255,255,255,0.05);
    border-radius: 999px;
    overflow: hidden;
  }}

  .score-fill {{
    height: 100%;
    background: linear-gradient(90deg, #38bdf8, #818cf8);
    border-radius: 999px;
  }}

  .score-label {{
    font-size: 12px;
    font-weight: 600;
    color: rgba(255,255,255,0.45);
    min-width: 40px;
    text-align: right;
  }}

  .timeline {{
    position: relative;
    padding-left: 32px;
  }}

  .timeline::before {{
    content: '';
    position: absolute;
    left: 11px;
    top: 4px;
    bottom: 4px;
    width: 1px;
    background: rgba(255,255,255,0.08);
  }}

  .timeline-item {{
    position: relative;
    padding-bottom: 24px;
    display: flex;
    align-items: flex-start;
  }}

  .timeline-marker {{
    position: absolute;
    left: -32px;
    top: 4px;
    width: 24px;
    display: flex;
    justify-content: center;
  }}

  .timeline-dot {{
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: rgba(255,255,255,0.15);
    border: 1px solid rgba(255,255,255,0.08);
    transition: all 0.4s cubic-bezier(0.32, 0.72, 0, 1);
  }}

  .timeline-item:hover .timeline-dot {{
    background: #38bdf8;
    border-color: #38bdf8;
    transform: scale(1.4);
  }}

  .timeline-dot.earliest {{
    background: #f59e0b;
    border-color: #f59e0b;
  }}

  .timeline-content {{
    display: flex;
    flex-direction: column;
    gap: 2px;
  }}

  .timeline-year {{
    font-size: 13px;
    font-weight: 600;
    color: rgba(255,255,255,0.4);
    letter-spacing: 0.02em;
  }}

  .timeline-platforms {{
    font-size: 14px;
    color: rgba(255,255,255,0.65);
  }}

  .timeline-footprint {{
    margin-top: 20px;
    padding: 14px 18px;
    background: rgba(255,255,255,0.025);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 12px;
    display: flex;
    align-items: center;
    gap: 10px;
  }}

  .footprint-label {{
    font-size: 11px;
    color: rgba(255,255,255,0.25);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-weight: 600;
  }}

  .footprint-value {{
    font-size: 14px;
    font-weight: 600;
    color: #38bdf8;
  }}

  .glass-table {{
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 16px;
    overflow: hidden;
  }}

  .table-header {{
    display: grid;
    grid-template-columns: 140px 1fr 100px;
    gap: 12px;
    padding: 12px 18px;
    background: rgba(255,255,255,0.025);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: rgba(255,255,255,0.2);
    border-bottom: 1px solid rgba(255,255,255,0.06);
  }}

  .table-row {{
    display: grid;
    grid-template-columns: 140px 1fr 100px;
    gap: 12px;
    padding: 12px 18px;
    align-items: center;
    border-bottom: 1px solid rgba(255,255,255,0.025);
    transition: all 0.4s cubic-bezier(0.32, 0.72, 0, 1);
    text-decoration: none;
    color: inherit;
  }}

  .table-row:last-child {{ border-bottom: none; }}

  .table-row:hover {{
    background: rgba(255,255,255,0.04);
  }}

  .cell-site {{
    font-size: 14px;
    font-weight: 500;
    color: rgba(255,255,255,0.75);
  }}

  .cell-url {{
    font-size: 13px;
    color: rgba(255,255,255,0.3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 400;
  }}

  .cell-status {{ text-align: right; }}

  .status-badge {{
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 12px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }}

  .status-badge.claimed {{
    background: rgba(52,211,153,0.08);
    color: #34d399;
    border: 1px solid rgba(52,211,153,0.18);
  }}

  .profiles-grid {{
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 16px;
  }}

  .profile-card .inner-core {{
    display: flex;
    flex-direction: column;
    gap: 16px;
  }}

  .profile-avatar-wrap img.avatar {{
    width: 48px;
    height: 48px;
    border-radius: 14px;
    object-fit: cover;
    border: 1px solid rgba(255,255,255,0.08);
  }}

  .profile-header h3 {{ font-size: 16px; font-weight: 600; }}
  .profile-header h3 a {{ color: rgba(255,255,255,0.8); }}
  .profile-header h3 a:hover {{ color: #38bdf8; }}

  .profile-site {{
    font-size: 12px;
    color: rgba(255,255,255,0.25);
  }}

  .profile-details {{
    display: flex;
    flex-direction: column;
    gap: 8px;
  }}

  .detail-row {{
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }}

  .detail-label {{
    font-size: 10px;
    font-weight: 600;
    color: rgba(255,255,255,0.25);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    min-width: 72px;
    flex-shrink: 0;
    padding-top: 1px;
  }}

  .detail-value {{
    font-size: 13px;
    color: rgba(255,255,255,0.55);
    line-height: 1.4;
  }}

  .graph-core {{ padding: 16px; }}

  .graph-svg {{
    width: 100%;
    height: 500px;
    border-radius: 12px;
    background: rgba(0,0,0,0.25);
  }}

  .dns-table {{
    display: flex;
    flex-direction: column;
    gap: 8px;
  }}

  .dns-row {{
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid rgba(255,255,255,0.04);
    font-size: 13px;
  }}

  .dns-type {{
    font-weight: 600;
    color: #38bdf8;
    min-width: 60px;
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }}

  .dns-value {{
    color: rgba(255,255,255,0.65);
    word-break: break-all;
  }}

  .whois-block {{
    margin-top: 16px;
    padding: 16px;
    background: rgba(0,0,0,0.3);
    border-radius: 8px;
    font-size: 11px;
    line-height: 1.7;
    color: rgba(255,255,255,0.5);
    overflow-x: auto;
    max-height: 300px;
    overflow-y: auto;
    white-space: pre-wrap;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
  }}

  .domain-pages {{ margin-top: 24px; }}

  .pages-heading {{
    font-size: 13px;
    font-weight: 600;
    color: rgba(255,255,255,0.5);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-bottom: 12px;
  }}

  .pages-grid {{
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
  }}

  .page-card {{
    background: rgba(255,255,255,0.02);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 10px;
    padding: 16px;
  }}

  .page-path {{
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #38bdf8;
    font-weight: 600;
    margin-bottom: 4px;
  }}

  .page-title {{
    font-size: 14px;
    font-weight: 600;
    color: rgba(255,255,255,0.85);
    margin-bottom: 8px;
  }}

  .page-desc {{
    font-size: 12px;
    color: rgba(255,255,255,0.45);
    margin-bottom: 8px;
    line-height: 1.5;
  }}

  .page-detail {{
    display: flex;
    gap: 8px;
    margin-top: 6px;
    font-size: 11px;
  }}

  .page-detail .detail-label {{
    color: rgba(255,255,255,0.3);
    min-width: 50px;
    flex-shrink: 0;
  }}

  .page-detail .detail-value {{
    color: rgba(255,255,255,0.6);
    word-break: break-all;
  }}

  .social-links {{
    display: flex;
    flex-direction: column;
    gap: 2px;
  }}

  .social-link {{
    color: #38bdf8;
    font-size: 11px;
  }}

  .page-preview {{
    margin-top: 8px;
    font-size: 11px;
    line-height: 1.6;
    color: rgba(255,255,255,0.35);
    border-top: 1px solid rgba(255,255,255,0.04);
    padding-top: 8px;
  }}

  .reveal {{
    opacity: 0;
    transform: translateY(40px);
    filter: blur(4px);
    transition: all 0.8s cubic-bezier(0.32, 0.72, 0, 1);
  }}

  .reveal.visible {{
    opacity: 1;
    transform: translateY(0);
    filter: blur(0);
  }}

  .report-footer {{
    margin-top: 80px;
    padding-top: 24px;
    border-top: 1px solid rgba(255,255,255,0.05);
    text-align: center;
    font-size: 12px;
    color: rgba(255,255,255,0.15);
    letter-spacing: 0.08em;
  }}

  .report-footer .raven-brand {{
    font-weight: 700;
    color: rgba(255,255,255,0.25);
  }}

  ::-webkit-scrollbar {{ width: 6px; }}
  ::-webkit-scrollbar-track {{ background: transparent; }}
  ::-webkit-scrollbar-thumb {{ background: rgba(255,255,255,0.08); border-radius: 999px; }}
  ::-webkit-scrollbar-thumb:hover {{ background: rgba(255,255,255,0.15); }}

  ::selection {{ background: rgba(56,189,248,0.2); color: rgba(255,255,255,0.95); }}

  @media (max-width: 768px) {{
    .container {{ padding: 28px 16px 64px; }}
    h1 {{ font-size: 32px; }}
    .stats-bento {{ grid-template-columns: 1fr 1fr; gap: 8px; }}
    .stat-card:nth-child(1) {{ grid-column: 1 / -1 !important; grid-row: auto !important; }}
    .stat-number {{ font-size: 28px; }}
    .profiles-grid {{ grid-template-columns: 1fr; }}
    .identity-grid {{ grid-template-columns: 1fr; }}
    .glass-table {{ border-radius: 12px; }}
    .table-header, .table-row {{ grid-template-columns: 1fr 1fr; }}
    .table-header span:nth-child(2), .table-row .cell-url {{ display: none; }}
    .similarity-grid {{ grid-template-columns: 1fr; }}
    .graph-svg {{ height: 300px; }}
  }}

  @media print {{
    body {{ background: #fff; color: #000; }}
    .reveal {{ opacity: 1; transform: none; filter: none; }}
    body::before {{ display: none; }}
    .stat-card {{ border-color: #ddd; background: #f8f8f8; }}
    .stat-number {{ color: #000; }}
    .stat-card.claimed {{ background: #e8f4fd; border-color: #b3dffc; }}
    a {{ color: #0066cc; }}
    .outer-shell {{ border-color: #ddd; background: #f8f8f8; }}
    .inner-core {{ background: #fff; }}
    .confidence-badge.very-high {{ background: #d4edda; color: #155724; border-color: #c3e6cb; }}
    .status-badge.claimed {{ background: #d4edda; color: #155724; border-color: #c3e6cb; }}
    .timeline::before {{ background: #ddd; }}
    .timeline-dot {{ background: #999; border-color: #999; }}
  }}
</style>
</head>
<body>
<div class="container">
  <header class="report-header">
    <div class="eyebrow"><i class="ph ph-eye"></i> OSINT Report</div>
    <h1>raven <span class="accent">// {username}</span></h1>
    <p class="timestamp">Generated {timestamp} &middot; {total} sites searched</p>
  </header>

  <div class="stats-bento reveal">
    <div class="stat-card claimed" style="grid-column: span 2; grid-row: span 2;">
      <div class="stat-icon"><i class="ph ph-check-circle"></i></div>
      <div class="stat-number">{claimed}</div>
      <div class="stat-label">Claimed</div>
    </div>
    <div class="stat-card">
      <div class="stat-icon"><i class="ph ph-circle-dashed"></i></div>
      <div class="stat-number">{available}</div>
      <div class="stat-label">Available</div>
    </div>
    <div class="stat-card">
      <div class="stat-icon"><i class="ph ph-question"></i></div>
      <div class="stat-number">{unknown}</div>
      <div class="stat-label">Unknown</div>
    </div>
    <div class="stat-card">
      <div class="stat-icon"><i class="ph ph-prohibit"></i></div>
      <div class="stat-number">{illegal}</div>
      <div class="stat-label">Illegal</div>
    </div>
    <div class="stat-card">
      <div class="stat-icon"><i class="ph ph-shield-warning"></i></div>
      <div class="stat-number">{waf}</div>
      <div class="stat-label">WAF</div>
    </div>
  </div>

  {cluster_section}

  {bio_similarity_section}

  {timeline_section}

  {claimed_section}

  {profile_section}

  {graph_section}

  {domain_section}

  <footer class="report-footer">
    Generated by <span class="raven-brand">Raven OSINT</span>
  </footer>
</div>
<script>
  document.addEventListener('DOMContentLoaded', function() {{
    var reveals = document.querySelectorAll('.reveal');
    if (reveals.length > 0) {{
      var observer = new IntersectionObserver(function(entries) {{
        entries.forEach(function(entry) {{
          if (entry.isIntersecting) {{
            entry.target.classList.add('visible');
            observer.unobserve(entry.target);
          }}
        }});
      }}, {{ threshold: 0.08 }});
      reveals.forEach(function(el) {{ observer.observe(el); }});
    }}
  }});
</script>
<script>
  var graphData = {graph_json};
  (function() {{
    if (!graphData || !graphData.nodes || graphData.nodes.length === 0) return;
    var svgEl = document.getElementById('graph');
    if (!svgEl) return;
    var w = svgEl.getBoundingClientRect().width || 800, h = 500;
    svgEl.innerHTML = '';
    var ns = 'http://www.w3.org/2000/svg';
    svgEl.setAttribute('viewBox', '0 0 ' + w + ' ' + h);
    var nodes = graphData.nodes.map(function(n) {{
      return {{
        id: n.id, site: n.site,
        x: w/2 + (Math.random()-0.5)*w*0.6,
        y: h/2 + (Math.random()-0.5)*h*0.6,
        vx: 0, vy: 0, fx: null, fy: null
      }};
    }});
    var links = (graphData.links || []).map(function(l) {{
      var src = typeof l.source === 'object' ? l.source.id : l.source;
      var tgt = typeof l.target === 'object' ? l.target.id : l.target;
      return {{ source: src, target: tgt }};
    }});
    var nodeMap = {{}}; nodes.forEach(function(n) {{ nodeMap[n.id] = n; }});
    var resolvedLinks = [];
    links.forEach(function(l) {{
      if (nodeMap[l.source] && nodeMap[l.target])
        resolvedLinks.push({{ source: nodeMap[l.source], target: nodeMap[l.target] }});
    }});
    function tick() {{
      // repulsion
      for (var i = 0; i < nodes.length; i++) {{
        for (var j = i+1; j < nodes.length; j++) {{
          var dx = nodes[j].x - nodes[i].x, dy = nodes[j].y - nodes[i].y;
          var dist = Math.sqrt(dx*dx + dy*dy) || 1;
          var force = 5000 / (dist * dist);
          var fx = dx/dist * force, fy = dy/dist * force;
          nodes[i].vx -= fx; nodes[i].vy -= fy;
          nodes[j].vx += fx; nodes[j].vy += fy;
        }}
      }}
      // attraction along links
      for (var k = 0; k < resolvedLinks.length; k++) {{
        var s = resolvedLinks[k].source, t = resolvedLinks[k].target;
        var dx = t.x - s.x, dy = t.y - s.y;
        var dist = Math.sqrt(dx*dx + dy*dy) || 1;
        var force = (dist - 180) * 0.01;
        var fx = dx/dist * force, fy = dy/dist * force;
        s.vx += fx; s.vy += fy;
        t.vx -= fx; t.vy -= fy;
      }}
      // center gravity
      for (var i = 0; i < nodes.length; i++) {{
        nodes[i].vx += (w/2 - nodes[i].x) * 0.001;
        nodes[i].vy += (h/2 - nodes[i].y) * 0.001;
      }}
      // apply velocity + damping
      for (var i = 0; i < nodes.length; i++) {{
        if (nodes[i].fx !== null) {{ nodes[i].x = nodes[i].fx; nodes[i].y = nodes[i].fy; continue; }}
        nodes[i].vx *= 0.85; nodes[i].vy *= 0.85;
        nodes[i].x += nodes[i].vx; nodes[i].y += nodes[i].vy;
        nodes[i].x = Math.max(20, Math.min(w-20, nodes[i].x));
        nodes[i].y = Math.max(20, Math.min(h-20, nodes[i].y));
      }}
      render();
    }}
    function render() {{
      var linkGroup = svgEl.querySelector('.links') || function() {{
        var g = document.createElementNS(ns, 'g'); g.setAttribute('class', 'links');
        svgEl.appendChild(g); return g;
      }}();
      linkGroup.innerHTML = '';
      for (var k = 0; k < resolvedLinks.length; k++) {{
        var line = document.createElementNS(ns, 'line');
        line.setAttribute('x1', resolvedLinks[k].source.x);
        line.setAttribute('y1', resolvedLinks[k].source.y);
        line.setAttribute('x2', resolvedLinks[k].target.x);
        line.setAttribute('y2', resolvedLinks[k].target.y);
        line.setAttribute('stroke', 'rgba(255,255,255,0.08)');
        line.setAttribute('stroke-width', '1.5');
        linkGroup.appendChild(line);
      }}
      var nodeGroup = svgEl.querySelector('.nodes') || function() {{
        var g = document.createElementNS(ns, 'g'); g.setAttribute('class', 'nodes');
        svgEl.appendChild(g); return g;
      }}();
      nodeGroup.innerHTML = '';
      for (var i = 0; i < nodes.length; i++) {{
        var circ = document.createElementNS(ns, 'circle');
        circ.setAttribute('cx', nodes[i].x); circ.setAttribute('cy', nodes[i].y);
        circ.setAttribute('r', '7');
        circ.setAttribute('fill', 'url(#grad)');
        circ.setAttribute('stroke', 'rgba(255,255,255,0.12)');
        circ.setAttribute('stroke-width', '1.5');
        (function(d) {{
          circ.addEventListener('mousedown', function(e) {{
            var startX = e.clientX, startY = e.clientY;
            function onMove(ev) {{
              var rect = svgEl.getBoundingClientRect();
              d.fx = ev.clientX - rect.left; d.fy = ev.clientY - rect.top;
            }}
            function onUp() {{
              window.removeEventListener('mousemove', onMove);
              window.removeEventListener('mouseup', onUp);
              setTimeout(function() {{ d.fx = null; d.fy = null; }}, 500);
            }}
            window.addEventListener('mousemove', onMove);
            window.addEventListener('mouseup', onUp);
          }});
        }})(nodes[i]);
        nodeGroup.appendChild(circ);
        var lbl = document.createElementNS(ns, 'text');
        lbl.setAttribute('x', nodes[i].x + 12);
        lbl.setAttribute('y', nodes[i].y + 3);
        lbl.setAttribute('fill', 'rgba(255,255,255,0.4)');
        lbl.setAttribute('font-size', '10px');
        lbl.appendChild(document.createTextNode(nodes[i].site));
        nodeGroup.appendChild(lbl);
      }}
    }}
    // add gradient def
    var defs = document.createElementNS(ns, 'defs');
    var grad = document.createElementNS(ns, 'radialGradient');
    grad.setAttribute('id', 'grad');
    var stop1 = document.createElementNS(ns, 'stop');
    stop1.setAttribute('offset', '0%'); stop1.setAttribute('stop-color', '#7dd3fc');
    grad.appendChild(stop1);
    var stop2 = document.createElementNS(ns, 'stop');
    stop2.setAttribute('offset', '100%'); stop2.setAttribute('stop-color', '#38bdf8');
    grad.appendChild(stop2);
    defs.appendChild(grad);
    svgEl.insertBefore(defs, svgEl.firstChild);
    for (var iter = 0; iter < 120; iter++) tick();
    // animate a few extra frames
    var frames = 0;
    function animate() {{
      tick();
      if (++frames < 20) requestAnimationFrame(animate);
    }}
    requestAnimationFrame(animate);
    // re-render on resize
    window.addEventListener('resize', function() {{
      w = svgEl.getBoundingClientRect().width || 800;
      svgEl.setAttribute('viewBox', '0 0 ' + w + ' ' + h);
    }});
  }})();
</script>
</body>
</html>"##,
        username = html_escape(username),
        timestamp = html_escape(&results.timestamp),
        total = results.total_sites,
        claimed = results.claimed_count,
        available = results.available_count,
        unknown = results.unknown_count,
        illegal = results.illegal_count,
        waf = results.waf_count,
        cluster_section = cluster_section,
        bio_similarity_section = bio_similarity_section,
        timeline_section = timeline_section,
        claimed_section = claimed_section,
        profile_section = profile_section,
        graph_section = graph_section,
        domain_section = domain_section,
        graph_json = graph_json,
    );

    Ok(html)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
