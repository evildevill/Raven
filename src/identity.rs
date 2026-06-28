use std::collections::HashMap;

use crate::types::ClaimedProfile;

pub fn calculate_confidence(profiles: &[ClaimedProfile]) -> f32 {
    if profiles.is_empty() {
        return 0.0;
    }

    let mut score = 0.0f32;

    let names: Vec<&str> = profiles.iter()
        .filter_map(|p| p.details.display_name.as_deref())
        .collect();
    if !names.is_empty() {
        let most_common = mode_str(&names);
        let match_ratio = names.iter().filter(|&&n| n == most_common).count() as f32
            / names.len() as f32;
        score += match_ratio * 30.0;
    }

    let avatar_hashes: Vec<u64> = profiles.iter()
        .filter_map(|p| p.avatar_phash)
        .collect();
    if avatar_hashes.len() >= 2 {
        let pairs = avatar_hashes.len() * (avatar_hashes.len() - 1) / 2;
        let matching = avatar_hashes.iter().enumerate()
            .flat_map(|(i, &h1)| avatar_hashes[i+1..].iter().map(move |&h2| (h1, h2)))
            .filter(|&(h1, h2)| phash_distance(h1, h2) < 10)
            .count();
        let avatar_ratio = matching as f32 / pairs as f32;
        score += avatar_ratio * 25.0;
    }

    let found_urls: Vec<&str> = profiles.iter()
        .map(|p| p.site_url.as_str())
        .collect();
    let cross_link_count = profiles.iter()
        .flat_map(|p| p.details.linked_urls.iter())
        .filter(|url| found_urls.iter().any(|&found| url.contains(found)))
        .count();
    let cross_score = (cross_link_count as f32 / profiles.len() as f32).min(1.0);
    score += cross_score * 25.0;

    let locations: Vec<&str> = profiles.iter()
        .filter_map(|p| p.details.location.as_deref())
        .collect();
    if locations.len() >= 2 {
        let most_common_loc = mode_str(&locations);
        let loc_ratio = locations.iter().filter(|&&l| l == most_common_loc).count() as f32
            / locations.len() as f32;
        score += loc_ratio * 10.0;
    }

    let account_bonus = (profiles.len() as f32 / 15.0).min(1.0) * 10.0;
    score += account_bonus;

    score.min(100.0)
}

fn mode_str<'a>(items: &[&'a str]) -> &'a str {
    let mut counts = HashMap::new();
    for &item in items {
        *counts.entry(item).or_insert(0usize) += 1;
    }
    counts.into_iter().max_by_key(|&(_, c)| c).map(|(s, _)| s).unwrap_or("")
}

fn phash_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[derive(Debug, Clone)]
pub enum Signal {
    SameName(String),
    SameAvatar { site_a: String, site_b: String, hash_distance: u32 },
    CrossLinked { from_site: String, to_url: String },
    SameLocation(String),
    SameWebsite(String),
    SameEmail(String),
    #[allow(dead_code)]
    SameBio { similarity: f32 },
}

#[derive(Debug, Clone)]
pub struct IdentityCluster {
    pub accounts: Vec<ClaimedProfile>,
    pub shared_signals: Vec<Signal>,
    pub inferred_name: Option<String>,
    pub inferred_location: Option<String>,
    pub emails_found: Vec<String>,
    pub phones_found: Vec<String>,
    #[allow(dead_code)]
    pub all_linked_urls: Vec<String>,
    pub confidence: f32,
}

