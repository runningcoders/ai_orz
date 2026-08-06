//! tasks/sendSubscribe — SSE 流式提交任务
//!
//! 流程：
//! 1. JWT 提取 user_id
//! 2. 创建 project + message（同 tasks/send）
//! 3. 订阅用户的 SSE channel（复用现有 message_push 机制）
//! 4. 返回 SSE 流：每次收到消息更新时推送完整 A2A Task
//!
//! SSE 推送粒度：完整 A2A Task（当前仅包含 messages，artifacts 暂为空）。
//!
//! 关键：复用现有 SSE 基础设施（message_push_dal），A2A 客户端和前端页面共享同一个
//! 用户级 broadcast channel，同一条消息会自动推送给所有订阅者。
//!
//! TODO: 后续会拓展统一事件系统，将 task/artifact 等运行时数据变更统一纳入事件流，
//! 通过消费者分发，届时可推送完整的 task 状态变更。

use axum::Extension;
use axum::response::sse::{Event, Sse};
use futures_util::{Stream, StreamExt};
use std::convert::Infallible;
use std::pin::Pin;
use tokio_stream::wrappers::BroadcastStream;

use common::api::a2a::SendTaskParams;
use common::error::Result;

use crate::handlers::a2a::mapper::{build_a2a_task, extract_text_from_a2a_message};
use crate::pkg::RequestContext;
use crate::service::dal::message_push::SsePushPayload;
use crate::service::domain::message;
use crate::service::domain::project::domain as project_domain;

type SseStream = Pin<Box<dyn Stream<Item = std::result::Result<Event, Infallible>> + Send>>;

/// 处理 tasks/sendSubscribe 请求（SSE 流式响应）
pub async fn handle_send_subscribe(
    Extension(ctx): Extension<RequestContext>,
    axum::Json(params): axum::Json<SendTaskParams>,
) -> Sse<SseStream> {
    let user_id = ctx.uid().to_string();
    if user_id.is_empty() {
        let stream = futures_util::stream::once(async {
            Ok(Event::default()
                .event("error")
                .data("A2A 请求缺少用户上下文"))
        });
        return Sse::new(Box::pin(stream));
    }

    let (project_id, session_id) = match do_create_project_and_message(ctx.clone(), params).await {
        Ok((pid, sid)) => (pid, sid),
        Err(e) => {
            let stream = futures_util::stream::once(async move {
                Ok(Event::default()
                    .event("error")
                    .data(format!("创建任务失败: {}", e)))
            });
            return Sse::new(Box::pin(stream));
        }
    };

    let subscribe_result = match message::domain()
        .delivery()
        .subscribe_sse(ctx.clone(), &user_id)
        .await
    {
        Ok(sr) => sr,
        Err(e) => {
            let stream = futures_util::stream::once(async move {
                Ok(Event::default()
                    .event("error")
                    .data(format!("订阅消息流失败: {}", e)))
            });
            return Sse::new(Box::pin(stream));
        }
    };

    let connection_id = subscribe_result.connection_id.clone();
    let rx = subscribe_result.receiver;

    let ctx_clone = ctx.clone();
    let project_id_clone = project_id.clone();
    let session_id_clone = session_id.clone();

    let stream = BroadcastStream::new(rx).then(move |msg| {
        let ctx = ctx_clone.clone();
        let project_id = project_id_clone.clone();
        let session_id = session_id_clone.clone();
        async move {
            match msg {
                Ok(data) => {
                    let payload: SsePushPayload = match serde_json::from_str(&data) {
                        Ok(p) => p,
                        Err(_) => {
                            return Ok(Event::default().event("error").data("无效的消息格式"));
                        }
                    };

                    if payload.project_id.as_deref() != Some(project_id.as_str()) {
                        return Ok(Event::default().event("ping").data("keep-alive"));
                    }

                    match build_task_event(ctx, &project_id, &session_id).await {
                        Ok(task_json) => Ok(Event::default().event("task").data(task_json)),
                        Err(e) => Ok(Event::default().event("error").data(format!("{}", e))),
                    }
                }
                Err(_) => Ok(Event::default().event("ping").data("keep-alive")),
            }
        }
    });

    let ctx_cleanup = ctx.clone();
    let conn_id_cleanup = connection_id.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = message::domain()
            .delivery()
            .unsubscribe_sse(ctx_cleanup, &conn_id_cleanup)
            .await;
    });

    Sse::new(Box::pin(stream))
}

async fn do_create_project_and_message(
    ctx: RequestContext,
    params: SendTaskParams,
) -> Result<(String, Option<String>)> {
    let user_id = ctx.uid();

    let agent = crate::service::domain::hr::domain()
        .resolve_agent(ctx.clone())
        .await?
        .ok_or_else(|| common::error::Error::not_found("无可用前台 Agent"))?;
    let agent_id = agent.po.id.clone();

    let content_text = extract_text_from_a2a_message(&params.message);
    let project_name = match content_text.char_indices().nth(50) {
        Some((idx, _)) => format!("A2A: {}...", &content_text[..idx]),
        None => format!("A2A: {}", content_text),
    };

    let project = project_domain()
        .project_manage()
        .create(
            ctx.clone(),
            project_name,
            format!("A2A 协议任务（session: {:?}）", params.session_id),
            0,
            vec!["a2a".to_string()],
            Some(agent_id.clone()),
            user_id.clone(),
            user_id.clone(),
        )
        .await?;

    let project_id = project.po.id.clone();

    project_domain()
        .project_manage()
        .start(ctx.clone(), &project_id, user_id.clone())
        .await?;

    let cmd = message::SendToAgentCommand {
        from_id: &user_id,
        from_role: common::enums::MessageRole::User,
        to_agent_id: &agent_id,
        content: &content_text,
        project_id: Some(&project_id),
        task_id: None,
        reply_to_id: None,
        attachment_ids: None,
        message_type: common::enums::MessageType::Text,
    };
    let _message = message::domain()
        .delivery()
        .send_to_agent(ctx.clone(), cmd)
        .await?;

    Ok((project_id, params.session_id))
}

async fn build_task_event(
    ctx: RequestContext,
    project_id: &str,
    session_id: &Option<String>,
) -> Result<String> {
    let project = project_domain()
        .project_manage()
        .get(ctx.clone(), project_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found("Project"))?;

    let messages = message::domain()
        .management()
        .list_by_project_id(ctx.clone(), project_id)
        .await?;

    // TODO: 后续统一事件系统会推送 artifact/task 等运行时数据变更
    // 当前仅推送消息，artifacts 传空数组
    let task = build_a2a_task(
        project_id,
        project.po.status,
        &messages,
        &[],
        session_id.clone(),
    );

    Ok(serde_json::to_string(&task)?)
}
