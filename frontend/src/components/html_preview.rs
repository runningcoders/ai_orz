//! HTML 内联预览（沙箱 iframe）——全站唯一的 HTML 渲染出口
//!
//! 站内需要把一段 HTML 显示出来的地方只有两类，都走本组件（调用方不得再自写
//! `iframe`，否则隔离口径就分叉了）：
//!
//! | 场景 | 源码来源 | 用法 |
//! |------|----------|------|
//! | 产物 / 附件详情页（`text/html`） | 鉴权 API 取回源码 | `HtmlPreview { html: Some(源码), .. }` |
//! | 文档中心架构图（`/docs/**` 下的自包含 HTML） | 同源静态 URL | `HtmlPreview { src: Some(url), .. }` |
//!
//! 为什么要分两种来源：产物 / 附件里的 HTML 只能走「前端取源码 + `srcdoc`」——
//! 后端 [`is_inline_safe`] 刻意不含 `text/html`（内联即同源 XSS），直接访问 `/file`
//! 拿到的永远是 `Content-Disposition: attachment`，把 `iframe` 直指 `/file` 只会触发下载。
//! 而文档中心的自包含 HTML 就在同源静态目录里（`build.rs` 打包进 `public/docs/`），
//! 必须给它 `src`：图内相对资源（`./assets/*`、内置脚本）要按自身目录解析，换成
//! `srcdoc` 会把这些路径解析到 SPA 页面上，等于把架构图打散。
//!
//! 两条分支共用同一套隔离口径（唯一定义点在本文件）：
//!
//! - `sandbox` **永不带** `allow-same-origin`：iframe 文档退化为不透明源，读不到父页
//!   DOM / Cookie / localStorage，同源接口也因 CORS 拿不到；
//! - 产物 / 附件用 [`HtmlPreviewSandbox::Strict`]（无 `allow-popups*`，比文档中心更严：
//!   产物 HTML 没有开窗需求，少开一分面）；文档中心用
//!   [`HtmlPreviewSandbox::WithPopups`]，逐字保留架构图既有的开窗能力（图内有新窗口
//!   链接）——**统一是收敛调用点，不是削既有权限**；
//! - `referrerpolicy="no-referrer"`：iframe 及其子资源请求不向任何源泄漏位置；
//! - `srcdoc` 由框架转义（不存在标签逃逸），**不**经 `dangerous_inner_html` 注入父文档。
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

/// iframe 的尺寸与外观（两条分支共用同一个常量，避免视觉口径分叉）。
///
/// 类名是 Tailwind 任意值写法（`h-[75vh]`），改动后需重新构建 CSS 产物。
const PREVIEW_CLASS: &str = "w-full h-[75vh] rounded-box border border-base-300 bg-base-100";

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

/// 沙箱策略预置。
///
/// 只开这两档，且都**不含** `allow-same-origin`。做成预置而不是让调用方传字符串，
/// 是为了让「不放松隔离」这条红线能被单测钉住（见 `tests::sandbox_policies_*`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HtmlPreviewSandbox {
    /// 产物 / 附件（默认）：允许脚本与下载，**不允许**开窗。
    #[default]
    Strict,
    /// 文档中心架构图：在 `Strict` 之上保留既有的开窗逃逸能力（图内新窗口链接）。
    WithPopups,
}

impl HtmlPreviewSandbox {
    /// iframe `sandbox` 属性的取值。
    pub fn attr(self) -> &'static str {
        match self {
            Self::Strict => "allow-scripts allow-downloads",
            Self::WithPopups => {
                "allow-scripts allow-downloads allow-popups allow-popups-to-escape-sandbox"
            }
        }
    }
}

