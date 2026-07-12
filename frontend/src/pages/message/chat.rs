use dioxus::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use wasm_bindgen::{closure::Closure, JsCast};

use crate::api::message::{load_latest_messages, load_older_messages, poll_new_messages, send_message_to_agent};
use crate::api::project::list_projects;
use common::api::{ListProjectsResponseItem, MessageListItem, SendMessageToAgentParams};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn format_time(ts: i64) -> String {
    let seconds = ts / 1000;
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    format!("{:02}:{:02}", hours, minutes)
}

fn status_text(status: i32) -> &'static str {
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

fn role_class(role: i32) -> &'static str {
    match role {
        0 => "user",
        1 => "agent",
        2 => "system",
        _ => "agent",
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

#[component]
pub fn MessageChat() -> Element {
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut selected_project = use_signal(|| Option::<String>::None);
    let mut messages = use_signal(Vec::<MessageListItem>::new);
    let mut is_typing = use_signal(|| false);
    let mut input_text = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut loading_projects = use_signal(|| true);
    let mut has_more = use_signal(|| true);
    let mut loading_messages = use_signal(|| false);

    let mut load_projects = move || {
        loading_projects.set(true);
        spawn(async move {
            match list_projects().await {
                Ok(resp) => {
                    projects.set(resp.projects);
                    if let Some(p) = projects.read().first() {
                        selected_project.set(Some(p.id.clone()));
                    }
                }
                Err(e) => {
                    error.set(format!("加载项目列表失败: {}", e));
                }
            }
            loading_projects.set(false);
        });
    };

    let mut load_messages = move |project_id: &str| {
        let project_id = project_id.to_string();
        loading_messages.set(true);
        spawn(async move {
            match load_latest_messages(Some(&project_id), Some(20)).await {
                Ok(resp) => {
                    let is_empty = resp.messages.is_empty();
                    messages.set(resp.messages);
                    has_more.set(!is_empty);
                }
                Err(e) => {
                    error.set(format!("加载消息失败: {}", e));
                }
            }
            loading_messages.set(false);
        });
    };

    let mut load_older = move || {
        if !has_more() || loading_messages() {
            return;
        }
        let project_id = match selected_project() {
            Some(id) => id,
            None => return,
        };
        let first_ts = match messages.read().first() {
            Some(m) => m.created_at,
            None => return,
        };
        loading_messages.set(true);
        spawn(async move {
            match load_older_messages(Some(&project_id), first_ts, Some(20)).await {
                Ok(resp) => {
                    if resp.messages.is_empty() {
                        has_more.set(false);
                    } else {
                        let mut current = messages.write();
                        let mut older = resp.messages;
                        older.append(&mut *current);
                        *current = older;
                    }
                }
                Err(e) => {
                    error.set(format!("加载更多消息失败: {}", e));
                }
            }
            loading_messages.set(false);
        });
    };

    let poll_new = move || {
        let project_id = match selected_project() {
            Some(id) => id,
            None => return,
        };
        let last_ts = messages.read().last().map(|m| m.created_at).unwrap_or(0);
        spawn(async move {
            match poll_new_messages(Some(&project_id), last_ts).await {
                Ok(resp) => {
                    if !resp.messages.is_empty() {
                        let mut current = messages.write();
                        current.extend(resp.messages);
                    }
                }
                Err(_) => {}
            }
        });
    };

    let handle_send = use_callback(move |_: ()| {
        let text = input_text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let project_id = match selected_project() {
            Some(id) => id,
            None => return,
        };

        // 获取项目的 owner_agent_id 作为消息接收者
        let to_agent_id = projects
            .read()
            .iter()
            .find(|p| p.id == project_id)
            .and_then(|p| p.owner_agent_id.clone());
        let to_agent_id = match to_agent_id {
            Some(id) => id,
            None => {
                error.set("该项目未分配 Agent，无法发送消息".to_string());
                return;
            }
        };

        input_text.set(String::new());
        is_typing.set(true);

        spawn(async move {
            let req = SendMessageToAgentParams {
                to_agent_id,
                content: text.clone(),
                project_id: Some(project_id.clone()),
                task_id: None,
                reply_to_id: None,
            };

            match send_message_to_agent(req).await {
                Ok(_) => {
                    let user_msg = MessageListItem {
                        message_id: format!("tmp_{}", now_ms()),
                        project_id: Some(project_id.clone()),
                        task_id: None,
                        from_id: "user".to_string(),
                        from_role: 0,
                        to_id: "agent".to_string(),
                        to_role: 1,
                        message_type: 0,
                        status: 3,
                        content: text,
                        reply_to_id: None,
                        created_at: now_ms(),
                    };
                    let mut current = messages.write();
                    current.push(user_msg);
                }
                Err(e) => {
                    error.set(format!("发送消息失败: {}", e));
                    is_typing.set(false);
                    return;
                }
            }

            let mut attempts = 0;
            while attempts < 30 {
                let (tx, rx) = futures_channel::oneshot::channel();
                let window = web_sys::window().unwrap();
                let mut tx = Some(tx);
                let timeout_closure = Closure::wrap(Box::new(move || {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(());
                    }
                }) as Box<dyn FnMut()>);
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    timeout_closure.as_ref().unchecked_ref(),
                    1000,
                );
                timeout_closure.forget();
                let _ = rx.await;
                let last_ts = messages.read().last().map(|m| m.created_at).unwrap_or(0);
                match poll_new_messages(Some(&project_id), last_ts).await {
                    Ok(resp) => {
                        if !resp.messages.is_empty() {
                            let mut current = messages.write();
                            current.extend(resp.messages);
                            is_typing.set(false);
                            break;
                        }
                    }
                    Err(_) => {}
                }
                attempts += 1;
            }

            if attempts >= 30 {
                is_typing.set(false);
            }
        });
    });

    let mut handle_project_click = move |project_id: String| {
        selected_project.set(Some(project_id.clone()));
        messages.set(Vec::new());
        has_more.set(true);
        load_messages(&project_id);
    };

    use_effect(move || {
        load_projects();
    });

    use_effect(move || {
        if selected_project().is_some() {
            let interval_id = {
                let window = web_sys::window().unwrap();
                let closure = Closure::wrap(Box::new(move || {
                    if !is_typing() {
                        poll_new();
                    }
                }) as Box<dyn FnMut()>);
                let id = window.set_interval_with_callback_and_timeout_and_arguments_0(
                    closure.as_ref().unchecked_ref(),
                    3000,
                ).unwrap();
                closure.forget();
                id
            };
            use_drop(move || {
                let window = web_sys::window().unwrap();
                window.clear_interval_with_handle(interval_id);
            });
        }
    });

    let current_project = projects
        .read()
        .iter()
        .find(|p| selected_project().as_deref() == Some(&p.id))
        .cloned();

    let project_items = projects.read().clone();

    let chat_content = if let Some(project) = current_project {
        let project_name = project.name.clone();
        rsx! {
            div { class: "chat-header",
                h2 { class: "chat-header-title", "{project_name}" }
            }

            div { class: "chat-messages",
                if error().is_empty() && loading_messages() && messages().is_empty() {
                    div { class: "state-loading", "加载消息中..." }
                } else if error().is_empty() && messages().is_empty() && !loading_messages() {
                    div { class: "state-empty",
                        div { class: "state-empty-icon", "💬" }
                        div { "暂无消息，开始对话吧" }
                    }
                } else {
                    div {
                        class: "message-list",
                        style: "min-height: 100%;",
                        onscroll: move |e| {
                            if e.scroll_top() == 0.0 {
                                load_older();
                            }
                        },
                        for msg in messages().iter() {
                            {
                                let msg_id = msg.message_id.clone();
                                let msg_content = msg.content.clone();
                                let msg_role = msg.from_role;
                                let msg_time = msg.created_at;
                                rsx! {
                                    div {
                                        class: "message-item {role_class(msg_role)}",
                                        key: "{msg_id}",
                                        div { class: "message-avatar", "{role_avatar(msg_role)}" }
                                        div {
                                            div { class: "message-bubble", "{msg_content}" }
                                            div { class: "message-time", "{format_time(msg_time)}" }
                                        }
                                    }
                                }
                            }
                        }
                        if is_typing() {
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

            div { class: "chat-input-area",
                div { class: "chat-input-container",
                    textarea {
                        class: "chat-input",
                        value: "{input_text}",
                        placeholder: "输入消息...",
                        oninput: move |e| input_text.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter && !e.modifiers().contains(Modifiers::SHIFT) {
                                e.prevent_default();
                                handle_send(());
                            }
                        },
                    }
                    button {
                        class: "chat-send-btn",
                        onclick: move |_| handle_send(()),
                        disabled: input_text().trim().is_empty(),
                        "发送"
                    }
                }
            }
        }
    } else {
        rsx! {
            div { class: "chat-empty",
                div { class: "chat-empty-icon", "📋" }
                div { class: "chat-empty-title", "选择项目" }
                div { class: "chat-empty-desc", "从左侧列表选择一个项目开始对话" }
            }
        }
    };

    rsx! {
        div { class: "chat-container",
            div { class: "chat-sidebar",
                div { class: "chat-sidebar-header",
                    h2 { class: "chat-sidebar-title", "项目列表" }
                }
                div { class: "chat-project-list",
                    if loading_projects() {
                        div { class: "state-loading", "加载中..." }
                    } else {
                        for project in project_items.iter() {
                            {
                                let id = project.id.clone();
                                let name = project.name.clone();
                                let status = project.status;
                                let is_active = selected_project() == Some(id.clone());
                                let item_class = if is_active { "chat-project-item active" } else { "chat-project-item" };
                                rsx! {
                                    div {
                                        class: "{item_class}",
                                        onclick: move |_| handle_project_click(id.clone()),
                                        div { class: "chat-project-name", "{name}" }
                                        div { class: "chat-project-status", "{status_text(status)}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "chat-main",
                {chat_content}
            }
        }
    }
}
