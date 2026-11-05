//! HTML 内联预览（沙箱 iframe）
//!
//! 产物 / 附件里的 `text/html` 以前只能下载：后端 [`is_inline_safe`] 刻意不含
//! `text/html`（内联即同源 XSS），直接访问 `/file` 拿到的永远是
//! `Content-Disposition: attachment`。要在页面上看到它，正路是「前端取源码 +
//! 沙箱 iframe 渲染」，本组件就是这一步，隔离口径对齐文档中心
//! （`pages/system/docs.rs` 的架构图内嵌）：
//!
//! - `sandbox` **不带** `allow-same-origin`：iframe 文档退化为不透明源，读不到父页
//!   DOM / Cookie / localStorage，同源接口也因 CORS 拿不到；
//! - 比文档中心**更严**：省掉 `allow-popups` 与 `allow-popups-to-escape-sandbox`
//!   （产物 HTML 没有开窗需求，少开一分面）；
//! - 内容走 `srcdoc`（属性值由框架转义，不存在标签逃逸），**不**经
//!   `dangerous_inner_html` 注入父文档。
//!
//! 注意：后端安全默认**未被改动**——`/file` 的响应头照旧，任何直接访问路径仍只会
//! 下载；这条链路只在用户主动打开详情页时消费一次源码。
//!
//! [`is_inline_safe`]: ../../../../src/pkg/http/file.rs

use dioxus::prelude::*;

/// `srcdoc` 内联的体积上限：4 MiB。
///
/// 超限不渲染（整份源码要进 DOM 属性，几十 MB 的 HTML 会把页面拖死），
/// 前端落到「过大，请下载查看」的提示；下载按钮始终可用。
pub const HTML_PREVIEW_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// MIME 主类型（去掉 `;charset=...` 与首尾空白）。
fn mime_base(raw: &str) -> &str {
    raw.split(';').next().unwrap_or("").trim()
}

/// 文件名是否带 html 后缀。
///
/// 历史数据的 `mime_type` 可能是泛型 `application/octet-stream`（上传时 Blob 未设
/// type），此时只能靠后缀兜底，否则同一份 HTML 在旧数据上永远看不到预览。
fn has_html_extension(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm") || lower.ends_with(".xhtml")
}

/// 该产物 / 附件能否用沙箱 iframe 内联渲染。
///
/// 判据：①体积不超过 [`HTML_PREVIEW_MAX_BYTES`]；②MIME 是 html 家族，或文件名
/// 带 html 后缀。产物 `name` 常是「季度报告」这类无后缀文案，所以调用方必须传
/// **存储文件名**（`file_path` 的 basename / 附件 `original_name`），不能拿显示名。
pub fn is_html_previewable(mime_type: &str, file_name: &str, size: u64) -> bool {
    if size > HTML_PREVIEW_MAX_BYTES {
        return false;
    }
    matches!(mime_base(mime_type), "text/html" | "application/xhtml+xml")
        || has_html_extension(file_name)
}

/// 沙箱 iframe 渲染一段 HTML 源码。
#[component]
pub fn HtmlPreview(html: String, title: String) -> Element {
    rsx! {
        iframe {
            class: "w-full h-[75vh] rounded-box border border-base-300 bg-base-100",
            title: "{title}",
            // 属性名走引号形态：srcdoc 不在 Dioxus 已知属性表里，直接写会被当成
            // 组件字段解析。
            "srcdoc": "{html}",
            // 无 allow-same-origin → 不透明源；无 allow-popups* → 不可开窗逃逸。
            "sandbox": "allow-scripts allow-downloads",
            "referrerpolicy": "no-referrer",
            "loading": "lazy",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_base_strips_charset_and_space() {
        assert_eq!(mime_base("text/html; charset=utf-8"), "text/html");
        assert_eq!(mime_base(" text/html "), "text/html");
        assert_eq!(mime_base(""), "");
    }

    #[test]
    fn html_mime_is_previewable() {
        assert!(is_html_previewable(
            "text/html; charset=utf-8",
            "季度报告",
            1024
        ));
        assert!(is_html_previewable("application/xhtml+xml", "", 0));
    }

    #[test]
    fn html_extension_covers_generic_mime() {
        assert!(is_html_previewable(
            "application/octet-stream",
            "archify.html",
            10
        ));
        assert!(is_html_previewable("", "index.HTM", 10));
        assert!(is_html_previewable("", "page.xhtml", 10));
        assert!(!is_html_previewable(
            "application/octet-stream",
            "report.docx",
            10
        ));
    }

    #[test]
    fn non_html_is_not_previewable() {
        assert!(!is_html_previewable("image/png", "a.png", 10));
        assert!(!is_html_previewable("application/pdf", "a.pdf", 10));
        assert!(!is_html_previewable("text/plain", "a.txt", 10));
    }

    #[test]
    fn oversized_html_falls_back_to_download() {
        assert!(!is_html_previewable(
            "text/html",
            "big.html",
            HTML_PREVIEW_MAX_BYTES + 1
        ));
        assert!(is_html_previewable(
            "text/html",
            "ok.html",
            HTML_PREVIEW_MAX_BYTES
        ));
    }
}
