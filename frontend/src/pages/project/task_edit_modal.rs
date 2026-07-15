//! 任务创建/编辑弹窗
//!
//! 支持两种模式：
//! - mode = "create"：创建新任务，assignee_type 默认 Agent
//! - mode = "edit"：编辑已有任务，预填充表单字段

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::api::hr::list_agents;
use crate::api::project::{create_task, get_task, list_projects, update_task};
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{
    CreateTaskRequest, GetTaskResponse, ListAgentsResponseItem, ListProjectsResponseItem,
    UpdateTaskRequest,
};
use common::enums::AssigneeType;

/// 弹窗模式
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEditMode {
    /// 在指定项目下创建任务
    Create { project_id: Option<String> },
    /// 编辑已有任务
    Edit { task_id: String },
}

/// 弹窗 Props
#[derive(Props, Clone, PartialEq)]
pub struct TaskEditModalProps {
    /// 弹窗模式（创建/编辑）
    pub mode: TaskEditMode,
    /// 是否显示
    pub show: bool,
    /// 关闭回调
    pub on_close: EventHandler<()>,
    /// 提交成功回调（用于通知父组件刷新）
    pub on_success: EventHandler<GetTaskResponse>,
}

#[component]
pub fn TaskEditModal(props: TaskEditModalProps) -> Element {
    // 表单状态
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut priority = use_signal(|| 0i32);
    let mut tags_input = use_signal(String::new); // 逗号分隔的字符串
    let mut due_at = use_signal(String::new); // ISO8601 字符串
    let mut assignee_type = use_signal(|| AssigneeType::Agent);
    let mut assignee_id = use_signal(String::new);
    let mut project_id = use_signal(String::new);
    let mut dependencies_input = use_signal(String::new); // 逗号分隔的 task id 列表

    // 下拉数据
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut agents = use_signal(Vec::<ListAgentsResponseItem>::new);

    // 提交状态
    let mut submitting = use_signal(|| false);
    let mut loading_data = use_signal(|| true);

    let toast = use_toast();
    let _navigator = use_navigator();

    // 编辑模式：加载已有任务数据
    let mode_for_load = props.mode.clone();
    let show_for_load = props.show;
    use_effect(move || {
        if !show_for_load {
            return;
        }
        loading_data.set(true);

        // 加载项目下拉数据
        let pid_initial = match &mode_for_load {
            TaskEditMode::Create { project_id } => project_id.clone().unwrap_or_default(),
            TaskEditMode::Edit { .. } => String::new(),
        };

        // 克隆 mode 供 spawn 闭包使用（use_effect 是 FnMut，不能 move 捕获变量）
        let mode_for_async = mode_for_load.clone();
        spawn(async move {
            // 加载项目列表
            match list_projects().await {
                Ok(list) => {
                    // 在 move 之前预先决定 project_id
                    let pid_to_set = if !pid_initial.is_empty() {
                        Some(pid_initial.clone())
                    } else {
                        list.projects.first().map(|p| p.id.clone())
                    };
                    projects.set(list.projects);
                    if let Some(pid) = pid_to_set {
                        project_id.set(pid);
                    }
                }
                Err(e) => toast.error(&e),
            }
            // 加载 Agent 列表
            match list_agents().await {
                Ok(resp) => {
                    // 在 move 之前预先决定默认 assignee
                    let first_agent_id = resp.agents.first().map(|a| a.id.clone());
                    agents.set(resp.agents);
                    if let Some(id) = first_agent_id {
                        assignee_id.set(id);
                    }
                }
                Err(e) => toast.error(&e),
            }

            // 编辑模式：加载任务详情
            if let TaskEditMode::Edit { task_id } = &mode_for_async {
                match get_task(task_id).await {
                    Ok(t) => {
                        title.set(t.title);
                        description.set(t.description.unwrap_or_default());
                        priority.set(t.priority);
                        tags_input.set(t.tags.join(","));
                        due_at.set(format_timestamp(t.due_at));
                        assignee_type.set(if t.assignee_type == 0 {
                            AssigneeType::User
                        } else {
                            AssigneeType::Agent
                        });
                        assignee_id.set(t.assignee_id);
                        project_id.set(t.project_id.unwrap_or_default());
                        dependencies_input.set(t.dependencies.join(","));
                    }
                    Err(e) => toast.error(&e),
                }
            }
            loading_data.set(false);
        });
    });

    // 提交
    let mode_for_submit = props.mode.clone();
    let on_success = props.on_success;
    let on_close = props.on_close;
    let handle_submit = move |_| {
        let title_val = title();
        if title_val.trim().is_empty() {
            toast.error("任务标题不能为空");
            return;
        }
        submitting.set(true);
        let mode_clone = mode_for_submit.clone();
        // 提前判断模式（避免 match 之后部分 move）
        let is_create_for_msg = matches!(&mode_clone, TaskEditMode::Create { .. });
        spawn(async move {
            let result = match mode_clone {
                TaskEditMode::Create { .. } => {
                    let req = CreateTaskRequest {
                        title: title_val.trim().to_string(),
                        description: if description().is_empty() {
                            None
                        } else {
                            Some(description())
                        },
                        priority: Some(priority()),
                        tags: parse_csv(&tags_input()),
                        root_user_id: None,
                        assignee_type: Some(assignee_type()),
                        assignee_id: if assignee_id().is_empty() {
                            "default".to_string()
                        } else {
                            assignee_id()
                        },
                        project_id: if project_id().is_empty() {
                            None
                        } else {
                            Some(project_id())
                        },
                        due_at: parse_timestamp(&due_at()),
                        dependencies: parse_csv(&dependencies_input()),
                    };
                    create_task(req).await
                }
                TaskEditMode::Edit { task_id } => {
                    let req = UpdateTaskRequest {
                        id: task_id.clone(),
                        title: Some(title_val.trim().to_string()),
                        description: Some(if description().is_empty() {
                            String::new()
                        } else {
                            description()
                        }),
                        priority: Some(priority()),
                        tags: parse_csv(&tags_input()),
                        due_at: parse_timestamp(&due_at()),
                        dependencies: parse_csv(&dependencies_input()),
                    };
                    update_task(&task_id, req).await
                }
            };
            submitting.set(false);
            match result {
                Ok(t) => {
                    toast.success(if is_create_for_msg {
                        "任务已创建"
                    } else {
                        "任务已更新"
                    });
                    on_success.call(t);
                    on_close.call(());
                }
                Err(e) => toast.error(&e),
            }
        });
    };

    let is_create = matches!(props.mode, TaskEditMode::Create { .. });
    let title_label = if is_create { "新建任务" } else { "编辑任务" };

    rsx! {
        Modal {
            title: title_label.to_string(),
            show: props.show,
            on_close: move |_| on_close.call(()),
            footer: Some(rsx! {
                div { class: "modal-footer-actions",
                    button {
                        class: "btn btn-secondary",
                        disabled: submitting(),
                        onclick: move |_| on_close.call(()),
                        "取消"
                    }
                    button {
                        class: "btn btn-primary",
                        disabled: submitting() || loading_data(),
                        onclick: handle_submit,
                        if submitting() { "提交中..." } else { if is_create { "创建" } else { "保存" } }
                    }
                }
            }),
            if loading_data() {
                div { class: "modal-body-stack",
                    p { class: "text-muted", "加载中..." }
                }
            } else {
                div { class: "modal-body-stack",
                    // 标题
                    div {
                        label { class: "form-label", "标题 *" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "请输入任务标题",
                            value: "{title}",
                            oninput: move |e| title.set(e.value().clone()),
                        }
                    }
                    // 描述
                    div {
                        label { class: "form-label", "描述" }
                        textarea {
                            class: "form-input",
                            placeholder: "请输入任务描述（可选）",
                            value: "{description}",
                            oninput: move |e| description.set(e.value().clone()),
                            rows: 3,
                        }
                    }
                    // 优先级
                    div {
                        label { class: "form-label", "优先级（0-10）" }
                        input {
                            class: "form-input",
                            r#type: "number",
                            min: "0",
                            max: "10",
                            value: "{priority}",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<i32>() {
                                    priority.set(v.clamp(0, 10));
                                }
                            },
                        }
                    }
                    // 标签
                    div {
                        label { class: "form-label", "标签（逗号分隔）" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "例如：urgent, frontend, bug",
                            value: "{tags_input}",
                            oninput: move |e| tags_input.set(e.value().clone()),
                        }
                    }
                    // 截止时间
                    div {
                        label { class: "form-label", "截止时间（Unix 毫秒时间戳）" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "例如：1720454400000",
                            value: "{due_at}",
                            oninput: move |e| due_at.set(e.value().clone()),
                        }
                    }
                    // 分配对象类型
                    div {
                        label { class: "form-label", "分配对象类型" }
                        select {
                            class: "form-input",
                            value: "{assignee_type().to_i32()}",
                            onchange: move |e| {
                                let v = e.value();
                                if v == "0" {
                                    assignee_type.set(AssigneeType::User);
                                } else {
                                    assignee_type.set(AssigneeType::Agent);
                                }
                            },
                            option { value: "1", "Agent" }
                            option { value: "0", "User" }
                        }
                    }
                    // 分配对象 ID
                    div {
                        label { class: "form-label", "分配对象" }
                        if matches!(assignee_type(), AssigneeType::Agent) {
                            select {
                                class: "form-input",
                                value: "{assignee_id}",
                                onchange: move |e| assignee_id.set(e.value().clone()),
                                option { value: "", "请选择 Agent" }
                                for agent in agents.read().iter() {
                                    option { value: "{agent.id}", "{agent.name}" }
                                }
                            }
                        } else {
                            input {
                                class: "form-input",
                                r#type: "text",
                                placeholder: "请输入用户 ID",
                                value: "{assignee_id}",
                                oninput: move |e| assignee_id.set(e.value().clone()),
                            }
                        }
                    }
                    // 关联项目
                    div {
                        label { class: "form-label", "关联项目" }
                        select {
                            class: "form-input",
                            value: "{project_id}",
                            onchange: move |e| project_id.set(e.value().clone()),
                            option { value: "", "无（独立任务）" }
                            for p in projects.read().iter() {
                                option { value: "{p.id}", "{p.name}" }
                            }
                        }
                    }
                    // 前置任务
                    div {
                        label { class: "form-label", "前置任务 ID（逗号分隔）" }
                        input {
                            class: "form-input",
                            r#type: "text",
                            placeholder: "例如：task_id_1, task_id_2",
                            value: "{dependencies_input}",
                            oninput: move |e| dependencies_input.set(e.value().clone()),
                        }
                    }
                }
            }
        }
    }
}

/// 解析逗号分隔字符串为 Vec<String>（过滤空值）
fn parse_csv(s: &str) -> Option<Vec<String>> {
    let items: Vec<String> = s
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// 解析时间戳字符串为 i64（毫秒）
fn parse_timestamp(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse::<i64>().ok()
}

/// 格式化时间戳为可编辑字符串
fn format_timestamp(ts: Option<i64>) -> String {
    ts.map(|t| t.to_string()).unwrap_or_default()
}
