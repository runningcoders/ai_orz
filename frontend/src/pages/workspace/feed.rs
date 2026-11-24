//! 工作台实时消息入口：SSE 订阅 + 可见时兜底轮询
//!
//! 两路数据源共用同一套过滤口径（`message_in_context` / `context_agent_filter`），
//! 保证「实时推来的、刷新看到的、上拉翻到的」三处一致。
//!
//! 从页面壳拆出的原因：这部分只与消息信号耦合，与场景 / 席位 / 渲染无关；
//! 页面壳保留状态与编排，这里只负责把外部消息接进 `messages`。

use dioxus::prelude::*;
use std::collections::HashSet;
use std::time::Duration;

use common::api::{ListMessagesRequest, MessageListItem};

use crate::api::message::poll_new_messages;
use crate::components::workspace_graph::WorkspaceView;
use crate::hooks::{EventSourceOptions, use_event_source};
use crate::utils::{
    MESSAGE_POLL_LIMIT, MESSAGE_POLL_SECS, page_hidden, replace_tmp_with_real, request_scope,
};

use super::helpers::{context_agent_filter, filter_in_context, message_in_context};

/// 内存里保留的消息上限（超出后丢最旧的；上拉可再翻回来）
const MAX_KEEP: usize = 500;

/// 实时通道需要的页面状态（`Signal` 本身可 Copy，整体按值传入）
#[derive(Clone, Copy)]
pub struct MessageFeedSignals {
    /// 当前会话的消息列表
    pub messages: Signal<Vec<MessageListItem>>,
    /// 对话上下文项目（`None` = 全局 / Agent 私聊）
    pub project_id: Signal<Option<String>>,
    /// 对话锚定的任务（TaskDetail 才有）
    pub task_id: Signal<Option<String>>,
    /// Agent 私聊收件人（其余视图为 `None`）
    pub to_agent_id: Signal<Option<String>>,
    /// 增量轮询游标（服务端时间戳，毫秒）
    pub poll_cursor: Signal<Option<i64>>,
    /// 有效对话视图（聚焦席位时是 AgentDetail，而非路由视图）
    pub view: Memo<WorkspaceView>,
    /// 侧边栏红点
    pub project_unread: Signal<HashSet<String>>,
    pub agent_unread: Signal<HashSet<String>>,
}

/// 挂载实时消息通道（SSE + 兜底轮询）
pub fn use_message_feed(signals: MessageFeedSignals) {
    use_sse(signals);
    use_polling_fallback(signals);
}

/// SSE 订阅：尽力而为的实时通道
fn use_sse(signals: MessageFeedSignals) {
    let mut messages = signals.messages;
    let mut project_unread = signals.project_unread;
    let mut agent_unread = signals.agent_unread;
    let chat_project_id = signals.project_id;
    let chat_to_agent_id = signals.to_agent_id;
    let view = signals.view;

    let on_message = EventHandler::new(move |data: String| {
        let Ok(msg) = serde_json::from_str::<MessageListItem>(&data) else {
            return;
        };
        let mut msgs = messages.write();
        // 移除同 content 的乐观消息（统一使用 replace_tmp_with_real）
        replace_tmp_with_real(&mut msgs, &msg);

        // 过滤口径与历史首屏 / 上拉翻页共用同一个判定（`message_in_context`），
        // 三处必须一致，否则会出现「实时推来的看得到、一刷新就消失」。
        let cur_pid = chat_project_id.read().clone();
        // raw_aid 供红点判定（保持「当前对话对象」原义），
        // cur_aid 是过滤用的（仅 Agent 私聊才收窄，见 context_agent_filter）
        let raw_aid = chat_to_agent_id.read().clone();
        let cur_aid = context_agent_filter(&view.read().clone(), raw_aid.clone());

        let matched = message_in_context(&msg, cur_pid.as_deref(), cur_aid.as_deref());

        if matched {
            // 去重
            if !msgs.iter().any(|m| m.message_id == msg.message_id) {
                msgs.push(msg);
                if msgs.len() > MAX_KEEP {
                    let drain = msgs.len() - MAX_KEEP;
                    msgs.drain(..drain);
                }
            }
        } else if msg.from_role == 1 {
            // 不属于当前视图 → 更新侧边栏红点；Agent 回复（from_role=1）才触发
            if let Some(pid) = &msg.project_id
                && cur_pid.as_deref() != Some(pid)
            {
                project_unread.write().insert(pid.clone());
            }
            let sender_aid = &msg.from_id;
            if raw_aid.as_deref() != Some(sender_aid) {
                agent_unread.write().insert(sender_aid.clone());
            }
        }
    });

    // 生命周期（创建 / 换 URL / 卸载）统一由 hook 管理
    use_event_source(
        "/api/v1/finance/messages/sse".to_string(),
        on_message,
        EventSourceOptions::default(),
    );
}

