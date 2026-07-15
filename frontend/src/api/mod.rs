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
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FormData, Request, RequestInit, Response};

use crate::config::current_config;

/// 全局 HTTP 客户端单例（复用连接池）
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

/// 获取全局 HTTP 客户端
pub fn client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| Client::new())
}

/// 构建请求（同源请求自动携带 Cookie）
fn build_request(method: Method, path: &str) -> RequestBuilder {
    let url = current_config().api_url(path);
    client().request(method, &url)
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

/// 发送 multipart/form-data 上传请求（仅在 wasm32 目标下使用）
///
/// wasm32 目标下，reqwest 的 Body 不支持直接传入 FormData，
/// 因此使用浏览器原生 fetch API（同源请求自动携带 Cookie）。
pub async fn api_post_multipart<T: serde::de::DeserializeOwned>(
    path: &str,
    form: FormData,
) -> Result<T, String> {
    let url = current_config().api_url(path);

    // 构造 RequestInit，body 直接传 FormData（浏览器 fetch 会自动识别 multipart 并设置 boundary）
    let mut opts = RequestInit::new();
    opts.method("POST");
    opts.body(Some(form.unchecked_ref()));
    // 同源请求自动携带 Cookie
    opts.credentials(web_sys::RequestCredentials::SameOrigin);

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("构造 Request 失败: {:?}", e))?;

    let window = web_sys::window().ok_or_else(|| "未找到 window 对象".to_string())?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("fetch 失败: {:?}", e))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|e| format!("Response 转换失败: {:?}", e))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let json_promise = resp
        .json()
        .map_err(|e| format!("json() 失败: {:?}", e))?;
    let json_value = JsFuture::from(json_promise)
        .await
        .map_err(|e| format!("JSON 解析失败: {:?}", e))?;

    // 用 serde_wasm_bindgen 反序列化 JsValue 到 ApiResponse<T>
    let api_resp: ApiResponse<T> = serde_wasm_bindgen::from_value(json_value)
        .map_err(|e| format!("反序列化 ApiResponse 失败: {}", e))?;

    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}
