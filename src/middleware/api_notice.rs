//! API Notice 中间件
//!
//! 请求结束时打印一条 notice 日志，包含请求关键信息 + 请求/响应体预览，
//! 便于后续排查问题（复用 tracing 统一日志系统，见 docs/design/logging_design.md）。
//!
//! 设计约束：
//! - 仅覆盖后端接口面（`/api/` 与 `/a2a` 前缀）；静态资源与 `/health` 探活不打
//! - 单条日志落在请求结束时（含 status / duration_ms / log_id），log_id 取自响应头
//!   （由 request_context_middleware 写回），可与业务日志链路关联
//! - body 只在「content-length 已知且 ≤ 上限」时缓冲预览——SSE 长连接（无 content-length）
//!   与大文件上传/下载天然跳过，不会阻塞或丢失流式 body
//! - 日志属于内部数据，不做脱敏（边界决策 2026-09-03：系统内部不脱敏，仅对外
//!   接口出口用 `redact!` 宏脱敏），风险由日志访问控制承担

use axum::{
    body::Body,
    http::{Request, header},
    middleware::Next,
    response::Response,
};

/// body 缓冲上限（字节）：超过此大小的请求/响应体不打印内容，只记大小
const MAX_BODY_LOG_BYTES: usize = 4 * 1024;

/// 预览字符串截断长度（字符）：脱敏后再截断，防止极端长 JSON 刷屏
const MAX_PREVIEW_CHARS: usize = 2048;

/// 是否为需要打 notice 日志的后端接口路径
fn is_api_path(path: &str) -> bool {
    path.starts_with("/api/") || path.starts_with("/a2a")
}

/// 读取 content-length；缺失（流式/chunked）返回 None
fn content_length(headers: &axum::http::HeaderMap) -> Option<usize> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
}

/// 判断 content-type 是否为文本类（可读预览）
fn is_text_content_type(content_type: Option<&str>) -> bool {
    let Some(ct) = content_type else {
        return false;
    };
    ct.contains("application/json")
        || ct.contains("text/")
        || ct.contains("javascript")
        || ct.contains("x-www-form-urlencoded")
}

/// 截断预览字符串，超长部分以标记结尾
fn truncate_preview(s: &str) -> String {
    if s.chars().count() <= MAX_PREVIEW_CHARS {
        s.to_string()
    } else {
        let cut: String = s.chars().take(MAX_PREVIEW_CHARS).collect();
        format!("{}…<truncated>", cut)
    }
}

/// 生成 body 预览：文本类内容截断后直接输出（内部日志不做脱敏）
fn body_preview(bytes: &[u8], content_type: Option<&str>) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if is_text_content_type(content_type) {
        truncate_preview(&String::from_utf8_lossy(bytes))
    } else {
        format!(
            "<binary {} bytes, content-type={}>",
            bytes.len(),
            content_type.unwrap_or("unknown")
        )
    }
}

