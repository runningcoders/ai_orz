//! Message Delivery 具体实现

use crate::models::file::FileMeta;
use crate::models::message::Message;
use crate::models::message::MessagePo;
use crate::models::message::TaskAssignmentMessage;
use crate::models::message::ToolCallMessage;
use crate::pkg::RequestContext;
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::resource_resolver;
use crate::service::domain::message::{
    DeliverMessageCommand, MessageDelivery, ReplyTarget, SendTaskAssignmentCommand,
    SendToAgentCommand, SendToUserCommand, SendToolCallRequestCommand, SendToolCallResultCommand,
    ToolCallExecutionOutcome,
};
use std::collections::HashSet;

use common::enums::{MessageRole, MessageType};
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
        if let Some(external_key) = cmd
            .external
            .external_key
            .as_deref()
            .filter(|k| !k.is_empty())
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

        // 先生成消息 ID：正文（含附件引用）就是这一条，不再有旁路附件消息
        let root_msg_id = generate_id();

        // root_id 继承：如果有 reply_to_id，查询父消息的 root_id 作为链根；
        // 否则当前消息自身为链根。修复：之前 root_id 始终为自身 ID，多轮对话
        // 每条消息都是独立 root，无法按 root_id 拉取完整对话链
        //
        // fallback 策略：若父消息 root_id 为 None（历史遗留数据），用父消息 ID
        // 作为链根而非当前消息 ID，使后续回复都能归到父消息下（父消息自身
        // root_id 仍为 None 但不影响新消息分组）
        // 同一次父消息查询顺带继承话题 ID 与群会话 ID：入站话题/群消息的 thread_id/
        // chat_id 由 `cmd` 直接带入；链式回复（无事件字段）回退继承父消息。
        let (chain_root_id, inherited_thread_id, inherited_chat_id) = match cmd.reply_to_id {
            Some(parent_id) => match self.message_dal.find_by_id(ctx.clone(), parent_id).await {
                Ok(Some(parent)) => (
                    parent.po.root_id.unwrap_or_else(|| parent_id.to_string()),
                    parent.po.thread_id,
                    parent.po.chat_id,
                ),
                _ => (root_msg_id.clone(), None, None),
            },
            None => (root_msg_id.clone(), None, None),
        };

        // 附件不再生成独立消息：正文里的 `[文件名](attachment:{id})` 引用是唯一
        // 事实源，展示与唤醒都从正文解析（将来真要反查关联用关系表，不落专用列）。
        //
        // 拼装发生在**请求侧**：站内发送由前端把引用写进 `content`，渠道入站由
        // `producer::message_channel` 在适配层完成「附件 → 协议引用」转换，两者共用
        // `common::markdown_protocol` 的拼装口。本层不改写请求侧已拼好的正文，只做：
        // ① 对 `attachment_ids` 声明的附件做归属校验（客户端可伪造正文引用，显式
        //    声明清单才是可信边界），并把「只声明、没写进正文」的 id 兜底补成引用——
        //    兼容只传数组的 LLM 工具调用 / 老客户端，避免附件静默丢失；
        // ② 把正文含资源引用的 Text 归一为 Mixed（幂等收口，handler / producer
        //    不必各判一次；AgentNotify 等语义类型保持不升级）。
        let mut content = cmd.content.to_string();
        let mut message_type = cmd.message_type;
        if let Some(att_ids) = cmd.attachment_ids.filter(|ids| !ids.is_empty()) {
            // 请求侧已经写进正文的引用不重复拼装，展示名也以请求侧为准（用户可能
            // 主动改名 / 在光标位置插入引用，投递域无权重排）。
            let mut already_referenced: HashSet<String> =
                common::markdown_protocol::extract_refs(&content)
                    .into_iter()
                    .filter(|(r, _)| r.scheme == "attachment")
                    .map(|(r, _)| r.id)
                    .collect();
            let mut missing = Vec::new();
            for att_id in att_ids {
                let attachment = self
                    .attachment_dal
                    .get_by_id(ctx.clone(), att_id)
                    .await?
                    .ok_or_else(|| err!(ResourceNotFound, "Attachment {} not found", att_id))?;

                // 校验归属：声明的附件必须属于当前用户
                if !cmd.from_id.is_empty()
                    && attachment.po.root_user_id != cmd.from_id
                    && ctx.uid() != attachment.po.root_user_id
                {
                    bail_err!(InvalidRequest, "Attachment {} 不属于当前用户", att_id);
                }

                // 正文已写过的引用、以及本次循环已补过的重复声明，都不再补拼
                // （`insert` 返回 false 表示 id 已在集合里）。
                if !already_referenced.insert(att_id.clone()) {
                    continue;
                }
                // 兜底展示名用原始文件名：模型据此判断附件内容，前端非图片附件也
                // 据此出链接。空名兜底「附件」，避免 `[](attachment:id)` 空链接。
                let name = attachment.po.original_name.trim();
                let name = if name.is_empty() { "附件" } else { name };
                missing.push(common::markdown_protocol::format_ref(
                    &common::markdown_protocol::ProtocolRef::attachment(att_id),
                    name,
                ));
            }
            content = common::markdown_protocol::append_refs(&content, &missing);
        }
        // Mixed 归一：正文含 attachment / artifact 引用即混排（mention 族不算）。
        // 只升级 Text —— 知会语义（AgentNotify，防协作乒乓）与结构化通知类型由
        // 调用方决定，不能被这里覆盖。
        if message_type == MessageType::Text
            && resource_resolver::content_has_resource_refs(&content)
        {
            message_type = MessageType::Mixed;
        }

        // 2. 落库：一条消息，正文即请求侧拼好的混排协议文本
        let mut po = MessagePo::new(
            root_msg_id.clone(),
            project_id,
            task_id,
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
            cmd.from_role,
            MessageRole::Agent,
            message_type,
            content,
            None,
            FileMeta::default(),
            cmd.reply_to_id.map(|s| s.to_string()),
            Some(chain_root_id.clone()),
            ctx.organization_id().cloned(),
            cmd.from_id.to_string(),
        );
        // 外部渠道消息键（渠道入站消息才有，供跨渠道消息链反查）
        po.external_key = cmd.external.external_key.clone();
        // 外部渠道话题键：优先用入站事件携带的 thread_id；链式回复回退继承父消息话题
        po.thread_id = cmd.external.thread_id.clone().or(inherited_thread_id);
        // 外部渠道会话键（群聊/私聊均有）：入站事件直接带入；链式回复回退继承父消息
        po.chat_id = cmd.external.chat_id.clone().or(inherited_chat_id);
        // 平台侧 @ 提及原始信息存档（无提及为空数组；JSON 存储细节由 PO 封装）
        po.set_external_mentions(cmd.external.external_mentions.clone());

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

        // 查一次父消息，同时继承消息链根（root_id）、话题 ID（thread_id）与
        // 群会话 ID（chat_id）：
        // - root_id：父消息的 root_id 作为链根；父消息缺失/无 reply_to_id → 自身为链根
        // - thread_id：父消息话题 ID（渠道入站话题消息才有），供出站话题内回复
        // - chat_id：父消息群会话 ID（渠道入站群消息才有），供出站按群直发
        // fallback：若父消息 root_id 为 None（历史遗留数据），用父消息 ID 作为链根
        let (root_id, inherited_thread_id, inherited_chat_id) = match cmd.reply_to_id {
            Some(parent_id) => match self.message_dal.find_by_id(ctx.clone(), parent_id).await {
                Ok(Some(parent)) => (
                    parent.po.root_id.unwrap_or_else(|| parent_id.to_string()),
                    parent.po.thread_id,
                    parent.po.chat_id,
                ),
                _ => (id.clone(), None, None),
            },
            None => (id.clone(), None, None),
        };

        let mut po = MessagePo::new(
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
        // 出站消息继承父消息话题 ID，供后续话题内回复与消息链分组
        po.thread_id = inherited_thread_id;
        // 出站消息继承父消息群会话 ID，供出站时按群回复/直发
        po.chat_id = inherited_chat_id;

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
        // 出站引用回复：按出站消息的 reply_to_id 反查父消息一次，产出渠道无关的
        // 回复目标（父消息外部渠道键 + 话题 ID），透传给渠道 DAL 决定是否走
        // 「引用回复」端点。查不到父消息 / 父消息尚未落外部键 → None，渠道侧降级普通发送。
        // 主键点查、成本极低；父消息的上下文无法跨「创建 → 推送」两段异步在内存传递。
        let reply_target = match cmd.message.po.reply_to_id.as_deref() {
            Some(parent_id) => match self.message_dal.find_by_id(ctx.clone(), parent_id).await? {
                Some(parent) => match parent.po.external_key.filter(|k| !k.is_empty()) {
                    Some(external_key) => Some(ReplyTarget {
                        external_key,
                        thread_id: parent.po.thread_id,
                        chat_id: parent.po.chat_id,
                    }),
                    None => None,
                },
                None => None,
            },
            None => None,
        };

        let channel_result = if cmd.options.channels {
            // 外部渠道出站降级（批2）：协议引用（[文本](attachment:id) 等）在
            // 渠道分发前统一降级为纯文本占位（[附件：名]/[产物：名]，未命中快照
            // 兜底防悬空）——lark/wechat/email 渠道 DAL 收到的即降级文本，零改动；
            // SSE 站内不降级（下方原样推送，前端 chip 渲染）。
            let (attachments, artifacts) = resource_resolver::resolve_message_resources(
                &ctx,
                &self.attachment_dal,
                &self.artifact_dal,
                cmd.message.content(),
            )
            .await;
            let registry = resource_resolver::build_resolved_registry(attachments, artifacts);
            let degraded =
                resource_resolver::degrade_content_for_external(cmd.message.content(), &registry);
            // 降级零改动（无协议引用，幂等）→ 原消息分发；有改动 → 局部降级副本，
            // 仅作用于本次渠道分发，库内消息保持协议原文（SSE 站内同原文）。
            let degraded_message;
            let outbound = if degraded == cmd.message.content() {
                cmd.message
            } else {
                let mut po = cmd.message.po.clone();
                po.content = degraded;
                degraded_message = Message::from_po(po);
                &degraded_message
            };
            self.message_channel_dal
                .deliver_message(ctx.clone(), outbound, cmd.user_id, reply_target)
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
