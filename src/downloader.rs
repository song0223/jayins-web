use anyhow::{Context, Result};
use reqwest::Client;
use std::path::Path;
use tokio::fs;

/// 从 URL 下载帖子图片
pub async fn download_images_from_url(
    url: &str,
    cookie: &str,
    csrf_token: &str,
    save_dir: &str,
) -> Result<Vec<String>> {
    let shortcode = extract_shortcode(url)
        .context("无法从链接提取 shortcode")?;

    let client = build_client(cookie, csrf_token)?;

    // 通过 GraphQL API 获取帖子数据
    let (images, caption) = fetch_post_data(&client, &shortcode).await?;

    if images.is_empty() {
        anyhow::bail!("未找到图片");
    }

    // 下载图片
    let save_path = Path::new(save_dir);
    ensure_dir(save_path)?;

    let mut downloaded = Vec::new();
    for (i, img_url) in images.iter().enumerate() {
        let filename = format!("{}_{}{}.jpg", shortcode, i + 1, "");
        let filepath = save_path.join(&filename);

        match download_single(&client, img_url, &filepath).await {
            Ok(size) => {
                downloaded.push(filename);
            }
            Err(e) => {
                eprintln!("下载失败 {}: {}", filename, e);
            }
        }
    }

    Ok(downloaded)
}

/// 构建 HTTP 客户端
fn build_client(cookie: &str, csrf_token: &str) -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("X-CSRFToken", csrf_token.parse().unwrap());
    headers.insert("X-IG-App-ID", "936619743392459".parse().unwrap());
    headers.insert("X-Requested-With", "XMLHttpRequest".parse().unwrap());
    headers.insert("Accept", "application/json".parse().unwrap());
    headers.insert("Referer", "https://www.instagram.com/".parse().unwrap());
    headers.insert("Cookie", cookie.parse().unwrap());

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .default_headers(headers)
        .build()?;

    Ok(client)
}

/// 通过 GraphQL API 获取帖子数据
async fn fetch_post_data(client: &Client, shortcode: &str) -> Result<(Vec<String>, String)> {
    let variables = serde_json::json!({"shortcode": shortcode});
    let form = [
        ("variables", variables.to_string()),
        ("doc_id", "8845758582119845".to_string()),
    ];

    let resp = client
        .post("https://www.instagram.com/graphql/query/")
        .form(&form)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("API 请求失败: {}", resp.status());
    }

    let data: serde_json::Value = resp.json().await?;

    if let Some(errors) = data.get("errors") {
        anyhow::bail!("API 返回错误: {}", errors);
    }

    let media = data
        .pointer("/data/xdt_shortcode_media")
        .context("无法解析帖子数据")?;

    let mut images = Vec::new();

    // 轮播帖
    if let Some(edges) = media
        .get("edge_sidecar_to_children")
        .and_then(|v| v.get("edges"))
        .and_then(|v| v.as_array())
    {
        for edge in edges {
            if let Some(node) = edge.get("node") {
                let is_video = node.get("is_video").and_then(|v| v.as_bool()).unwrap_or(false);
                if !is_video {
                    if let Some(display_url) = node.get("display_url").and_then(|v| v.as_str()) {
                        images.push(display_url.to_string());
                    }
                }
            }
        }
    } else {
        // 单图帖
        let is_video = media.get("is_video").and_then(|v| v.as_bool()).unwrap_or(false);
        if !is_video {
            if let Some(display_url) = media.get("display_url").and_then(|v| v.as_str()) {
                images.push(display_url.to_string());
            }
        }
    }

    // 提取文案
    let caption = media
        .pointer("/edge_media_to_caption/edges/0/node/text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok((images, caption))
}

/// 从链接中提取 shortcode
fn extract_shortcode(url: &str) -> Option<String> {
    let re = regex::Regex::new(r"instagram\.com/(?:p|reel|tv)/([A-Za-z0-9_-]+)").ok()?;
    re.captures(url)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

/// 确保目录存在
fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// 下载单个图片
async fn download_single(client: &Client, url: &str, path: &Path) -> Result<u64> {
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }

    let bytes = resp.bytes().await?;
    let size = bytes.len() as u64;

    if size < 1000 {
        anyhow::bail!("文件太小 ({} bytes)", size);
    }

    fs::write(path, &bytes).await?;
    Ok(size)
}
