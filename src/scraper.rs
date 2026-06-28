use reqwest::Client;
use scraper::{Html, Selector};

use crate::types::{ProfileDetails, ScrapeConfig};

pub async fn scrape_profile(
    client: &Client,
    url: &str,
    selectors: Option<&ScrapeConfig>,
) -> Option<ProfileDetails> {
    let html = client
        .get(url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;

    let doc = Html::parse_document(&html);
    let mut profile = ProfileDetails::default();

    profile.display_name = og(&doc, "og:title")
        .or_else(|| meta(&doc, "twitter:title"));
    profile.avatar_url = og(&doc, "og:image")
        .or_else(|| meta(&doc, "twitter:image"));
    profile.bio = og(&doc, "og:description")
        .or_else(|| meta(&doc, "description"));

    if let Some(cfg) = selectors {
        if let Some(sel) = &cfg.display_name {
            profile.display_name = profile.display_name.or_else(|| css_text(&doc, sel));
        }
        if let Some(sel) = &cfg.bio {
            profile.bio = profile.bio.or_else(|| css_text(&doc, sel));
        }
        if let Some(sel) = &cfg.location {
            profile.location = css_text(&doc, sel);
        }
        if let Some(sel) = &cfg.website {
            profile.website = css_text(&doc, sel)
                .or_else(|| css_attr(&doc, sel, "href"));
        }
        if let Some(sel) = &cfg.followers {
            profile.followers = css_text(&doc, sel);
        }
        if let Some(sel) = &cfg.joined_date {
            profile.joined_date = css_text(&doc, sel)
                .or_else(|| css_attr(&doc, sel, "datetime"));
        }
        if let Some(sel) = &cfg.avatar {
            profile.avatar_url = profile.avatar_url.or_else(|| css_attr(&doc, sel, "src"));
        }
        if let Some(sel) = &cfg.following {
            profile.following = css_text(&doc, sel);
        }
        if let Some(sel) = &cfg.post_count {
            profile.post_count = css_text(&doc, sel);
        }
    }

    let bio_text = profile.bio.as_deref().unwrap_or("");
    let name_text = profile.display_name.as_deref().unwrap_or("");
    let full_text = format!("{bio_text} {name_text}");

    profile.emails = extract_emails(&full_text);
    profile.phone_numbers = extract_phones(&full_text);
    profile.linked_urls = extract_urls(&full_text);

    Some(profile)
}

fn og(doc: &Html, property: &str) -> Option<String> {
    let sel = Selector::parse(&format!("meta[property='{property}']")).ok()?;
    doc.select(&sel).next()?.value().attr("content").map(|s| s.trim().to_string())
}

fn meta(doc: &Html, name: &str) -> Option<String> {
    let sel = Selector::parse(&format!("meta[name='{name}']")).ok()?;
    doc.select(&sel).next()?.value().attr("content").map(|s| s.trim().to_string())
}

fn css_text(doc: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    let text = doc.select(&sel).next()?.text().collect::<String>();
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn css_attr(doc: &Html, selector: &str, attr: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    doc.select(&sel).next()?.value().attr(attr).map(|s| s.to_string())
}

fn extract_emails(text: &str) -> Vec<String> {
    let re = fancy_regex::Regex::new(r"[\w.+\-]+@[\w\-]+\.[\w.]+").unwrap();
    re.find_iter(text)
        .filter_map(|m| m.ok())
        .map(|m| m.as_str().to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

fn extract_phones(text: &str) -> Vec<String> {
    let re = fancy_regex::Regex::new(r"\+?[\d\s\-().]{10,17}").unwrap();
    re.find_iter(text)
        .filter_map(|m| m.ok())
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| s.chars().filter(|c| c.is_ascii_digit()).count() >= 9)
        .collect()
}

fn extract_urls(text: &str) -> Vec<String> {
    let re = fancy_regex::Regex::new(r#"https?://[^\s"'<>]+"#).unwrap();
    re.find_iter(text)
        .filter_map(|m| m.ok())
        .map(|m| m.as_str().trim_end_matches(&['.', ',', ')', ']'][..]).to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_emails_finds_email() {
        let emails = extract_emails("Contact: test@example.com");
        assert!(emails.contains(&"test@example.com".to_string()));
    }

    #[test]
    fn test_extract_emails_empty() {
        let emails = extract_emails("no emails here");
        assert!(emails.is_empty());
    }

    #[test]
    fn test_extract_phones_finds_number() {
        let phones = extract_phones("Call +1-555-123-4567");
        assert!(!phones.is_empty());
    }

    #[test]
    fn test_extract_urls_finds_https() {
        let urls = extract_urls("Visit https://example.com/path");
        assert!(urls.contains(&"https://example.com/path".to_string()));
    }

    #[test]
    fn test_extract_urls_no_duplicates() {
        let urls = extract_urls("https://a.com https://a.com");
        assert_eq!(urls.len(), 1);
    }
}
