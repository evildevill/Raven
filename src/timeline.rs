use chrono::{DateTime, Utc, NaiveDate};
use serde::Serialize;

use crate::types::ClaimedProfile;

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEntry {
    pub site_name: String,
    pub event: TimelineEvent,
    pub date: String,
    pub date_parsed: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub enum TimelineEvent {
    AccountCreated,
    #[allow(dead_code)]
    LastSeen,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityTimeline {
    pub entries: Vec<TimelineEntry>,
    pub earliest_account: Option<TimelineEntry>,
    pub digital_footprint_years: Option<u32>,
    pub platforms_by_year: std::collections::BTreeMap<String, Vec<String>>,
}

pub fn build_timeline(profiles: &[ClaimedProfile]) -> ActivityTimeline {
    let mut entries: Vec<TimelineEntry> = Vec::new();

    for profile in profiles {
        if let Some(date_str) = &profile.details.joined_date {
            let parsed = parse_date(date_str);
            entries.push(TimelineEntry {
                site_name: profile.site_name.clone(),
                event: TimelineEvent::AccountCreated,
                date: date_str.clone(),
                date_parsed: parsed,
            });
        }
    }

    entries.sort_by(|a, b| a.date_parsed.cmp(&b.date_parsed));

    let earliest = entries.first().cloned();

    let footprint_years = earliest.as_ref().and_then(|e| e.date_parsed).map(|dt| {
        let now = Utc::now();
        let years = (now - dt).num_days() / 365;
        years as u32
    });

    let mut by_year: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for entry in &entries {
        if let Some(dt) = entry.date_parsed {
            let year = dt.format("%Y").to_string();
            by_year.entry(year).or_default().push(entry.site_name.clone());
        }
    }

    ActivityTimeline {
        entries,
        earliest_account: earliest,
        digital_footprint_years: footprint_years,
        platforms_by_year: by_year,
    }
}

fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.into());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0).map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc));
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return d.and_hms_opt(0, 0, 0).map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc));
    }
    if let Ok(year) = s.parse::<i32>() {
        return NaiveDate::from_ymd_opt(year, 1, 1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProfileDetails;

    fn make_profile_with_date(site: &str, date: Option<&str>) -> ClaimedProfile {
        ClaimedProfile {
            site_name: site.to_string(),
            site_url: format!("https://{site}.com/user"),
            username: "test".to_string(),
            details: ProfileDetails {
                joined_date: date.map(|s| s.to_string()),
                ..Default::default()
            },
            avatar_phash: None,
        }
    }

    #[test]
    fn test_build_timeline_empty() {
        let t = build_timeline(&[]);
        assert!(t.entries.is_empty());
        assert!(t.earliest_account.is_none());
    }

    #[test]
    fn test_build_timeline_single() {
        let p = make_profile_with_date("GitHub", Some("2020-01-01"));
        let t = build_timeline(&[p]);
        assert_eq!(t.entries.len(), 1);
        assert!(t.earliest_account.is_some());
    }

    #[test]
    fn test_build_timeline_sorting() {
        let p1 = make_profile_with_date("Reddit", Some("2022-01-01"));
        let p2 = make_profile_with_date("GitHub", Some("2020-01-01"));
        let t = build_timeline(&[p1, p2]);
        assert_eq!(t.entries[0].site_name, "GitHub");
        assert_eq!(t.entries[1].site_name, "Reddit");
    }

    #[test]
    fn test_build_timeline_by_year() {
        let p1 = make_profile_with_date("GitHub", Some("2020-06-01"));
        let p2 = make_profile_with_date("Reddit", Some("2022-03-15"));
        let t = build_timeline(&[p1, p2]);
        assert!(t.platforms_by_year.contains_key("2020"));
        assert!(t.platforms_by_year.contains_key("2022"));
    }

    #[test]
    fn test_parse_date_rfc3339() {
        let d = parse_date("2020-01-15T10:30:00Z");
        assert!(d.is_some());
    }

    #[test]
    fn test_parse_date_ymd() {
        let d = parse_date("2020-01-15");
        assert!(d.is_some());
    }

    #[test]
    fn test_parse_date_year_only() {
        let d = parse_date("2020");
        assert!(d.is_some());
    }

    #[test]
    fn test_parse_date_invalid() {
        let d = parse_date("not-a-date");
        assert!(d.is_none());
    }

    #[test]
    fn test_footprint_years() {
        let p = make_profile_with_date("GitHub", Some("2018-01-01"));
        let t = build_timeline(&[p]);
        assert!(t.digital_footprint_years.is_some());
        assert!(t.digital_footprint_years.unwrap() >= 6);
    }
}
