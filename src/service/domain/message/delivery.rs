//! Message Delivery 具体实现

use crate::models::file::FileMeta;
use crate::models::message::Message;
use crate::models::message::MessagePo;
use crate::models::message::TaskAssignmentMessage;
use crate::models::message::ToolCallMessage;
use crate::pkg::RequestContext;
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::{
    DeliverMessageCommand, MessageDelivery, SendTaskAssignmentCommand, SendToAgentCommand,
    SendToUserCommand, SendToolCallRequestCommand, SendToolCallResultCommand,
    ToolCallExecutionOutcome,
};
use common::enums::{FileType, MessageRole, MessageType};
use common::error::{Result, bail_err, err};
use serde_json::json;

use crate::enrich_ctx;

const TOOL_CALL_RESULT_INLINE_CONTENT_LIMIT: usize = 8 * 1024;

fn bounded_inline_tool_result(result: serde_json::Value) -> serde_json::Value {
    match serde_json::to_string(&result) {
        Ok(serialized) if serialized.len() <= TOOL_CALL_RESULT_INLINE_CONTENT_LIMIT => result,
        _ => json!({
            "truncated": true,
            "message": "tool result exceeded inline message limit"
        }),
    }
}

/// 生成新的消息 ID
fn generate_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 消息 → SSE 推送体
///
/// 与 `MessageListItem` 字段对齐：前端把 SSE 事件体直接反序列化为
/// `MessageListItem`，缺字段会让「刚推送的消息」与历史消息行为不一致
/// （如 `root_id` 缺失导致引用块跳转失效）。
fn build_sse_payload(message: &Message) -> crate::service::dal::message_push::SsePushPayload {
    let file_meta = message.file_meta().map(|fm| {
        let name = fm
            .file_path
            .rsplit('/')
            .next()
            .unwrap_or(&fm.file_path)
            .to_string();
        common::api::message::FileMetaInfo {
            name,
            mime_type: fm.mime_type.clone(),
            size: fm.file_size,
        }
    });
    crate::service::dal::message_push::SsePushPayload {
        message_id: message.id().to_string(),
        project_id: message.project_id().map(|s| s.to_string()),
        task_id: message.task_id().map(|s| s.to_string()),
        from_id: message.from_id().to_string(),
        from_role: message.from_role() as i32,
        to_id: message.to_id().to_string(),
        to_role: message.to_role() as i32,
        message_type: message.message_type() as i32,
        status: message.status() as i32,
        content: message.content().to_string(),
        reply_to_id: message.reply_to_id().map(|s| s.to_string()),
        root_id: message.root_id().map(|s| s.to_string()),
        created_at: message.created_at(),
        file_type: message.file_type().map(|ft| ft as i32),
        file_meta,
    }
}

/// 将 Attachment FileType 映射到对应的 MessageType
fn map_file_type_to_message_type(file_type: FileType) -> MessageType {
    match file_type {
        FileType::Image => MessageType::Image,
        FileType::Audio => MessageType::Audio,
        FileType::Video => MessageType::Video,
        // 文档与二进制文件统一归类为 File
        FileType::Document | FileType::Binary => MessageType::File,
    }
}

