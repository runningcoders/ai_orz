//! Artifact 详情页 - 展示元信息 + 按类型预览（图片/音视频/PDF/文本编辑）
//!
//! 元信息与内容分两条链路取：
//! - 元信息走 `GET /artifacts/{id}`（任意类型都可用）；
//! - 文本内容走 `GET /artifacts/{id}/content`（JSON 信封，仅**生成内容**型的文本产物可用）。
//!
//! 此前元信息也来自 `/content`，而该接口对非文本 / 非 GeneratedContent 直接 400，
//! 导致图片型产物连元信息都拿不到、页面只剩「无法在线预览」。分开取之后，非文本产物
//! 按 `file_type` 渲染预览，字节统一走 `GET /artifacts/{id}/file`
//! （流式 + Range，视频可拖进度条）。

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::project::{get_artifact, get_artifact_content, update_artifact};
use crate::components::artifact_meta_modal::ArtifactMetaModal;
use crate::components::code_editor::CodeEditor;
use crate::components::hud::{HudPanel, PageHeader};
use crate::components::markdown::MarkdownRenderer;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::UpdateArtifactRequest;
use common::enums::{ArtifactSourceType, FileType};

/// 产物原始字节流地址（流式 + Range）。
fn file_url(id: &str) -> String {
    format!("/api/v1/project/artifacts/{id}/file")
}

/// `<a download>` 的文件名：优先取存储路径的 basename（与服务端 `Content-Disposition`
/// 同一个真相），不要拿显示名覆盖。产物的 `name` 常是「季度报告」这类无后缀文案，
/// 直接当 download 名会让用户下到一个没有扩展名、打不开的文件。
fn download_name(file_path: &str, fallback: &str) -> String {
    match file_path.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_name_keeps_extension() {
        assert_eq!(
            download_name("attachments/20261104/att-1.png", "季度报告"),
            "att-1.png"
        );
        assert_eq!(download_name("report.md", "季度报告"), "report.md");
    }

    #[test]
    fn download_name_falls_back_when_path_is_empty() {
        assert_eq!(download_name("", "季度报告"), "季度报告");
        assert_eq!(download_name("dir/", "季度报告"), "季度报告");
    }
}

