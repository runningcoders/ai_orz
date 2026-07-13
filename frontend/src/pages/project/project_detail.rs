//! 项目详情页 - 基本信息、状态管理、任务列表、产物列表

use dioxus::prelude::*;

use crate::api::project::{
    create_artifact, delete_artifact, get_project, list_artifacts, list_project_tasks,
    update_project_status, update_task_status,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading, SuccessAlert};
use common::api::{ArtifactDetail, CreateArtifactRequest, GetProjectResponse, TaskListItem};
use common::enums::ArtifactSourceType;

fn project_status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",
        1 => "badge badge-info",
        2 => "badge badge-warning",
        3 => "badge badge-primary",
        4 => "badge badge-success",
        5 => "badge badge-neutral",
        _ => "badge badge-neutral",
    }
}

fn project_status_text(status: i32) -> &'static str {
    match status {
        0 => "已删除",
        1 => "活跃",
        2 => "待审核",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

fn task_status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",
        1 => "badge badge-warning",
        2 => "badge badge-info",
        3 => "badge badge-primary",
        4 => "badge badge-success",
        5 => "badge badge-neutral",
        _ => "badge badge-neutral",
    }
}

fn task_status_text(status: i32) -> &'static str {
    match status {
        0 => "已取消",
        1 => "待审核",
        2 => "待处理",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

fn artifact_source_type_text(source_type: ArtifactSourceType) -> &'static str {
    match source_type {
        ArtifactSourceType::Attachment => "附件",
        ArtifactSourceType::GeneratedContent => "生成内容",
        ArtifactSourceType::RemoteUrl => "远程链接",
    }
}

