//! API 客户端模块 - 统一 HTTP 客户端、JWT 注入、错误处理

pub mod auth;
pub mod finance;
pub mod hr;
pub mod message;
pub mod organization;
pub mod project;
pub mod system;

use common::api::ApiResponse;
use reqwest::{Client, Method, RequestBuilder};
use std::sync::OnceLock;

use crate::config::current_config;

/// 全局 HTTP 客户端单例（复用连接池）
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

/// 获取全局 HTTP 客户端
pub fn client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| Client::new())
}

/// 从 localStorage 获取 JWT token
fn get_token() -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok()??;
    storage.get("ai_orz_token").ok()?
}

/// 构建带 JWT 的请求
fn build_request(method: Method, path: &str) -> RequestBuilder {
    let url = current_config().api_url(path);
    let req = client().request(method, &url);
    match get_token() {
        Some(token) if !token.is_empty() => req.bearer_auth(&token),
        _ => req,
    }
}

/// 发送 GET 请求并解析 ApiResponse<T>
pub async fn api_get<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let resp = build_request(Method::GET, path).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}

/// 发送 GET 请求，返回可选数据（用于列表可能为空的场景）
pub async fn api_get_or_default<T: serde::de::DeserializeOwned + Default>(path: &str) -> Result<T, String> {
    let resp = build_request(Method::GET, path).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    Ok(api_resp.data.unwrap_or_default())
}

/// 发送 POST 请求
pub async fn api_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(path: &str, body: &B) -> Result<T, String> {
    let resp = build_request(Method::POST, path).json(body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}

/// 发送 POST 请求（无响应体）
pub async fn api_post_empty<B: serde::Serialize>(path: &str, body: &B) -> Result<(), String> {
    let resp = build_request(Method::POST, path).json(body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    Ok(())
}

/// 发送 PUT 请求
pub async fn api_put<T: serde::de::DeserializeOwned, B: serde::Serialize>(path: &str, body: &B) -> Result<T, String> {
    let resp = build_request(Method::PUT, path).json(body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}

/// 发送 PUT 请求（无响应体）
pub async fn api_put_empty<B: serde::Serialize>(path: &str, body: &B) -> Result<(), String> {
    let resp = build_request(Method::PUT, path).json(body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    Ok(())
}

/// 发送 DELETE 请求
pub async fn api_delete(path: &str) -> Result<(), String> {
    let resp = build_request(Method::DELETE, path).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    Ok(())
}

/// 发送纯文本 GET 请求（用于 /health 等非标准 API）
pub async fn api_get_text(path: &str) -> Result<String, String> {
    let url = current_config().api_url(path);
    let resp = client().get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}
