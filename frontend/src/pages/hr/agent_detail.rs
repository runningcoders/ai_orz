//! Agent 详情页 - 基本信息、状态管理、工具包/技能包管理

use dioxus::prelude::*;

use crate::api::hr::{
    bind_tool_to_agent, get_agent, install_skill_pack, install_tool_pack,
    list_installed_skill_packs, list_installed_tool_packs, list_tools, uninstall_skill_pack,
    uninstall_tool_pack, unbind_tool_from_agent, update_agent_status,
};
use crate::components::state::{EmptyState, ErrorAlert, Loading, SuccessAlert};
use common::api::{GetAgentResponse, ToolListItem};

// ===== 枚举映射 =====

fn agent_status_label(status: i32) -> &'static str {
    match status {
        0 => "已删除",
        1 => "面试中",
        2 => "待入职",
        3 => "已入职",
        4 => "已离职",
        5 => "待离职",
        _ => "未知",
    }
}

fn agent_status_badge_class(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",
        1 => "badge badge-info",
        2 => "badge badge-warning",
        3 => "badge badge-success",
        4 => "badge badge-neutral",
        5 => "badge badge-warning",
        _ => "badge badge-neutral",
    }
}

fn runtime_state_label(state: i32) -> &'static str {
    match state {
        0 => "空闲",
        1 => "休息中",
        2 => "忙碌",
        _ => "未知",
    }
}

fn runtime_state_badge_class(state: i32) -> &'static str {
    match state {
        0 => "badge badge-success",
        1 => "badge badge-warning",
        2 => "badge badge-info",
        _ => "badge badge-neutral",
    }
}

fn tool_status_label(status: &common::enums::ToolStatus) -> &'static str {
    match status {
        common::enums::ToolStatus::Enabled => "启用",
        common::enums::ToolStatus::Disabled => "禁用",
        common::enums::ToolStatus::Stale => "异常",
    }
}

fn tool_status_badge_class(status: &common::enums::ToolStatus) -> &'static str {
    match status {
        common::enums::ToolStatus::Enabled => "badge badge-success",
        common::enums::ToolStatus::Disabled => "badge badge-neutral",
        common::enums::ToolStatus::Stale => "badge badge-error",
    }
}

// 可切换的生命周期状态（不含 Deleted=0，删除走列表页）
const STATUS_OPTIONS: &[(i32, &str)] = &[
    (1, "面试中"),
    (2, "待入职"),
    (3, "已入职"),
    (5, "待离职"),
    (4, "已离职"),
];