/// 兜底轮询：可见时按固定间隔拉当前会话的增量
///
/// 为什么需要它：SSE 是尽力而为的实时通道 —— 断线（休眠 / 换网）、内存
/// broadcast 缓冲溢出都会静默丢事件且不补发；而展示面（首屏 / 上拉）是查库的。
/// 两边一旦分叉，症状就是「刷新才看到」。这条循环让推送面重新对齐展示面，
/// 顺带补上「别的标签页 / 手机发的消息本页没有乐观气泡」。
///
/// ⚠️ 这个 effect 只起**一个**循环：同步执行的闭包体里不读任何 signal，
///    异步块里读的 signal 不进依赖表，否则每拍都会再 spawn 一个循环。
fn use_polling_fallback(signals: MessageFeedSignals) {
    let mut messages = signals.messages;
    let chat_project_id = signals.project_id;
    let chat_task_id = signals.task_id;
    let chat_to_agent_id = signals.to_agent_id;
    let mut poll_cursor = signals.poll_cursor;
    let view = signals.view;

    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::sleep(Duration::from_secs(MESSAGE_POLL_SECS)).await;
                // 页面不可见时零请求；切回来下一拍自然恢复
                if page_hidden() {
                    continue;
                }
                let pid = chat_project_id.read().clone();
                let tid = chat_task_id.read().clone();
                let cur_view = view.read().clone();
                // 任务视图锚点未就绪时首屏都没加载，轮询同样跳过
                if matches!(cur_view, WorkspaceView::TaskDetail(_)) && pid.is_none() {
                    continue;
                }
                let agent_only = context_agent_filter(&cur_view, chat_to_agent_id.read().clone());
                // 请求侧 project_id 与首屏 / 上拉同口径（默认对话用哨兵、Agent 私聊保持 None）
                let req_pid = if matches!(cur_view, WorkspaceView::AgentDetail(_)) {
                    None
                } else {
                    request_scope(pid.as_deref())
                };
                // 游标只认**服务端**时间戳。没有游标时传 `None` ⇒ 取「最新一页」，
                // 用响应里的时间戳校准；重复的按 `message_id` 去掉。
                let cursor = poll_cursor();
                let resp = match poll_new_messages(ListMessagesRequest {
                    project_id: req_pid,
                    // 开区间 + 回退 1ms：同一毫秒内的多条不会被游标跨过
                    after_timestamp: cursor.map(|c| c.saturating_sub(1)),
                    limit: Some(MESSAGE_POLL_LIMIT),
                    ..Default::default()
                })
                .await
                {
                    Ok(resp) => resp,
                    // 轮询失败静默：SSE 仍在跑，下一拍重试（不打扰用户）
                    Err(_) => continue,
                };
                // 竞态守卫：请求期间切了视图 / 会话 ⇒ 丢弃本次结果
                if chat_project_id.read().clone() != pid
                    || chat_task_id.read().clone() != tid
                    || *view.read() != cur_view
                {
                    continue;
                }
                // 游标只前进不回退：被前端滤掉的消息也要算进去，
                // 否则原始页一直被它们占住、永远推不到更新的一条
                if let Some(newest) = resp.messages.last().map(|m| m.created_at) {
                    poll_cursor.set(Some(cursor.map_or(newest, |c| newest.max(c))));
                }
                let fresh = filter_in_context(resp.messages, pid.as_deref(), agent_only.as_deref());
                if fresh.is_empty() {
                    continue;
                }
                let mut current = messages.write();
                if crate::utils::message::append_new_messages(&mut current, fresh) > 0
                    && current.len() > MAX_KEEP
                {
                    let drain = current.len() - MAX_KEEP;
                    current.drain(..drain);
                }
            }
        });
    });
}
