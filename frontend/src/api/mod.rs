//! API 客户端模块 - 统一 HTTP 客户端、JWT 注入、错误处理
//!
//! 所有函数统一返回 `Result<T, ApiError>`，错误信息包含 HTTP 状态码、
//! 后端 error_code（如有）和解析后的 message。

pub mod auth;
pub mod background_task;
pub mod finance;
pub mod generic_token_integration;
pub mod github_integration;
pub mod hr;
pub mod lark_integration;
pub mod log_stats;
pub mod message;
pub mod organization;
pub mod project;
pub mod seed;
pub mod system;

use common::api::ApiResponse;
use reqwest::{Client, Method, RequestBuilder};
use std::sync::OnceLock;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FormData, Request, RequestInit, Response};

use crate::config::current_config;

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

pub fn client() -> &'static Client {
    HTTP_CLIENT.get_or_init(Client::new)
}

fn build_request(method: Method, path: &str) -> RequestBuilder {
    let url = current_config().api_url(path);
    client().request(method, &url)
}

/// 401 处理：cookie 过期时清除登录态并重定向到登录页
pub(crate) fn handle_unauthorized(status: u16) {
    if status == 401 {
        crate::store::auth::clear_login_state();
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/login");
        }
    }
}

/// 从 reqwest 响应体解析 ApiError（提取 error_code 和 message）
fn parse_api_error_from_body(body_text: &str, http_status: u16) -> ApiError {
    let error_code = serde_json::from_str::<serde_json::Value>(body_text)
        .ok()
        .and_then(|v| {
            v.get("error_code")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        });
    let message = serde_json::from_str::<serde_json::Value>(body_text)
        .ok()
        .and_then(|v| {
            v.get("message")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| format!("HTTP {}", http_status));

    ApiError {
        http_status,
        error_code,
        message,
    }
}

pub(crate) async fn parse_error_response(resp: reqwest::Response) -> ApiError {
    let http_status = resp.status().as_u16();
    let body_text = resp.text().await.unwrap_or_default();
    parse_api_error_from_body(&body_text, http_status)
}

/// 网络层错误 → ApiError
pub(crate) fn network_err(e: reqwest::Error) -> ApiError {
    ApiError {
        http_status: 0,
        error_code: None,
        message: e.to_string(),
    }
}

pub async fn api_get<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let resp = build_request(Method::GET, path)
        .send()
        .await
        .map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: e.to_string(),
    })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    api_resp.data.ok_or_else(|| ApiError {
        http_status: 200,
        error_code: None,
        message: "响应数据为空".to_string(),
    })
}

pub async fn api_get_or_default<T: serde::de::DeserializeOwned + Default>(
    path: &str,
) -> Result<T, ApiError> {
    let resp = build_request(Method::GET, path)
        .send()
        .await
        .map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: e.to_string(),
    })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    Ok(api_resp.data.unwrap_or_default())
}

pub async fn api_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let resp = build_request(Method::POST, path)
        .json(body)
        .send()
        .await
        .map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: e.to_string(),
    })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    api_resp.data.ok_or_else(|| ApiError {
        http_status: 200,
        error_code: None,
        message: "响应数据为空".to_string(),
    })
}

pub async fn api_post_empty<B: serde::Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    let resp = build_request(Method::POST, path)
        .json(body)
        .send()
        .await
        .map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> =
        resp.json().await.map_err(|e| ApiError {
            http_status: 200,
            error_code: None,
            message: e.to_string(),
        })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    Ok(())
}

pub async fn api_put<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let resp = build_request(Method::PUT, path)
        .json(body)
        .send()
        .await
        .map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: e.to_string(),
    })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    api_resp.data.ok_or_else(|| ApiError {
        http_status: 200,
        error_code: None,
        message: "响应数据为空".to_string(),
    })
}

pub async fn api_put_empty<B: serde::Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    let resp = build_request(Method::PUT, path)
        .json(body)
        .send()
        .await
        .map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> =
        resp.json().await.map_err(|e| ApiError {
            http_status: 200,
            error_code: None,
            message: e.to_string(),
        })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    Ok(())
}

pub async fn api_patch<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let resp = build_request(Method::PATCH, path)
        .json(body)
        .send()
        .await
        .map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: e.to_string(),
    })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    api_resp.data.ok_or_else(|| ApiError {
        http_status: 200,
        error_code: None,
        message: "响应数据为空".to_string(),
    })
}

pub async fn api_delete(path: &str) -> Result<(), ApiError> {
    let resp = build_request(Method::DELETE, path)
        .send()
        .await
        .map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> =
        resp.json().await.map_err(|e| ApiError {
            http_status: 200,
            error_code: None,
            message: e.to_string(),
        })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ApiError {
    pub http_status: u16,
    pub error_code: Option<String>,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.error_code {
            Some(code) => write!(f, "{}: {}", code, self.message),
            None => write!(f, "HTTP {}: {}", self.http_status, self.message),
        }
    }
}

pub async fn api_get_text(path: &str) -> Result<String, ApiError> {
    let url = current_config().api_url(path);
    let resp = client().get(&url).send().await.map_err(network_err)?;
    let status = resp.status();
    if !status.is_success() {
        handle_unauthorized(status.as_u16());
        return Err(parse_error_response(resp).await);
    }
    resp.text().await.map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: e.to_string(),
    })
}