pub fn build_identity_cluster(profiles: Vec<ClaimedProfile>) -> IdentityCluster {
    let mut signals = Vec::new();

    let names: Vec<(usize, &str)> = profiles.iter().enumerate()
        .filter_map(|(i, p)| p.details.display_name.as_deref().map(|n| (i, n)))
        .collect();
    if let Some((_, name)) = names.first() {
        if names.iter().filter(|(_, n)| n == name).count() > 1 {
            signals.push(Signal::SameName(name.to_string()));
        }
    }

    for i in 0..profiles.len() {
        for j in (i + 1)..profiles.len() {
            if let (Some(h1), Some(h2)) = (profiles[i].avatar_phash, profiles[j].avatar_phash) {
                let dist = phash_distance(h1, h2);
                if dist < 10 {
                    signals.push(Signal::SameAvatar {
                        site_a: profiles[i].site_name.clone(),
                        site_b: profiles[j].site_name.clone(),
                        hash_distance: dist,
                    });
                }
            }
        }
    }

    for profile in &profiles {
        for url in &profile.details.linked_urls {
            let matches_another = profiles.iter()
                .filter(|p| p.site_name != profile.site_name)
                .any(|p| url.contains(&p.site_url));
            if matches_another {
                signals.push(Signal::CrossLinked {
                    from_site: profile.site_name.clone(),
                    to_url: url.clone(),
                });
            }
        }
    }

    let locations: Vec<&str> = profiles.iter()
        .filter_map(|p| p.details.location.as_deref())
        .collect();
    if locations.len() >= 2 {
        let loc = mode_str(&locations);
        signals.push(Signal::SameLocation(loc.to_string()));
    }

    let websites: Vec<&str> = profiles.iter()
        .filter_map(|p| p.details.website.as_deref())
        .collect();
    let website_counts = websites.iter().fold(HashMap::new(), |mut m, &w| {
        *m.entry(w).or_insert(0usize) += 1;
        m
    });
    for (site, count) in &website_counts {
        if *count > 1 {
            signals.push(Signal::SameWebsite(site.to_string()));
        }
    }

    let emails: Vec<String> = profiles.iter()
        .flat_map(|p| p.details.emails.iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    for email in &emails {
        signals.push(Signal::SameEmail(email.clone()));
    }

    let name_strs: Vec<&str> = profiles.iter()
        .filter_map(|p| p.details.display_name.as_deref()).collect();
    let inferred_name = if !name_strs.is_empty() {
        Some(mode_str(&name_strs).to_string())
    } else {
        None
    };

    let loc_strs: Vec<&str> = profiles.iter()
        .filter_map(|p| p.details.location.as_deref()).collect();
    let inferred_location = if !loc_strs.is_empty() {
        Some(mode_str(&loc_strs).to_string())
    } else {
        None
    };

    let all_urls: Vec<String> = profiles.iter()
        .flat_map(|p| p.details.linked_urls.iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let phones: Vec<String> = profiles.iter()
        .flat_map(|p| p.details.phone_numbers.iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let confidence = calculate_confidence(&profiles);

    IdentityCluster {
        accounts: profiles,
        shared_signals: signals,
        inferred_name,
        inferred_location,
        emails_found: emails,
        phones_found: phones,
        all_linked_urls: all_urls,
        confidence,
    }
}

pub fn jaccard_similarity(a: &str, b: &str) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let normalize = |s: &str| {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(|w| w.to_string())
            .collect::<std::collections::HashSet<String>>()
    };

    let set_a = normalize(a);
    let set_b = normalize(b);

    let intersection = set_a.intersection(&set_b).count() as f32;
    let union = set_a.union(&set_b).count() as f32;

    if union == 0.0 { 0.0 } else { intersection / union }
}

pub fn find_similar_bios(
    profiles: &[ClaimedProfile],
    threshold: f32,
) -> Vec<(String, String, f32)> {
    let mut similar = Vec::new();

    for i in 0..profiles.len() {
        for j in (i + 1)..profiles.len() {
            if let (Some(bio_a), Some(bio_b)) = (
                profiles[i].details.bio.as_deref(),
                profiles[j].details.bio.as_deref(),
            ) {
                let sim = jaccard_similarity(bio_a, bio_b);
                if sim >= threshold {
                    similar.push((
                        profiles[i].site_name.clone(),
                        profiles[j].site_name.clone(),
                        sim,
                    ));
                }
            }
        }
    }

    similar
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProfileDetails;

    fn make_profile(site: &str, name: Option<&str>, bio: Option<&str>) -> ClaimedProfile {
        ClaimedProfile {
            site_name: site.to_string(),
            site_url: format!("https://{site}.com/user"),
            username: "test".to_string(),
            details: ProfileDetails {
                display_name: name.map(|s| s.to_string()),
                bio: bio.map(|s| s.to_string()),
                ..Default::default()
            },
            avatar_phash: None,
        }
    }

    #[test]
    fn test_confidence_empty() {
        assert_eq!(calculate_confidence(&[]), 0.0);
    }

    #[test]
    fn test_confidence_single() {
        let p = make_profile("GitHub", Some("John"), None);
        let c = calculate_confidence(&[p]);
        assert!(c > 0.0);
    }

    #[test]
    fn test_jaccard_identical() {
        assert!((jaccard_similarity("hello world", "hello world") - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_jaccard_empty() {
        assert_eq!(jaccard_similarity("", "hello"), 0.0);
    }

    #[test]
    fn test_jaccard_partial() {
        let sim = jaccard_similarity("I like rust", "I like python");
        assert!(sim > 0.0 && sim < 1.0);
    }

    #[test]
    fn test_jaccard_no_overlap() {
        assert_eq!(jaccard_similarity("abc def", "ghi jkl"), 0.0);
    }

    #[test]
    fn test_find_similar_bios() {
        let p1 = make_profile("GitHub", None, Some("Software developer from London"));
        let p2 = make_profile("Reddit", None, Some("Software developer from London"));
        let similar = find_similar_bios(&[p1, p2], 0.5);
        assert!(!similar.is_empty());
    }

    #[test]
    fn test_find_similar_bios_no_match() {
        let p1 = make_profile("GitHub", None, Some("I like cats"));
        let p2 = make_profile("Reddit", None, Some("I like dogs"));
        let similar = find_similar_bios(&[p1, p2], 0.8);
        assert!(similar.is_empty());
    }

    #[test]
    fn test_mode_str() {
        assert_eq!(mode_str(&["a", "b", "a"]), "a");
    }

    #[test]
    fn test_mode_str_empty() {
        assert_eq!(mode_str(&[]), "");
    }

    #[test]
    fn test_build_cluster_no_signals() {
        let p = make_profile("GitHub", None, None);
        let cluster = build_identity_cluster(vec![p]);
        assert!(cluster.shared_signals.is_empty());
    }

    #[test]
    fn test_build_cluster_name_signal() {
        let p1 = make_profile("GitHub", Some("John"), None);
        let p2 = make_profile("Reddit", Some("John"), None);
        let cluster = build_identity_cluster(vec![p1, p2]);
        let has_name = cluster.shared_signals.iter().any(|s| matches!(s, Signal::SameName(_)));
        assert!(has_name);
    }
}
