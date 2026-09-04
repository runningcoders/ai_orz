//! tasks/send — 异步提交任务
//!
//! 异步流程（与飞书/前端聊天的唤醒链路完全一致）：
//! 1. JWT 提取 user_id
//! 2. 调用 resolve_agent(ctx) 获取前台 Agent（agent 与 project 是两个维度，不耦合）
//! 3. 创建 project（对应 A2A task），将 agent.id 作为 owner_agent_id 绑定
//! 4. 创建 message（from=user, to=agent）→ 自动入队 event_queue
//! 5. 立即返回 working 状态的 A2aTask（不等待 Agent 回复）
//!
//! 唤醒由 consumer 异步完成（复用现有链路）：
//! - consumer worker dequeue → handle_agent_message
//! - 内部自动 wake_agent_brain（幂等）+ awaken
//! - Agent 回复 message → 客户端通过 tasks/get 轮询获取结果
//!
//! 关键：handler 层显式组合 agent 与 project 两个维度（先拿 agent，再创建 project 绑定），
//! 不在 domain 层混合。resolve_agent 只接受 ctx，不感知 project。
//! handler 层不调用 wake_agent_brain / awaken，唤醒由 consumer 异步闭环。

use common::api::AgentMatchCriteria;
use common::api::a2a::{A2aTask, SendTaskParams};
use common::constants::agent_roles::ROLE_A2A_GATEWAY;
use common::enums::{MessageType, ProjectStatus};
use common::error::{Result, bail_err};

use crate::handlers::a2a::mapper::{build_a2a_task, extract_text_from_a2a_message};
use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::pkg::RequestContext;
use crate::service::dal::message_channel;
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::message::{self, SendToAgentCommand};
use crate::service::domain::project::domain as project_domain;

/// 处理 tasks/send 请求（异步提交）
pub async fn handle_send_task(ctx: RequestContext, params: SendTaskParams) -> Result<A2aTask> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "A2A 请求缺少用户上下文");
    }

    // 1. handler 层显式查询前台 Agent（A2A 入口优先命中 a2a_gateway 系统角色）
    let agent = hr_domain()
        .resolve_agent(ctx.clone(), AgentMatchCriteria::by_role(ROLE_A2A_GATEWAY))
        .await?
        .ok_or_else(|| common::error::Error::not_found("无可用前台 Agent"))?;
    let agent_id = agent.po.id.clone();

    // 2. 创建 project（对应 A2A task），将 agent.id 作为 owner_agent_id 绑定
    //    来源审计（方案 B）：联邦请求往 tags 追加 federation:<对端org>，FTS5 已索引 tags，
    //    后续可按项目维度检索外部来源；本地 JWT 路径无 caller，tags 仅含 "a2a"。
    let content_text = extract_text_from_a2a_message(&params.message);
    // 用 char_indices 做字符级截断，避免 UTF-8 字节切片 panic
    let project_name = match content_text.char_indices().nth(50) {
        Some((idx, _)) => format!("A2A: {}...", &content_text[..idx]),
        None => format!("A2A: {}", content_text),
    };
    let mut tags = vec!["a2a".to_string()];
    if let Some(peer_org) = ctx.caller_organization_id() {
        tags.push(format!("federation:{}", peer_org));
    }

    let project = project_domain()
        .project_manage()
        .create(
            ctx.clone(),
            project_name.clone(),
            format!("A2A 协议任务（session: {:?}）", params.session_id),
            0, // 默认优先级
            tags,
            Some(agent_id.clone()), // ← handler 层已查询 agent，直接绑定
            user_id.clone(),
            user_id.clone(),
        )
        .await?;

    let project_id = project.po.id.clone();

    // 3. 启动项目（流转到 InProgress）
    project_domain()
        .project_manage()
        .start(ctx.clone(), &project_id, user_id.clone())
        .await?;

    // 4. 创建 message（from=user, to=agent）→ 自动入队 event_queue
    //    consumer 异步消费 → wake_agent_brain（幂等）+ awaken → Agent 回复
    let cmd = SendToAgentCommand {
        from_id: &user_id,
        from_role: common::enums::MessageRole::User,
        to_agent_id: &agent_id,
        content: &content_text,
        project_id: Some(&project_id),
        task_id: None,
        reply_to_id: None,
        attachment_ids: None,
        message_type: MessageType::Text,
    };
    let _message = message::domain()
        .delivery()
        .send_to_agent(ctx.clone(), cmd)
        .await?;

    // 4.5 如果提供了 notification_url，创建 A2aCallback 渠道（PushNotifications）
    //    后续消息推送时，deliver_message 会按 scope_project 过滤并推送到该 URL
    if let Some(notification_url) = params.notification_url {
        let channel_po = MessageChannelPo::new(
            uuid::Uuid::now_v7().to_string(),
            ctx.organization_id().unwrap_or(&"".to_string()).to_string(),
            user_id.clone(),
            None, // 不绑定特定 Agent
            common::enums::ChannelType::A2aCallback,
            format!("A2A Callback for {}", project_name),
            Some(notification_url),
            None, // access_token
            None, // secret
            ChannelConfig::default(),
            user_id.clone(),
        );
        let mut channel = MessageChannel::from_po(channel_po);
        channel.po.scope_project = Some(project_id.clone());
        message_channel::dal()
            .create_channel(ctx.clone(), &channel)
            .await?;
    }

    // 5. 立即返回 working 状态的 A2aTask（不等待 Agent 回复）
    //    客户端通过 tasks/get 轮询，直到状态变为 completed
    let task = build_a2a_task(
        &project_id,
        ProjectStatus::InProgress, // working 状态
        &[],                       // 暂无消息（Agent 回复尚未产生）
        &[],
        params.session_id,
    );

    Ok(task)
}
