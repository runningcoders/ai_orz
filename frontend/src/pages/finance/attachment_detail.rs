//! 附件详情页 - 展示元信息 + 按类型预览（图片/音视频/PDF/文本编辑）
//!
//! 元信息与内容分两条链路取：
//! - 元信息走 `GET /attachments/{id}`（任意类型都可用）；
//! - 文本内容走 `GET /attachments/{id}/content`（JSON 信封 + 乐观锁，仅纯文本类可用；
//!   PDF 虽被归为 Document，但走 `/file` 的 iframe 预览，不读文本）。
//!
//! 此前元信息也来自 `/content`，而该接口对非文本直接 400，导致图片连元信息都拿不到、
//! 页面只剩「无法在线预览」。分开取之后，非文本附件按 `file_type` 渲染预览，字节统一
//! 走 `GET /attachments/{id}/file`（流式 + Range，视频可拖进度条）。

use crate::api::finance::{get_attachment, get_attachment_content, update_attachment_content};
use crate::components::code_editor::CodeEditor;
use crate::components::html_preview::{HtmlPreview, is_html_previewable};
use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::UpdateAttachmentContentRequest;
use common::enums::FileType;
use dioxus::prelude::*;
use dioxus_router::Link;

/// 附件原始字节流地址（流式 + Range）。
fn file_url(id: &str) -> String {
    format!("/api/v1/finance/attachments/{id}/file")
}

