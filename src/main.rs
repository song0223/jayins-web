use axum::{
    extract::State,
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;

mod downloader;
mod profile;

/// 默认 Cookie（自动续期）
const DEFAULT_COOKIE: &str = "ds_user_id=6009511404; csrftoken=en2hyrbjkI3AjRBUKDUPcaLyNsGYhocx; wd=1671x626;sessionid=6009511404%3AFCWfjPPZclOaRK%3A13%3AAYi9DXyR3mqXBKwXoY6hYslKzF6HJR470-N-OHJmKQ";

/// 应用状态
#[derive(Clone)]
struct AppState {
    cookie: Arc<Mutex<String>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            cookie: Arc::new(Mutex::new(DEFAULT_COOKIE.to_string())),
        }
    }
}

/// Cookie 更新请求
#[derive(Deserialize)]
struct CookieRequest {
    cookie: String,
    csrf_token: Option<String>,
}

/// 主页帖子请求
#[derive(Deserialize)]
struct ProfileRequest {
    url: String,
}

/// 下载请求
#[derive(Deserialize)]
struct DownloadRequest {
    url: String,
}

/// 下载的图片
#[derive(Serialize, Clone)]
struct DownloadedImage {
    filename: String,
    url: String,
    data: String, // base64
}

/// API 响应
#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

/// 服务状态
#[derive(Serialize)]
struct StatusResponse {
    status: String,
    cookie_configured: bool,
    version: String,
}

#[tokio::main]
async fn main() {
    let state = AppState::new();

    let app = Router::new()
        // Web 界面
        .route("/", get(index_page))
        // API 端点
        .route("/api/status", get(status))
        .route("/api/cookie", post(update_cookie))
        .route("/api/profile", post(get_profile))
        .route("/api/download", post(download_post))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🚀 Jayins Web 服务启动: http://0.0.0.0:3000");
    println!("   访问地址: https://kuaishu.xin/ins");
    axum::serve(listener, app).await.unwrap();
}

/// Web 管理界面
async fn index_page() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

/// 服务状态
async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let cookie = state.cookie.lock().unwrap();
    Json(StatusResponse {
        status: "running".to_string(),
        cookie_configured: !cookie.is_empty(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// 更新 Cookie
async fn update_cookie(
    State(state): State<AppState>,
    Json(req): Json<CookieRequest>,
) -> Json<ApiResponse> {
    let mut cookie = state.cookie.lock().unwrap();
    *cookie = req.cookie.clone();

    Json(ApiResponse {
        success: true,
        message: "Cookie 已更新".to_string(),
        data: None,
    })
}

/// 获取主页帖子
async fn get_profile(
    State(state): State<AppState>,
    Json(req): Json<ProfileRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    let cookie = state.cookie.lock().unwrap().clone();

    match profile::fetch_profile_posts(&req.url, &cookie).await {
        Ok(posts) => {
            let data = serde_json::to_value(&posts).unwrap_or_default();
            Ok(Json(ApiResponse {
                success: true,
                message: format!("找到 {} 个帖子", posts.len()),
                data: Some(data),
            }))
        }
        Err(e) => Ok(Json(ApiResponse {
            success: false,
            message: format!("获取失败: {}", e),
            data: None,
        })),
    }
}

/// 下载帖子图片（返回 base64 数据）
async fn download_post(
    State(state): State<AppState>,
    Json(req): Json<DownloadRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    let cookie = state.cookie.lock().unwrap().clone();

    match downloader::fetch_and_download(&req.url, &cookie).await {
        Ok(images) => Ok(Json(ApiResponse {
            success: true,
            message: format!("下载完成，共 {} 张图片", images.len()),
            data: Some(serde_json::json!({ "images": images })),
        })),
        Err(e) => Ok(Json(ApiResponse {
            success: false,
            message: format!("下载失败: {}", e),
            data: None,
        })),
    }
}
