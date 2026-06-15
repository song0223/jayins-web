use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;

/// 下载的图片
#[derive(Serialize, Clone)]
pub struct DownloadedImage {
    pub filename: String,
    pub url: String,
    pub data: String, // base64
    pub size: u64,
}

/// 获取并下载帖子图片（返回 base64 数据）
pub async fn fetch_and_download(url: &str, cookie: &str) -> Result<Vec<DownloadedImage>> {
    let shortcode = extract_shortcode(url)
        .context("无法从链接提取 shortcode")?;

    let client = build_client(cookie)?;

    // 通过嵌入页面获取帖子图片 URL
    let (image_urls, _caption) = fetch_post_data(&client, &shortcode).await?;

    if image_urls.is_empty() {
        anyhow::bail!("未找到图片");
    }

    // 下载图片并转为 base64
    let mut images = Vec::new();
    for (i, img_url) in image_urls.iter().enumerate() {
        let ext = guess_extension(img_url);
        let filename = format!("{}_{}{}", shortcode, i + 1, ext);

        match download_as_base64(&client, img_url).await {
            Ok((data, size)) => {
                images.push(DownloadedImage {
                    filename,
                    url: img_url.clone(),
                    data,
                    size,
                });
            }
            Err(e) => {
                eprintln!("下载失败 {}: {}", filename, e);
            }
        }
    }

    Ok(images)
}

/// 构建 HTTP 客户端
fn build_client(cookie: &str) -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("X-IG-App-ID", "936619743392459".parse().unwrap());
    headers.insert("Referer", "https://www.instagram.com/".parse().unwrap());
    headers.insert("Cookie", cookie.parse().unwrap());

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .default_headers(headers)
        .build()?;

    Ok(client)
}

/// 通过 GraphQL API 获取帖子图片 URL
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
        anyhow::bail!("GraphQL API 请求失败: {}", resp.status());
    }

    let text = resp.text().await?;
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("解析 JSON 失败: {}", e))?;

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

/// 下载图片并转为 base64
async fn download_as_base64(client: &Client, url: &str) -> Result<(String, u64)> {
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.starts_with("image/") {
        anyhow::bail!("不是图片: {}", content_type);
    }

    let bytes = resp.bytes().await?;
    let size = bytes.len() as u64;

    if size < 1000 {
        anyhow::bail!("文件太小");
    }

    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);

    Ok((data, size))
}

/// 从链接中提取 shortcode
fn extract_shortcode(url: &str) -> Option<String> {
    let re = regex::Regex::new(r"instagram\.com/(?:p|reel|tv)/([A-Za-z0-9_-]+)").ok()?;
    re.captures(url)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

/// 猜测文件扩展名
fn guess_extension(url: &str) -> &str {
    if url.contains(".png") { ".png" }
    else if url.contains(".webp") { ".webp" }
    else { ".jpg" }
}
