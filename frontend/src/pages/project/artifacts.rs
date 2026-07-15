//! 项目产物管理

use dioxus::prelude::*;

use crate::api::project::{create_artifact, delete_artifact, list_artifacts, list_projects};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{ArtifactDetail, CreateArtifactRequest, ListProjectsResponseItem};
use common::enums::ArtifactSourceType;

fn source_type_text(source_type: ArtifactSourceType) -> &'static str {
    match source_type {
        ArtifactSourceType::Attachment => "附件",
        ArtifactSourceType::GeneratedContent => "生成内容",
        ArtifactSourceType::RemoteUrl => "远程链接",
    }
}

fn format_file_size(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[component]
pub fn ProjectArtifacts() -> Element {
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut artifacts = use_signal(Vec::<ArtifactDetail>::new);
    let mut loading = use_signal(|| true);
    let mut show_add_modal = use_signal(|| false);
    let mut selected_project_id = use_signal(String::new);

    // 创建表单状态
    let mut new_name = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let toast = use_toast();

    // 初始加载项目列表
    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_projects().await {
                Ok(list) => {
                    let items = list.projects;
                    if !items.is_empty() {
                        let first_id = items[0].id.clone();
                        selected_project_id.set(first_id.clone());
                        match list_artifacts(&first_id).await {
                            Ok(list) => artifacts.set(list),
                            Err(e) => toast.error(&e),
                        }
                    }
                    projects.set(items);
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    // 切换项目时重新加载产物
    use_effect(move || {
        let pid = selected_project_id();
        if pid.is_empty() {
            return;
        }
        loading.set(true);
        spawn(async move {
            match list_artifacts(&pid).await {
                Ok(list) => artifacts.set(list),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        let pid = selected_project_id();
        if pid.is_empty() {
            toast.error("请先选择一个项目");
            return;
        }
        spawn(async move {
            if new_name().is_empty() {
                toast.error("产物名称不能为空");
                return;
            }
            creating.set(true);
            let req = CreateArtifactRequest {
                project_id: pid,
                task_id: None,
                name: new_name(),
                description: if new_description().is_empty() {
                    None
                } else {
                    Some(new_description())
                },
                source_type: ArtifactSourceType::GeneratedContent,
                attachment_id: None,
                content: None,
                file_name: None,
                mime_type: None,
                file_type: None,
                tags: None,
            };
            match create_artifact(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_description.set(String::new());
                    toast.success("创建成功");
                    let pid = selected_project_id();
                    if !pid.is_empty() {
                        match list_artifacts(&pid).await {
                            Ok(list) => artifacts.set(list),
                            Err(e) => toast.error(&e),
                        }
                    }
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let artifacts_list = artifacts.read().clone();
    let projects_list = projects.read().clone();

    rsx! {
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "项目产物管理" }
                button { class: "btn btn-accent", onclick: move |_| show_add_modal.set(true), "+ 创建产物" }
            }

            if !projects_list.is_empty() {
                div { class: "form-group", style: "padding: 0 16px 8px;",
                    label { class: "form-label", "选择项目" }
                    select {
                        class: "form-select",
                        value: "{selected_project_id}",
                        onchange: move |e| selected_project_id.set(e.value()),
                        for p in projects_list.iter() {
                            option { value: "{p.id}", "{p.name}" }
                        }
                    }
                }
            }

            if loading() {
                Loading {}
            } else if selected_project_id().is_empty() {
                EmptyState { icon: "📁".to_string(), message: "请先选择一个项目".to_string() }
            } else if artifacts_list.is_empty() {
                EmptyState { icon: "📦".to_string(), message: "暂无产物".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "描述" }
                        th { "来源类型" }
                        th { "文件大小" }
                        th { "创建时间" }
                        th { "操作" }
                    }}
                    tbody {
                        for a in artifacts_list.iter() {
                            {
                                let id = a.id.clone();
                                let name = a.name.clone();
                                let description = a.description.clone();
                                let source_type = a.source_type;
                                let file_size = a.file_size;
                                let created_at = a.created_at;
                                let id_delete = id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { class: "detail-table-value-bold", "{name}" }
                                        td {
                                            if description.is_empty() {
                                                span { class: "text-muted", "无描述" }
                                            } else {
                                                "{description}"
                                            }
                                        }
                                        td { span { class: "badge badge-info", "{source_type_text(source_type)}" } }
                                        td { class: "text-mono text-muted", "{format_file_size(file_size)}" }
                                        td { class: "text-mono text-muted", "{created_at}" }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id_delete = id_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_artifact(&id_delete).await {
                                                            toast.error(&format!("删除失败: {}", e));
                                                        } else {
                                                            let pid = selected_project_id();
                                                            if !pid.is_empty() {
                                                                match list_artifacts(&pid).await {
                                                                    Ok(list) => artifacts.set(list),
                                                                    Err(e) => toast.error(&e),
                                                                }
                                                            }
                                                        }
                                                    });
                                                }, "删除"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Modal {
            title: "创建产物".to_string(),
            show: show_add_modal(),
            on_close: move |_| show_add_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "产物名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入产物名称" }
                }
                div { class: "form-group",
                    label { class: "form-label", "描述" }
                    textarea { class: "form-textarea", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "产物描述（可选）" }
                }
            }
        }
    }
}
