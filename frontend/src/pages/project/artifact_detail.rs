//! Artifact 详情页 - 展示元信息 + 内容查看/编辑

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::project::{get_artifact_content, update_artifact_content};
use crate::components::code_editor::CodeEditor;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{ArtifactDetail, UpdateArtifactContentRequest};
use common::enums::ArtifactSourceType;

#[component]
pub fn ProjectArtifactDetail(id: String) -> Element {
    let toast = use_toast();

    let mut artifact = use_signal(|| Option::<ArtifactDetail>::None);
    let mut content = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut content_dirty = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut is_text_type = use_signal(|| false);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_artifact_content(&id).await {
                Ok(resp) => {
                    artifact.set(Some(resp.artifact));
                    content.set(resp.content.content);
                    is_text_type.set(true);
                    content_dirty.set(false);
                }
                Err(e) => {
                    toast.error(&format!("加载产物内容失败: {}", e));
                }
            }
            loading.set(false);
        });
    });

    let on_save = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            saving.set(true);
            spawn(async move {
                match update_artifact_content(UpdateArtifactContentRequest {
                    artifact_id: id.clone(),
                    content: content(),
                    expected_updated_at: None,
                })
                .await
                {
                    Ok(_) => {
                        toast.success("内容已保存");
                        content_dirty.set(false);
                    }
                    Err(e) => toast.error(&format!("保存失败: {}", e)),
                }
                saving.set(false);
            });
        }
    };

    let artifact_data = artifact.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "产物详情" }
                Link { class: "btn btn-ghost", to: crate::pages::Route::ProjectArtifacts {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(a) = artifact_data {
                div { class: "card bg-base-100 shadow-md mb-6",
                    div { class: "card-body",
                        h2 { class: "card-title", "{a.name}" }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4 mt-4",
                            div { div { class: "text-sm text-base-content/60", "描述" }, div { class: "font-medium", "{a.description}" } }
                            div { div { class: "text-sm text-base-content/60", "文件大小" }, div { class: "font-mono", "{crate::utils::format_file_size(a.file_size)}" } }
                            div { div { class: "text-sm text-base-content/60", "来源类型" }, span { class: "badge badge-info", "{source_type_text(a.source_type)}" } }
                            div { div { class: "text-sm text-base-content/60", "MIME 类型" }, div { class: "font-mono", "{a.mime_type}" } }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(a.created_at)}" } }
                        }
                    }
                }
                if is_text_type() {
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-body",
                            div { class: "flex justify-between items-center mb-4",
                                h2 { class: "card-title text-lg", "📄 内容" }
                                div { class: "flex gap-2",
                                    if content_dirty() { span { class: "text-xs text-warning", "● 未保存" } }
                                    button {
                                        class: "btn btn-primary btn-sm",
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
            } else {
                EmptyState { icon: "❓".to_string(), message: "产物不存在或已被删除".to_string() }
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