/// API Notice 中间件
///
/// 请求结束时打印一条 notice 日志：method / path / status / duration_ms /
/// log_id（响应头）+ 请求/响应体预览（限长 + 脱敏）。
pub async fn api_notice_middleware(req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path().to_string();
    if !is_api_path(&path) {
        return next.run(req).await;
    }

    let method = req.method().clone();
    let query = req.uri().query().unwrap_or("").to_string();
    let start = std::time::Instant::now();

    // ---- 请求体预览：仅在 content-length 已知且 ≤ 上限时缓冲（流式/大请求原样透传）----
    let (parts, body) = req.into_parts();
    let req_ct = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let req_len = content_length(&parts.headers);

    let (req_preview, request) = if req_len.is_some_and(|l| l <= MAX_BODY_LOG_BYTES) {
        match axum::body::to_bytes(body, MAX_BODY_LOG_BYTES).await {
            Ok(bytes) => {
                let preview = body_preview(&bytes, req_ct.as_deref());
                (preview, Request::from_parts(parts, Body::from(bytes)))
            }
            Err(_) => (
                "<body read failed>".to_string(),
                // 理论上不可达（长度已预检）；兜底放行空体避免挂死请求
                Request::from_parts(parts, Body::empty()),
            ),
        }
    } else {
        let preview = match req_len {
            Some(l) => format!("<body too large: {} bytes>", l),
            None => String::new(), // 无 body（GET）或流式上传，不打预览
        };
        (preview, Request::from_parts(parts, body))
    };

    // ---- 执行请求 ----
    let response = next.run(request).await;

    // ---- 响应体预览：仅 JSON 且 content-length 已知且 ≤ 上限时缓冲（SSE 等流式天然跳过）----
    let status = response.status();
    let log_id = response
        .headers()
        .get(common::constants::http_header::LOG_ID)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();
    let resp_ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let resp_len = content_length(response.headers());

    let (resp_preview, response) = if resp_ct.as_deref().is_some_and(|ct| {
        ct.contains("application/json") && resp_len.is_some_and(|l| l <= MAX_BODY_LOG_BYTES)
    }) {
        let (parts, body) = response.into_parts();
        match axum::body::to_bytes(body, MAX_BODY_LOG_BYTES).await {
            Ok(bytes) => {
                let preview = body_preview(&bytes, resp_ct.as_deref());
                (preview, Response::from_parts(parts, Body::from(bytes)))
            }
            Err(_) => (
                "<body read failed>".to_string(),
                Response::from_parts(parts, Body::empty()),
            ),
        }
    } else {
        let preview = match resp_len {
            Some(l) => format!("<body omitted: {} bytes>", l),
            None => "<streaming or unknown length>".to_string(),
        };
        (preview, response)
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    // ---- 打印 notice 日志（≥500 走 warn，其余 info）----
    // 注意：log_*! 宏的无上下文分支要求第一个参数是字符串字面量，且 tracing 事件宏
    // 要求 message 在最后——因此这里用纯格式化消息，body 预览作为格式化参数传入
    // （预览值里的 `{}` 花括号出现在参数里而非字面量中，不会被误解析）
    let path_and_query = if query.is_empty() {
        path.clone()
    } else {
        format!("{}?{}", path, query)
    };
    if status.as_u16() >= 500 {
        crate::log_warn!(
            "api notice: {} {} status={} duration_ms={} log_id={} req_body={} resp_body={}",
            method,
            path_and_query,
            status.as_u16(),
            duration_ms,
            log_id,
            req_preview,
            resp_preview
        );
    } else {
        crate::log_info!(
            "api notice: {} {} status={} duration_ms={} log_id={} req_body={} resp_body={}",
            method,
            path_and_query,
            status.as_u16(),
            duration_ms,
            log_id,
            req_preview,
            resp_preview
        );
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_path_gate() {
        assert!(is_api_path("/api/v1/system/tasks"));
        assert!(is_api_path("/a2a"));
        assert!(!is_api_path("/api")); // 前缀必须带 /
        assert!(!is_api_path("/health"));
        assert!(!is_api_path("/assets/output.css"));
        assert!(!is_api_path("/login"));
    }

    #[test]
    fn body_preview_truncates_without_masking() {
        // 内部日志不做脱敏：原文直接进预览（截断逻辑照常）
        let body = br#"{"username":"alice","api_key":"sk-123"}"#;
        let preview = body_preview(body, Some("application/json"));
        assert_eq!(preview, r#"{"username":"alice","api_key":"sk-123"}"#);

        let preview = body_preview(
            b"username=alice&password=hunter2",
            Some("application/x-www-form-urlencoded"),
        );
        assert_eq!(preview, "username=alice&password=hunter2");

        // 超长截断
        let long = "x".repeat(MAX_PREVIEW_CHARS + 10);
        let preview = body_preview(long.as_bytes(), Some("text/plain"));
        assert!(preview.ends_with("<truncated>"));
        assert!(preview.chars().count() < MAX_PREVIEW_CHARS + 20);

        // 二进制
        let preview = body_preview(&[0u8, 1, 2], Some("application/octet-stream"));
        assert!(preview.starts_with("<binary 3 bytes"));
    }

    #[test]
    fn content_length_parse() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, "42".parse().unwrap());
        assert_eq!(content_length(&headers), Some(42));

        headers.remove(header::CONTENT_LENGTH);
        assert_eq!(content_length(&headers), None);
    }
}
