//! 产物预览卡片弹窗（对话页侧栏的产物入口共用）
//!
//! 为什么要有这张卡：对话页右侧产物列表此前点击直接跳产物详情页，看一份产物要离开
//! 当前会话，回来还得重新找回对话位置。把详情页里的「常用基础信息 + 预览 + 下载」
//! 抽成弹窗后，看一眼就关，不打断会话上下文。
//!
//! 与详情页（`pages/project/artifact_detail.rs`）的分工：
//!
//! | 能力 | 详情页 | 本卡片 |
//! |------|--------|--------|
//! | 基础信息 | 全量 | 常用项 |
//! | 预览 | 有 | 有（同一套分派判据） |
//! | 下载原件 | 有 | 有 |
//! | 编辑（内容 / 元信息 / 标签） | 有 | 无，纯只读 |
//!
//! 信息框 `card-actions` 一带给了「🔗 打开详情页」快捷入口（需要编辑时再去详情页）：
//! 点击先复位本弹窗状态、再 `navigator.push` 到既有深链路由 `ProjectArtifactDetail`。
//! 走同页 SPA 跳转而非新开标签——平台既有页面跳转一律如此，新开会脱离应用外壳。
//!
//! 预览分派与详情页同口径：HTML 走共享组件 `HtmlPreview`——前端取源码 + `srcdoc` +
//! 沙箱 iframe，**不能**把 iframe 直指 `/file`（后端对 `text/html` 强制 `attachment`，
//! 那样只会触发下载）；媒体走各自的只读标签；PDF 走 iframe 直指 `/file`（PDF 在后端
//! `is_inline_safe` 白名单内，可内联）。
//!
//! 安全默认一条不动：沙箱仍是共享组件默认的 `Strict`（无 `allow-same-origin`），
//! 弹窗不新增任何放宽隔离的能力，也不经 `dangerous_inner_html` 注入父文档。

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::components::html_preview::{HtmlPreview, is_html_previewable};
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::utils::{format_datetime, format_file_size};
use common::api::ArtifactDetail;
use common::enums::{ArtifactSourceType, FileType};

/// 产物原始字节流地址（流式 + Range，与详情页同一个端点）。
fn file_url(id: &str) -> String {
    format!("/api/v1/project/artifacts/{id}/file")
}

/// 存储文件名（`file_path` 的 basename）。
///
/// 预览判据与 `<a download>` 都要它：产物的 `name` 常是「季度报告」这类无后缀文案，
/// 拿显示名当文件名会让用户下到一个没扩展名、打不开的文件。
fn stored_file_name(file_path: &str) -> &str {
    file_path.rsplit('/').next().unwrap_or("")
}

/// `<a download>` 的文件名：优先取存储路径的 basename，路径为空时回落到显示名。
fn download_name(file_path: &str, fallback: &str) -> String {
    match stored_file_name(file_path) {
        name if !name.is_empty() => name.to_string(),
        _ => fallback.to_string(),
    }
}

/// MIME 主类型比较（忽略 `;charset=...` 与大小写）。
fn mime_matches(mime_type: &str, expect: &str) -> bool {
    mime_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case(expect)
}

/// 文件名是否带指定后缀（不区分大小写）。
fn has_extension(file_name: &str, ext: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(&format!(".{}", ext.to_ascii_lowercase()))
}

/// 预览分派结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactPreviewKind {
    /// HTML：沙箱 iframe + `srcdoc`（源码由前端取）。
    Html,
    /// 图片：`img` 直指 `/file`。
    Image,
    /// 视频：`video` 直指 `/file`（流式 + Range，可拖进度条）。
    Video,
    /// 音频：`audio` 直指 `/file`。
    Audio,
    /// PDF：iframe 直指 `/file`。
    Pdf,
    /// 文档：只读 Markdown 预览（仅生成内容型有文本）。
    Text,
    /// 其余二进制：只能下载。
    Unsupported,
}