#[component]
pub fn FinanceAttachmentDetail(id: String) -> Element {
    // 方案 B：订阅路由并把 id 同步到响应式 rid，use_resource 绑定 rid，
    // 拉取仅在 id 变化时触发
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::FinanceAttachmentDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    let toast = use_toast();

    // 元信息：任意文件类型都能拿到
    let detail_res = use_resource(move || {
        let id = rid();
        async move { get_attachment(&id).await }
    });

    // 文本内容：仅「Document 且非 PDF」才请求。PDF 同样被标成 Document，但 `/content`
    // 会把整个 PDF 读进内存再做 UTF-8 校验然后失败——白跑一趟还吃内存。
    // 读 detail_res 建立依赖：元信息到位后自动触发。
    let text_res = use_resource(move || {
        let id = rid();
        let target = detail_res
            .read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|d| {
                (
                    d.file_type,
                    d.mime_type.clone(),
                    d.original_name.clone(),
                    d.size,
                )
            });
        async move {
            let (file_type, mime_type, original_name, size) = target?;
            // HTML 归「内联渲染」而不是「文本编辑」：既不该进代码编辑器改，
            // 也不该白跑一趟 /content。
            if file_type != FileType::Document
                || mime_type.starts_with("application/pdf")
                || is_html_previewable(&mime_type, &original_name, size)
            {
                return None;
            }
            get_attachment_content(&id)
                .await
                .ok()
                .map(|resp| resp.text.content)
        }
    });

    // HTML 附件：走 `/file` 取源码交给沙箱 iframe（后端对 text/html 强制下载）。
    let html_res = use_resource(move || {
        let id = rid();
        let target = detail_res
            .read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|d| (d.mime_type.clone(), d.original_name.clone(), d.size));
        async move {
            let (mime_type, original_name, size) = target?;
            if !is_html_previewable(&mime_type, &original_name, size) {
                return None;
            }
            crate::api::api_get_text(&format!("/api/v1/finance/attachments/{id}/file"))
                .await
                .ok()
        }
    });

    let mut content = use_signal(String::new);
    let mut content_dirty = use_signal(|| false);
    let mut saving = use_signal(|| false);

    // 文本到位时填充编辑器；用户已编辑则保留编辑内容
    // （peek 读取 content_dirty，避免被自身写回再次触发 effect）
    use_effect(move || {
        let loaded = text_res.read().as_ref().and_then(|t| t.clone());
        if let Some(text) = loaded
            && !*content_dirty.peek()
        {
            content.set(text);
        }
    });

    let on_save = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            saving.set(true);
            spawn(async move {
                match update_attachment_content(UpdateAttachmentContentRequest {
                    id,
                    content: content(),
                    expected_updated_at: None,
                })
                .await
                {
                    Ok(_) => {
                        toast.success("内容已保存");
                        content_dirty.set(false);
                    }
                    Err(e) => toast.error(format!("保存失败: {}", e)),
                }
                saving.set(false);
            });
        }
    };

    let detail = detail_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    // None = 还在加载 / 不适用；Some(None) = 取文本失败（不支持的文本类型）
    let text_loaded = text_res.read().as_ref().and_then(|t| t.clone());

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "附件详情" }
                Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::FinanceAttachments {}, "← 返回列表" }
            }
            if detail_res.read().as_ref().is_none() {
                Loading {}
            } else if let Some(a) = detail {
                HudPanel { signal: Some(true), extra_class: Some("mb-6".to_string()),
                    title: Some(a.original_name.clone()),
                    div { class: "card-actions justify-end mt-4",
                        a {
                            class: "btn hud-btn btn-primary btn-sm",
                            href: file_url(&a.id),
                            download: a.original_name.clone(),
                            "⬇️ 下载原件"
                        }
                    }
                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4 mt-4",
                            div { div { class: "text-sm text-base-content/60", "存储名" }, div { class: "font-mono text-sm", "{a.stored_name}" } }
                            div { div { class: "text-sm text-base-content/60", "大小" }, div { class: "font-mono", "{crate::utils::format_file_size(a.size)}" } }
                            div { div { class: "text-sm text-base-content/60", "MIME 类型" }, div { class: "font-mono", "{a.mime_type}" } }
                            div { div { class: "text-sm text-base-content/60", "用途" }, span { class: "badge orz-tag badge-sm", "{a.purpose}" } }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(a.created_at)}" } }
                        }
                }
                {
                    let url = file_url(&a.id);
                    let is_pdf = a.mime_type.starts_with("application/pdf");
                    let is_html = is_html_previewable(&a.mime_type, &a.original_name, a.size);
                    match a.file_type {
                        _ if is_html => rsx! {
                            HudPanel { signal: Some(true), extra_class: Some("mb-6".to_string()),
                                h3 { class: "font-semibold mb-3", "🌐 HTML 预览" }
                                div { class: "text-xs text-base-content/60 mb-2",
                                    "沙箱隔离渲染（不加载本站身份）；依赖外部资源或文件过大时请下载原件"
                                }
                                if let Some(html) = html_res.read().as_ref().and_then(|h| h.clone()) {
                                    HtmlPreview { html: Some(html), title: a.original_name.clone() }
                                } else if html_res.read().as_ref().is_none() {
                                    Loading {}
                                } else {
                                    EmptyState {
                                        icon: "🌐".to_string(),
                                        message: "HTML 源码读取失败，请下载查看原件".to_string(),
                                    }
                                }
                            }
                        },
                        FileType::Image => rsx! {
                            HudPanel { signal: Some(true), extra_class: Some("mb-6".to_string()),
                                h3 { class: "font-semibold mb-3", "🖼️ 图片" }
                                div { class: "flex justify-center",
                                    a {
                                        href: url.clone(),
                                        target: "_blank",
                                        rel: "noopener noreferrer",
                                        class: "attachment-preview-link",
                                        img {
                                            class: "attachment-preview-image",
                                            src: url.clone(),
                                            alt: a.original_name.clone(),
                                            loading: "lazy",
                                        }
                                    }
                                }
                            }
                        },
                        FileType::Video => rsx! {
                            HudPanel { signal: Some(true), extra_class: Some("mb-6".to_string()),
                                h3 { class: "font-semibold mb-3", "🎬 视频" }
                                video {
                                    class: "attachment-preview-media",
                                    src: url.clone(),
                                    controls: "true",
                                    preload: "metadata",
                                }
                            }
                        },
                        FileType::Audio => rsx! {
                            HudPanel { signal: Some(true), extra_class: Some("mb-6".to_string()),
                                h3 { class: "font-semibold mb-3", "🎧 音频" }
                                audio {
                                    class: "w-full",
                                    src: url.clone(),
                                    controls: "true",
                                    preload: "metadata",
                                }
                            }
                        },
                        FileType::Document if is_pdf => rsx! {
                            HudPanel { signal: Some(true), extra_class: Some("mb-6".to_string()),
                                h3 { class: "font-semibold mb-3", "📄 PDF" }
                                iframe {
                                    class: "attachment-preview-frame",
                                    src: url.clone(),
                                    title: a.original_name.clone(),
                                }
                            }
                        },
                        FileType::Document => rsx! {
                            HudPanel { signal: Some(true),
                                div { class: "card-body",
                                    PageHeader {
                                        eyebrow: Some("FINANCE".to_string()),
                                        title: "📄 内容".to_string(),
                                        actions: Some(rsx!{
                                        div { class: "flex gap-2",
                                            if content_dirty() { span { class: "text-xs text-warning", "● 未保存" } }
                                            button {
                                                class: "btn hud-btn btn-primary btn-sm",
                                                disabled: saving() || !content_dirty(),
                                                onclick: on_save,
                                                if saving() { "保存中..." } else { "💾 保存" }
                                            }
                                        }
                                        }),
                                    },
                                    if text_loaded.is_some() {
                                        CodeEditor {
                                            value: content(),
                                            on_input: move |v| { content.set(v); content_dirty.set(true); },
                                            language: "text".to_string(),
                                            min_lines: 20,
                                        }
                                    } else if text_res.read().as_ref().is_none() {
                                        Loading {}
                                    } else {
                                        EmptyState {
                                            icon: "📦".to_string(),
                                            message: "此文档无法作为纯文本预览，可下载查看原件".to_string(),
                                        }
                                    }
                                }
                            }
                        },
                        _ => rsx! {
                            EmptyState {
                                icon: "📦".to_string(),
                                message: "此类型附件不支持在线预览，请下载查看".to_string(),
                            }
                        },
                    }
                }
            } else {
                EmptyState { icon: "📦".to_string(), message: "该附件不存在或无权访问".to_string() }
            }
        }
    }
}