#[component]
pub fn HrAgentDetail(id: String) -> Element {
    let mut agent_data = use_signal(|| None::<GetAgentResponse>);
    let mut tool_packs = use_signal(Vec::<String>::new);
    let mut skill_packs = use_signal(Vec::<String>::new);
    let mut all_tools = use_signal(Vec::<ToolListItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut new_tool_tag = use_signal(String::new);
    let mut new_skill_tag = use_signal(String::new);

    // 初始加载：先 get_agent，再加载工具包、技能包和工具列表
    use_effect({
        let id = id.clone();
        move || {
            let current_id = id.clone();
            loading.set(true);
            error.set(String::new());
            success.set(String::new());
            spawn(async move {
                match get_agent(&current_id).await {
                    Ok(a) => {
                        agent_data.set(Some(a));
                        match list_installed_tool_packs(&current_id).await {
                            Ok(resp) => tool_packs.set(resp.installed_tags),
                            Err(e) => error.set(format!("加载工具包失败: {}", e)),
                        }
                        match list_installed_skill_packs(&current_id).await {
                            Ok(resp) => skill_packs.set(resp.skill_packs),
                            Err(e) => error.set(format!("加载技能包失败: {}", e)),
                        }
                        match list_tools().await {
                            Ok(resp) => all_tools.set(resp.tools),
                            Err(e) => error.set(format!("加载工具列表失败: {}", e)),
                        }
                    }
                    Err(e) => {
                        agent_data.set(None);
                        error.set(e);
                    }
                }
                loading.set(false);
            });
        }
    });

    let agent = agent_data.read().clone();
    let tool_packs_list = tool_packs.read().clone();
    let skill_packs_list = skill_packs.read().clone();
    let all_tools_list = all_tools.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }

            div { class: "card-header",
                h2 { class: "card-title", "Agent 详情" }
                Link { to: crate::pages::Route::HrAgents {},
                    style: "color: var(--color-mistral-orange); text-decoration: none; font-weight: 500;",
                    "← 返回列表"
                }
            }

            if loading() {
                Loading {}
            } else if let Some(a) = &agent {
                // 1. 基本信息卡片
                div { style: "margin-bottom: var(--space-6);",
                    h3 { style: "font-size: 16px; font-weight: 600; margin-bottom: var(--space-3); color: var(--color-text-primary);", "基本信息" }
                    table { class: "table",
                        tbody {
                            tr { td { class: "text-secondary", style: "width: 160px;", "名称" },
                                td { style: "font-weight: 500;", "{a.name}" } }
                            tr { td { class: "text-secondary", "角色" },
                                td { "{a.roles.join(\", \")}" } }
                            tr { td { class: "text-secondary", "模型提供商" },
                                td { class: "text-mono", "{a.model_provider_id}" } }
                            tr { td { class: "text-secondary", "生命周期状态" },
                                td { span { class: "{agent_status_badge_class(a.status)}", "{agent_status_label(a.status)}" } } }
                            tr { td { class: "text-secondary", "运行时状态" },
                                td { span { class: "{runtime_state_badge_class(a.runtime_state)}", "{runtime_state_label(a.runtime_state)}" } } }
                            tr { td { class: "text-secondary", "当前消息" },
                                td {
                                    if let Some(mid) = &a.current_message_id {
                                        span { class: "text-mono", "{mid}" }
                                    } else {
                                        span { class: "text-secondary", "—" }
                                    }
                                } }
                            tr { td { class: "text-secondary", "描述" },
                                td {
                                    if let Some(desc) = &a.description {
                                        "{desc}"
                                    } else {
                                        span { class: "text-secondary", "—" }
                                    }
                                } }
                            tr { td { class: "text-secondary", "能力标签" },
                                td {
                                    if let Some(caps) = &a.capabilities {
                                        if caps.is_empty() {
                                            span { class: "text-secondary", "—" }
                                        } else {
                                            for cap in caps.iter() {
                                                span { class: "badge badge-info",
                                                    style: "margin-right: var(--space-2); margin-bottom: var(--space-1);",
                                                    "{cap}" }
                                            }
                                        }
                                    } else {
                                        span { class: "text-secondary", "—" }
                                    }
                                } }
                            tr { td { class: "text-secondary", "灵魂提示词" },
                                td {
                                    if let Some(soul) = &a.soul {
                                        if soul.is_empty() {
                                            span { class: "text-secondary", "—" }
                                        } else {
                                            pre { style: "white-space: pre-wrap; word-wrap: break-word; margin: 0; font-family: inherit; max-height: 240px; overflow: auto;", "{soul}" }
                                        }
                                    } else {
                                        span { class: "text-secondary", "—" }
                                    }
                                } }
                        }
                    }
                }

                // 2. 状态管理区域
                div { style: "margin-bottom: var(--space-6);",
                    h3 { style: "font-size: 16px; font-weight: 600; margin-bottom: var(--space-3); color: var(--color-text-primary);", "状态管理" }
                    div { style: "display: flex; flex-wrap: wrap; gap: var(--space-2);",
                        for (status, label) in STATUS_OPTIONS.iter() {
                            {
                                let is_current = a.status == *status;
                                let btn_class = if is_current { "btn btn-accent btn-sm" } else { "btn btn-ghost btn-sm" };
                                let target_status = *status;
                                let agent_id = id.clone();
                                rsx! {
                                    button {
                                        class: "{btn_class}",
                                        disabled: is_current,
                                        onclick: move |_| {
                                            let agent_id = agent_id.clone();
                                            spawn(async move {
                                                match update_agent_status(&agent_id, target_status).await {
                                                    Ok(_) => {
                                                        success.set(format!("状态已更新为：{}", agent_status_label(target_status)));
                                                        error.set(String::new());
                                                        match get_agent(&agent_id).await {
                                                            Ok(a) => agent_data.set(Some(a)),
                                                            Err(e) => error.set(format!("刷新 Agent 失败: {}", e)),
                                                        }
                                                    }
                                                    Err(e) => error.set(format!("状态更新失败: {}", e)),
                                                }
                                            });
                                        },
                                        if is_current { "{label}（当前）" } else { "{label}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // 3. 工具包管理区域
                div { style: "margin-bottom: var(--space-6);",
                    h3 { style: "font-size: 16px; font-weight: 600; margin-bottom: var(--space-3); color: var(--color-text-primary);", "工具包管理" }
                    div { class: "form-group",
                        label { class: "form-label", "安装新工具包" }
                        div { style: "display: flex; gap: var(--space-2);",
                            input { class: "form-input", value: "{new_tool_tag}",
                                oninput: move |e| new_tool_tag.set(e.value()),
                                placeholder: "输入工具包 tag，如 project_management",
                                style: "flex: 1;" }
                            {
                                let agent_id = id.clone();
                                rsx! {
                                    button {
                                        class: "btn btn-accent",
                                        onclick: move |_| {
                                            let tag = new_tool_tag().trim().to_string();
                                            if tag.is_empty() {
                                                error.set("请输入工具包 tag".to_string());
                                                success.set(String::new());
                                                return;
                                            }
                                            let agent_id = agent_id.clone();
                                            spawn(async move {
                                                match install_tool_pack(&agent_id, &tag).await {
                                                    Ok(_) => {
                                                        success.set(format!("工具包 [{}] 安装成功", tag));
                                                        error.set(String::new());
                                                        new_tool_tag.set(String::new());
                                                        match list_installed_tool_packs(&agent_id).await {
                                                            Ok(resp) => tool_packs.set(resp.installed_tags),
                                                            Err(e) => error.set(format!("刷新工具包失败: {}", e)),
                                                        }
                                                    }
                                                    Err(e) => error.set(format!("安装失败: {}", e)),
                                                }
                                            });
                                        },
                                        "安装"
                                    }
                                }
                            }
                        }
                    }
                    div { style: "margin-top: var(--space-3);",
                        div { class: "text-secondary", style: "margin-bottom: var(--space-2);", "已安装工具包（{tool_packs_list.len()}）" }
                        if tool_packs_list.is_empty() {
                            EmptyState { icon: "🎒".to_string(), message: "暂无已安装工具包".to_string() }
                        } else {
                            div { style: "display: flex; flex-wrap: wrap; gap: var(--space-2);",
                                for tag in tool_packs_list.iter() {
                                    {
                                        let tag_clone = tag.clone();
                                        let agent_id = id.clone();
                                        rsx! {
                                            span { class: "badge badge-neutral",
                                                style: "display: inline-flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-3);",
                                                span { class: "text-mono", "{tag}" }
                                                button {
                                                    class: "btn btn-danger btn-sm",
                                                    style: "padding: 0 var(--space-2); font-size: 12px; line-height: 1.4;",
                                                    onclick: move |_| {
                                                        let agent_id = agent_id.clone();
                                                        let tag_clone = tag_clone.clone();
                                                        spawn(async move {
                                                            match uninstall_tool_pack(&agent_id, &tag_clone).await {
                                                                Ok(_) => {
                                                                    success.set(format!("工具包 [{}] 已卸载", tag_clone));
                                                                    error.set(String::new());
                                                                    match list_installed_tool_packs(&agent_id).await {
                                                                        Ok(resp) => tool_packs.set(resp.installed_tags),
                                                                        Err(e) => error.set(format!("刷新工具包失败: {}", e)),
                                                                    }
                                                                }
                                                                Err(e) => error.set(format!("卸载失败: {}", e)),
                                                            }
                                                        });
                                                    },
                                                    "×"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 4. 技能包管理区域
                div {
                    h3 { style: "font-size: 16px; font-weight: 600; margin-bottom: var(--space-3); color: var(--color-text-primary);", "技能包管理" }
                    div { class: "form-group",
                        label { class: "form-label", "安装新技能包" }
                        div { style: "display: flex; gap: var(--space-2);",
                            input { class: "form-input", value: "{new_skill_tag}",
                                oninput: move |e| new_skill_tag.set(e.value()),
                                placeholder: "输入技能包 tag",
                                style: "flex: 1;" }
                            {
                                let agent_id = id.clone();
                                rsx! {
                                    button {
                                        class: "btn btn-accent",
                                        onclick: move |_| {
                                            let tag = new_skill_tag().trim().to_string();
                                            if tag.is_empty() {
                                                error.set("请输入技能包 tag".to_string());
                                                success.set(String::new());
                                                return;
                                            }
                                            let agent_id = agent_id.clone();
                                            spawn(async move {
                                                match install_skill_pack(&agent_id, &tag).await {
                                                    Ok(_) => {
                                                        success.set(format!("技能包 [{}] 安装成功", tag));
                                                        error.set(String::new());
                                                        new_skill_tag.set(String::new());
                                                        match list_installed_skill_packs(&agent_id).await {
                                                            Ok(resp) => skill_packs.set(resp.skill_packs),
                                                            Err(e) => error.set(format!("刷新技能包失败: {}", e)),
                                                        }
                                                    }
                                                    Err(e) => error.set(format!("安装失败: {}", e)),
                                                }
                                            });
                                        },
                                        "安装"
                                    }
                                }
                            }
                        }
                    }
                    div { style: "margin-top: var(--space-3);",
                        div { class: "text-secondary", style: "margin-bottom: var(--space-2);", "已安装技能包（{skill_packs_list.len()}）" }
                        if skill_packs_list.is_empty() {
                            EmptyState { icon: "📚".to_string(), message: "暂无已安装技能包".to_string() }
                        } else {
                            div { style: "display: flex; flex-wrap: wrap; gap: var(--space-2);",
                                for tag in skill_packs_list.iter() {
                                    {
                                        let tag_clone = tag.clone();
                                        let agent_id = id.clone();
                                        rsx! {
                                            span { class: "badge badge-neutral",
                                                style: "display: inline-flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-3);",
                                                span { class: "text-mono", "{tag}" }
                                                button {
                                                    class: "btn btn-danger btn-sm",
                                                    style: "padding: 0 var(--space-2); font-size: 12px; line-height: 1.4;",
                                                    onclick: move |_| {
                                                        let agent_id = agent_id.clone();
                                                        let tag_clone = tag_clone.clone();
                                                        spawn(async move {
                                                            match uninstall_skill_pack(&agent_id, &tag_clone).await {
                                                                Ok(_) => {
                                                                    success.set(format!("技能包 [{}] 已卸载", tag_clone));
                                                                    error.set(String::new());
                                                                    match list_installed_skill_packs(&agent_id).await {
                                                                        Ok(resp) => skill_packs.set(resp.skill_packs),
                                                                        Err(e) => error.set(format!("刷新技能包失败: {}", e)),
                                                                    }
                                                                }
                                                                Err(e) => error.set(format!("卸载失败: {}", e)),
                                                            }
                                                        });
                                                    },
                                                    "×"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 5. 工具绑定区域
                div { style: "margin-bottom: var(--space-6);",
                    h3 { style: "font-size: 16px; font-weight: 600; margin-bottom: var(--space-3); color: var(--color-text-primary);", "绑定工具" }
                    div { style: "margin-top: var(--space-3);",
                        div { class: "text-secondary", style: "margin-bottom: var(--space-2);", "可用工具（{all_tools_list.len()}）" }
                        if all_tools_list.is_empty() {
                            EmptyState { icon: "🛠️".to_string(), message: "暂无可用工具".to_string() }
                        } else {
                            div { style: "display: flex; flex-direction: column; gap: var(--space-2);",
                                for tool in all_tools_list.iter() {
                                    {
                                        let tool_clone = tool.clone();
                                        let agent_id = id.clone();
                                        let is_bound = a.tools.contains(&tool.id);
                                        rsx! {
                                            div { style: "display: flex; align-items: center; gap: var(--space-3); padding: var(--space-3); border: 1px solid var(--color-border); border-radius: var(--radius-md);",
                                                div { style: "flex: 1; min-width: 0;",
                                                    div { style: "display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1);",
                                                        span { style: "font-weight: 600; color: var(--color-text-primary);", "{tool_clone.name}" }
                                                        span { class: "{tool_status_badge_class(&tool_clone.status)}", "{tool_status_label(&tool_clone.status)}" }
                                                    }
                                                    if let Some(desc) = &tool_clone.description {
                                                        if !desc.is_empty() {
                                                            div { style: "font-size: 13px; color: var(--color-text-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{desc}" }
                                                        }
                                                    }
                                                }
                                                {
                                                    let tool_id = tool_clone.id.clone();
                                                    if is_bound {
                                                        rsx! {
                                                            button {
                                                                class: "btn btn-danger btn-sm",
                                                                onclick: move |_| {
                                                                    let agent_id = agent_id.clone();
                                                                    let tool_id = tool_id.clone();
                                                                    spawn(async move {
                                                                        match unbind_tool_from_agent(&agent_id, &tool_id).await {
                                                                            Ok(_) => {
                                                                                success.set("工具解绑成功".to_string());
                                                                                error.set(String::new());
                                                                                match get_agent(&agent_id).await {
                                                                                    Ok(a) => agent_data.set(Some(a)),
                                                                                    Err(e) => error.set(format!("刷新 Agent 失败: {}", e)),
                                                                                }
                                                                            }
                                                                            Err(e) => error.set(format!("解绑失败: {}", e)),
                                                                        }
                                                                    });
                                                                },
                                                                "解绑"
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {
                                                            button {
                                                                class: "btn btn-accent btn-sm",
                                                                onclick: move |_| {
                                                                    let agent_id = agent_id.clone();
                                                                    let tool_id = tool_id.clone();
                                                                    spawn(async move {
                                                                        match bind_tool_to_agent(&agent_id, &tool_id).await {
                                                                            Ok(_) => {
                                                                                success.set("工具绑定成功".to_string());
                                                                                error.set(String::new());
                                                                                match get_agent(&agent_id).await {
                                                                                    Ok(a) => agent_data.set(Some(a)),
                                                                                    Err(e) => error.set(format!("刷新 Agent 失败: {}", e)),
                                                                                }
                                                                            }
                                                                            Err(e) => error.set(format!("绑定失败: {}", e)),
                                                                        }
                                                                    });
                                                                },
                                                                "绑定"
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
            } else {
                EmptyState { icon: "🤖".to_string(), message: "未找到该 Agent".to_string() }
            }
        }
    }
}
