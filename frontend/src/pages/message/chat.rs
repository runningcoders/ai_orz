use dioxus::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use wasm_bindgen::{closure::Closure, JsCast};

use crate::api::finance::upload_attachment;
use crate::api::hr::get_reception_agent;
use crate::api::message::{load_latest_messages, load_older_messages, send_message_to_agent};
use crate::api::project::{create_project, list_projects};
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{
    CreateProjectRequest, GetReceptionAgentResponse, ListProjectsResponseItem, MessageListItem,
    SendMessageToAgentParams,
};

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

fn role_avatar(role: i32) -> &'static str {
    match role {
        0 => "U",
        1 => "A",
        2 => "S",
        _ => "?",
    }
}

/// 消息类型常量
const MSG_TEXT: i32 = 0;
const MSG_IMAGE: i32 = 1;
const MSG_FILE: i32 = 2;
const MSG_AUDIO: i32 = 3;
const MSG_VIDEO: i32 = 4;
const MSG_TOOL_CALL_REQUEST: i32 = 5;
const MSG_TOOL_CALL_RESULT: i32 = 6;
const MSG_TASK_ASSIGNMENT: i32 = 9;

/// 判断是否为附件消息
fn is_attachment_message(msg_type: i32) -> bool {
    matches!(msg_type, MSG_IMAGE | MSG_FILE | MSG_AUDIO | MSG_VIDEO)
}

/// 待发送的附件信息（仅用于 UI 展示，发送后清空）
#[derive(Debug, Clone, PartialEq)]
struct PendingAttachment {
    /// 附件 ID（已上传到服务器）
    id: String,
    /// 文件名（仅展示）
    name: String,
}

