//! 文件字节流响应基建：把磁盘上的单个文件转成 HTTP 响应。
//!
//! 与 [`super::client`] 方向相反——那边管「发出去的请求」，这里管「返回给浏览器的
//! 文件字节」。任何需要把**用户资产原始字节**吐给 `<img> / <video> / <audio> / <a>`
//! 的场景都走这里（附件、产物下载等），避免每个 handler 各写一套。
//!
//! # 能力
//!
//! - **流式读取**：`tokio::fs::File` + [`ReaderStream`] 按块推流，不再整文件读入内存；
//! - **Range 请求**：支持单区间 `bytes=a-b`、`bytes=a-`、`bytes=-n`，命中返回 `206`
//!   + `Content-Range`，越界返回 `416` + `Content-Range: bytes */size`；
//!     多区间（`bytes=0-1,3-4`）与语法错误一律忽略 Range，退回 `200` 全量（RFC 9110 允许）；
//! - **安全头**：`X-Content-Type-Options: nosniff` 恒定；`Content-Disposition` 按 MIME
//!   白名单决定 `inline` / `attachment`。
//!
//! # 安全边界（重要）
//!
//! 文件字节是**任意用户上传内容**。若把 `text/html`、`image/svg+xml` 以 `inline`
//! 返回，脚本会在**本站源**执行（存储型 XSS）。因此 [`is_inline_safe`] 只放行
//! 位图 / 音视频 / 纯文本 / PDF，其余一律 `attachment` 强制下载。
//!
//! # 未实现（按需再加）
//!
//! - `ETag` / `If-Range` / `If-Modified-Since` 条件请求：当前没有校验器，因此调用方
//!   给的 `Cache-Control` 必须自己权衡新鲜度与回源成本——带 `max-age` 时，源文件被
//!   覆盖更新后，浏览器在缓存期内仍会展示旧字节；改用 `no-cache` 则每次全量回源。
//!   要两者兼得需要先补 validator + 304，本模块暂不提供。
//! - 多区间 `multipart/byteranges` 响应：浏览器播放器只用单区间，收益低。

use std::fmt::Write as _;
use std::path::Path;

use axum::{
    body::Body,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use common::error::{Result, err};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;

/// `mime_type` 缺失或含非法字符时的兜底类型。
pub const DEFAULT_MIME: &str = "application/octet-stream";

/// 流式读取的块大小（64 KiB）：小于默认 8 KiB 会导致大文件系统调用过多，
/// 过大则单块占用内存偏高。
const STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// 文件响应选项。
#[derive(Debug, Clone, Default)]
pub struct FileServeOptions<'a> {
    /// 原始 MIME 类型（可能为空、含参数或非法字符，内部会归一化）。
    pub mime_type: &'a str,
    /// 原始文件名：用于 `Content-Disposition` 的 `filename` 参数。
    pub file_name: &'a str,
    /// `Cache-Control` 值；`None` 表示不设置该头。
    pub cache_control: Option<&'a str>,
    /// 强制下载：即便 MIME 属于内联安全白名单，也用 `attachment` 返回。
    ///
    /// 供「下载按钮」这类显式下载语义使用；普通渲染场景传 `false`。
    pub force_download: bool,
}

/// 单区间 Range（`end` 含）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    /// 起始字节偏移。
    pub start: u64,
    /// 结束字节偏移（含）。
    pub end: u64,
}

impl ByteRange {
    /// 区间长度（bytes）。
    pub fn len_bytes(&self) -> u64 {
        self.end - self.start + 1
    }
}

/// Range 解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeOutcome {
    /// 无 Range、语法非法或多区间：忽略，返回 `200` 全量。
    Ignore,
    /// 可满足：返回 `206` + 该区间。
    Satisfiable(ByteRange),
    /// 语法合法但越界：返回 `416`。
    Unsatisfiable,
}

