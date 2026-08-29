//! Artifact 详情页 - 展示元信息 + 内容查看/编辑

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::project::{get_artifact_content, update_artifact};
use crate::components::artifact_meta_modal::ArtifactMetaModal;
use crate::components::code_editor::CodeEditor;
use crate::components::hud::{HudPanel, PageHeader};
use crate::components::markdown::MarkdownRenderer;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::UpdateArtifactRequest;
use common::enums::ArtifactSourceType;

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

    let mut artifact_res = use_resource(move || {
        let id = rid();
        async move { get_artifact_content(&id).await }
    });
    let mut content = use_signal(String::new);
    let mut content_dirty = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut is_text_type = use_signal(|| false);
    let mut show_meta_modal = use_signal(|| false);

    // 同步：resource 完成时填充内容与元数据；用户已编辑则保留编辑内容
    // （peek 读取 content_dirty，避免被自身写回再次触发 effect）
    use_effect(move || {
        if let Some(resp) = artifact_res.read().as_ref().and_then(|r| r.as_ref().ok()) {
            if !*content_dirty.peek() {
                content.set(resp.content.content.clone());
            }
            is_text_type.set(true);
            content_dirty.set(false);
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

    let artifact_data = artifact_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|resp| resp.artifact.clone());

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: "PROJECT".to_string(),
                title: "产物详情".to_string(),
                actions: Some(rsx! {
                    Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::ProjectArtifacts {}, "← 返回列表" }
                }),
            }
            if artifact_res.read().as_ref().is_none() {
                Loading {}
            } else if let Some(a) = artifact_data {
                HudPanel {
                    title: "{a.name}".to_string(),
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
                            div { div { class: "text-sm text-base-content/60", "来源类型" }, span { class: "badge hud-badge badge-info", "{source_type_text(a.source_type)}" } }
                            div { div { class: "text-sm text-base-content/60", "MIME 类型" }, div { class: "font-mono", "{a.mime_type}" } }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(a.created_at)}" } }
                        }
                    }
                }
                if is_text_type() {
                    HudPanel {
                        title: "📄 内容".to_string(),
                        eyebrow: "CONTENT".to_string(),
                        div { class: "card-body",
                            div { class: "flex justify-end mb-4",
                                div { class: "flex gap-2",
                                    if content_dirty() { span { class: "text-xs text-warning", "● 未保存" } }
                                    button {
                                        class: "btn hud-btn btn-primary btn-sm",
                                        disabled: saving() || !content_dirty(),
                                        onclick: on_save,
                                        if saving() { "保存中..." } else { "💾 保存" }
                                    }
                                }
                            }
                            CodeEditor {
                                value: content(),
                                on_input: move |v| { content.set(v); content_dirty.set(true); },
                                language: "markdown".to_string(),
                                min_lines: 20,
                            }
                        }
                    }
                } else {
                    EmptyState { icon: "📦".to_string(), message: "此产物为二进制文件，不支持在线查看内容".to_string() }
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
                                    artifact_res.restart();
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
                EmptyState { icon: "📦".to_string(), message: "该产物无法在线预览（可能为二进制或非文本内容）".to_string() }
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
