use anyhow::{Context, Result};
use serde::Serialize;

/// 帖子信息
#[derive(Debug, Clone, Serialize)]
pub struct ProfilePost {
    pub url: String,
    pub id: String,
    pub cover_url: String,
    pub is_video: bool,
}

/// 通过 Instagram API 获取主页帖子
pub async fn fetch_profile_posts(profile_url: &str, cookie: &str) -> Result<Vec<ProfilePost>> {
    let username = extract_username(profile_url)
        .context("无法从链接提取用户名")?;

    let client = reqwest::Client::new();

    // 获取用户 ID
    let api_url = format!("https://www.instagram.com/api/v1/users/web_profile_info/?username={}", username);
    let resp = client
        .get(&api_url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .header("X-IG-App-ID", "936619743392459")
        .header("Cookie", cookie)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("获取用户信息失败: {}", resp.status());
    }

    let data: serde_json::Value = resp.json().await?;

    let user_id = data.pointer("/data/user/id")
        .and_then(|v| v.as_str())
        .context("无法获取用户 ID")?;

    // 使用移动端 API 获取帖子
    let feed_url = format!("https://www.instagram.com/api/v1/feed/user/{}/?count=12", user_id);
    let resp = client
        .get(&feed_url)
        .header("User-Agent", "Instagram 275.0.0.27.98 Android (33/13; 420dpi; 1080x2400; samsung; SM-G991B; o1s; exynos2100; en_US; 458229258)")
        .header("X-IG-App-ID", "936619743392459")
        .header("Cookie", cookie)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("获取帖子列表失败: {}", resp.status());
    }

    let data: serde_json::Value = resp.json().await?;
    let items = data.get("items")
        .and_then(|v| v.as_array())
        .context("无法获取帖子列表")?;

    let mut posts = Vec::new();
    for item in items {
        let code = item.get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let media_type = item.get("media_type")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        let cover_url = item.pointer("/image_versions2/candidates/0/url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let is_video = media_type == 2;
        let url = if is_video {
            format!("https://www.instagram.com/reel/{}/", code)
        } else {
            format!("https://www.instagram.com/p/{}/", code)
        };

        posts.push(ProfilePost {
            url,
            id: code.to_string(),
            cover_url,
            is_video,
        });
    }

    Ok(posts)
}

/// 从链接中提取用户名
fn extract_username(url: &str) -> Option<String> {
    let re = regex::Regex::new(r"instagram\.com/([A-Za-z0-9._]+)/?").ok()?;
    re.captures(url)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}