#[component]
pub fn ProjectArtifactDetail(id: String) -> Element {
    // 方案 B：订阅路由并把 id 同步到响应式 rid，use_resource 绑定 rid，
    // 拉取仅在 id 变化时触发
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::ProjectArtifactDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    let toast = use_toast();

    // 元信息：任意来源类型 / 文件类型都能拿到
    let mut detail_res = use_resource(move || {
        let id = rid();
        async move { get_artifact(&id).await }
    });

    // 文本内容：只有「生成内容 + 文档 + 非 PDF」才请求。
    // 其余组合该接口必 400（附件型不落产物目录、二进制非 UTF-8），发了只是白费一次往返。
    // 读 detail_res 建立依赖：元信息到位后自动触发。
    let text_res = use_resource(move || {
        let id = rid();
        let detail = detail_res
            .read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .cloned();
        async move {
            let d = detail?;
            let is_pdf = d.mime_type.starts_with("application/pdf");
            if d.source_type != ArtifactSourceType::GeneratedContent
                || d.file_type != FileType::Document
                || is_pdf
            {
                return None;
            }
            get_artifact_content(&id)
                .await
                .ok()
                .map(|resp| resp.content.content)
        }
    });

    let mut content = use_signal(String::new);
    let mut content_dirty = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut show_meta_modal = use_signal(|| false);
    let mut preview_mode = use_signal(|| true);

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
                match update_artifact(UpdateArtifactRequest {
                    artifact_id: id.clone(),
                    content: Some(content()),
                    name: None,
                    description: None,
                    tags: None,
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
    // Some(None) = 无文本（非文本产物或取文本失败）；None = 还在加载
    let text_loaded = text_res.read().as_ref().and_then(|t| t.clone());

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: "PROJECT".to_string(),
                title: "产物详情".to_string(),
                actions: Some(rsx! {
                    Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::ProjectArtifacts {}, "← 返回列表" }
                }),
            }
            if detail_res.read().as_ref().is_none() {
                Loading {}
            } else if let Some(a) = detail {
                HudPanel {
                    title: a.name.clone(),
                    eyebrow: "ARTIFACT".to_string(),
                    signal: true,
                    div { class: "card-body",
                        div { class: "card-actions justify-end",
                            button {
                                class: "btn hud-btn btn-ghost btn-sm",
                                onclick: move |_| show_meta_modal.set(true),
                                "✏️ 编辑信息"
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4 mt-4",
                            div { div { class: "text-sm text-base-content/60", "描述" }, MarkdownRenderer { content: a.description.clone(), compact: true } }
                            div { div { class: "text-sm text-base-content/60", "文件大小" }, div { class: "font-mono", "{crate::utils::format_file_size(a.file_size)}" } }
                            div { div { class: "text-sm text-base-content/60", "来源类型" }, span { class: "badge orz-tag badge-sm", "{source_type_text(a.source_type)}" } }
                            div { div { class: "text-sm text-base-content/60", "MIME 类型" }, div { class: "font-mono", "{a.mime_type}" } }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(a.created_at)}" } }
                        }
                    }
                }
                {
                    let url = file_url(&a.id);
                    let is_pdf = a.mime_type.starts_with("application/pdf");
                    match a.file_type {
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
                                            alt: a.name.clone(),
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
                                    title: a.name.clone(),
                                }
                            }
                        },
                        FileType::Document => rsx! {
                            HudPanel {
                                title: "📄 内容".to_string(),
                                eyebrow: "CONTENT".to_string(),
                                div { class: "card-body",
                                    if text_loaded.is_some() {
                                        div { class: "flex justify-end mb-4",
                                            div { class: "flex gap-2 items-center",
                                                div { class: "join",
                                                    button {
                                                        class: if preview_mode() {
                                                            "btn hud-btn btn-sm join-item btn-primary"
                                                        } else {
                                                            "btn hud-btn btn-sm join-item btn-ghost"
                                                        },
                                                        onclick: move |_| preview_mode.set(true),
                                                        "预览"
                                                    }
                                                    button {
                                                        class: if preview_mode() {
                                                            "btn hud-btn btn-sm join-item btn-ghost"
                                                        } else {
                                                            "btn hud-btn btn-sm join-item btn-primary"
                                                        },
                                                        onclick: move |_| preview_mode.set(false),
                                                        "编辑"
                                                    }
                                                }
                                                if content_dirty() { span { class: "text-xs text-warning", "● 未保存" } }
                                                button {
                                                    class: "btn hud-btn btn-primary btn-sm",
                                                    disabled: saving() || !content_dirty(),
                                                    onclick: on_save,
                                                    if saving() { "保存中..." } else { "💾 保存" }
                                                }
                                            }
                                        }
                                        if preview_mode() {
                                            MarkdownRenderer { content: content(), compact: false }
                                        } else {
                                            CodeEditor {
                                                value: content(),
                                                on_input: move |v| { content.set(v); content_dirty.set(true); },
                                                language: "markdown".to_string(),
                                                min_lines: 20,
                                            }
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
                            EmptyState { icon: "📦".to_string(), message: "此产物为二进制文件，不支持在线预览，请下载查看".to_string() }
                        },
                    }
                }
                div { class: "mt-4",
                    a {
                        class: "btn hud-btn btn-ghost btn-sm",
                        href: file_url(&a.id),
                        download: download_name(&a.file_path, &a.name),
                        "⬇️ 下载原件"
                    }
                }
                ArtifactMetaModal {
                    artifact: a.clone(),
                    show: show_meta_modal(),
                    on_save: move |(name, description, tags)| {
                        let id = id.clone();
                        spawn(async move {
                            match update_artifact(UpdateArtifactRequest {
                                artifact_id: id.clone(),
                                content: None,
                                name,
                                description,
                                tags,
                                expected_updated_at: None,
                            }).await {
                                Ok(_) => {
                                    detail_res.restart();
                                    toast.success("产物信息已更新");
                                    show_meta_modal.set(false);
                                }
                                Err(e) => toast.error(format!("更新失败: {}", e)),
                            }
                        });
                    },
                    on_close: move |_| show_meta_modal.set(false),
                }
            } else {
                EmptyState { icon: "📦".to_string(), message: "该产物不存在或无权访问" }
            }
        }
    }
}

fn source_type_text(t: ArtifactSourceType) -> &'static str {
    match t {
        ArtifactSourceType::Attachment => "附件",
        ArtifactSourceType::GeneratedContent => "生成内容",
        ArtifactSourceType::RemoteUrl => "远程链接",
    }
}
