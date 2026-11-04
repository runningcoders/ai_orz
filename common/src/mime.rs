//! MIME 类型推断与归一（前后端共用）。
//!
//! # 为什么放在 common
//!
//! 上传链路上「文件后缀 → MIME」的判定**两端都要用**：
//! - 前端构造 `Blob` 时必须给出真实 `type`，否则 multipart 的 part `Content-Type`
//!   就是 `application/octet-stream`（浏览器对未设 type 的 Blob 也这么兜底）；
//! - 后端落库前要对空值 / 泛型值再做一次兜底：渠道入站（微信图片下载响应常常只给
//!   `application/octet-stream`）、老客户端、非浏览器调用方都拿不到可信类型。
//!
//! 两端各写一份表必然漂移——前端按后缀判成 `image/png`、后端表里没有该后缀又退回
//! 泛型，就会出现「同一张图有时候能内联、有时候只能当文件下载」。所以这里是唯一
//! 事实源，任何新增类型只改这一处。
//!
//! # 边界
//!
//! 只做**后缀**推断，不嗅探字节魔数：上传内容是任意用户数据，且本函数的用途是
//! 「给 UI / 响应头一个合理类型」，不是安全判定。真正确认内容由下游按需处理。

/// 无法识别时的泛型 MIME。
pub const OCTET_STREAM: &str = "application/octet-stream";

/// 去掉 MIME 参数并 trim（`"image/png; charset=binary"` → `"image/png"`）。
pub fn normalize(mime_type: &str) -> &str {
    mime_type.split(';').next().unwrap_or_default().trim()
}

/// 是否是「没有信息量」的类型：空值或 `application/octet-stream`。
///
/// 这两类都应当视为「调用方也不知道是什么」，可以放心用文件名后缀覆盖；
/// 其余类型一律尊重调用方（客户端可能比后缀更准，例如改了后缀的文件）。
pub fn is_generic(mime_type: &str) -> bool {
    let base = normalize(mime_type);
    base.is_empty() || base.eq_ignore_ascii_case(OCTET_STREAM)
}

/// 从文件名后缀推断 MIME；无法识别时返回 [`OCTET_STREAM`]。
///
/// 兼容带目录的路径（只看最后一段），且不会把 `.bashrc` 这类点开头文件名当成后缀。
pub fn infer_mime_type(file_name: &str) -> &'static str {
    let ext = extension_of(file_name).to_ascii_lowercase();
    match ext.as_str() {
        // ---- 图片 ----
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        // ---- 音视频 ----
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        // ---- 文档 ----
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "xml" => "application/xml",
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "rtf" => "application/rtf",
        "epub" => "application/epub+zip",
        // ---- 压缩包 ----
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "bz2" => "application/x-bzip2",
        "xz" => "application/x-xz",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        // ---- 代码 / 脚本 ----
        "js" | "mjs" | "cjs" => "application/javascript",
        "ts" | "tsx" => "text/x-typescript",
        "jsx" => "text/x-jsx",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "kt" => "text/x-kotlin",
        "swift" => "text/x-swift",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" | "cc" | "cxx" => "text/x-c++",
        "sh" | "bash" | "zsh" => "application/x-sh",
        "sql" => "application/sql",
        "rb" => "text/x-ruby",
        "php" => "application/x-httpd-php",
        "lua" => "text/x-lua",
        "pl" => "text/x-perl",
        "r" => "text/x-r",
        // ---- 字体 ----
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => OCTET_STREAM,
    }
}

/// 归一 MIME；泛型值按文件名后缀补齐。
///
/// 非泛型值原样返回（保留调用方给出的类型，包括自定义 `application/x-*`）；
/// 泛型值且后缀也识别不出时，退回 [`OCTET_STREAM`]。
pub fn normalize_or_infer(mime_type: &str, file_name: &str) -> String {
    let trimmed = mime_type.trim();
    if !is_generic(trimmed) {
        return trimmed.to_string();
    }
    infer_mime_type(file_name).to_string()
}

/// 取后缀（不含点，保留原大小写）；无后缀返回空串。
fn extension_of(file_name: &str) -> &str {
    let base = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .trim();
    match base.rsplit_once('.') {
        // `.bashrc` 的 stem 为空 → 不算后缀；结尾是点（`a.`）也不算
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => ext,
        _ => "",
    }
}

#[cfg(test)]
#[path = "mime_tests.rs"]
mod tests;