/// 决定这份产物用哪种预览形态。
///
/// 顺序即优先级：HTML 先判（它可能同时是 `Document`，而按 Markdown 渲染 HTML 是错的），
/// 再判 PDF（`Document` 里最需要专用预览的一类），最后才按 `file_type` 兜底。
pub fn preview_kind(
    mime_type: &str,
    file_name: &str,
    size: u64,
    file_type: FileType,
) -> ArtifactPreviewKind {
    if is_html_previewable(mime_type, file_name, size) {
        return ArtifactPreviewKind::Html;
    }
    if mime_matches(mime_type, "application/pdf") || has_extension(file_name, "pdf") {
        return ArtifactPreviewKind::Pdf;
    }
    match file_type {
        FileType::Image => ArtifactPreviewKind::Image,
        FileType::Video => ArtifactPreviewKind::Video,
        FileType::Audio => ArtifactPreviewKind::Audio,
        FileType::Document => ArtifactPreviewKind::Text,
        FileType::Binary => ArtifactPreviewKind::Unsupported,
    }
}

/// 文件类型中文文案。
pub fn file_type_text(file_type: FileType) -> &'static str {
    match file_type {
        FileType::Document => "文档",
        FileType::Image => "图片",
        FileType::Audio => "音频",
        FileType::Video => "视频",
        FileType::Binary => "二进制",
    }
}

/// 来源类型中文文案。
pub fn source_type_text(source_type: ArtifactSourceType) -> &'static str {
    match source_type {
        ArtifactSourceType::Attachment => "附件",
        ArtifactSourceType::GeneratedContent => "生成内容",
        ArtifactSourceType::RemoteUrl => "远程链接",
    }
}

/// 预览区标题。
fn preview_title(kind: ArtifactPreviewKind) -> &'static str {
    match kind {
        ArtifactPreviewKind::Html => "🌐 HTML 预览",
        ArtifactPreviewKind::Image => "🖼️ 图片",
        ArtifactPreviewKind::Video => "🎬 视频",
        ArtifactPreviewKind::Audio => "🎧 音频",
        ArtifactPreviewKind::Pdf => "📄 PDF",
        ArtifactPreviewKind::Text => "📄 内容预览",
        ArtifactPreviewKind::Unsupported => "📦 无在线预览",
    }
}

/// 基础信息的一行（标签 + 值）。
fn info_item(label: &str, value: String) -> Element {
    rsx! {
        div {
            div { class: "text-xs text-base-content/60", "{label}" }
            div { class: "truncate font-mono", "{value}" }
        }
    }
}

