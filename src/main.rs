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

/// 应用状态
#[derive(Clone)]
struct AppState {
    cookie: Arc<Mutex<String>>,
    csrf_token: Arc<Mutex<String>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            cookie: Arc::new(Mutex::new(String::new())),
            csrf_token: Arc::new(Mutex::new(String::new())),
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
    save_dir: Option<String>,
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

    if let Some(csrf) = req.csrf_token {
        let mut csrf_token = state.csrf_token.lock().unwrap();
        *csrf_token = csrf;
    }

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

/// 下载帖子图片
async fn download_post(
    State(state): State<AppState>,
    Json(req): Json<DownloadRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    let cookie = state.cookie.lock().unwrap().clone();
    let csrf_token = state.csrf_token.lock().unwrap().clone();
    let save_dir = req.save_dir.unwrap_or_else(|| {
        dirs_next::download_dir()
            .map(|p| p.join("jayins").to_string_lossy().to_string())
            .unwrap_or_else(|| "downloads".to_string())
    });

    match downloader::download_images_from_url(&req.url, &cookie, &csrf_token, &save_dir).await {
        Ok(downloaded) => Ok(Json(ApiResponse {
            success: true,
            message: format!("下载完成，共 {} 张图片", downloaded.len()),
            data: Some(serde_json::json!({
                "files": downloaded,
                "save_dir": save_dir
            })),
        })),
        Err(e) => Ok(Json(ApiResponse {
            success: false,
            message: format!("下载失败: {}", e),
            data: None,
        })),
    }
}
