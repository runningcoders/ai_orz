use crate::api::{hr::*, StatsOptions};
use crate::pages::hr::agent_memory_panel::AgentMemoryPanel;
use crate::api::message::{load_latest_messages, send_message_to_agent};
use crate::components::state::Loading;
use crate::components::stats::AgentStatsPanel;
use crate::store::toast::use_toast;
use common::api::{GetAgentResponse, MessageListItem, SendMessageToAgentParams, ToolListItem};
use dioxus::prelude::*;
use dioxus_router::Link;
use std::collections::HashSet;
use wasm_bindgen::{closure::Closure, JsCast};

fn format_time(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    let dt = Local.timestamp_opt(timestamp / 1000, 0).unwrap();
    dt.format("%H:%M").to_string()
}

fn is_attachment_message(msg_type: i32) -> bool {
    matches!(msg_type, 1 | 2 | 3 | 4)
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
            if msg.message_type == 1 {
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
        0 => "user",
        1 => "agent",
        2 => "system",
        _ => "other",
    }
}

fn role_avatar(role: i32) -> &'static str {
    match role {
        0 => "U",
        1 => "A",
        2 => "S",
        _ => "?",
    }
}

fn binding_status_badge_class(is_bound: bool) -> &'static str {
    if is_bound {
        "badge badge-success"
    } else {
        "badge badge-ghost"
    }
}

fn agent_status_label(status: i32) -> String {
    match status {
        0 => "空闲".to_string(),
        1 => "思考中".to_string(),
        2 => "已入职".to_string(),
        3 => "休息中".to_string(),
        _ => status.to_string(),
    }
}

const STATUS_OPTIONS: &[(i32, &str)] = &[
    (0, "空闲"),
    (1, "思考中"),
    (2, "已入职"),
    (3, "休息中"),
];