/// 产物预览卡片弹窗：基础信息 + 预览 + 下载原件，**纯只读**（无编辑入口）。
///
/// 归属信息（项目名 / 任务标题）由调用方带入——侧栏本就有这两份数据，
/// 弹窗不再为一行文字多发一次请求。
///
/// 挂载方式沿用 `Modal` 的「需要时才渲染」：父侧条件渲染本组件即可，
/// 因此本组件自身没有 `show` 参数，也不做「隐藏态」分支。
#[component]
pub fn ArtifactPreviewModal(
    artifact: ArtifactDetail,
    /// 所属项目名（空串表示未知：默认对话的游离任务产物不显示该行）。
    project_name: String,
    /// 所属任务标题；`None` = 项目级产物。
    #[props(default)]
    task_title: Option<String>,
    on_close: EventHandler<()>,
) -> Element {
    let id = artifact.id.clone();
    let url = file_url(&id);
    let stored_name = stored_file_name(&artifact.file_path).to_string();
    let kind = preview_kind(
        &artifact.mime_type,
        &stored_name,
        artifact.file_size,
        artifact.file_type,
    );
    let download_file_name = download_name(&artifact.file_path, &artifact.name);
    // 「打开详情页」用：既有深链路由的 id（此处先克隆，避免被闭包 move 走影响后续 use_resource）
    let id_for_link = id.clone();
    let navigator = use_navigator();

    // HTML 源码：走鉴权 API 取文本（`/file` 对 text/html 是 attachment，只影响导航不影响 fetch）
    let id_for_html = id.clone();
    let html_res = use_resource(move || {
        let id = id_for_html.clone();
        async move {
            if kind != ArtifactPreviewKind::Html {
                return None;
            }
            crate::api::api_get_text(&file_url(&id)).await.ok()
        }
    });

    // 纯文本文档：只有「生成内容 + 文档」才有可读文本，其余组合该接口必 400，不发请求
    let wants_text = kind == ArtifactPreviewKind::Text
        && artifact.source_type == ArtifactSourceType::GeneratedContent;
    let id_for_text = id.clone();
    let text_res = use_resource(move || {
        let id = id_for_text.clone();
        async move {
            if !wants_text {
                return None;
            }
            crate::api::project::get_artifact_content(&id)
                .await
                .ok()
                .map(|resp| resp.content.content)
        }
    });

    rsx! {
        Modal {
            show: true,
            title: artifact.name.clone(),
            on_close,
            width_class: Some("max-w-5xl".to_string()),
            div { class: "space-y-3",
                // 基础信息框：下载按钮上移进信息框（沿用详情页 card-actions 口径）
                div { class: "card bg-base-100 border border-base-300",
                    div { class: "card-body p-3 gap-3",
                        div { class: "card-actions justify-end",
                            button {
                                class: "btn hud-btn btn-secondary btn-sm",
                                onclick: move |_| {
                                    // 先复位弹窗状态再跳转：即便侧栏被布局级复用，也不会残留 top layer 弹窗
                                    on_close.call(());
                                    navigator.push(crate::pages::Route::ProjectArtifactDetail {
                                        id: id_for_link.clone(),
                                    });
                                },
                                "🔗 打开详情页"
                            }
                            a {
                                class: "btn hud-btn btn-primary btn-sm",
                                href: url.clone(),
                                download: download_file_name.clone(),
                                "⬇️ 下载原件"
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-3 text-sm",
                            {info_item("文件类型", file_type_text(artifact.file_type).to_string())}
                            {info_item("文件大小", format_file_size(artifact.file_size))}
                            {info_item("MIME 类型", artifact.mime_type.clone())}
                            div {
                                div { class: "text-xs text-base-content/60", "来源类型" }
                                span { class: "badge orz-tag badge-sm", "{source_type_text(artifact.source_type)}" }
                            }
                            if !project_name.is_empty() {
                                {info_item("所属项目", project_name.clone())}
                            }
                            if let Some(title) = task_title.clone() {
                                {info_item("所属任务", title)}
                            }
                            {info_item("创建时间", format_datetime(artifact.created_at))}
                            {info_item("更新时间", format_datetime(artifact.updated_at))}
                        }
                        if !artifact.description.trim().is_empty() {
                            div {
                                div { class: "text-xs text-base-content/60 mb-1", "描述" }
                                MarkdownRenderer { content: artifact.description.clone(), compact: true }
                            }
                        }
                        if !artifact.tags.is_empty() {
                            div { class: "flex flex-wrap gap-1",
                                for tag in artifact.tags.iter() {
                                    span { key: "{tag}", class: "badge orz-tag badge-sm", "{tag}" }
                                }
                            }
                        }
                    }
                }
                // 预览区：分派与详情页同口径
                div {
                    h4 { class: "font-semibold mb-2 text-sm", "{preview_title(kind)}" }
                    {
                        match kind {
                            ArtifactPreviewKind::Html => rsx! {
                                div {
                                    if let Some(html) = html_res.read().as_ref().and_then(|h| h.clone()) {
                                        HtmlPreview { html: Some(html), title: artifact.name.clone() }
                                    } else if html_res.read().as_ref().is_none() {
                                        div { class: "flex items-center justify-center py-8", Loading { size: "md" } }
                                    } else {
                                        EmptyState {
                                            icon: "🌐".to_string(),
                                            message: "HTML 源码读取失败，请下载查看原件".to_string(),
                                        }
                                    }
                                }
                            },
                            ArtifactPreviewKind::Image => rsx! {
                                div { class: "flex justify-center",
                                    img {
                                        class: "attachment-preview-image",
                                        src: url.clone(),
                                        alt: artifact.name.clone(),
                                        loading: "lazy",
                                    }
                                }
                            },
                            ArtifactPreviewKind::Video => rsx! {
                                video {
                                    class: "attachment-preview-media",
                                    src: url.clone(),
                                    controls: "true",
                                    preload: "metadata",
                                }
                            },
                            ArtifactPreviewKind::Audio => rsx! {
                                audio {
                                    class: "w-full",
                                    src: url.clone(),
                                    controls: "true",
                                    preload: "metadata",
                                }
                            },
                            ArtifactPreviewKind::Pdf => rsx! {
                                iframe {
                                    class: "attachment-preview-frame",
                                    src: url.clone(),
                                    title: artifact.name.clone(),
                                }
                            },
                            ArtifactPreviewKind::Text => rsx! {
                                div {
                                    if let Some(text) = text_res.read().as_ref().and_then(|t| t.clone()) {
                                        MarkdownRenderer { content: text, compact: false }
                                    } else if text_res.read().as_ref().is_none() {
                                        div { class: "flex items-center justify-center py-8", Loading { size: "md" } }
                                    } else {
                                        EmptyState {
                                            icon: "📄".to_string(),
                                            message: "此文档无法作为纯文本预览，可下载查看原件".to_string(),
                                        }
                                    }
                                }
                            },
                            ArtifactPreviewKind::Unsupported => rsx! {
                                EmptyState {
                                    icon: "📦".to_string(),
                                    message: "此产物为二进制文件，不支持在线预览，请下载查看".to_string(),
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_wins_over_document_branch() {
        // HTML 的 file_type 也是 Document：必须先判 HTML，否则会被当 Markdown 渲染
        assert_eq!(
            preview_kind(
                "text/html; charset=utf-8",
                "report.html",
                1024,
                FileType::Document
            ),
            ArtifactPreviewKind::Html
        );
        // 泛型 mime 的历史数据靠后缀兜底
        assert_eq!(
            preview_kind(
                "application/octet-stream",
                "index.htm",
                10,
                FileType::Binary
            ),
            ArtifactPreviewKind::Html
        );
        // 超过内联体积上限回落（与 HtmlPreview 同一判据）
        assert_ne!(
            preview_kind(
                "text/html",
                "big.html",
                crate::components::html_preview::HTML_PREVIEW_MAX_BYTES + 1,
                FileType::Document
            ),
            ArtifactPreviewKind::Html
        );
    }

    #[test]
    fn pdf_is_detected_by_mime_or_extension() {
        assert_eq!(
            preview_kind("application/pdf", "a.bin", 10, FileType::Document),
            ArtifactPreviewKind::Pdf
        );
        assert_eq!(
            preview_kind(
                "application/octet-stream",
                "季度报告.PDF",
                10,
                FileType::Document
            ),
            ArtifactPreviewKind::Pdf
        );
    }

    #[test]
    fn media_and_document_follow_file_type() {
        assert_eq!(
            preview_kind("image/png", "a.png", 10, FileType::Image),
            ArtifactPreviewKind::Image
        );
        assert_eq!(
            preview_kind("video/mp4", "a.mp4", 10, FileType::Video),
            ArtifactPreviewKind::Video
        );
        assert_eq!(
            preview_kind("audio/mpeg", "a.mp3", 10, FileType::Audio),
            ArtifactPreviewKind::Audio
        );
        assert_eq!(
            preview_kind("text/markdown", "a.md", 10, FileType::Document),
            ArtifactPreviewKind::Text
        );
        assert_eq!(
            preview_kind("application/zip", "a.zip", 10, FileType::Binary),
            ArtifactPreviewKind::Unsupported
        );
    }

    #[test]
    fn mime_matches_ignores_charset_and_case() {
        assert!(mime_matches("Text/HTML; charset=utf-8", "text/html"));
        assert!(mime_matches(" application/pdf ", "application/pdf"));
        assert!(!mime_matches("text/html", "application/pdf"));
        assert!(!mime_matches("", "application/pdf"));
    }

    #[test]
    fn download_name_keeps_extension_and_falls_back() {
        assert_eq!(
            download_name("artifacts/20261105/art-1.html", "季度报告"),
            "art-1.html"
        );
        assert_eq!(download_name("report.md", "季度报告"), "report.md");
        assert_eq!(download_name("", "季度报告"), "季度报告");
        assert_eq!(download_name("dir/", "季度报告"), "季度报告");
    }

    #[test]
    fn info_texts_cover_all_variants() {
        for ft in [
            FileType::Document,
            FileType::Image,
            FileType::Audio,
            FileType::Video,
            FileType::Binary,
        ] {
            assert!(!file_type_text(ft).is_empty());
        }
        for st in [
            ArtifactSourceType::Attachment,
            ArtifactSourceType::GeneratedContent,
            ArtifactSourceType::RemoteUrl,
        ] {
            assert!(!source_type_text(st).is_empty());
        }
    }
}