impl MessageDomainImpl {
    /// 收件人「角色 ⟷ ID」一致性门闩
    ///
    /// 背景：`to_role` 由本模块**硬编码**（`send_to_user` 恒 `User`、`send_to_agent` 恒
    /// `Agent`），因此「角色与 ID 实指同一类实体」这条不变量只能在这里守。一旦破防，
    /// 消息会落成 `to_role=User + to_id=<Agent>` 这种错配行：消费端按角色分流到
    /// 「投递给用户」分支 → 找不到该用户的任何渠道 → `All delivery channels failed`
    /// → 重试 8 次后 `DISCARDED`。**工具调用本身返回 Completed，失败只写 ERROR 日志**，
    /// 属于典型的「静默丢失」。
    ///
    /// 这里的错误文案不是给人看的，而是**回灌给模型**的：工具错误的 message 原样进入
    /// tool result（见 `consumer::message::tool_error_message`），所以文案要直接点名
    /// 「该改用哪个工具」，这是修正 Agent 认知的唯一时机。
    ///
    /// 只拦**跨类型碰撞**，刻意**不做存在性校验**：
    /// - 调用方遍布框架内部（渠道入站、任务调度、A2A 回传）与测试夹具，存在 `system`
    ///   这类合成 ID，联邦/外部 Agent 也未必有本地行；
    /// - 「ID 是否真实存在」的判定归各投递链路自身，这里只负责「角色与 ID 明显不符」。
    async fn ensure_recipient_role(
        &self,
        ctx: &RequestContext,
        id: &str,
        expected: MessageRole,
    ) -> Result<()> {
        match expected {
            MessageRole::User => {
                if let Some(agent) = self.agent_dal.find_by_id(ctx.clone(), id).await? {
                    bail_err!(
                        InvalidRequest,
                        "收件人 ID「{}」是 Agent「{}」，不是用户：send_message 只能发给用户。\
                         跨 Agent 协作/知会请改用 send_message_to_agent（to_agent_id=\"{}\"）；\
                         派发任务用 send_task_assignment_message。",
                        id,
                        agent.po.name,
                        id
                    );
                }
            }
            MessageRole::Agent => {
                if let Some(user) = self.user_dal.find_by_id(ctx.clone(), id).await? {
                    let name = if user.display_name.is_empty() {
                        user.username.clone()
                    } else {
                        user.display_name.clone()
                    };
                    bail_err!(
                        InvalidRequest,
                        "收件人 ID「{}」是用户「{}」，不是 Agent：\
                         send_message_to_agent / send_task_assignment_message 只能发给 Agent。\
                         给用户发消息请改用 send_message（to_user_id=\"{}\"）。",
                        id,
                        name,
                        id
                    );
                }
            }
            // System 收件人无对应实体表（如 send_tool_call_request 的执行器），不校验
            MessageRole::System => {}
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl MessageDelivery for MessageDomainImpl {
    async fn send_to_agent(
        &self,
        ctx: RequestContext,
        cmd: SendToAgentCommand<'_>,
    ) -> Result<Message> {
        // 收件人角色门闩：把用户 ID 当 to_agent_id 传（反向误用）同样会产生
        // `to_role=Agent + to_id=<用户>` 的死信，且永远不会有人被唤醒。先拦。
        self.ensure_recipient_role(&ctx, cmd.to_agent_id, MessageRole::Agent)
            .await?;

        // 入站幂等吸收：external_key 已存在 → 该外部消息已落过库（游标回退 / 服务端
        // 重推 / 事件重投后的重复拉取），返回既有消息避免重复投递给 Agent。
        // 先查后插存在 TOCTOU 窗口，但 AOP 队列按 event_id 在途去重已挡掉绝大多数
        // 并发路径，残余窗口与「单实例」威胁模型匹配（email 适配层同款模式）。
        // 查重失败上抛：宁可重复投递也不静默丢消息。
        if let Some(external_key) = cmd.external_key.filter(|k| !k.is_empty())
            && let Some(existing_id) = self
                .message_dal
                .find_id_by_external_key(ctx.clone(), external_key)
                .await?
        {
            log_info!(
                &ctx,
                "message_delivery",
                "duplicate external message skipped: external_key={} existing_id={}",
                external_key,
                existing_id
            );
            if let Some(existing) = self
                .message_dal
                .find_by_id(ctx.clone(), &existing_id)
                .await?
            {
                return Ok(existing);
            }
        }

        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let task_id = cmd
            .task_id
            .or_else(|| ctx.task_id().map(|s| s.as_str()))
            .map(|s| s.to_string());

        // 先生成根消息 ID（文本消息的 ID），附件消息使用 reply_to_id 链回根
        let root_msg_id = generate_id();

        // root_id 继承：如果有 reply_to_id，查询父消息的 root_id 作为链根；
        // 否则当前消息自身为链根。修复：之前 root_id 始终为自身 ID，多轮对话
        // 每条消息都是独立 root，无法按 root_id 拉取完整对话链
        //
        // fallback 策略：若父消息 root_id 为 None（历史遗留数据），用父消息 ID
        // 作为链根而非当前消息 ID，使后续回复都能归到父消息下（父消息自身
        // root_id 仍为 None 但不影响新消息分组）
        let chain_root_id = match cmd.reply_to_id {
            Some(parent_id) => match self.message_dal.find_by_id(ctx.clone(), parent_id).await {
                Ok(Some(parent)) => parent.po.root_id.unwrap_or_else(|| parent_id.to_string()),
                _ => root_msg_id.clone(),
            },
            None => root_msg_id.clone(),
        };

        // 1. 处理附件消息：按数组顺序创建 N 条附件消息
        if let Some(att_ids) = cmd.attachment_ids {
            for att_id in att_ids {
                let attachment = self
                    .attachment_dal
                    .get_by_id(ctx.clone(), att_id)
                    .await?
                    .ok_or_else(|| err!(ResourceNotFound, "Attachment {} not found", att_id))?;

                // 校验归属：附件必须属于当前用户
                if !cmd.from_id.is_empty()
                    && attachment.po.root_user_id != cmd.from_id
                    && ctx.uid() != attachment.po.root_user_id
                {
                    bail_err!(InvalidRequest, "Attachment {} 不属于当前用户", att_id);
                }

                let att_msg_id = generate_id();
                let file_type = map_file_type_to_message_type(attachment.po.file_type);
                let file_meta = FileMeta::new(
                    attachment.po.relative_path.clone(),
                    attachment.po.mime_type.clone(),
                    attachment.po.size as u64,
                );

                let att_po = MessagePo::new(
                    att_msg_id.clone(),
                    project_id.clone(),
                    task_id.clone(),
                    cmd.from_id.to_string(),
                    cmd.to_agent_id.to_string(),
                    cmd.from_role,
                    MessageRole::Agent,
                    file_type,
                    attachment.po.id.clone(),
                    Some(attachment.po.file_type),
                    file_meta,
                    Some(root_msg_id.clone()),
                    Some(chain_root_id.clone()),
                    ctx.organization_id().cloned(),
                    cmd.from_id.to_string(),
                );

                let att_message = Message::from_po(att_po);
                let att_ctx = enrich_ctx!(&ctx, &att_message);
                self.message_dal
                    .save_message(att_ctx.clone(), &att_message)
                    .await?;
            }
        }

        // 2. 创建文本消息（root_id 继承自父消息或自身）
        let mut po = MessagePo::new(
            root_msg_id.clone(),
            project_id,
            task_id,
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
            cmd.from_role,
            MessageRole::Agent,
            cmd.message_type,
            cmd.content.to_string(),
            None,
            FileMeta::default(),
            cmd.reply_to_id.map(|s| s.to_string()),
            Some(chain_root_id.clone()),
            ctx.organization_id().cloned(),
            cmd.from_id.to_string(),
        );
        // 外部渠道消息键（渠道入站消息才有，供跨渠道消息链反查）
        po.external_key = cmd.external_key.map(|s| s.to_string());

        let message = Message::from_po(po);
        let ctx = enrich_ctx!(&ctx, &message);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_to_user(
        &self,
        ctx: RequestContext,
        cmd: SendToUserCommand<'_>,
    ) -> Result<Message> {
        // 收件人角色门闩：`to_role` 在此硬编码为 User，而 Agent 常把自己同伴的 ID
        // 当 `to_user_id` 传进来 → 必产生投递必失败的死信。这里直接报错并给出正确工具。
        self.ensure_recipient_role(&ctx, cmd.to_user_id, MessageRole::User)
            .await?;

        let id = generate_id();
        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let task_id = cmd
            .task_id
            .or_else(|| ctx.task_id().map(|s| s.as_str()))
            .map(|s| s.to_string());

        // root_id 继承：如果有 reply_to_id，查询父消息的 root_id；否则自身为 root
        // fallback：若父消息 root_id 为 None（历史遗留数据），用父消息 ID 作为链根
        let root_id = match cmd.reply_to_id {
            Some(parent_id) => match self.message_dal.find_by_id(ctx.clone(), parent_id).await {
                Ok(Some(parent)) => parent.po.root_id.unwrap_or_else(|| parent_id.to_string()),
                _ => id.clone(),
            },
            None => id.clone(),
        };

        let po = MessagePo::new(
            id.clone(),
            project_id,
            task_id,
            cmd.from_agent_id.to_string(),
            cmd.to_user_id.to_string(),
            MessageRole::Agent,
            MessageRole::User,
            MessageType::Text,
            cmd.content.to_string(),
            None,
            Default::default(),
            cmd.reply_to_id.map(|s| s.to_string()),
            Some(root_id),
            ctx.organization_id().cloned(),
            cmd.from_agent_id.to_string(),
        );

        let message = Message::from_po(po);
        let ctx = enrich_ctx!(&ctx, &message);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_tool_call_request(
        &self,
        ctx: RequestContext,
        cmd: SendToolCallRequestCommand<'_>,
    ) -> Result<Message> {
        let id = generate_id();
        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let task_id = cmd
            .task_id
            .or_else(|| ctx.task_id().map(|s| s.as_str()))
            .map(|s| s.to_string());

        let mut payload = ToolCallMessage::new_request(
            cmd.request_id.to_string(),
            cmd.tool_id.to_string(),
            cmd.tool_name.to_string(),
            project_id,
            task_id,
            cmd.from_agent_id.to_string(),
            cmd.to_executor_id.to_string(),
            cmd.reply_to_id.map(|s| s.to_string()),
            cmd.args,
        );
        // 填充 ctx 字段，供 consumer 异步路径重建 ctx
        // 修复：之前 from_role=Agent 时 user_id 不设置，log_id/model_* 全部丢失
        payload.from_log_id = Some(ctx.log_id.clone());
        payload.from_user_id = ctx.user_id().cloned();
        payload.from_model_provider_id = ctx.model_provider_id().cloned();
        payload.from_model_name = ctx.model_name().cloned();

        let content = serde_json::to_string(&payload)
            .map_err(|e| err!(Internal, "failed to serialize tool call request").with_source(e))?;

        let po = MessagePo::new(
            id.clone(),
            payload.project_id.clone(),
            payload.task_id.clone(),
            payload.from_id.clone(),
            payload.to_id.clone(),
            MessageRole::Agent,
            MessageRole::System,
            MessageType::ToolCallRequest,
            content,
            None,
            Default::default(),
            payload.reply_to_id.clone(),
            Some(id), // root_id
            ctx.organization_id().cloned(),
            payload.from_id.clone(),
        );

        let message = Message::from_po(po);
        let ctx = enrich_ctx!(&ctx, &message);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_tool_call_result(
        &self,
        ctx: RequestContext,
        cmd: SendToolCallResultCommand<'_>,
    ) -> Result<Message> {
        if cmd.request_message.po.message_type != MessageType::ToolCallRequest {
            bail_err!(InvalidRequest, "request_message must be ToolCallRequest");
        }

        let request: ToolCallMessage = serde_json::from_str(&cmd.request_message.po.content)
            .map_err(|e| {
                err!(InvalidRequest, "invalid tool call request message").with_source(e)
            })?;

        let (mut result_payload, trace_ref) = match cmd.outcome {
            ToolCallExecutionOutcome::Success {
                result,
                result_file_meta,
                trace_ref,
            } => (
                request.new_success_result(bounded_inline_tool_result(result), result_file_meta),
                trace_ref,
            ),
            ToolCallExecutionOutcome::Failure {
                error_message,
                trace_ref,
            } => (request.new_error_result(error_message), trace_ref),
        };
        if let Some(trace_ref) = trace_ref {
            result_payload.trace_ref = Some(trace_ref);
        }

        let content = serde_json::to_string(&result_payload)
            .map_err(|e| err!(Internal, "failed to serialize tool call result").with_source(e))?;

        let id = generate_id();
        let po = MessagePo::new(
            id.clone(),
            result_payload.project_id.clone(),
            result_payload.task_id.clone(),
            result_payload.from_id.clone(),
            result_payload.to_id.clone(),
            MessageRole::System,
            MessageRole::Agent,
            MessageType::ToolCallResult,
            content,
            None,
            Default::default(),
            Some(cmd.request_message.id().to_string()),
            cmd.request_message.po.root_id.clone().or(Some(id)),
            cmd.request_message.po.organization_id.clone(),
            result_payload.from_id.clone(),
        );

        let message = Message::from_po(po);
        let ctx = enrich_ctx!(&ctx, &message);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_task_assignment(
        &self,
        ctx: RequestContext,
        cmd: SendTaskAssignmentCommand<'_>,
    ) -> Result<Message> {
        // 收件人角色门闩：任务分配的目标必须是 Agent（to_role 在此硬编码为 Agent）
        self.ensure_recipient_role(&ctx, cmd.to_agent_id, MessageRole::Agent)
            .await?;

        let id = generate_id();
        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());

        let payload = TaskAssignmentMessage::new(
            cmd.task_id.to_string(),
            cmd.task_title.to_string(),
            cmd.task_description.map(|s| s.to_string()),
            project_id.clone(),
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
        );

        let content = serde_json::to_string(&payload).map_err(|e| {
            err!(Internal, "failed to serialize task assignment message").with_source(e)
        })?;

        let po = MessagePo::new(
            id.clone(),
            project_id,
            Some(cmd.task_id.to_string()),
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
            cmd.from_role,
            MessageRole::Agent,
            MessageType::TaskAssignment,
            content,
            None,
            Default::default(),
            None,
            Some(id),
            ctx.organization_id().cloned(),
            cmd.from_id.to_string(),
        );

        let message = Message::from_po(po);
        let ctx = enrich_ctx!(&ctx, &message);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn deliver_message(
        &self,
        ctx: RequestContext,
        cmd: DeliverMessageCommand<'_>,
    ) -> Result<crate::service::dal::message_channel::DeliveryResult> {
        // 1. 投递到已配置的消息渠道（飞书/微信/钉钉等）
        // 飞书凭证由渠道 DAL 按引用 ID 直查凭证行（主键查询，无需预加载用户）
        //
        // 渠道入站消息走 `sse_only`：它本就来自该渠道，回灌会形成回声。
        let channel_result = if cmd.options.channels {
            self.message_channel_dal
                .deliver_message(ctx.clone(), cmd.message, cmd.user_id)
                .await?
        } else {
            crate::service::dal::message_channel::DeliveryResult::empty()
        };

        // 2. 投递到 SSE 长连接（如果用户有在线连接）
        let sse_delivered = if cmd.options.sse {
            let sse_payload = build_sse_payload(cmd.message);
            self.message_push_dal
                .push_to_sse(ctx, cmd.user_id, &sse_payload)
                .await
                .map(|r| r.delivered_count)
                .unwrap_or(0)
        } else {
            0
        };

        Ok(crate::service::dal::message_channel::DeliveryResult {
            total: channel_result.total,
            success: channel_result.success,
            failed: channel_result.failed,
            details: channel_result.details,
            sse_delivered,
        })
    }

    async fn subscribe_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<super::SubscribeResult> {
        let connection_id = uuid::Uuid::now_v7().to_string();
        let receiver = self
            .message_push_dal
            .subscribe_sse(ctx, user_id, &connection_id)
            .await;
        Ok(super::SubscribeResult {
            connection_id,
            receiver,
        })
    }

    async fn unsubscribe_sse(&self, ctx: RequestContext, connection_id: &str) -> Result<()> {
        self.message_push_dal
            .unsubscribe_sse(ctx, connection_id)
            .await;
        Ok(())
    }
}
