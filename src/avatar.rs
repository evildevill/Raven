use image::{DynamicImage, imageops::FilterType};
use reqwest::Client;

pub async fn download_and_phash(client: &Client, url: &str) -> Option<u64> {
    let bytes = client
        .get(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .bytes()
        .await
        .ok()?;

    let img = image::load_from_memory(&bytes).ok()?;
    Some(compute_phash(&img))
}

pub fn compute_phash(img: &DynamicImage) -> u64 {
    let small = img
        .resize_exact(8, 8, FilterType::Lanczos3)
        .grayscale()
        .to_luma8();

    let pixels: Vec<u8> = small.pixels().map(|p| p.0[0]).collect();
    let sum: u64 = pixels.iter().map(|&p| p as u64).sum();
    let mean = sum / 64;

    pixels.iter().enumerate().fold(0u64, |hash, (i, &p)| {
        if p as u64 > mean {
            hash | (1u64 << i)
        } else {
            hash
        }
    })
}

#[allow(dead_code)]
pub fn phash_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

use crate::types::ClaimedProfile;

pub async fn find_avatar_matches(
    client: &Client,
    profiles: &mut Vec<ClaimedProfile>,
) {
    let futures: Vec<_> = profiles.iter()
        .filter_map(|p| p.details.avatar_url.as_deref().map(|url| {
            let client = client.clone();
            let url = url.to_string();
            async move { download_and_phash(&client, &url).await }
        }))
        .collect();

    let hashes = futures::future::join_all(futures).await;

    for (profile, hash) in profiles.iter_mut().zip(hashes.iter()) {
        profile.avatar_phash = *hash;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phash_same_image() {
        let img = DynamicImage::new_luma8(16, 16);
        let h1 = compute_phash(&img);
        let h2 = compute_phash(&img);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_phash_distance_identity() {
        let img = DynamicImage::new_luma8(16, 16);
        let h = compute_phash(&img);
        assert_eq!(phash_distance(h, h), 0);
    }

    #[test]
    fn test_phash_distance_different() {
        // Create a checkerboard-like pattern vs a solid image
        let mut img1 = DynamicImage::new_luma8(16, 16);
        let mut img2 = DynamicImage::new_luma8(16, 16);
        if let Some(buf) = img1.as_mut_luma8() {
            for (i, p) in buf.pixels_mut().enumerate() {
                p.0[0] = if i % 2 == 0 { 255 } else { 0 };
            }
        }
        if let Some(buf) = img2.as_mut_luma8() {
            for p in buf.pixels_mut() {
                p.0[0] = 128;
            }
        }
        let h1 = compute_phash(&img1);
        let h2 = compute_phash(&img2);
        assert!(phash_distance(h1, h2) > 0, "expected different hashes, got {h1} vs {h2}");
    }
}
