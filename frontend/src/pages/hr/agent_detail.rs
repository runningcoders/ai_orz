use crate::api::hr::{
    bind_tool_to_agent, get_agent, install_skill_pack, install_tool_pack,
    list_installed_skill_packs, list_installed_tool_packs, list_tools, uninstall_skill_pack,
    uninstall_tool_pack, unbind_tool_from_agent, update_agent_status,
};
use crate::api::message::{load_latest_messages, send_message_to_agent};
use crate::components::state::{EmptyState, ErrorAlert, Loading, SuccessAlert};
use common::api::{GetAgentResponse, MessageListItem, SendMessageToAgentParams, ToolListItem};
use dioxus::prelude::*;
use dioxus_router::Link;
use std::collections::HashSet;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{Event, MessageEvent, EventSource};

fn format_time(timestamp: i64) -> String {
    use chrono::{DateTime, Local, TimeZone};
    let dt = Local.timestamp_opt(timestamp, 0).unwrap();
    dt.format("%H:%M").to_string()
}

fn is_attachment_message(msg_type: i32) -> bool {
    msg_type >= 2 && msg_type <= 6
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn render_message_content(msg: &MessageListItem) -> Element {
    if is_attachment_message(msg.message_type) {
        if let Some(fm) = &msg.file_meta {
            let file_url = format!("/api/v1/finance/attachments/{}/content", msg.content);
            if msg.message_type == 2 {
                rsx! {
                    div { class: "message-attachment message-attachment-image",
                        img { src: "{file_url}", class: "message-image", loading: "lazy" }
                    }
                }
            } else {
                rsx! {
                    div { class: "message-attachment message-attachment-file",
                        a { href: "{file_url}", class: "attachment-download",
                            div { class: "file-icon", "📄" }
                            div { class: "file-info",
                                span { class: "attachment-name", "{fm.name}" }
                                span { class: "attachment-size", "{format_file_size(fm.size)}" }
                            }
                        }
                    }
                }
            }
        } else {
            rsx! { div { class: "message-bubble", "{msg.content}" } }
        }
    } else {
        rsx! {
            div { class: "message-bubble", "{msg.content}" }
            div { class: "message-time", "{format_time(msg.created_at)}" }
        }
    }
}

fn render_chat_messages(messages: &[MessageListItem], is_typing: bool) -> Element {
    if messages.is_empty() && !is_typing {
        rsx! {
            div { class: "agent-chat-messages",
                div { class: "state-empty",
                    div { class: "state-empty-icon", "💬" }
                    div { "暂无对话记录，发送消息开始对话" }
                }
            }
        }
    } else {
        rsx! {
            div { class: "agent-chat-messages",
                for msg in messages.iter().cloned() {
                    div { class: "message-item {role_class(msg.from_role)}", key: "{msg.message_id}",
                        div { class: "message-avatar", "{role_avatar(msg.from_role)}" }
                        div {
                            {render_message_content(&msg)}
                        }
                    }
                }
                if is_typing {
                    div { class: "message-item agent",
                        div { class: "message-avatar", "A" }
                        div { class: "typing-indicator",
                            div { class: "typing-dot" }
                            div { class: "typing-dot" }
                            div { class: "typing-dot" }
                        }
                    }
                }
            }
        }
    }
}

fn role_class(role: i32) -> &'static str {
    match role {
        1 => "user",
        2 => "agent",
        _ => "system",
    }
}

fn role_avatar(role: i32) -> &'static str {
    match role {
        1 => "U",
        2 => "A",
        _ => "S",
    }
}

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
        0 => "badge badge-danger",
        1 => "badge badge-warning",
        2 => "badge badge-info",
        3 => "badge badge-success",
        4 => "badge badge-danger",
        5 => "badge badge-warning",
        _ => "badge",
    }
}

fn runtime_state_label(state: i32) -> &'static str {
    match state {
        0 => "空闲",
        1 => "思考中",
        2 => "执行中",
        3 => "休息中",
        _ => "未知",
    }
}

fn runtime_state_badge_class(state: i32) -> &'static str {
    match state {
        0 => "badge badge-ghost",
        1 => "badge badge-accent",
        2 => "badge badge-primary",
        3 => "badge badge-info",
        _ => "badge",
    }
}

fn binding_status_badge_class(is_bound: bool) -> &'static str {
    if is_bound {
        "badge badge-success"
    } else {
        "badge badge-ghost"
    }
}

const STATUS_OPTIONS: &[(i32, &str)] = &[
    (1, "面试中"),
    (2, "待入职"),
    (3, "已入职"),
    (5, "待离职"),
    (4, "已离职"),
];