/// 把磁盘文件转成 HTTP 响应（流式，支持 Range）。
///
/// - `range_header`：原始 `Range` 头值（`None` = 客户端没要区间）；
/// - 文件不存在/不是普通文件 → `NotFound` 错误；读失败 → `Internal` 错误。
pub async fn serve_file(
    path: &Path,
    range_header: Option<&str>,
    opts: FileServeOptions<'_>,
) -> Result<Response> {
    let meta = match tokio::fs::metadata(path).await {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(err!(NotFound, "文件不存在或已被删除"));
        }
        Err(e) => return Err(err!(Internal, "读取文件元信息失败: {}", e)),
    };
    if meta.is_dir() {
        return Err(err!(InvalidRequest, "不能以文件方式访问目录"));
    }
    let size = meta.len();

    let mut headers = base_headers(&opts, size);

    // 空文件：任何 Range 都无从谈起，直接 200 空体。
    if size == 0 {
        return Ok((StatusCode::OK, headers, Body::empty()).into_response());
    }

    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| err!(Internal, "打开文件失败: {}", e))?;

    let outcome = range_header
        .map(|raw| parse_range(raw, size))
        .unwrap_or(RangeOutcome::Ignore);

    match outcome {
        RangeOutcome::Unsatisfiable => {
            headers.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{size}"))
                    .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
            );
            // 416 不需要 body，也无需 Content-Length（长度本身不可满足）
            headers.remove(header::CONTENT_LENGTH);
            Ok((StatusCode::RANGE_NOT_SATISFIABLE, headers, Body::empty()).into_response())
        }
        RangeOutcome::Satisfiable(range) => {
            file.seek(SeekFrom::Start(range.start))
                .await
                .map_err(|e| err!(Internal, "定位文件读取位置失败: {}", e))?;
            headers.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {}-{}/{}", range.start, range.end, size))
                    .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
            );
            headers.insert(header::CONTENT_LENGTH, header_value_u64(range.len_bytes()));
            let stream =
                ReaderStream::with_capacity(file.take(range.len_bytes()), STREAM_CHUNK_SIZE);
            Ok((
                StatusCode::PARTIAL_CONTENT,
                headers,
                Body::from_stream(stream),
            )
                .into_response())
        }
        RangeOutcome::Ignore => {
            let stream = ReaderStream::with_capacity(file.take(size), STREAM_CHUNK_SIZE);
            Ok((StatusCode::OK, headers, Body::from_stream(stream)).into_response())
        }
    }
}

/// 组装与内容无关的公共响应头（类型 / 长度 / 呈现方式 / 缓存 / 安全）。
fn base_headers(opts: &FileServeOptions<'_>, size: u64) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, normalize_mime(opts.mime_type));
    headers.insert(header::CONTENT_LENGTH, header_value_u64(size));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        content_disposition(opts.mime_type, opts.file_name, opts.force_download),
    );
    // 禁止浏览器按内容嗅探改写类型：否则一个被标成 text/plain 的 HTML 仍可能被
    // 当页面执行，绕过 Content-Disposition 的下载约束。
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    if let Some(cache_control) = opts.cache_control
        && let Ok(value) = HeaderValue::from_str(cache_control)
    {
        headers.insert(header::CACHE_CONTROL, value);
    }
    headers
}

fn header_value_u64(value: u64) -> HeaderValue {
    HeaderValue::from_str(&value.to_string()).unwrap_or_else(|_| HeaderValue::from_static("0"))
}

/// 归一化 MIME：空串或含非法 header 字符时退化为 [`DEFAULT_MIME`]。
pub fn normalize_mime(raw: &str) -> HeaderValue {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return HeaderValue::from_static(DEFAULT_MIME);
    }
    HeaderValue::from_bytes(trimmed.as_bytes())
        .unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_MIME))
}

/// MIME 主类型（去掉 `;charset=...` 等参数）。
fn mime_base(raw: &str) -> &str {
    raw.split(';').next().unwrap_or("").trim()
}

/// 该 MIME 是否能安全地以 `inline` 返回给当前源。
///
/// 刻意**不含** `image/svg+xml` 与 `text/html`：二者可内嵌脚本，内联即同源 XSS。
pub fn is_inline_safe(mime_type: &str) -> bool {
    matches!(
        mime_base(mime_type),
        // 位图（不含 SVG）
        "image/png"
            | "image/jpeg"
            | "image/gif"
            | "image/webp"
            | "image/bmp"
            | "image/avif"
            | "image/heic"
            | "image/heif"
            // 音视频
            | "video/mp4"
            | "video/webm"
            | "video/quicktime"
            | "video/ogg"
            | "audio/mpeg"
            | "audio/mp4"
            | "audio/aac"
            | "audio/ogg"
            | "audio/wav"
            | "audio/webm"
            // 纯文本与 PDF（PDF 由浏览器内置阅读器渲染，不执行宿主脚本）
            | "text/plain"
            | "application/pdf"
    )
}