/// 拉取前端静态资源文本（相对页面 origin，如 /docs/index.json）。
///
/// 与 api_get_text 不同：不拼 API base URL，直接使用当前页面 window.origin
/// 拼绝对路径后通过 web_sys fetch 拉取（reqwest wasm 下对相对路径处理不稳定）。
pub async fn fetch_static_text(path: &str) -> Result<String, ApiError> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, Response};

    let window = web_sys::window().ok_or_else(|| ApiError {
        http_status: 0,
        error_code: None,
        message: "未找到 window 对象".to_string(),
    })?;
    let origin = window.location().origin().map_err(|_| ApiError {
        http_status: 0,
        error_code: None,
        message: "获取当前 origin 失败".to_string(),
    })?;
    let abs_path = if path.starts_with('/') {
        format!("{}{}", origin.trim_end_matches('/'), path)
    } else {
        format!("{}/{}", origin.trim_end_matches('/'), path)
    };

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_credentials(web_sys::RequestCredentials::SameOrigin);

    let request = Request::new_with_str_and_init(&abs_path, &opts).map_err(|e| ApiError {
        http_status: 0,
        error_code: None,
        message: format!("构造静态资源 Request 失败: {:?}", e),
    })?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| ApiError {
            http_status: 0,
            error_code: None,
            message: format!("静态资源网络请求失败: {:?}", e),
        })?;
    let resp: Response = resp_value.dyn_into().map_err(|_| ApiError {
        http_status: 0,
        error_code: None,
        message: "响应转换失败".to_string(),
    })?;

    let status = resp.status();
    if !(200..300).contains(&status) {
        return Err(ApiError {
            http_status: status,
            error_code: None,
            message: format!("静态资源请求失败: {} {}", status, path),
        });
    }

    let text_promise = resp.text().map_err(|e| ApiError {
        http_status: status,
        error_code: None,
        message: format!("获取响应文本失败: {:?}", e),
    })?;
    let text = JsFuture::from(text_promise).await.map_err(|e| ApiError {
        http_status: status,
        error_code: None,
        message: format!("读取响应文本失败: {:?}", e),
    })?;
    Ok(text.as_string().unwrap_or_default())
}

pub async fn api_post_multipart<T: serde::de::DeserializeOwned>(
    path: &str,
    form: FormData,
) -> Result<T, ApiError> {
    let url = current_config().api_url(path);

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(form.unchecked_ref());
    opts.set_credentials(web_sys::RequestCredentials::SameOrigin);

    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| ApiError {
        http_status: 0,
        error_code: None,
        message: format!("构造 Request 失败: {:?}", e),
    })?;

    let window = web_sys::window().ok_or_else(|| ApiError {
        http_status: 0,
        error_code: None,
        message: "未找到 window 对象".to_string(),
    })?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| ApiError {
            http_status: 0,
            error_code: None,
            message: format!("fetch 失败: {:?}", e),
        })?;

    let resp: Response = resp_value.dyn_into().map_err(|e| ApiError {
        http_status: 0,
        error_code: None,
        message: format!("Response 转换失败: {:?}", e),
    })?;

    let status = resp.status();
    if !resp.ok() {
        handle_unauthorized(status);
        // 尝试读取 body 解析错误信息
        if let Ok(json_promise) = resp.text()
            && let Ok(body_value) = JsFuture::from(json_promise).await
            && let Some(body_text) = body_value.as_string()
        {
            return Err(parse_api_error_from_body(&body_text, status));
        }
        return Err(ApiError {
            http_status: status,
            error_code: None,
            message: format!("HTTP {}", status),
        });
    }

    let json_promise = resp.json().map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: format!("json() 失败: {:?}", e),
    })?;
    let json_value = JsFuture::from(json_promise).await.map_err(|e| ApiError {
        http_status: 200,
        error_code: None,
        message: format!("JSON 解析失败: {:?}", e),
    })?;

    let api_resp: ApiResponse<T> =
        serde_wasm_bindgen::from_value(json_value).map_err(|e| ApiError {
            http_status: 200,
            error_code: None,
            message: format!("反序列化 ApiResponse 失败: {}", e),
        })?;

    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    api_resp.data.ok_or_else(|| ApiError {
        http_status: 200,
        error_code: None,
        message: "响应数据为空".to_string(),
    })
}

/// 构造分页 URL：把 `PaginationParams` 序列化为 query string 附加到 base_url
pub fn build_pagination_url(base_url: &str, pagination: &common::api::PaginationParams) -> String {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = pagination.limit {
        params.push(format!("limit={}", l));
    }
    if let Some(o) = pagination.offset {
        params.push(format!("offset={}", o));
    }
    if params.is_empty() {
        base_url.to_string()
    } else {
        format!("{}?{}", base_url, params.join("&"))
    }
}

/// 构造 query string：从 `&[(&str, Option<String>)]` 过滤 None 后拼接
pub fn build_query_string(params: &[(&str, Option<String>)]) -> String {
    let pairs: Vec<String> = params
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|val| format!("{}={}", k, val)))
        .collect();
    if pairs.is_empty() {
        String::new()
    } else {
        format!("?{}", pairs.join("&"))
    }
}