#[component]
pub fn MessageChat() -> Element {
    // 权限检查：未登录时重定向到登录页
    if !crate::hooks::use_require_auth() {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center",
                span { class: "loading loading-spinner loading-lg" }
            }
        };
    }

    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut selected_project = use_signal(|| Option::<String>::None);
    let mut messages = use_signal(Vec::<MessageListItem>::new);
    let mut is_typing = use_signal(|| false);
    let mut input_text = use_signal(String::new);
    let error = use_signal(String::new);
    let mut loading_projects = use_signal(|| true);
    let mut has_more = use_signal(|| true);
    let mut loading_messages = use_signal(|| false);
    let mut sse_connected = use_signal(|| false);

    // 前台 Agent 信息（默认对话框顶部展示）
    let mut reception_agent = use_signal(|| Option::<GetReceptionAgentResponse>::None);

    // 新建项目弹窗状态
    let mut show_create_project = use_signal(|| false);
    let mut new_project_name = use_signal(String::new);
    let mut new_project_desc = use_signal(String::new);
    let mut creating_project = use_signal(|| false);

    // 工具卡片展开状态（message_id -> expanded）
    let mut tool_expanded = use_signal(|| std::collections::HashSet::<String>::new());

    // 附件上传状态
    let mut pending_attachments = use_signal(Vec::<PendingAttachment>::new);
    let mut uploading = use_signal(|| false);
    let toast = use_toast();

    // 快捷指令菜单状态
    let mut show_slash_menu = use_signal(|| false);
    let selected_slash_index = use_signal(|| 0);

    // 移动端 sidebar 抽屉状态
    let mut sidebar_open = use_signal(|| false);
    let is_mobile = crate::hooks::use_breakpoint();

    let mut load_projects = move || {
        loading_projects.set(true);
        spawn(async move {
            match list_projects().await {
                Ok(resp) => {
                    projects.set(resp.projects);
                    // 不自动选择第一个项目，让用户从「默认对话」开始
                }
                Err(e) => {
                    toast.error(&format!("加载项目列表失败: {}", e));
                }
            }
            loading_projects.set(false);
        });
    };

    // 加载前台 Agent 信息（用于默认对话框顶部展示）
    let load_reception_agent = move || {
        spawn(async move {
            match get_reception_agent().await {
                Ok(resp) => {
                    reception_agent.set(Some(resp));
                }
                Err(_) => {
                    // 无可用前台 Agent 时静默处理（默认对话框仍可用，发送时后端兜底）
                    reception_agent.set(None);
                }
            }
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
                    toast.error(&format!("加载消息失败: {}", e));
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
                    toast.error(&format!("加载更多消息失败: {}", e));
                }
            }
            loading_messages.set(false);
        });
    };

    let mut handle_sse_message = move |data: String| {
        let msg: MessageListItem = match serde_json::from_str(&data) {
            Ok(m) => m,
            Err(_) => return,
        };
        let cur_project = selected_project();
        // 修复 H1：默认对话框（cur_project=None）应接收 project_id=None 的消息
        let project_match = match (&cur_project, &msg.project_id) {
            (Some(cur), Some(proj)) => cur.as_str() == proj.as_str(),
            (None, None) => true,
            _ => false,
        };
        if project_match {
            let mut current = messages.write();
            // 修复 H2：乐观消息用 tmp_ 前缀，真实消息 ID 不同导致重复。
            // 策略：收到真实消息时移除同 content 的 tmp_ 前缀消息
            current.retain(|m| !(m.message_id.starts_with("tmp_") && m.content == msg.content));
            if current.iter().any(|m| m.message_id == msg.message_id) {
                return;
            }
            current.push(msg);
            is_typing.set(false);
        }
    };

    use_effect(move || {
        load_projects();
        load_reception_agent();
    });

    use_effect(move || {
        if let Some(proj_id) = selected_project() {
            load_messages(&proj_id);
        }
    });

    use_effect(move || {
        // 修复 H5：EventSource::new 可能失败（浏览器不支持或 URL 无效），不能 unwrap
        let event_source = match web_sys::EventSource::new("/api/v1/finance/messages/sse") {
            Ok(es) => es,
            Err(_) => {
                toast.error("SSE 连接初始化失败，实时消息将无法接收");
                return;
            }
        };
        sse_connected.set(true);

        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let data = event.data().as_string().unwrap_or_default();
            handle_sse_message(data);
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        event_source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        // 修复 H7：存储 Closure 避免 forget() 泄漏，在 use_drop 中通过 set_onmessage(None)
        // 触发 Rust 侧 Closure drop 回收
        let on_message = Some(on_message);

        let on_open = Closure::wrap(Box::new(move |_: web_sys::Event| {
            sse_connected.set(true);
        }) as Box<dyn FnMut(web_sys::Event)>);
        event_source.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        let on_open = Some(on_open);

        let on_error = Closure::wrap(Box::new(move |_: web_sys::Event| {
            sse_connected.set(false);
        }) as Box<dyn FnMut(web_sys::Event)>);
        event_source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        let on_error = Some(on_error);

        use_drop(move || {
            // 先清除回调引用，使 Closure 自然 drop 回收内存
            event_source.set_onmessage(None);
            event_source.set_onopen(None);
            event_source.set_onerror(None);
            drop(on_message);
            drop(on_open);
            drop(on_error);
            event_source.close();
        });
    });

    let slash_commands = [
        ("/clear", "清空对话"),
        ("/help", "显示帮助"),
    ];

    let handle_input = {
        let mut input_text = input_text;
        let mut show_slash_menu = show_slash_menu;
        let mut selected_slash_index = selected_slash_index;
        move |value: String| {
            input_text.set(value.clone());
            let trimmed = value.trim_start();
            if trimmed.starts_with('/') && !trimmed.contains(' ') {
                show_slash_menu.set(true);
                selected_slash_index.set(0);
            } else {
                show_slash_menu.set(false);
            }
        }
    };

    let handle_send = use_callback(move |_: ()| {
        let text = input_text().trim().to_string();
        let attachments = pending_attachments();
        if text.is_empty() && attachments.is_empty() {
            return;
        }
        if show_slash_menu() {
            let filtered: Vec<&str> = slash_commands
                .iter()
                .filter(|(cmd, _)| cmd.starts_with(&input_text().trim_start().to_lowercase()))
                .map(|(cmd, _)| *cmd)
                .collect();
            if !filtered.is_empty() {
                let idx = selected_slash_index().min(filtered.len() as i32 - 1) as usize;
                match filtered[idx] {
                    "/clear" => {
                        messages.set(Vec::new());
                        input_text.set(String::new());
                        show_slash_menu.set(false);
                        toast.info("对话已清空");
                    }
                    "/help" => {
                        show_slash_menu.set(false);
                        toast.info("可用指令: /clear - 清空对话, /help - 显示帮助");
                    }
                    _ => {}
                }
                return;
            }
        }
        let project_id = selected_project();

        // to_agent_id 路由优先级：
        // 1. Project 对话框：从 project.owner_agent_id 取（若为 None 则不传，后端走 resolve_agent 兜底）
        // 2. 默认对话框：不传 to_agent_id，后端走 resolve_agent(ctx) 兜底
        let to_agent_id = if let Some(ref pid) = project_id {
            projects
                .read()
                .iter()
                .find(|p| &p.id == pid)
                .and_then(|p| p.owner_agent_id.clone())
        } else {
            // 默认对话框：使用已加载的前台 Agent（若有）
            reception_agent().map(|a| a.agent_id)
        };

        let attachment_ids: Vec<String> = attachments.iter().map(|a| a.id.clone()).collect();
        let attachment_ids_opt = if attachment_ids.is_empty() {
            None
        } else {
            Some(attachment_ids)
        };

        input_text.set(String::new());
        pending_attachments.set(Vec::new());
        is_typing.set(true);

        spawn(async move {
            let req = SendMessageToAgentParams {
                to_agent_id,
                content: text.clone(),
                project_id: project_id.clone(),
                task_id: None,
                reply_to_id: None,
                attachment_ids: attachment_ids_opt,
            };

            match send_message_to_agent(req).await {
                Ok(_) => {
                    let user_msg = MessageListItem {
                        message_id: format!("tmp_{}", now_ms()),
                        project_id: project_id.clone(),
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
                        file_type: None,
                        file_meta: None,
                    };
                    let mut current = messages.write();
                    current.push(user_msg);
                }
                Err(e) => {
                    toast.error(&format!("发送消息失败: {}", e));
                    is_typing.set(false);
                    return;
                }
            }
        });
    });

    // 处理文件选择：上传到附件服务，把 ID 加入待发送列表
    let handle_file_select = move |files: Vec<dioxus::html::FileData>| {
        if uploading() {
            return;
        }
        if files.is_empty() {
            return;
        }
        uploading.set(true);
        spawn(async move {
            for fd in files {
                let file_name = fd.name();
                let bytes = match fd.read_bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        toast.error(&format!("读取文件 {} 失败: {:?}", file_name, e));
                        uploading.set(false);
                        return;
                    }
                };

                // 构造 Blob 与 FormData
                let blob_parts: js_sys::Array = js_sys::Array::new();
                let uint8_array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
                uint8_array.copy_from(&bytes);
                blob_parts.push(&uint8_array);
                let blob_bag = web_sys::BlobPropertyBag::new();
                blob_bag.set_type("application/octet-stream");
                let blob = web_sys::Blob::new_with_str_sequence_and_options(
                    &blob_parts,
                    &blob_bag,
                )
                .ok();

                let form = web_sys::FormData::new().unwrap();
                let _ = form.append_with_str("purpose", "message");
                if let Some(blob) = blob {
                    let _ = form.append_with_blob_and_filename("file", &blob, &file_name);
                }

                match upload_attachment(form).await {
                    Ok(detail) => {
                        let mut current = pending_attachments.write();
                        current.push(PendingAttachment {
                            id: detail.id,
                            name: detail.original_name,
                        });
                    }
                    Err(e) => {
                        toast.error(&format!("上传文件 {} 失败: {}", file_name, e));
                    }
                }
            }
            uploading.set(false);
        });
    };

    let mut handle_project_click = move |project_id: String| {
        selected_project.set(Some(project_id.clone()));
        messages.set(Vec::new());
        has_more.set(true);
        load_messages(&project_id);
        if is_mobile() {
            sidebar_open.set(false);
        }
    };

    // 点击「默认对话」条目：清空选中项目，清空消息
    let handle_default_chat_click = move |_| {
        selected_project.set(None);
        messages.set(Vec::new());
        has_more.set(true);
        if is_mobile() {
            sidebar_open.set(false);
        }
    };

    // 提交新建项目：自动绑定前台 Agent 作为 owner_agent_id
    let handle_create_project_submit = move |_| {
        let name = new_project_name().trim().to_string();
        if name.is_empty() {
            toast.error("项目名称不能为空");
            return;
        }
        let desc = new_project_desc().trim().to_string();
        let owner_agent_id = reception_agent().map(|a| a.agent_id);

        creating_project.set(true);
        spawn(async move {
            let req = CreateProjectRequest {
                name: name.clone(),
                description: if desc.is_empty() { None } else { Some(desc) },
                priority: None,
                tags: None,
                owner_agent_id,
            };
            match create_project(req).await {
                Ok(resp) => {
                    // CreateProjectResponse = GetProjectResponse，字段直接在响应上
                    let new_project = ListProjectsResponseItem {
                        id: resp.id.clone(),
                        name: resp.name.clone(),
                        description: resp.description,
                        status: resp.status,
                        priority: resp.priority,
                        tags: resp.tags,
                        root_user_id: resp.root_user_id,
                        owner_agent_id: resp.owner_agent_id,
                        created_at: resp.created_at,
                        updated_at: resp.updated_at,
                    };
                    selected_project.set(Some(new_project.id.clone()));
                    projects.write().push(new_project.clone());
                    messages.set(Vec::new());
                    has_more.set(true);
                    load_messages(&new_project.id);
                    show_create_project.set(false);
                    new_project_name.set(String::new());
                    new_project_desc.set(String::new());
                    toast.info(&format!("项目「{}」已创建", name));
                }
                Err(e) => {
                    toast.error(&format!("创建项目失败: {}", e));
                }
            }
            creating_project.set(false);
        });
    };

    let current_project = projects
        .read()
        .iter()
        .find(|p| selected_project().as_deref() == Some(&p.id))
        .cloned();

    let project_items = projects.read().clone();

    let chat_content = if let Some(project) = current_project {
        let project_name = project.name.clone();
        rsx! {
            div { class: "p-3 border-b border-base-300 flex items-center justify-between bg-base-100 gap-2",
                if is_mobile() {
                    button {
                        class: "btn btn-ghost btn-sm",
                        onclick: move |_| sidebar_open.set(true),
                        "←"
                    }
                }
                h2 { class: "font-semibold text-lg truncate", "{project_name}" }
                div { class: "flex items-center gap-2 ml-auto",
                    if sse_connected() {
                        span { class: "text-success text-sm", "● 实时" }
                    } else {
                        span { class: "text-base-content/50 text-sm", "○ 连接中..." }
                    }
                }
            }

            div { class: "flex-1 overflow-y-auto p-4 bg-base-100",
                if error().is_empty() && loading_messages() && messages().is_empty() {
                    div { class: "flex items-center justify-center py-12",
                        span { class: "loading loading-spinner loading-md" }
                        span { class: "ml-2 text-base-content/60", "加载消息中..." }
                    }
                } else if error().is_empty() && messages().is_empty() && !loading_messages() {
                    div { class: "text-center py-12",
                        div { class: "text-5xl mb-3", "💬" }
                        div { class: "text-base-content/60", "暂无消息，开始对话吧" }
                    }
                } else {
                    div {
                        class: "flex flex-col gap-1 min-h-full",
                        onscroll: move |e| {
                            if e.scroll_top() == 0.0 {
                                load_older();
                            }
                        },
                        for entry in group_messages_by_date(&messages()) {
                            match entry {
                                MessageListEntry::DateDivider(label) => rsx! {
                                    div { class: "divider my-2", key: "divider-{label}-{messages().len()}", "{label}" }
                                },
                                MessageListEntry::Message(msg) => rsx! {
                                    {
                                        let msg_id = msg.message_id.clone();
                                        let msg_role = msg.from_role;
                                        let msg_clone = msg.clone();
                                        let expanded = tool_expanded.read().contains(&msg_id);
                                        let is_user = msg_role == 0;
                                        let is_system = msg_role == 2;
                                        rsx! {
                                            div {
                                                class: if is_user { "chat chat-end" } else if is_system { "chat chat-start" } else { "chat chat-start" },
                                                key: "{msg_id}",
                                                div { class: "chat-image avatar",
                                                    div {
                                                        class: if is_user { "w-10 rounded-full bg-primary text-primary-content flex items-center justify-center font-bold" } else if is_system { "w-10 rounded-full bg-info text-info-content flex items-center justify-center font-bold" } else { "w-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold" },
                                                        "{role_avatar(msg_role)}"
                                                    }
                                                }
                                                {
                                                    render_message_content(&msg_clone, expanded, is_user, is_system, {
                                                        let mid = msg_id.clone();
                                                        move || {
                                                            if tool_expanded.read().contains(&mid) {
                                                                tool_expanded.write().remove(&mid);
                                                            } else {
                                                                tool_expanded.write().insert(mid.clone());
                                                            }
                                                        }
                                                    }, toast.clone())
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                        if is_typing() {
                            div { class: "chat chat-start",
                                div { class: "chat-image avatar",
                                    div { class: "w-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold", "A" }
                                }
                                div { class: "chat-bubble chat-bubble-neutral",
                                    div { class: "typing-indicator flex gap-1",
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

            {chat_input_area(
                is_mobile(),
                input_text,
                show_slash_menu,
                selected_slash_index,
                slash_commands,
                handle_input,
                handle_send,
                uploading,
                pending_attachments,
                handle_file_select,
                toast,
                messages,
            )}
        }
    } else {
        // 默认对话框（无 project_id，后端走 resolve_agent 兜底路由前台 Agent）
        let reception_name = reception_agent()
            .map(|a| a.agent_name)
            .unwrap_or_else(|| "前台 Agent".to_string());
        rsx! {
            div { class: "p-3 border-b border-base-300 flex items-center justify-between bg-base-100 gap-2",
                if is_mobile() {
                    button {
                        class: "btn btn-ghost btn-sm",
                        onclick: move |_| sidebar_open.set(true),
                        "←"
                    }
                }
                h2 { class: "font-semibold text-lg", "默认对话" }
                span { class: "text-sm text-base-content/60 truncate", "当前前台：{reception_name}" }
                div { class: "flex items-center gap-2 ml-auto",
                    if sse_connected() {
                        span { class: "text-success text-sm", "● 实时" }
                    } else {
                        span { class: "text-base-content/50 text-sm", "○ 连接中..." }
                    }
                }
            }

            div { class: "flex-1 overflow-y-auto p-4 bg-base-100",
                if messages().is_empty() {
                    div { class: "text-center py-12",
                        div { class: "text-5xl mb-3", "💬" }
                        div { class: "text-base-content/60", "与前台 Agent 直接沟通，复杂需求可新建项目组织" }
                    }
                } else {
                    div {
                        class: "flex flex-col gap-1 min-h-full",
                        for entry in group_messages_by_date(&messages()) {
                            match entry {
                                MessageListEntry::DateDivider(label) => rsx! {
                                    div { class: "divider my-2", key: "divider-{label}-{messages().len()}", "{label}" }
                                },
                                MessageListEntry::Message(msg) => rsx! {
                                    {
                                        let msg_id = msg.message_id.clone();
                                        let msg_role = msg.from_role;
                                        let msg_clone = msg.clone();
                                        let expanded = tool_expanded.read().contains(&msg_id);
                                        let is_user = msg_role == 0;
                                        let is_system = msg_role == 2;
                                        rsx! {
                                            div {
                                                class: if is_user { "chat chat-end" } else if is_system { "chat chat-start" } else { "chat chat-start" },
                                                key: "{msg_id}",
                                                div { class: "chat-image avatar",
                                                    div {
                                                        class: if is_user { "w-10 rounded-full bg-primary text-primary-content flex items-center justify-center font-bold" } else if is_system { "w-10 rounded-full bg-info text-info-content flex items-center justify-center font-bold" } else { "w-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold" },
                                                        "{role_avatar(msg_role)}"
                                                    }
                                                }
                                                {
                                                    render_message_content(&msg_clone, expanded, is_user, is_system, {
                                                        let mid = msg_id.clone();
                                                        move || {
                                                            if tool_expanded.read().contains(&mid) {
                                                                tool_expanded.write().remove(&mid);
                                                            } else {
                                                                tool_expanded.write().insert(mid.clone());
                                                            }
                                                        }
                                                    }, toast.clone())
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                        if is_typing() {
                            div { class: "chat chat-start",
                                div { class: "chat-image avatar",
                                    div { class: "w-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold", "A" }
                                }
                                div { class: "chat-bubble chat-bubble-neutral",
                                    div { class: "typing-indicator flex gap-1",
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

            {chat_input_area(
                is_mobile(),
                input_text,
                show_slash_menu,
                selected_slash_index,
                slash_commands,
                handle_input,
                handle_send,
                uploading,
                pending_attachments,
                handle_file_select,
                toast,
                messages,
            )}
        }
    };

    let sidebar_visible_on_mobile = selected_project().is_none() || sidebar_open();

    rsx! {
        div { class: "h-[calc(100vh-4rem)] flex flex-col lg:flex-row relative",
            if is_mobile() && sidebar_visible_on_mobile {
                div {
                    class: "fixed inset-0 bg-black/50 z-40 lg:hidden",
                    onclick: move |_| sidebar_open.set(false),
                }
            }
            div {
                class: if is_mobile() {
                    if sidebar_visible_on_mobile {
                        "fixed inset-y-0 left-0 z-50 w-72 bg-base-200 flex flex-col border-r border-base-300 transition-transform"
                    } else {
                        "fixed inset-y-0 left-0 z-50 w-72 bg-base-200 flex flex-col border-r border-base-300 transition-transform -translate-x-full"
                    }
                } else {
                    "w-full lg:w-72 bg-base-200 flex flex-col border-r border-base-300"
                },
                div { class: "p-3 border-b border-base-300 flex items-center justify-between",
                    h2 { class: "font-semibold", "项目列表" }
                    button {
                        class: "btn btn-primary btn-sm",
                        r#type: "button",
                        onclick: move |_| show_create_project.set(true),
                        "+ 新建项目"
                    }
                }
                div { class: "flex-1 overflow-y-auto",
                    {
                        let is_active = selected_project().is_none();
                        let item_class = if is_active {
                            "px-3 py-2 cursor-pointer bg-primary/10 border-l-4 border-primary transition-colors"
                        } else {
                            "px-3 py-2 cursor-pointer hover:bg-base-300 transition-colors"
                        };
                        rsx! {
                            div {
                                class: "{item_class}",
                                onclick: handle_default_chat_click,
                                div { class: "font-medium", "💬 默认对话" }
                                div { class: "text-xs text-base-content/60", "与前台 Agent 直接沟通" }
                            }
                        }
                    }
                    if loading_projects() {
                        div { class: "flex items-center justify-center py-8",
                            span { class: "loading loading-spinner loading-sm" }
                            span { class: "ml-2 text-sm text-base-content/60", "加载中..." }
                        }
                    } else {
                        for project in project_items.iter() {
                            {
                                let id = project.id.clone();
                                let name = project.name.clone();
                                let status = project.status;
                                let is_active = selected_project() == Some(id.clone());
                                let item_class = if is_active {
                                    "px-3 py-2 cursor-pointer bg-primary/10 border-l-4 border-primary transition-colors"
                                } else {
                                    "px-3 py-2 cursor-pointer hover:bg-base-300 transition-colors"
                                };
                                rsx! {
                                    div {
                                        class: "{item_class}",
                                        onclick: move |_| handle_project_click(id.clone()),
                                        div { class: "font-medium", "{name}" }
                                        div { class: "text-xs text-base-content/60", "{status_text(status)}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "flex-1 flex flex-col min-w-0",
                {chat_content}
            }

            // 新建项目弹窗
            Modal {
                show: show_create_project(),
                title: "新建项目".to_string(),
                on_close: move |_| show_create_project.set(false),
                footer: rsx! {
                    button {
                        class: "btn btn-ghost",
                        onclick: move |_| show_create_project.set(false),
                        "取消"
                    }
                    button {
                        class: "btn btn-primary",
                        disabled: creating_project() || new_project_name().trim().is_empty(),
                        onclick: handle_create_project_submit,
                        if creating_project() {
                            span { class: "loading loading-spinner loading-sm mr-1" }
                            "创建中..."
                        } else { "创建" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text", "项目名称" }
                        }
                        input {
                            r#type: "text",
                            class: "input input-bordered w-full",
                            placeholder: "输入项目名称",
                            value: "{new_project_name}",
                            oninput: move |e| new_project_name.set(e.value()),
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text", "项目描述（可选）" }
                        }
                        textarea {
                            class: "textarea textarea-bordered w-full",
                            placeholder: "输入项目描述",
                            value: "{new_project_desc}",
                            oninput: move |e| new_project_desc.set(e.value()),
                        }
                    }
                    div { class: "text-sm text-base-content/60",
                        "项目将自动绑定当前前台 Agent 作为负责人"
                    }
                }
            }
        }
    }
}

/// 渲染聊天输入区域（Project 对话框和默认对话框共用）
///
/// 提取为函数避免在两个分支中重复 100+ 行代码。
/// Dioxus 的 Signal<T> 是 Copy 的，可以直接作为参数传递。
#[allow(clippy::too_many_arguments)]
fn chat_input_area(
    _is_mobile: bool,
    mut input_text: Signal<String>,
    mut show_slash_menu: Signal<bool>,
    mut selected_slash_index: Signal<i32>,
    slash_commands: [(&'static str, &'static str); 2],
    mut handle_input: impl FnMut(String) + 'static,
    handle_send: Callback,
    uploading: Signal<bool>,
    mut pending_attachments: Signal<Vec<PendingAttachment>>,
    mut handle_file_select: impl FnMut(Vec<dioxus::html::FileData>) + 'static,
    toast: crate::store::toast::ToastState,
    mut messages: Signal<Vec<MessageListItem>>,
) -> Element {
    rsx! {
        div { class: "p-3 border-t border-base-300 bg-base-100 relative",
            if show_slash_menu() {
                {
                    let filtered: Vec<(&str, &str)> = slash_commands
                        .iter()
                        .filter(|(cmd, _)| cmd.starts_with(&input_text().trim_start().to_lowercase()))
                        .copied()
                        .collect();
                    if !filtered.is_empty() {
                        let selected = selected_slash_index().min(filtered.len() as i32 - 1).max(0) as usize;
                        let input_text = input_text;
                        let messages = messages;
                        let show_slash_menu = show_slash_menu;
                        let toast = toast;
                        let selected_slash_index = selected_slash_index;
                        rsx! {
                            div { class: "absolute bottom-full left-3 right-3 mb-1 bg-base-100 rounded-lg shadow-lg border border-base-300 overflow-hidden z-10",
                                for (i, (cmd, desc)) in filtered.iter().enumerate() {
                                    {
                                        let cmd = cmd.to_string();
                                        let cmd_clone = cmd.clone();
                                        let desc = *desc;
                                        let is_selected = i == selected;
                                        let mut input_text = input_text;
                                        let mut messages = messages;
                                        let mut show_slash_menu = show_slash_menu;
                                        let toast = toast;
                                        let mut selected_slash_index = selected_slash_index;
                                        rsx! {
                                            div {
                                                class: if is_selected { "px-3 py-2 cursor-pointer bg-primary/10 flex gap-3 items-center" } else { "px-3 py-2 cursor-pointer hover:bg-base-200 flex gap-3 items-center" },
                                                onclick: move |_| {
                                                    match cmd_clone.as_str() {
                                                        "/clear" => {
                                                            messages.set(Vec::new());
                                                            input_text.set(String::new());
                                                            show_slash_menu.set(false);
                                                            toast.info("对话已清空");
                                                        }
                                                        "/help" => {
                                                            show_slash_menu.set(false);
                                                            toast.info("可用指令: /clear - 清空对话, /help - 显示帮助");
                                                        }
                                                        _ => {}
                                                    }
                                                },
                                                onmouseenter: move |_| {
                                                    selected_slash_index.set(i as i32);
                                                },
                                                span { class: "font-mono font-semibold text-primary", "{cmd}" }
                                                span { class: "text-sm text-base-content/60", "{desc}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }
            if !pending_attachments().is_empty() {
                div { class: "flex flex-wrap gap-2 mb-2",
                    for att in pending_attachments().iter() {
                        div {
                            class: "badge badge-lg gap-2",
                            key: "{att.id}",
                            span { "📎" }
                            span { "{att.name}" }
                            button {
                                class: "btn btn-ghost btn-xs btn-circle",
                                onclick: {
                                    let id = att.id.clone();
                                    move |_| {
                                        let mut current = pending_attachments.write();
                                        current.retain(|a| a.id != id);
                                    }
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            div { class: "flex items-end gap-2",
                input {
                    r#type: "file",
                    multiple: "true",
                    class: "hidden",
                    id: "chat-file-input",
                    onchange: move |e| {
                        handle_file_select(e.files());
                    },
                }
                button {
                    class: "btn btn-ghost btn-square",
                    r#type: "button",
                    disabled: uploading(),
                    onclick: move |_| {
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Some(el) = doc.get_element_by_id("chat-file-input") {
                                    let _ = el.dyn_into::<web_sys::HtmlElement>().map(|h| h.click());
                                }
                            }
                        }
                    },
                    if uploading() {
                        span { class: "loading loading-spinner loading-sm" }
                    } else { "📎" }
                }
                textarea {
                    class: "textarea textarea-bordered w-full resize-none",
                    rows: "2",
                    value: "{input_text}",
                    placeholder: "输入消息...",
                    oninput: move |e| handle_input(e.value()),
                    onkeydown: move |e| {
                        if show_slash_menu() {
                            let filtered: Vec<(&str, &str)> = slash_commands
                                .iter()
                                .filter(|(cmd, _)| cmd.starts_with(&input_text().trim_start().to_lowercase()))
                                .copied()
                                .collect();
                            if !filtered.is_empty() {
                                if e.key() == Key::ArrowDown {
                                    e.prevent_default();
                                    let next = (selected_slash_index() + 1).min(filtered.len() as i32 - 1);
                                    selected_slash_index.set(next);
                                    return;
                                }
                                if e.key() == Key::ArrowUp {
                                    e.prevent_default();
                                    let prev = (selected_slash_index() - 1).max(0);
                                    selected_slash_index.set(prev);
                                    return;
                                }
                                if e.key() == Key::Enter && !e.modifiers().contains(Modifiers::SHIFT) {
                                    e.prevent_default();
                                    let idx = selected_slash_index().min(filtered.len() as i32 - 1).max(0) as usize;
                                    match filtered[idx].0 {
                                        "/clear" => {
                                            messages.set(Vec::new());
                                            input_text.set(String::new());
                                            show_slash_menu.set(false);
                                            toast.info("对话已清空");
                                        }
                                        "/help" => {
                                            show_slash_menu.set(false);
                                            toast.info("可用指令: /clear - 清空对话, /help - 显示帮助");
                                        }
                                        _ => {}
                                    }
                                    return;
                                }
                                if e.key() == Key::Escape {
                                    e.prevent_default();
                                    show_slash_menu.set(false);
                                    return;
                                }
                            }
                        }
                        if e.key() == Key::Enter && !e.modifiers().contains(Modifiers::SHIFT) {
                            e.prevent_default();
                            handle_send(());
                        }
                    },
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| handle_send(()),
                    disabled: input_text().trim().is_empty() && pending_attachments().is_empty(),
                    "发送"
                }
            }
        }
    }
}

fn copy_to_clipboard(content: &str, toast: &crate::store::toast::ToastState) {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let clipboard = navigator.clipboard();
        let promise = clipboard.write_text(content);
        let toast = *toast;
        let _content = content.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => {
                    toast.info("已复制到剪贴板");
                }
                Err(_) => {
                    toast.error("复制失败");
                }
            }
        });
    } else {
        toast.error("剪贴板不可用");
    }
}

/// 根据消息类型渲染不同内容
fn render_message_content(
    msg: &MessageListItem,
    expanded: bool,
    is_user: bool,
    is_system: bool,
    toggle_expand: impl FnMut() + 'static,
    toast: crate::store::toast::ToastState,
) -> Element {
    let bubble_class = if is_user {
        "chat-bubble chat-bubble-primary"
    } else if is_system {
        "chat-bubble chat-bubble-info/20 text-info-content text-xs"
    } else {
        "chat-bubble chat-bubble-neutral"
    };
    let time_class = "chat-footer text-xs opacity-50 mt-1";

    if is_attachment_message(msg.message_type) {
        return render_attachment_message(msg, bubble_class, time_class);
    }

    match msg.message_type {
        MSG_TOOL_CALL_REQUEST | MSG_TOOL_CALL_RESULT => {
            render_tool_call_card(&msg.content, msg.message_type == MSG_TOOL_CALL_RESULT, expanded, toggle_expand, msg.created_at, time_class)
        }
        MSG_TASK_ASSIGNMENT => {
            render_task_card(&msg.content, msg.created_at, bubble_class, time_class)
        }
        MSG_TEXT => {
            let content = msg.content.clone();
            let toast_copy = toast;
            rsx! {
                div { class: "group relative",
                    div { class: "{bubble_class} break-words whitespace-pre-wrap", "{msg.content}" }
                    button {
                        class: "absolute -top-2 -right-2 btn btn-ghost btn-xs opacity-0 group-hover:opacity-100 transition-opacity bg-base-100 shadow",
                        onclick: move |_| copy_to_clipboard(&content, &toast_copy),
                        "复制"
                    }
                }
                div { class: "{time_class}", "{format_time(msg.created_at)}" }
            }
        }
        _ => rsx! {
            div { class: "{bubble_class} break-words whitespace-pre-wrap", "{msg.content}" }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
    }
}

/// 渲染附件消息
fn render_attachment_message(msg: &MessageListItem, bubble_class: &str, time_class: &str) -> Element {
    let file_meta = match &msg.file_meta {
        Some(fm) => fm,
        None => {
            return rsx! {
                div { class: "{bubble_class} break-words", "{msg.content}" }
                div { class: "{time_class}", "{format_time(msg.created_at)}" }
            };
        }
    };

    let file_path = &msg.content;
    let file_url = format!("/api/v1/finance/attachments/{}/content", file_path);

    match msg.message_type {
        MSG_IMAGE => rsx! {
            div { class: "{bubble_class} p-1",
                img {
                    src: "{file_url}",
                    class: "max-w-xs rounded-lg",
                    loading: "lazy",
                }
            }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
        MSG_VIDEO => rsx! {
            div { class: "{bubble_class} p-2 space-y-2",
                video {
                    src: "{file_url}",
                    controls: "true",
                    class: "max-w-xs rounded-lg",
                    preload: "metadata",
                    "您的浏览器不支持视频播放"
                }
                div { class: "flex items-center gap-2 text-sm opacity-80",
                    span { "🎬" }
                    span { "{file_meta.name}" }
                    span { class: "opacity-60", "{format_file_size(file_meta.size)}" }
                }
            }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
        MSG_AUDIO => rsx! {
            div { class: "{bubble_class} p-2 space-y-2 min-w-[200px]",
                div { class: "flex items-center gap-2 text-sm",
                    span { "🎵" }
                    span { "{file_meta.name}" }
                }
                audio {
                    src: "{file_url}",
                    controls: "true",
                    class: "w-full",
                    preload: "metadata",
                }
                div { class: "text-xs opacity-60", "{format_file_size(file_meta.size)}" }
            }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
        _ => rsx! {
            a {
                href: "{file_url}",
                class: "{bubble_class} flex items-center gap-3 no-underline hover:opacity-90 transition-opacity",
                div { class: "text-2xl", "📄" }
                div { class: "flex flex-col",
                    span { "{file_meta.name}" }
                    span { class: "text-xs opacity-60", "{format_file_size(file_meta.size)}" }
                }
            }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
    }
}

/// 格式化文件大小
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

/// 渲染工具调用卡片
fn render_tool_call_card(
    content: &str,
    is_result: bool,
    expanded: bool,
    mut toggle_expand: impl FnMut() + 'static,
    time: i64,
    time_class: &str,
) -> Element {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(content);

    match parsed {
        Ok(json) => {
            let tool_name = json.get("tool_name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            let (status_badge, header_class) = if is_result {
                match json.get("is_success").and_then(|v| v.as_bool()) {
                    Some(true) => ("badge badge-success", "flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-base-300/50 transition-colors border-l-4 border-l-success"),
                    Some(false) => ("badge badge-error", "flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-base-300/50 transition-colors border-l-4 border-l-error"),
                    None => ("badge badge-info", "flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-base-300/50 transition-colors border-l-4 border-l-info"),
                }
            } else {
                ("badge badge-warning", "flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-base-300/50 transition-colors border-l-4 border-l-warning")
            };

            let header_icon = if is_result { "⚙️" } else { "🔧" };
            let status_label = if is_result {
                match json.get("is_success").and_then(|v| v.as_bool()) {
                    Some(true) => "执行成功",
                    Some(false) => "执行失败",
                    None => "已执行",
                }
            } else {
                "调用请求"
            };

            let args_str = json.get("args").and_then(|v| serde_json::to_string_pretty(v).ok());
            let result_str = json.get("result").and_then(|v| serde_json::to_string_pretty(v).ok());
            let error_msg = json.get("error_message").and_then(|v| v.as_str()).map(|s| s.to_string());

            rsx! {
                div { class: "chat-bubble chat-bubble-neutral p-0 overflow-hidden max-w-md",
                    div {
                        class: "{header_class}",
                        onclick: move |_| toggle_expand(),
                        span { "{header_icon}" }
                        span { class: "font-medium text-sm", "{tool_name}" }
                        span { class: "{status_badge} badge-sm", "{status_label}" }
                        span { class: "ml-auto text-xs opacity-60",
                            if expanded { "▼" } else { "▶" }
                        }
                    }
                    if expanded {
                        div { class: "px-3 pb-3 space-y-2 text-sm",
                            if let Some(args) = &args_str {
                                div {
                                    div { class: "text-xs font-semibold opacity-60 mb-1", "参数" }
                                    pre { class: "bg-base-200 p-2 rounded text-xs overflow-x-auto", "{args}" }
                                }
                            }
                            if is_result {
                                if let Some(result) = &result_str {
                                    div {
                                        div { class: "text-xs font-semibold opacity-60 mb-1", "结果" }
                                        pre { class: "bg-base-200 p-2 rounded text-xs overflow-x-auto", "{result}" }
                                    }
                                }
                                if let Some(err) = &error_msg {
                                    div {
                                        div { class: "text-xs font-semibold text-error mb-1", "错误" }
                                        div { class: "text-error text-sm", "{err}" }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "{time_class}", "{format_time(time)}" }
            }
        }
        Err(_) => rsx! {
            div { class: "chat-bubble chat-bubble-neutral break-words", "{content}" }
            div { class: "{time_class}", "{format_time(time)}" }
        },
    }
}

/// 渲染任务分配卡片
fn render_task_card(content: &str, time: i64, bubble_class: &str, time_class: &str) -> Element {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(content);

    match parsed {
        Ok(json) => {
            let title = json.get("task_title").and_then(|v| v.as_str()).unwrap_or("未知任务").to_string();
            let description = json.get("task_description").and_then(|v| v.as_str());

            rsx! {
                div { class: "{bubble_class} p-0 overflow-hidden max-w-md",
                    div { class: "flex items-center gap-2 px-3 py-2 border-b border-base-300/30",
                        span { "📋" }
                        span { class: "font-medium", "任务分配" }
                    }
                    div { class: "px-3 py-2 space-y-1",
                        div { class: "font-semibold", "{title}" }
                        if let Some(desc) = description {
                            if !desc.is_empty() {
                                div { class: "text-sm opacity-80", "{desc}" }
                            }
                        }
                    }
                }
                div { class: "{time_class}", "{format_time(time)}" }
            }
        }
        Err(_) => rsx! {
            div { class: "{bubble_class} break-words", "{content}" }
            div { class: "{time_class}", "{format_time(time)}" }
        },
    }
}

/// 消息项：可能是普通消息，也可能是日期分隔条
enum MessageListEntry {
    /// 日期分隔条
    DateDivider(String),
    /// 普通消息
    Message(MessageListItem),
}

/// 将消息按日期分组，生成带日期分隔条的列表
fn group_messages_by_date(messages: &[MessageListItem]) -> Vec<MessageListEntry> {
    let mut entries: Vec<MessageListEntry> = Vec::new();
    let mut current_date = String::new();
    for msg in messages {
        let label = format_date_group_label(msg.created_at);
        if label != current_date {
            entries.push(MessageListEntry::DateDivider(label.clone()));
            current_date = label;
        }
        entries.push(MessageListEntry::Message(msg.clone()));
    }
    entries
}

/// 格式化毫秒时间戳为日期分组标签 (今天 / 昨天 / YYYY-MM-DD)
fn format_date_group_label(ts_ms: i64) -> String {
    use chrono::{DateTime, Utc};
    let secs = (ts_ms / 1000) as i64;
    let dt: DateTime<Utc> = DateTime::<Utc>::from_timestamp(secs, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

    let now: DateTime<Utc> = DateTime::<Utc>::from_timestamp(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        0,
    )
    .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

    let today = today_key(now);
    let key = today_key(dt);
    if key == today {
        return "今天".to_string();
    }
    let yesterday = today_pred(today, -1);
    if key == yesterday {
        return "昨天".to_string();
    }
    format!("{:04}-{:02}-{:02}", key.0, key.1, key.2)
}

fn today_key(dt: chrono::DateTime<chrono::Utc>) -> (i32, u32, u32) {
    use chrono::Datelike;
    (dt.year(), dt.month(), dt.day())
}

/// 计算给定日期前后 offset 天
fn today_pred((y, m, d): (i32, u32, u32), offset: i32) -> (i32, u32, u32) {
    use chrono::{Datelike, Duration, NaiveDate};
    let date = NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let new_date = date + Duration::days(offset as i64);
    (new_date.year(), new_date.month(), new_date.day())
}