#[component]
pub fn ProjectDetail(id: String) -> Element {
    let mut project = use_signal(|| None::<GetProjectResponse>);
    let mut tasks = use_signal(Vec::<TaskListItem>::new);
    let mut artifacts = use_signal(Vec::<ArtifactDetail>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);

    // 产物新增 Modal 状态
    let mut show_artifact_modal = use_signal(|| false);
    let mut new_artifact_name = use_signal(String::new);
    let mut new_artifact_description = use_signal(String::new);

    // 初始加载：先取项目，再取任务列表和产物列表
    let id_for_load = id.clone();
    use_effect(move || {
        loading.set(true);
        error.set(String::new());
        let id_clone = id_for_load.clone();
        spawn(async move {
            match get_project(&id_clone).await {
                Ok(p) => project.set(Some(p)),
                Err(e) => error.set(e),
            }
            match list_project_tasks(&id_clone).await {
                Ok(resp) => tasks.set(resp.tasks),
                Err(e) => error.set(e),
            }
            match list_artifacts(&id_clone).await {
                Ok(list) => artifacts.set(list),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    // 项目状态切换：启动(3)
    let id_for_start = id.clone();
    let start_project = move |_| {
        let id_clone = id_for_start.clone();
        spawn(async move {
            match update_project_status(&id_clone, 3).await {
                Ok(_) => {
                    success.set("项目已启动".to_string());
                    match get_project(&id_clone).await {
                        Ok(p) => project.set(Some(p)),
                        Err(e) => error.set(e),
                    }
                }
                Err(e) => error.set(format!("启动失败: {}", e)),
            }
        });
    };

    // 项目状态切换：完成(4)
    let id_for_complete = id.clone();
    let complete_project = move |_| {
        let id_clone = id_for_complete.clone();
        spawn(async move {
            match update_project_status(&id_clone, 4).await {
                Ok(_) => {
                    success.set("项目已完成".to_string());
                    match get_project(&id_clone).await {
                        Ok(p) => project.set(Some(p)),
                        Err(e) => error.set(e),
                    }
                }
                Err(e) => error.set(format!("完成失败: {}", e)),
            }
        });
    };

    // 项目状态切换：归档(5)
    let id_for_archive = id.clone();
    let archive_project = move |_| {
        let id_clone = id_for_archive.clone();
        spawn(async move {
            match update_project_status(&id_clone, 5).await {
                Ok(_) => {
                    success.set("项目已归档".to_string());
                    match get_project(&id_clone).await {
                        Ok(p) => project.set(Some(p)),
                        Err(e) => error.set(e),
                    }
                }
                Err(e) => error.set(format!("归档失败: {}", e)),
            }
        });
    };

    // 打开新增产物 Modal
    let open_artifact_modal = move |_| {
        new_artifact_name.set(String::new());
        new_artifact_description.set(String::new());
        show_artifact_modal.set(true);
    };

    // 关闭新增产物 Modal
    let close_artifact_modal = move |_| {
        show_artifact_modal.set(false);
    };

    // 提交新增产物
    let id_for_artifact_create = id.clone();
    let submit_artifact = move |_| {
        let pid = id_for_artifact_create.clone();
        let name = new_artifact_name.read().clone();
        let description = new_artifact_description.read().clone();
        if name.trim().is_empty() {
            error.set("产物名称不能为空".to_string());
            return;
        }
        spawn(async move {
            let req = CreateArtifactRequest {
                project_id: pid.clone(),
                task_id: None,
                name: name.trim().to_string(),
                description: Some(description).filter(|s| !s.trim().is_empty()),
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
                    success.set("产物已创建".to_string());
                    show_artifact_modal.set(false);
                    new_artifact_name.set(String::new());
                    new_artifact_description.set(String::new());
                    match list_artifacts(&pid).await {
                        Ok(list) => artifacts.set(list),
                        Err(e) => error.set(e),
                    }
                }
                Err(e) => error.set(format!("创建产物失败: {}", e)),
            }
        });
    };

    let project_data = project.read().clone();
    let tasks_list = tasks.read().clone();
    let artifacts_list = artifacts.read().clone();

    rsx! {
        ErrorAlert { message: error() }
        SuccessAlert { message: success() }

        if loading() {
            div { class: "card", Loading {} }
        } else if let Some(p) = &project_data {
            // 区域 1：项目基本信息卡片
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "{p.name}" }
                }
                div { style: "padding: 16px; display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px;",
                    div {
                        label { class: "form-label", "描述" }
                        if let Some(desc) = &p.description {
                            if desc.is_empty() {
                                span { class: "text-muted", "暂无描述" }
                            } else {
                                "{desc}"
                            }
                        } else {
                            span { class: "text-muted", "暂无描述" }
                        }
                    }
                    div {
                        label { class: "form-label", "状态" }
                        span { class: "{project_status_badge(p.status)}", "{project_status_text(p.status)}" }
                    }
                    div {
                        label { class: "form-label", "优先级" }
                        span { "{p.priority}" }
                    }
                    div {
                        label { class: "form-label", "标签" }
                        if p.tags.is_empty() {
                            span { class: "text-muted", "无标签" }
                        } else {
                            for tag in p.tags.iter() {
                                span { class: "badge badge-neutral", style: "margin-right: 4px;", "{tag}" }
                            }
                        }
                    }
                    div {
                        label { class: "form-label", "创建时间" }
                        span { class: "text-mono text-muted", "{p.created_at}" }
                    }
                }
            }

            // 区域 2：状态管理
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "状态管理" }
                }
                div { style: "padding: 16px; display: flex; gap: 8px; flex-wrap: wrap;",
                    if p.status != 3 {
                        button { class: "btn btn-primary", onclick: start_project, "启动项目" }
                    }
                    if p.status != 4 {
                        button { class: "btn btn-accent", onclick: complete_project, "完成项目" }
                    }
                    if p.status != 5 {
                        button { class: "btn btn-secondary", onclick: archive_project, "归档项目" }
                    }
                }
            }

            // 区域 3：任务列表
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "任务列表" }
                }
                if tasks_list.is_empty() {
                    EmptyState { icon: "📋".to_string(), message: "暂无任务".to_string() }
                } else {
                    table { class: "table",
                        thead { tr {
                            th { "标题" }
                            th { "状态" }
                            th { "优先级" }
                            th { "进度" }
                            th { "操作" }
                        }}
                        tbody {
                            for t in tasks_list.iter() {
                                {
                                    let task_id = t.id.clone();
                                    let task_title = t.title.clone();
                                    let task_status = t.status;
                                    let task_priority = t.priority;
                                    let task_progress = t.progress;
                                    let tid_start = task_id.clone();
                                    let tid_complete = task_id.clone();
                                    let pid_start = id.clone();
                                    let pid_complete = id.clone();
                                    rsx! {
                                        tr { key: "{task_id}",
                                            td { "{task_title}" }
                                            td { span { class: "{task_status_badge(task_status)}", "{task_status_text(task_status)}" } }
                                            td { "{task_priority}" }
                                            td {
                                                div { style: "display: flex; align-items: center; gap: 6px;",
                                                    div { style: "width: 100px; height: 8px; background: var(--color-border-light); border-radius: 3px; overflow: hidden;",
                                                        div { style: "width: {task_progress}%; height: 100%; background: var(--color-mistral-orange); border-radius: 3px;" }
                                                    }
                                                    span { class: "text-muted text-mono", style: "font-size: 12px;", "{task_progress}%" }
                                                }
                                            }
                                            td {
                                                div { style: "display: flex; gap: 6px;",
                                                    if task_status != 3 {
                                                        button { class: "btn btn-secondary btn-sm",
                                                            onclick: move |_| {
                                                                let tid = tid_start.clone();
                                                                let pid = pid_start.clone();
                                                                spawn(async move {
                                                                    match update_task_status(&tid, 3).await {
                                                                        Ok(_) => {
                                                                            success.set("任务已开始".to_string());
                                                                            match list_project_tasks(&pid).await {
                                                                                Ok(resp) => tasks.set(resp.tasks),
                                                                                Err(e) => error.set(e),
                                                                            }
                                                                        }
                                                                        Err(e) => error.set(format!("操作失败: {}", e)),
                                                                    }
                                                                });
                                                            },
                                                            "开始"
                                                        }
                                                    }
                                                    if task_status != 4 {
                                                        button { class: "btn btn-accent btn-sm",
                                                            onclick: move |_| {
                                                                let tid = tid_complete.clone();
                                                                let pid = pid_complete.clone();
                                                                spawn(async move {
                                                                    match update_task_status(&tid, 4).await {
                                                                        Ok(_) => {
                                                                            success.set("任务已完成".to_string());
                                                                            match list_project_tasks(&pid).await {
                                                                                Ok(resp) => tasks.set(resp.tasks),
                                                                                Err(e) => error.set(e),
                                                                            }
                                                                        }
                                                                        Err(e) => error.set(format!("操作失败: {}", e)),
                                                                    }
                                                                });
                                                            },
                                                            "完成"
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
                }
            }

            // 区域 4：产物列表
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "项目产物" }
                }
                div { style: "padding: 16px;",
                    div { style: "margin-bottom: 12px;",
                        button { class: "btn btn-primary", onclick: open_artifact_modal, "+ 新增产物" }
                    }
                    if artifacts_list.is_empty() {
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
                                        let artifact_id = a.id.clone();
                                        let artifact_name = a.name.clone();
                                        let artifact_description = a.description.clone();
                                        let artifact_source_type = a.source_type;
                                        let artifact_file_size = a.file_size;
                                        let artifact_created_at = a.created_at;
                                        let aid_delete = artifact_id.clone();
                                        let pid_refresh = id.clone();
                                        rsx! {
                                            tr { key: "{artifact_id}",
                                                td { "{artifact_name}" }
                                                td {
                                                    if artifact_description.is_empty() {
                                                        span { class: "text-muted", "暂无描述" }
                                                    } else {
                                                        "{artifact_description}"
                                                    }
                                                }
                                                td { span { class: "badge badge-neutral", "{artifact_source_type_text(artifact_source_type)}" } }
                                                td { "{artifact_file_size}" }
                                                td { span { class: "text-mono text-muted", "{artifact_created_at}" } }
                                                td {
                                                    button { class: "btn btn-error btn-sm",
                                                        onclick: move |_| {
                                                            let aid = aid_delete.clone();
                                                            let pid = pid_refresh.clone();
                                                            spawn(async move {
                                                                match delete_artifact(&aid).await {
                                                                    Ok(_) => {
                                                                        success.set("产物已删除".to_string());
                                                                        match list_artifacts(&pid).await {
                                                                            Ok(list) => artifacts.set(list),
                                                                            Err(e) => error.set(e),
                                                                        }
                                                                    }
                                                                    Err(e) => error.set(format!("删除失败: {}", e)),
                                                                }
                                                            });
                                                        },
                                                        "删除"
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
            }

            // 新增产物 Modal
            Modal {
                title: "新增产物".to_string(),
                show: show_artifact_modal(),
                on_close: close_artifact_modal,
                footer: Some(rsx! {
                    div { style: "display: flex; gap: 8px; justify-content: flex-end;",
                        button { class: "btn btn-secondary", onclick: move |_| { show_artifact_modal.set(false); }, "取消" }
                        button { class: "btn btn-primary", onclick: submit_artifact, "创建" }
                    }
                }),
                div { style: "padding: 16px; display: flex; flex-direction: column; gap: 12px;",
                    div {
                        label { class: "form-label", "名称" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "请输入产物名称",
                            value: "{new_artifact_name}",
                            oninput: move |e| new_artifact_name.set(e.value().clone()),
                        }
                    }
                    div {
                        label { class: "form-label", "描述" }
                        textarea {
                            class: "form-input",
                            placeholder: "请输入产物描述（可选）",
                            value: "{new_artifact_description}",
                            oninput: move |e| new_artifact_description.set(e.value().clone()),
                            rows: 3,
                        }
                    }
                }
            }
        } else {
            div { class: "card", EmptyState { icon: "📁".to_string(), message: "项目不存在".to_string() } }
        }
    }
}