/// 构造 `Content-Disposition`：`inline` / `attachment` + RFC 5987 文件名。
///
/// 文件名同时给 ASCII 回退（`filename=`）与 UTF-8 百分号编码（`filename*=`），
/// 保证中文名在各类浏览器都能正确落地，且 header 值恒为 ASCII（不会因 CR/LF 注入）。
pub fn content_disposition(mime_type: &str, file_name: &str, force_download: bool) -> HeaderValue {
    let kind = if force_download || !is_inline_safe(mime_type) {
        "attachment"
    } else {
        "inline"
    };
    if file_name.trim().is_empty() {
        return HeaderValue::from_static(if kind == "attachment" {
            "attachment"
        } else {
            "inline"
        });
    }
    let ascii_fallback = ascii_filename_fallback(file_name);
    let encoded = rfc5987_encode(file_name);
    let value = format!("{kind}; filename=\"{ascii_fallback}\"; filename*=UTF-8''{encoded}");
    HeaderValue::from_str(&value).unwrap_or_else(|_| {
        HeaderValue::from_static(if kind == "attachment" {
            "attachment"
        } else {
            "inline"
        })
    })
}

/// ASCII 回退文件名：非可见 ASCII、引号、反斜杠一律替换为 `_`（顺带杜绝 header 注入）。
fn ascii_filename_fallback(file_name: &str) -> String {
    file_name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' {
                c
            } else if c == ' ' {
                ' '
            } else {
                '_'
            }
        })
        .collect()
}

/// RFC 5987 `attr-char` 百分号编码（`filename*=UTF-8''...`）。
fn rfc5987_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        let c = *byte as char;
        let is_attr_char = c.is_ascii_alphanumeric()
            || matches!(
                c,
                '!' | '#' | '$' | '&' | '+' | '-' | '.' | '^' | '_' | '`' | '|' | '~'
            );
        if is_attr_char {
            out.push(c);
        } else {
            // 写入 String 不会失败（只可能是格式化错误），忽略返回值
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// 解析单区间 `Range` 头。
///
/// 语法非法（非 `bytes=`、解析失败、`end < start`）与多区间一律 [`RangeOutcome::Ignore`]；
/// 只有「语法合法但起点越界 / 后缀长度为 0」才判 [`RangeOutcome::Unsatisfiable`]。
pub fn parse_range(header_value: &str, size: u64) -> RangeOutcome {
    // `serve_file` 对空文件已提前返回 200 空体，但本函数是 `pub` 的：直接调用时
    // `size - 1` / 后缀区间的 `end` 都会下溢 panic（debug 下直接崩、release 下回绕）。
    // 空文件没有任何可满足的区间，统一按「忽略 Range」处理。
    if size == 0 {
        return RangeOutcome::Ignore;
    }
    let trimmed = header_value.trim();
    // HTTP 方法/头名大小写不敏感：`bytes=` 前缀按 ASCII 忽略大小写匹配。
    // 用 `get(..6)` 而非切片索引，遇到多字节字符时安全返回 None。
    let Some(prefix) = trimmed.get(..6) else {
        return RangeOutcome::Ignore;
    };
    if !prefix.eq_ignore_ascii_case("bytes=") {
        return RangeOutcome::Ignore;
    }
    let spec = trimmed[6..].trim();
    // 多区间：`0-1,3-4`。浏览器播放器不发，按 RFC 允许直接忽略。
    if spec.contains(',') {
        return RangeOutcome::Ignore;
    }
    let Some((start_raw, end_raw)) = spec.split_once('-') else {
        return RangeOutcome::Ignore;
    };
    let start_raw = start_raw.trim();
    let end_raw = end_raw.trim();

    if start_raw.is_empty() {
        // `bytes=-N`：末尾 N 字节
        let Ok(suffix) = end_raw.parse::<u64>() else {
            return RangeOutcome::Ignore;
        };
        if suffix == 0 {
            return RangeOutcome::Unsatisfiable;
        }
        let start = size.saturating_sub(suffix);
        return RangeOutcome::Satisfiable(ByteRange {
            start,
            end: size - 1,
        });
    }

    let Ok(start) = start_raw.parse::<u64>() else {
        return RangeOutcome::Ignore;
    };
    if start >= size {
        return RangeOutcome::Unsatisfiable;
    }
    let end = if end_raw.is_empty() {
        size - 1
    } else {
        match end_raw.parse::<u64>() {
            Ok(end) => end.min(size - 1),
            Err(_) => return RangeOutcome::Ignore,
        }
    };
    if end < start {
        return RangeOutcome::Ignore;
    }
    RangeOutcome::Satisfiable(ByteRange { start, end })
}
#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