#[component]
pub fn HrAgentDetail(id: String) -> Element {
    let agent_id = id.clone();
    let mut agent_data = use_signal(|| None::<GetAgentResponse>);
    let mut tool_packs = use_signal(Vec::<String>::new);
    let mut skill_packs = use_signal(Vec::<String>::new);
    let mut all_tools = use_signal(Vec::<ToolListItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut new_tool_tag = use_signal(String::new);
    let mut new_skill_tag = use_signal(String::new);

    let mut messages = use_signal(Vec::<MessageListItem>::new);
    let mut input_text = use_signal(String::new);
    let mut is_typing = use_signal(|| false);
    let mut sse_connected = use_signal(|| false);

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
                        match load_latest_messages(None, Some(20)).await {
                            Ok(resp) => messages.set(resp.messages),
                            Err(e) => error.set(format!("加载消息失败: {}", e)),
                        }
                    }
                    Err(e) => {
                        error.set(format!("加载 Agent 失败: {}", e));
                        agent_data.set(None);
                    }
                }
                loading.set(false);
            });
        }
    });

    let mut handle_send_message = move |agent_id: &str, content: &str| {
        if content.trim().is_empty() { return; }
        let agent_id = agent_id.to_string();
        let content = content.trim().to_string();
        input_text.set(String::new());
        is_typing.set(true);
        spawn(async move {
            let req = SendMessageToAgentParams {
                to_agent_id: agent_id.clone(),
                content: content.clone(),
                project_id: None,
                task_id: None,
                reply_to_id: None,
                attachment_ids: None,
            };
            match send_message_to_agent(req).await {
                Ok(_) => {},
                Err(e) => {
                    error.set(format!("发送消息失败: {}", e));
                    is_typing.set(false);
                }
            }
        });
    };

    use_effect(move || {
        let event_source = web_sys::EventSource::new("/api/v1/finance/messages/sse").unwrap();
        sse_connected.set(true);

        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let data = event.data().as_string().unwrap_or_default();
            if data.is_empty() { return; }
            let msg: MessageListItem = match serde_json::from_str(&data) {
                Ok(m) => m,
                Err(_) => return,
            };
            if msg.to_id == agent_id || msg.from_id == agent_id {
                let mut current = messages.write();
                if current.iter().any(|m| m.message_id == msg.message_id) {
                    return;
                }
                current.push(msg);
                is_typing.set(false);
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        event_source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        let on_error = Closure::wrap(Box::new(move |_: web_sys::Event| {
            sse_connected.set(false);
        }) as Box<dyn FnMut(web_sys::Event)>);
        event_source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();

        use_drop(move || {
            event_source.close();
        });
    });

    let agent = agent_data.read().clone();
    let tool_packs_list = tool_packs.read().clone();
    let skill_packs_list = skill_packs.read().clone();
    let all_tools_list = all_tools.read().clone();
    let messages_list = messages.read().clone();

    if loading() {
        rsx! {
            div { class: "card",
                Loading {}
            }
        }
    } else if agent.is_none() {
        rsx! {
            div { class: "card",
                ErrorAlert { message: error() }
                EmptyState { icon: "🤖".to_string(), message: "未找到该 Agent".to_string() }
            }
        }
    } else {
        let a = agent.unwrap();
        let agent_tools: HashSet<String> = a.tools.iter().cloned().collect();
        
        rsx! {
            div { class: "card",
                ErrorAlert { message: error() }
                SuccessAlert { message: success() }

                div { class: "card-header",
                    h2 { class: "card-title", "Agent 详情" }
                    Link { to: crate::pages::Route::HrAgents {},
                        class: "detail-back-link",
                        "← 返回列表"
                    }
                }

                div { class: "detail-section",
                    h3 { class: "detail-section-title", "基本信息" }
                    table { class: "table",
                        tbody {
                            tr { td { class: "text-secondary detail-table-label", "名称" },
                                td { class: "detail-table-value-bold", "{a.name}" } }
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
                                            div { class: "tag-list",
                                                for cap in caps.iter() {
                                                    span { class: "badge badge-info tag-item", "{cap}" }
                                                }
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
                                            pre { class: "soul-prompt", "{soul}" }
                                        }
                                    } else {
                                        span { class: "text-secondary", "—" }
                                    }
                                } }
                        }
                    }
                }

                div { class: "detail-section",
                    h3 { class: "detail-section-title", "状态切换" }
                    div { class: "status-buttons",
                        for (status, label) in STATUS_OPTIONS {
                            let is_current = a.status == *status;
                            let btn_class = if is_current { "btn btn-accent btn-sm" } else { "btn btn-ghost btn-sm" };
                            let target_status = *status;
                            let aid = agent_id.clone();
                            rsx! {
                                button {
                                    class: "{btn_class}",
                                    disabled: is_current,
                                    onclick: move |_| {
                                        let agent_id = aid.clone();
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

                div { class: "detail-section",
                    h3 { class: "detail-section-title", "工具包管理" }
                    div { class: "pack-management",
                        div { class: "pack-input-row",
                            input {
                                class: "input",
                                value: "{new_tool_tag}",
                                placeholder: "输入工具包 tag",
                                oninput: move |e| new_tool_tag.set(e.value()),
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter {
                                        e.prevent_default();
                                        let aid = agent_id.clone();
                                        let tag = new_tool_tag().trim().to_string();
                                        if tag.is_empty() {
                                            error.set("请输入工具包 tag".to_string());
                                            success.set(String::new());
                                            return;
                                        }
                                        spawn(async move {
                                            match install_tool_pack(&aid, &tag).await {
                                                Ok(_) => {
                                                    success.set(format!("工具包 [{}] 安装成功", tag));
                                                    error.set(String::new());
                                                    new_tool_tag.set(String::new());
                                                    match list_installed_tool_packs(&aid).await {
                                                        Ok(resp) => tool_packs.set(resp.installed_tags),
                                                        Err(e) => error.set(format!("刷新工具包列表失败: {}", e)),
                                                    }
                                                }
                                                Err(e) => error.set(format!("安装工具包失败: {}", e)),
                                            }
                                        });
                                    }
                                },
                            }
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    let aid = agent_id.clone();
                                    let tag = new_tool_tag().trim().to_string();
                                    if tag.is_empty() {
                                        error.set("请输入工具包 tag".to_string());
                                        success.set(String::new());
                                        return;
                                    }
                                    spawn(async move {
                                        match install_tool_pack(&aid, &tag).await {
                                            Ok(_) => {
                                                success.set(format!("工具包 [{}] 安装成功", tag));
                                                error.set(String::new());
                                                new_tool_tag.set(String::new());
                                                match list_installed_tool_packs(&aid).await {
                                                    Ok(resp) => tool_packs.set(resp.installed_tags),
                                                    Err(e) => error.set(format!("刷新工具包列表失败: {}", e)),
                                                }
                                            }
                                            Err(e) => error.set(format!("安装工具包失败: {}", e)),
                                        }
                                    });
                                },
                                "安装工具包"
                            }
                        }
                        if !tool_packs_list.is_empty() {
                            div { class: "pack-tags",
                                for tag in tool_packs_list.iter() {
                                    let tag_clone = tag.clone();
                                    let aid = agent_id.clone();
                                    span {
                                        class: "badge badge-accent",
                                        "{tag}"
                                        button {
                                            class: "badge-remove",
                                            onclick: move |_| {
                                                let agent_id = aid.clone();
                                                spawn(async move {
                                                    match uninstall_tool_pack(&agent_id, &tag_clone).await {
                                                        Ok(_) => {
                                                            success.set(format!("工具包 [{}] 已卸载", tag_clone));
                                                            error.set(String::new());
                                                            match list_installed_tool_packs(&agent_id).await {
                                                                Ok(resp) => tool_packs.set(resp.installed_tags),
                                                                Err(e) => error.set(format!("刷新工具包列表失败: {}", e)),
                                                            }
                                                        }
                                                        Err(e) => error.set(format!("卸载工具包失败: {}", e)),
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

                div { class: "detail-section",
                    h3 { class: "detail-section-title", "技能包管理" }
                    div { class: "pack-management",
                        div { class: "pack-input-row",
                            input {
                                class: "input",
                                value: "{new_skill_tag}",
                                placeholder: "输入技能包 tag",
                                oninput: move |e| new_skill_tag.set(e.value()),
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter {
                                        e.prevent_default();
                                        let aid = agent_id.clone();
                                        let tag = new_skill_tag().trim().to_string();
                                        if tag.is_empty() {
                                            error.set("请输入技能包 tag".to_string());
                                            success.set(String::new());
                                            return;
                                        }
                                        spawn(async move {
                                            match install_skill_pack(&aid, &tag).await {
                                                Ok(_) => {
                                                    success.set(format!("技能包 [{}] 安装成功", tag));
                                                    error.set(String::new());
                                                    new_skill_tag.set(String::new());
                                                    match list_installed_skill_packs(&aid).await {
                                                        Ok(resp) => skill_packs.set(resp.skill_packs),
                                                        Err(e) => error.set(format!("刷新技能包列表失败: {}", e)),
                                                    }
                                                }
                                                Err(e) => error.set(format!("安装技能包失败: {}", e)),
                                            }
                                        });
                                    }
                                },
                            }
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    let aid = agent_id.clone();
                                    let tag = new_skill_tag().trim().to_string();
                                    if tag.is_empty() {
                                        error.set("请输入技能包 tag".to_string());
                                        success.set(String::new());
                                        return;
                                    }
                                    spawn(async move {
                                        match install_skill_pack(&aid, &tag).await {
                                            Ok(_) => {
                                                success.set(format!("技能包 [{}] 安装成功", tag));
                                                error.set(String::new());
                                                new_skill_tag.set(String::new());
                                                match list_installed_skill_packs(&aid).await {
                                                    Ok(resp) => skill_packs.set(resp.skill_packs),
                                                    Err(e) => error.set(format!("刷新技能包列表失败: {}", e)),
                                                }
                                            }
                                            Err(e) => error.set(format!("安装技能包失败: {}", e)),
                                        }
                                    });
                                },
                                "安装技能包"
                            }
                        }
                        if !skill_packs_list.is_empty() {
                            div { class: "pack-tags",
                                for tag in skill_packs_list.iter() {
                                    let tag_clone = tag.clone();
                                    let aid = agent_id.clone();
                                    span {
                                        class: "badge badge-info",
                                        "{tag}"
                                        button {
                                            class: "badge-remove",
                                            onclick: move |_| {
                                                let agent_id = aid.clone();
                                                spawn(async move {
                                                    match uninstall_skill_pack(&agent_id, &tag_clone).await {
                                                        Ok(_) => {
                                                            success.set(format!("技能包 [{}] 已卸载", tag_clone));
                                                            error.set(String::new());
                                                            match list_installed_skill_packs(&agent_id).await {
                                                                Ok(resp) => skill_packs.set(resp.skill_packs),
                                                                Err(e) => error.set(format!("刷新技能包列表失败: {}", e)),
                                                            }
                                                        }
                                                        Err(e) => error.set(format!("卸载技能包失败: {}", e)),
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

                div { class: "detail-section",
                    h3 { class: "detail-section-title", "工具绑定" }
                    div { class: "tool-bindings",
                        if !all_tools_list.is_empty() {
                            div { class: "tool-grid",
                                for tool in all_tools_list.iter() {
                                    let tool_clone = tool.clone();
                                    let is_bound = agent_tools.contains(&tool.id);
                                    let aid = agent_id.clone();
                                    div {
                                        class: "tool-card",
                                        key: "{tool.id}",
                                        div { class: "tool-card-header",
                                            span { class: "tool-name", "{tool.name}" }
                                            span { class: "{binding_status_badge_class(is_bound)}",
                                                if is_bound { "已绑定" } else { "未绑定" }
                                            }
                                        }
                                        div { class: "tool-card-body",
                                            p { class: "text-sm text-secondary", "{tool.description}" }
                                            if let Some(tags) = &tool.tags {
                                                div { class: "tool-tags",
                                                    for tag in tags.iter() {
                                                        span { class: "badge badge-ghost", "{tag}" }
                                                    }
                                                }
                                            }
                                        }
                                        div { class: "tool-card-footer",
                                            button {
                                                class: if is_bound { "btn btn-danger btn-sm" } else { "btn btn-primary btn-sm" },
                                                onclick: move |_| {
                                                    let agent_id = aid.clone();
                                                    let tool_id = tool_clone.id.clone();
                                                    spawn(async move {
                                                        let result = if is_bound {
                                                            unbind_tool_from_agent(&agent_id, &tool_id).await
                                                        } else {
                                                            bind_tool_to_agent(&agent_id, &tool_id).await
                                                        };
                                                        match result {
                                                            Ok(_) => {
                                                                success.set(format!("工具 {} {}", tool_clone.name, if is_bound { "已解绑" } else { "已绑定" }));
                                                                error.set(String::new());
                                                                match get_agent(&agent_id).await {
                                                                    Ok(a) => agent_data.set(Some(a)),
                                                                    Err(e) => error.set(format!("刷新 Agent 失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => error.set(format!("操作失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                if is_bound { "解绑" } else { "绑定" }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "state-empty",
                                div { class: "state-empty-icon", "🔧" }
                                div { "暂无可用工具" }
                            }
                        }
                    }
                }

                div { class: "detail-section",
                    h3 { class: "detail-section-title", "与 Agent 对话" }
                    div { class: "agent-chat-container",
                        {render_chat_messages(&messages_list, is_typing())}
                        div { class: "agent-chat-input-area",
                            textarea {
                                class: "chat-input",
                                value: "{input_text}",
                                placeholder: "输入消息...",
                                oninput: move |e| input_text.set(e.value()),
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter && !e.modifiers().contains(Modifiers::SHIFT) {
                                        e.prevent_default();
                                        let aid = agent_id.clone();
                                        let txt = input_text().clone();
                                        handle_send_message(&aid, &txt);
                                    }
                                },
                            }
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    let aid = agent_id.clone();
                                    let txt = input_text().clone();
                                    handle_send_message(&aid, &txt);
                                },
                                disabled: input_text().trim().is_empty(),
                                "发送"
                            }
                        }
                    }
                }
            }
        }
    }
}