/// 预览内容的来源判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewContent<'a> {
    /// 直接内联源码（`srcdoc`）。
    Inline(&'a str),
    /// 交给 iframe 自己加载（同源静态 URL）。
    Url(&'a str),
    /// 两者都没给（或都是空白）：渲染占位提示，不留一张空 iframe。
    Empty,
}

/// 决定走 `srcdoc` 还是 `src`：源码优先。
///
/// 纯空白内容按「没给」处理——`srcdoc=""` 会渲染一张白板，比显式占位更让人困惑。
pub fn preview_content<'a>(html: Option<&'a str>, src: Option<&'a str>) -> PreviewContent<'a> {
    if let Some(text) = html.filter(|s| !s.trim().is_empty()) {
        return PreviewContent::Inline(text);
    }
    match src.filter(|s| !s.trim().is_empty()) {
        Some(url) => PreviewContent::Url(url),
        None => PreviewContent::Empty,
    }
}

/// 沙箱 iframe 渲染 HTML：产物 / 附件传 `html`（源码），文档中心传 `src`（静态 URL）。
#[component]
pub fn HtmlPreview(
    /// 内联源码（产物 / 附件走这条）。
    #[props(default)]
    html: Option<String>,
    /// 静态 URL（文档中心架构图走这条）。
    #[props(default)]
    src: Option<String>,
    /// iframe 标题：给读屏软件的替代文本，不显示在页面上。
    title: String,
    /// 沙箱预置，默认最严的 [`HtmlPreviewSandbox::Strict`]。
    #[props(default)]
    sandbox: HtmlPreviewSandbox,
    /// 是否懒加载。详情页可懒（默认 `true`）；文档中心是用户主动点开的主体内容，
    /// 用 `false` 立即加载。
    #[props(default = true)]
    lazy: bool,
) -> Element {
    let sandbox_attr = sandbox.attr();
    let loading_attr = if lazy { "lazy" } else { "eager" };
    let content = preview_content(html.as_deref(), src.as_deref());
    rsx! {
        if let PreviewContent::Inline(text) = content {
            iframe {
                class: PREVIEW_CLASS,
                title: "{title}",
                // 属性名走引号形态：srcdoc 不在 Dioxus 已知属性表里，直接写会被当成
                // 组件字段解析。
                "srcdoc": "{text}",
                "sandbox": "{sandbox_attr}",
                "referrerpolicy": "no-referrer",
                "loading": "{loading_attr}",
            }
        } else if let PreviewContent::Url(url) = content {
            iframe {
                class: PREVIEW_CLASS,
                title: "{title}",
                src: "{url}",
                "sandbox": "{sandbox_attr}",
                "referrerpolicy": "no-referrer",
                "loading": "{loading_attr}",
            }
        } else {
            div { class: "text-sm text-base-content/60 py-6 text-center",
                "没有可预览的 HTML 内容"
            }
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

    #[test]
    fn sandbox_policies_never_allow_same_origin() {
        for mode in [HtmlPreviewSandbox::Strict, HtmlPreviewSandbox::WithPopups] {
            let attr = mode.attr();
            assert!(
                !attr.contains("allow-same-origin"),
                "{mode:?} 不得放开 allow-same-origin：一旦同源，iframe 内脚本可直通父页"
            );
        }
    }

    #[test]
    fn strict_sandbox_forbids_popups_but_keeps_scripts() {
        let strict = HtmlPreviewSandbox::Strict.attr();
        assert!(strict.contains("allow-scripts"));
        assert!(strict.contains("allow-downloads"));
        assert!(!strict.contains("allow-popups"));
    }

    #[test]
    fn docs_sandbox_keeps_existing_popup_capability() {
        // 文档中心架构图统一到本组件后，开窗能力必须逐字保留（统一 ≠ 削权限）。
        let docs = HtmlPreviewSandbox::WithPopups.attr();
        assert!(docs.contains("allow-scripts"));
        assert!(docs.contains("allow-downloads"));
        assert!(docs.contains("allow-popups"));
        assert!(docs.contains("allow-popups-to-escape-sandbox"));
    }

    #[test]
    fn default_sandbox_is_strict() {
        assert_eq!(HtmlPreviewSandbox::default(), HtmlPreviewSandbox::Strict);
        assert_eq!(
            HtmlPreviewSandbox::default().attr(),
            HtmlPreviewSandbox::Strict.attr()
        );
    }

    #[test]
    fn preview_content_prefers_inline_source() {
        assert_eq!(
            preview_content(Some("<p>x</p>"), None),
            PreviewContent::Inline("<p>x</p>")
        );
        assert_eq!(
            preview_content(Some("<p>x</p>"), Some("/docs/a.html")),
            PreviewContent::Inline("<p>x</p>")
        );
    }

    #[test]
    fn preview_content_falls_back_to_url() {
        assert_eq!(
            preview_content(None, Some("/docs/a.html?theme=dark")),
            PreviewContent::Url("/docs/a.html?theme=dark")
        );
        // 空白源码视同未给：srcdoc="" 只会渲染白板
        assert_eq!(
            preview_content(Some("  \n"), Some("/docs/a.html")),
            PreviewContent::Url("/docs/a.html")
        );
    }

    #[test]
    fn preview_content_empty_when_nothing_supplied() {
        assert_eq!(preview_content(None, None), PreviewContent::Empty);
        assert_eq!(preview_content(Some(""), Some("  ")), PreviewContent::Empty);
    }
}