#[component]
pub fn HrAgentDetail(id: String) -> Element {
    let mut agent_data = use_signal(|| Option::<GetAgentResponse>::None);
    let mut messages = use_signal(Vec::<MessageListItem>::new);
    let mut is_typing = use_signal(|| false);
    let mut input_message = use_signal(String::new);
    let toast = use_toast();
    let mut tool_packs = use_signal(Vec::<String>::new);
    let mut skill_packs = use_signal(Vec::<String>::new);
    let mut all_tools = use_signal(Vec::<ToolListItem>::new);

    let agent_tool_ids = agent_data
        .read()
        .as_ref()
        .map(|a| a.tools.iter().cloned().collect::<HashSet<_>>())
        .unwrap_or_default();

    let skill_packs_list = skill_packs.read().clone();
    let tool_packs_list = tool_packs.read().clone();
    let all_tools_list = all_tools.read().clone();

    let id_for_load = id.clone();
    let load_data = move || {
        let aid = id_for_load.clone();
        spawn(async move {
            let stats_options = StatsOptions {
                with_stats: true,
                with_model_call_stats: true,
                stats_interval: Some("daily".to_string()),
            };
            match get_agent(&aid, Some(&stats_options)).await {
                Ok(a) => agent_data.set(Some(a)),
                Err(e) => toast.error(&format!("获取 Agent 失败: {}", e)),
            }
            match list_installed_tool_packs(&aid).await {
                Ok(resp) => tool_packs.set(resp.installed_tags),
                Err(e) => toast.error(&format!("获取工具包失败: {}", e)),
            }
            match list_installed_skill_packs(&aid).await {
                Ok(resp) => skill_packs.set(resp.skill_packs),
                Err(e) => toast.error(&format!("获取技能包失败: {}", e)),
            }
            match list_tools().await {
                Ok(resp) => all_tools.set(resp.tools),
                Err(e) => toast.error(&format!("获取工具列表失败: {}", e)),
            }
            match load_latest_messages(None, Some(20)).await {
                Ok(resp) => messages.set(resp.messages),
                Err(e) => toast.error(&format!("加载消息失败: {}", e)),
            }
        });
    };

    use_effect(move || {
        load_data();
    });

    // SSE 监听 - 内联处理，克隆需要的信号
    let sse_id = id.clone();

    use_effect(move || {
        let event_source = web_sys::EventSource::new("/api/v1/finance/messages/sse").unwrap();
        let inner_id = sse_id.clone();
        let mut inner_messages = messages.clone();
        let mut inner_is_typing = is_typing.clone();

        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let data = event.data().as_string().unwrap_or_default();
            let msg: MessageListItem = match serde_json::from_str(&data) {
                Ok(m) => m,
                Err(_) => return,
            };
            if msg.to_id == inner_id || msg.from_id == inner_id {
                let mut current = inner_messages.write();
                if current.iter().any(|m| m.message_id == msg.message_id) {
                    return;
                }
                current.push(msg);
                inner_is_typing.set(false);
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        event_source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        use_drop(move || {
            event_source.close();
        });
    });

    let id_for_send = id.clone();
    let handle_send = use_callback(move |_: ()| {
        let text = input_message().trim().to_string();
        if text.is_empty() {
            return;
        }
        let aid = id_for_send.clone();

        input_message.set(String::new());
        is_typing.set(true);

        spawn(async move {
            let req = SendMessageToAgentParams {
                to_agent_id: aid.clone(),
                content: text.clone(),
                project_id: None,
                task_id: None,
                reply_to_id: None,
                attachment_ids: None,
            };

            match send_message_to_agent(req).await {
                Ok(_) => {}
                Err(e) => {
                    toast.error(&format!("发送消息失败: {}", e));
                    is_typing.set(false);
                }
            }
        });
    });

    match agent_data.read().as_ref() {
        None => rsx! { Loading {} },
        Some(a) => {
            let capabilities = a.capabilities.clone().unwrap_or_default();
            let desc = a.description.as_deref().unwrap_or("");
            let agent_id_signal = use_signal(|| id.clone());

            rsx! {
                div { class: "card",
                    div { class: "card-header",
                        h2 { class: "card-title", "{a.name}" }
                        p { class: "card-subtitle", "{desc}" }
                    }

                    div { class: "card-body",
                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "基本信息" }
                            div { class: "detail-grid",
                                div { class: "detail-item",
                                    span { class: "detail-label", "ID" }
                                    span { class: "detail-value", "{a.id}" }
                                }
                                div { class: "detail-item",
                                    span { class: "detail-label", "状态" }
                                    span { class: "{binding_status_badge_class(a.status != 0)}",
                                        "{agent_status_label(a.status)}"
                                    }
                                }
                                div { class: "detail-item",
                                    span { class: "detail-label", "组织" }
                                    span { class: "detail-value", "{a.model_provider_id}" }
                                }
                                div { class: "detail-item",
                                    span { class: "detail-label", "创建时间" }
                                    span { class: "detail-value", "{format_time(a.created_at)}" }
                                }
                            }
                        }

                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "核心能力" }
                            if !capabilities.is_empty() {
                                div { class: "capability-list",
                                    for cap in capabilities.iter() {
                                        span { class: "badge badge-info", "{cap}" }
                                    }
                                }
                            } else {
                                div { class: "text-secondary text-sm", "暂无核心能力" }
                            }
                        }

                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "状态切换" }
                            div { class: "status-buttons",
                                for (status, label) in STATUS_OPTIONS {
                                    {
                                        let is_current = a.status == *status;
                                        let btn_class = if is_current { "btn btn-accent btn-sm" } else { "btn btn-ghost btn-sm" };
                                        let target_status_val = *status;
                                        let aid = agent_id_signal();
                                        let label_str = label.to_string();
                                        let label_for_closure = label_str.clone();
                                        rsx! {
                                            button {
                                                class: "{btn_class}",
                                                disabled: is_current,
                                                onclick: move |_| {
                                                    let agent_id = aid.clone();
                                                    let label_clone = label_for_closure.clone();
                                                    spawn(async move {
                                                        match update_agent_status(&agent_id, target_status_val).await {
                                                            Ok(_) => {
                                                                toast.success(&format!("状态已更新为：{}", label_clone));
                                                                let stats_options = StatsOptions {
                                                                    with_stats: true,
                                                                    with_model_call_stats: true,
                                                                    stats_interval: Some("daily".to_string()),
                                                                };
                                                                match get_agent(&agent_id, Some(&stats_options)).await {
                                                                    Ok(a) => agent_data.set(Some(a)),
                                                                    Err(e) => toast.error(&format!("刷新 Agent 失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(&format!("状态更新失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                if is_current { "{label_str}（当前）" } else { "{label_str}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "工具包" }
                            div { class: "pack-management",
                                input {
                                    class: "input input-sm",
                                    r#type: "text",
                                    placeholder: "输入工具包 tag 名称",
                                    oninput: move |e| input_message.set(e.value().clone()),
                                }
                                button {
                                    class: "btn btn-primary btn-sm",
                                    onclick: move |_| {
                                        let tag = input_message().trim().to_string();
                                        if tag.is_empty() {
                                            return;
                                        }
                                        let aid = agent_id_signal();
                                        input_message.set(String::new());
                                        spawn(async move {
                                            match install_tool_pack(&aid, &tag).await {
                                                Ok(_) => {
                                                    toast.success(&format!("工具包 [{}] 已安装", tag));
                                                    match list_installed_tool_packs(&aid).await {
                                                        Ok(resp) => tool_packs.set(resp.installed_tags),
                                                        Err(e) => toast.error(&format!("刷新工具包列表失败: {}", e)),
                                                    }
                                                }
                                                Err(e) => toast.error(&format!("安装工具包失败: {}", e)),
                                            }
                                        });
                                    },
                                    "安装工具包"
                                }
                            }
                            if !tool_packs_list.is_empty() {
                                div { class: "pack-tags",
                                    for tag in tool_packs_list.iter() {
                                        {
                                            let tag_clone = tag.clone();
                                            let aid = agent_id_signal();
                                            rsx! {
                                                span {
                                                    class: "badge badge-accent",
                                                    "{tag}"
                                                    button {
                                                        class: "badge-remove",
                                                        onclick: move |_| {
                                                            let agent_id = aid.clone();
                                                            let t = tag_clone.clone();
                                                            spawn(async move {
                                                                match uninstall_tool_pack(&agent_id, &t).await {
                                                                    Ok(_) => {
                                                                        toast.success(&format!("工具包 [{}] 已卸载", t));
                                                                        match list_installed_tool_packs(&agent_id).await {
                                                                            Ok(resp) => tool_packs.set(resp.installed_tags),
                                                                            Err(e) => toast.error(&format!("刷新工具包列表失败: {}", e)),
                                                                        }
                                                                    }
                                                                    Err(e) => toast.error(&format!("卸载工具包失败: {}", e)),
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

                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "技能包" }
                            div { class: "pack-management",
                                input {
                                    class: "input input-sm",
                                    r#type: "text",
                                    placeholder: "输入技能包 tag 名称",
                                    oninput: move |e| input_message.set(e.value().clone()),
                                }
                                button {
                                    class: "btn btn-primary btn-sm",
                                    onclick: move |_| {
                                        let tag = input_message().trim().to_string();
                                        if tag.is_empty() {
                                            return;
                                        }
                                        let aid = agent_id_signal();
                                        input_message.set(String::new());
                                        spawn(async move {
                                            match install_skill_pack(&aid, &tag).await {
                                                Ok(_) => {
                                                    toast.success(&format!("技能包 [{}] 已安装", tag));
                                                    match list_installed_skill_packs(&aid).await {
                                                        Ok(resp) => skill_packs.set(resp.skill_packs),
                                                        Err(e) => toast.error(&format!("刷新技能包列表失败: {}", e)),
                                                    }
                                                }
                                                Err(e) => toast.error(&format!("安装技能包失败: {}", e)),
                                            }
                                        });
                                    },
                                    "安装技能包"
                                }
                            }
                            if !skill_packs_list.is_empty() {
                                div { class: "pack-tags",
                                    for tag in skill_packs_list.iter() {
                                        {
                                            let tag_clone = tag.clone();
                                            let aid = agent_id_signal();
                                            rsx! {
                                                span {
                                                    class: "badge badge-info",
                                                    "{tag}"
                                                    button {
                                                        class: "badge-remove",
                                                        onclick: move |_| {
                                                            let agent_id = aid.clone();
                                                            let t = tag_clone.clone();
                                                            spawn(async move {
                                                                match uninstall_skill_pack(&agent_id, &t).await {
                                                                    Ok(_) => {
                                                                        toast.success(&format!("技能包 [{}] 已卸载", t));
                                                                        match list_installed_skill_packs(&agent_id).await {
                                                                            Ok(resp) => skill_packs.set(resp.skill_packs),
                                                                            Err(e) => toast.error(&format!("刷新技能包列表失败: {}", e)),
                                                                        }
                                                                    }
                                                                    Err(e) => toast.error(&format!("卸载技能包失败: {}", e)),
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

                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "工具绑定" }
                            div { class: "tool-bindings",
                                if !all_tools_list.is_empty() {
                                    div { class: "tool-grid",
                                        for tool in all_tools_list.iter() {
                                            {
                                                let tool_clone = tool.clone();
                                                let is_bound = agent_tool_ids.contains(&tool.id);
                                                let aid = agent_id_signal();
                                                let tool_id = tool.id.clone();
                                                let tool_name = tool.name.clone();
                                                let desc = tool.description.as_deref().unwrap_or("");
                                                let tags = tool.tags.clone();
                                                rsx! {
                                                    div {
                                                        class: "tool-card",
                                                        key: "{tool_id}",
                                                        div { class: "tool-card-header",
                                                            span { class: "tool-name", "{tool_name}" }
                                                            span { class: "{binding_status_badge_class(is_bound)}",
                                                                if is_bound { "已绑定" } else { "未绑定" }
                                                            }
                                                        }
                                                        div { class: "tool-card-body",
                                                            p { class: "text-sm text-secondary", "{desc}" }
                                                            if !tags.is_empty() {
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
                                                                    let tid = tool_clone.id.clone();
                                                                    let tname = tool_clone.name.clone();
                                                                    let ib = is_bound;
                                                                    spawn(async move {
                                                                        let result = if ib {
                                                                            unbind_tool_from_agent(&agent_id, &tid).await
                                                                        } else {
                                                                            bind_tool_to_agent(&agent_id, &tid).await
                                                                        };
                                                                        match result {
                                                                            Ok(_) => {
                                                                                toast.success(&format!("工具 {} {}", tname, if ib { "已解绑" } else { "已绑定" }));
                                                                                let stats_options = StatsOptions {
                                                                                    with_stats: true,
                                                                                    with_model_call_stats: true,
                                                                                    stats_interval: Some("daily".to_string()),
                                                                                };
                                                                                match get_agent(&agent_id, Some(&stats_options)).await {
                                                                                    Ok(a) => agent_data.set(Some(a)),
                                                                                    Err(e) => toast.error(&format!("刷新 Agent 失败: {}", e)),
                                                                                }
                                                                            }
                                                                            Err(e) => toast.error(&format!("操作失败: {}", e)),
                                                                        }
                                                                    });
                                                                },
                                                                if is_bound { "解绑" } else { "绑定" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    div { class: "state-empty",
                                        div { class: "state-empty-icon", "🔧" }
                                        div { "暂无工具可用" }
                                    }
                                }
                            }
                        }

                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "对话" }
                            {render_chat_messages(&messages(), is_typing())}
                            div { class: "agent-chat-input",
                                input {
                                    class: "input",
                                    r#type: "text",
                                    placeholder: "输入消息...",
                                    value: input_message,
                                    oninput: move |e| input_message.set(e.value().clone()),
                                    onkeydown: move |e| {
                                        if e.key() == Key::Enter {
                                            e.prevent_default();
                                            handle_send(());
                                        }
                                    },
                                }
                                button {
                                    class: "btn btn-primary",
                                    onclick: move |_| handle_send(()),
                                    "发送"
                                }
                            }
                        }

                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "记忆" }
                            AgentMemoryPanel { agent_id: Some(id.clone()) }
                        }

                        if a.stats.is_some() || a.model_call_stats.is_some() {
                            AgentStatsPanel {
                                stats: a.stats.clone(),
                                model_call_stats: a.model_call_stats.clone(),
                            }
                        }
                    }

                    div { class: "card-footer",
                        Link { to: "/hr/agents", class: "btn btn-ghost", "返回列表" }
                    }
                }
            }
        }
    }
}
