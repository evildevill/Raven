use reqwest::Client;

use crate::types::ProfileDetails;

pub async fn enrich_profile(
    client: &Client,
    site_name: &str,
    username: &str,
    profile: &mut ProfileDetails,
) {
    match site_name {
        "GitHub" => enrich_github(client, username, profile).await,
        "Reddit" => enrich_reddit(client, username, profile).await,
        "HackerNews" => enrich_hackernews(client, username, profile).await,
        "Dev.to" => enrich_devto(client, username, profile).await,
        "Keybase" => enrich_keybase(client, username, profile).await,
        _ => {}
    }
}

async fn enrich_github(client: &Client, username: &str, profile: &mut ProfileDetails) {
    let url = format!("https://api.github.com/users/{username}");
    if let Ok(resp) = client
        .get(&url)
        .header("User-Agent", "raven-osint")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            // API data overrides scraped OG data (API is more reliable)
            if let Some(name) = data["name"].as_str().map(|s| s.to_string()) {
                profile.display_name = Some(name);
            }
            if let Some(bio) = data["bio"].as_str().map(|s| s.to_string()) {
                profile.bio = Some(bio);
            }
            if let Some(loc) = data["location"].as_str().map(|s| s.to_string()) {
                profile.location = Some(loc);
            }
            if let Some(blog) = data["blog"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string()) {
                profile.website = Some(blog);
            }
            profile.followers = data["followers"].as_u64().map(|n| n.to_string());
            profile.following = data["following"].as_u64().map(|n| n.to_string());
            if let Some(date) = data["created_at"].as_str().map(|s| s.to_string()) {
                profile.joined_date = Some(date);
            }
            if let Some(avatar) = data["avatar_url"].as_str().map(|s| s.to_string()) {
                profile.avatar_url = Some(avatar);
            }

            if let Some(email) = data["email"].as_str() {
                if !email.is_empty() && !profile.emails.contains(&email.to_string()) {
                    profile.emails.push(email.to_string());
                }
            }

            profile.extra.insert("public_repos".to_string(),
                data["public_repos"].as_u64().unwrap_or(0).to_string());
            profile.extra.insert("public_gists".to_string(),
                data["public_gists"].as_u64().unwrap_or(0).to_string());
        }
    }
}

async fn enrich_reddit(client: &Client, username: &str, profile: &mut ProfileDetails) {
    let url = format!("https://www.reddit.com/user/{username}/about.json");
    if let Ok(resp) = client
        .get(&url)
        .header("User-Agent", "raven-osint/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            let d = &data["data"];
            if let Some(icon) = d["icon_img"].as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.split('?').next().unwrap_or(s).to_string())
            {
                profile.avatar_url = Some(icon);
            }
            if let Some(ts) = d["created_utc"].as_f64()
                .and_then(|ts| chrono::DateTime::from_timestamp(ts as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d").to_string()))
            {
                profile.joined_date = Some(ts);
            }
            profile.extra.insert("link_karma".to_string(),
                d["link_karma"].as_i64().unwrap_or(0).to_string());
            profile.extra.insert("comment_karma".to_string(),
                d["comment_karma"].as_i64().unwrap_or(0).to_string());
            profile.verified = d["is_employee"].as_bool();
        }
    }
}

async fn enrich_hackernews(client: &Client, username: &str, profile: &mut ProfileDetails) {
    let url = format!("https://hacker-news.firebaseio.com/v0/user/{username}.json");
    if let Ok(resp) = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(about) = data["about"].as_str().map(|s| s.to_string()) {
                profile.bio = Some(about);
            }
            if let Some(date) = data["created"].as_u64()
                .and_then(|ts| chrono::DateTime::from_timestamp(ts as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d").to_string()))
            {
                profile.joined_date = Some(date);
            }
            profile.extra.insert("karma".to_string(),
                data["karma"].as_i64().unwrap_or(0).to_string());
        }
    }
}

async fn enrich_devto(client: &Client, username: &str, profile: &mut ProfileDetails) {
    let url = format!("https://dev.to/api/users/by_username?url={username}");
    if let Ok(resp) = client
        .get(&url)
        .header("User-Agent", "raven-osint/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(name) = data["name"].as_str().map(|s| s.to_string()) {
                profile.display_name = Some(name);
            }
            if let Some(summary) = data["summary"].as_str().map(|s| s.to_string()) {
                profile.bio = Some(summary);
            }
            if let Some(loc) = data["location"].as_str().map(|s| s.to_string()) {
                profile.location = Some(loc);
            }
            if let Some(date) = data["joined_at"].as_str().map(|s| s.to_string()) {
                profile.joined_date = Some(date);
            }
            if let Some(img) = data["profile_image"].as_str().map(|s| s.to_string()) {
                profile.avatar_url = Some(img);
            }
            if let Some(web) = data["website_url"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string()) {
                profile.website = Some(web);
            }
            profile.followers = data["followers_count"].as_u64().map(|n| n.to_string());
            profile.extra.insert("github_username".to_string(),
                data["github_username"].as_str().unwrap_or("").to_string());
            profile.extra.insert("twitter_username".to_string(),
                data["twitter_username"].as_str().unwrap_or("").to_string());
        }
    }
}

async fn enrich_keybase(client: &Client, username: &str, profile: &mut ProfileDetails) {
    let url = format!("https://keybase.io/_/api/1.0/user/lookup.json?usernames={username}");
    if let Ok(resp) = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(them) = data["them"].as_array().and_then(|a| a.first()) {
                if let Some(proofs) = them["proofs_summary"]["all"].as_array() {
                    for proof in proofs {
                        if let (Some(proof_type), Some(proof_url)) = (
                            proof["proof_type"].as_str(),
                            proof["service_url"].as_str(),
                        ) {
                            profile.extra.insert(
                                format!("keybase_proof_{proof_type}"),
                                proof_url.to_string(),
                            );
                            profile.linked_urls.push(proof_url.to_string());
                        }
                    }
                }
                if let Some(pic) = them["pictures"]["primary"]["url"].as_str().map(|s| s.to_string()) {
                    profile.avatar_url = Some(pic);
                }
            }
        }
    }
}
