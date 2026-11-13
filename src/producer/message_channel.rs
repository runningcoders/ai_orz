use common::api::AgentMatchCriteria;
use common::constants::agent_roles::{
    ROLE_FEISHU_RECEPTION, ROLE_RECEPTION, ROLE_WECHAT_RECEPTION,
};
use common::enums::{CallerType, ChannelType, MemoryType, MessageRole, MessageStatus, MessageType};
use common::error::{Result, err};
use std::collections::HashSet;
use std::sync::Arc;

use crate::models::attachment::AttachmentUpload;
use crate::models::memory::Memory;
use crate::pkg::RequestContext;
use crate::pkg::adapter::AdaptedMessage;
use crate::pkg::adapter::message::MessageAdapterCallback;
use crate::producer::inbound_filter::{self, InboundGateDecision};
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use crate::service::dao::message::MessageQuery;
use crate::service::domain::finance::FinanceDomain;
use crate::service::domain::hr::HrDomain;
use crate::service::domain::message::{
    DeliverMessageCommand, DeliveryOptions, MessageDomain, SendToAgentCommand,
};

/// 群聊消息是否值得 Agent 回复的小脑判定指令（渠道无关）
const INBOUND_RELEVANCE_INSTRUCTIONS: &str = "结合【最近群聊】【相关记忆】【最近记忆】判断这条群聊消息是否需要 AI 助手接入回复。\
需要：明确向助手提问、@ 助手、指派任务、请求帮助，或讨论助手能处理的事务。\
不需要：成员之间的闲聊、寒暄、与助手无关的通知、已经在被其他人回答的对话、纯表情或图片。\
结合上下文里是否已有助手参与、当前话题是否延续来判断；拿不准时倾向需要。";

/// 群聊上下文抬取条数上限
const RECENT_MESSAGE_LIMIT: usize = 10;
/// 最近短期记忆条数上限
const RECENT_MEMORY_LIMIT: usize = 5;
/// 相关短期记忆条数上限
const SEARCH_MEMORY_LIMIT: usize = 3;
/// 群消息小脑判定默认超时（毫秒）
const DEFAULT_GROUP_TRIGGER_TIMEOUT_MS: u64 = 1000;
/// 超时配置允许范围（毫秒）：过小必然超时，过大阻塞入站链路
const MIN_GROUP_TRIGGER_TIMEOUT_MS: u64 = 200;
const MAX_GROUP_TRIGGER_TIMEOUT_MS: u64 = 5000;

struct MessageChannelProducer {
    hr_domain: Arc<dyn HrDomain>,
    message_domain: Arc<dyn MessageDomain>,
    /// 附件资产管理（渠道入站图片落库为内部 Attachment）
    finance_domain: Arc<dyn FinanceDomain>,
}

impl MessageChannelProducer {
    pub fn new() -> Self {
        Self {
            hr_domain: crate::service::domain::hr::domain(),
            message_domain: crate::service::domain::message::domain(),
            finance_domain: crate::service::domain::finance::domain(),
        }
    }
}

/// 渠道入站「附件 → 正文协议引用」转换（入库前解析转换，纯函数）
///
/// 站内发送由前端用同一个 `common::markdown_protocol` 拼装口完成，两条入口写进
/// 请求 / 落库的正文形态必须逐字节一致。纯图片入站（飞书 / 微信无文字）的
/// `[图片]` 占位在**确实有附件**时整体替代：用户手打「[图片]」且不带附件时
/// 必须原样保留，否则会把人家的话清空。
fn compose_inbound_content(base: &str, attachments: &[(String, String)]) -> String {
    let refs: Vec<String> = attachments
        .iter()
        .map(|(id, name)| {
            let name = name.trim();
            let name = if name.is_empty() { "附件" } else { name };
            common::markdown_protocol::format_ref(
                &common::markdown_protocol::ProtocolRef::attachment(id),
                name,
            )
        })
        .collect();
    let base =
        if !refs.is_empty() && base.trim() == common::constants::message::IMAGE_ONLY_PLACEHOLDER {
            ""
        } else {
            base
        };
    common::markdown_protocol::append_refs(base, &refs)
}

/// 渠道类型 → 渠道专属接待角色（档位链第 2 级）
fn reception_role_of(channel_type: ChannelType) -> &'static str {
    match channel_type {
        ChannelType::Lark => ROLE_FEISHU_RECEPTION,
        ChannelType::Wechat => ROLE_WECHAT_RECEPTION,
        _ => ROLE_RECEPTION,
    }
}

impl MessageChannelProducer {
    /// 入站消息的 Agent 路由档位链（有序，首个命中档位胜出，见设计文档 §4.3.4）：
    ///
    /// 1. 渠道显式绑定的 Agent（`by_id`，决定性命中）
    /// 2. 渠道专属接待角色（如 `feishu_reception` / `wechat_reception`）
    /// 3. 通用接待角色 `reception`（兜底，恒存在）
    async fn resolve_target_agent(
        &self,
        ctx: RequestContext,
        msg: &AdaptedMessage,
    ) -> Result<Option<String>> {
        let mut chain = Vec::new();
        if let Some(agent_id) = msg.to_agent_id.as_deref().filter(|s| !s.is_empty()) {
            chain.push(AgentMatchCriteria::by_id(agent_id));
        }
        chain.push(AgentMatchCriteria::by_role(reception_role_of(
            msg.channel_type,
        )));
        chain.push(AgentMatchCriteria::by_role(ROLE_RECEPTION));

        Ok(self
            .hr_domain
            .resolve_agent_multi(ctx, chain)
            .await?
            .map(|agent| agent.po.id))
    }

    /// 补组织上下文（渠道入站链路的必补项）
    ///
    /// 渠道入站的 ctx 只带 `caller_type` + `user_id`，没有组织绑定；而
    /// `send_to_agent` 直接取 `ctx.organization_id()` 落 `messages.organization_id`。
    /// 该列为 NULL 的行会被消息列表的 `AND organization_id = ?` 永久过滤掉
    /// （SQL 里 NULL 不等于任何值）→ 网页端**刷新也看不到**渠道入站消息。
    ///
    /// 组织维度的唯一源头是 UserPo（与唤醒侧 `handle_agent_message` 的兜底同口径）。
    async fn resolve_organization(&self, ctx: &RequestContext, user_id: &str) -> Option<String> {
        if user_id.is_empty() {
            return None;
        }
        match crate::service::domain::organization::domain()
            .user_manage()
            .get_user_by_id(ctx.clone(), user_id)
            .await
        {
            Ok(Some(user)) if !user.organization_id.is_empty() => Some(user.organization_id),
            Ok(_) => None,
            Err(e) => {
                log_warn!(
                    ctx,
                    "message_channel_producer",
                    "resolve organization failed: user_id={} err={}",
                    user_id,
                    e
                );
                None
            }
        }
    }
    /// 渠道入站统一过滤：协议转换完成后、进入内部消息系统前判定是否处理
    ///
    /// 规则与渠道无关：私聊恒放行；群聊读取渠道的 `group_trigger` 配置，
    /// 按 `mention` / `prefix` / `all` 判定。`all` 模式再经小脑结合群聊上下文
    /// 与 Agent 记忆判断是否值得回复。任何降级（无渠道 / 查询失败 / 小脑不可用）
    /// 均放行，避免静默吞消息。
    async fn should_process_inbound(
        &self,
        ctx: &RequestContext,
        msg: &AdaptedMessage,
        agent_id: &str,
    ) -> bool {
        if !msg.is_group {
            return true;
        }
        let Some(channel_id) = msg.channel_id.as_deref().filter(|id| !id.is_empty()) else {
            log_warn!(
                ctx,
                "message_channel_producer",
                "group inbound without channel id, pass through: from={} channel={:?}",
                msg.from_id,
                msg.channel_type
            );
            return true;
        };
        let channel = match self
            .finance_domain
            .message_channel_manage()
            .get_message_channel(ctx.clone(), channel_id)
            .await
        {
            Ok(Some(channel)) => channel,
            Ok(None) => {
                log_warn!(
                    ctx,
                    "message_channel_producer",
                    "group inbound channel not found, pass through: channel_id={}",
                    channel_id
                );
                return true;
            }
            Err(e) => {
                log_warn!(
                    ctx,
                    "message_channel_producer",
                    "load channel for inbound filter failed, pass through: channel_id={} err={}",
                    channel_id,
                    e
                );
                return true;
            }
        };

        let config = channel.config();
        let trigger = config.group_trigger.as_deref().unwrap_or("mention");
        let prefix = config.group_trigger_prefix.as_deref().unwrap_or("");
        let timeout_ms = config
            .group_trigger_timeout_ms
            .unwrap_or(DEFAULT_GROUP_TRIGGER_TIMEOUT_MS)
            .clamp(MIN_GROUP_TRIGGER_TIMEOUT_MS, MAX_GROUP_TRIGGER_TIMEOUT_MS);
        match inbound_filter::group_trigger_gate(
            msg.is_group,
            msg.addressed_to_agent,
            trigger,
            prefix,
            &msg.content,
        ) {
            InboundGateDecision::Process => true,
            InboundGateDecision::Skip => false,
            InboundGateDecision::Judge => {
                self.judge_inbound_relevance(ctx, msg, agent_id, timeout_ms)
                    .await
            }
        }
    }

    /// 用「群聊历史 + Agent 短期记忆 + 本次消息」请小脑判断是否值得回复
    async fn judge_inbound_relevance(
        &self,
        ctx: &RequestContext,
        msg: &AdaptedMessage,
        agent_id: &str,
        timeout_ms: u64,
    ) -> bool {
        // 预算跟随模型最佳上下文：容量取自默认小脑 provider 的
        // min(推荐上下文, 最大上下文)，未配置则不裁（不拍固定字符数）。
        let capacity = crate::service::domain::runtime::context_capacity(ctx).await;
        let context = self
            .build_inbound_context(ctx, msg, agent_id, capacity)
            .await;
        let probability = crate::service::domain::runtime::judge_relevance(
            ctx,
            INBOUND_RELEVANCE_INSTRUCTIONS,
            &context,
            timeout_ms,
        )
        .await;
        let pass = inbound_filter::passes_relevance(probability);
        log_info!(
            ctx,
            "message_channel_producer",
            "group inbound relevance judged: from={} channel={:?} probability={:?} pass={}",
            msg.from_id,
            msg.channel_type,
            probability,
            pass
        );
        pass
    }

    /// 组装小脑判定上下文（历史 / 记忆任一来源失败只丢对应片段）
    async fn build_inbound_context(
        &self,
        ctx: &RequestContext,
        msg: &AdaptedMessage,
        agent_id: &str,
        capacity_tokens: Option<u64>,
    ) -> String {
        let history = self.load_chat_history(ctx, msg, RECENT_MESSAGE_LIMIT).await;
        let (recent_memories, searched_memories) = self
            .load_agent_memories(ctx, msg, agent_id, RECENT_MEMORY_LIMIT, SEARCH_MEMORY_LIMIT)
            .await;
        inbound_filter::build_group_context(
            &recent_memories,
            &searched_memories,
            &history,
            &msg.content,
            &msg.from_id,
            capacity_tokens,
        )
    }

    /// 读取同会话最近消息（按时间正序返回）
    ///
    /// 按 `chat_id` 定位会话；消息带话题时叠加 `thread_id` 兼容过滤，
    /// 避免把不同话题的内容混在一起（无话题消息仍保留）。
    async fn load_chat_history(
        &self,
        ctx: &RequestContext,
        msg: &AdaptedMessage,
        limit: usize,
    ) -> Vec<(MessageRole, String)> {
        let Some(chat_id) = msg.external.chat_id.as_deref().filter(|id| !id.is_empty()) else {
            return Vec::new();
        };
        let query = MessageQuery {
            chat_id: Some(chat_id.to_string()),
            organization_id: ctx.organization_id().cloned(),
            thread_id_compatible: msg.external.thread_id.clone(),
            status_in: Some(vec![
                MessageStatus::Pending,
                MessageStatus::Processing,
                MessageStatus::Processed,
                MessageStatus::Failed,
            ]),
            order_by: Some("created_at DESC".to_string()),
            limit: Some(limit),
            ..Default::default()
        };
        match self
            .message_domain
            .management()
            .query(ctx.clone(), query)
            .await
        {
            Ok(mut messages) => {
                messages.reverse();
                messages
                    .into_iter()
                    .map(|message| (message.from_role(), message.content().to_string()))
                    .collect()
            }
            Err(e) => {
                log_warn!(
                    ctx,
                    "message_channel_producer",
                    "load chat history failed, continue without it: chat_id={} err={}",
                    chat_id,
                    e
                );
                Vec::new()
            }
        }
    }

    /// 读取 Agent 最近短期记忆 + 用消息正文检索到的相关短期记忆
    ///
    /// 未绑定 Agent 时返回空（上下文退化为群聊消息）；搜索结果按记忆 ID 去重。
    async fn load_agent_memories(
        &self,
        ctx: &RequestContext,
        msg: &AdaptedMessage,
        agent_id: &str,
        recent_limit: usize,
        search_limit: usize,
    ) -> (Vec<String>, Vec<String>) {
        if agent_id.trim().is_empty() {
            return (Vec::new(), Vec::new());
        }
        let runtime = crate::service::domain::runtime::domain();
        let recent = match runtime
            .memory()
            .get_recent_context(ctx.clone(), agent_id, recent_limit)
            .await
        {
            Ok(memories) => memories,
            Err(e) => {
                log_warn!(
                    ctx,
                    "message_channel_producer",
                    "load recent memory failed, continue without it: agent_id={} err={}",
                    agent_id,
                    e
                );
                Vec::new()
            }
        };
        let recent_pairs: Vec<(String, String)> =
            recent.iter().filter_map(short_term_memory).collect();
        let recent_ids: HashSet<String> = recent_pairs.iter().map(|(id, _)| id.clone()).collect();

        let searched = match runtime
            .memory()
            .search(
                ctx.clone(),
                MemorySearch {
                    keyword: Some(msg.content.trim().to_string()),
                    filters: MemoryQuery {
                        agent_id: Some(agent_id.to_string()),
                        memory_type: Some(MemoryType::ShortTerm),
                        limit: Some(search_limit),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await
        {
            Ok(memories) => memories,
            Err(e) => {
                log_warn!(
                    ctx,
                    "message_channel_producer",
                    "search memory failed, continue without it: agent_id={} err={}",
                    agent_id,
                    e
                );
                Vec::new()
            }
        };

        let recent_summaries = recent_pairs
            .into_iter()
            .map(|(_, summary)| summary)
            .collect();
        let searched_summaries = searched
            .iter()
            .filter_map(short_term_memory)
            .filter(|(id, _)| !recent_ids.contains(id))
            .map(|(_, summary)| summary)
            .collect();
        (recent_summaries, searched_summaries)
    }
}

/// 提取短期记忆的 `(id, summary)`；非短期记忆返回 `None`
fn short_term_memory(memory: &Memory) -> Option<(String, String)> {
    memory
        .short_term_summary()
        .map(|(id, summary)| (id.to_string(), summary.to_string()))
}

#[async_trait::async_trait]
impl MessageAdapterCallback for MessageChannelProducer {
    async fn on_message(&self, msg: AdaptedMessage) -> Result<()> {
        // 根据 from_role 设置 caller_type 和 user_id
        // 外部渠道消息一般为 User，但保留根据 from_role 推断的灵活性
        let mut builder = RequestContext::builder().caller_type(match msg.from_role {
            MessageRole::User => CallerType::User,
            MessageRole::Agent => CallerType::Agent,
            MessageRole::System => CallerType::System,
        });
        if msg.from_role == MessageRole::User {
            builder = builder.user_id(msg.from_id.clone());
        }
        let ctx = builder.build();

        // 组织上下文兜底：入站链路没有 HTTP 头注入，org 只能按归属用户反查。
        // 缺失会让消息落库成 organization_id = NULL，被列表查询永久过滤。
        let ctx = match ctx.organization_id() {
            Some(_) => ctx,
            None => match self.resolve_organization(&ctx, &msg.from_id).await {
                Some(org_id) => ctx.to_builder().organization_id(org_id).build(),
                None => {
                    log_warn!(
                        &ctx,
                        "message_channel_producer",
                        "channel inbound without organization context: from_user={} channel={:?}",
                        msg.from_id,
                        msg.channel_type
                    );
                    ctx
                }
            },
        };

        let Some(to_agent_id) = self.resolve_target_agent(ctx.clone(), &msg).await? else {
            log_warn!(
                &ctx,
                "message_channel_producer",
                "no available onboarded agent for routing from_user={} channel={:?}",
                msg.from_id,
                msg.channel_type
            );
            return Ok(());
        };

        // 统一入站过滤：协议转换已完成，进入内部消息系统前判定是否值得处理。
        // 所有渠道共用同一套规则；`all` 模式由小脑结合群聊上下文与记忆判断。
        if !self.should_process_inbound(&ctx, &msg, &to_agent_id).await {
            log_info!(
                &ctx,
                "message_channel_producer",
                "group inbound skipped by trigger policy: from={} channel={:?} chat_id={:?}",
                msg.from_id,
                msg.channel_type,
                msg.external.chat_id
            );
            return Ok(());
        }

        // 渠道附件落库：先转为内部 Attachment 资产，随后**在适配层**把 id 解析成
        // 正文协议引用再入库（单条失败只丢该附件并留痕，不阻断文本消息入站）。
        // 与微信 / 飞书「先解析转换，然后才入库」的口径一致：投递域拿到的 content
        // 已经是混排文本，不再负责拼装。
        let mut inbound_attachments: Vec<(String, String)> = Vec::new();
        for attachment in &msg.attachments {
            let upload = AttachmentUpload {
                original_name: attachment.file_name.clone(),
                mime_type: attachment.mime_type.clone(),
                purpose: "message".to_string(),
                bytes: attachment.bytes.clone(),
            };
            match self
                .finance_domain
                .attachment_manage()
                .create_attachment(ctx.clone(), upload)
                .await
            {
                Ok(created) => {
                    inbound_attachments.push((created.po.id, attachment.file_name.clone()));
                }
                Err(e) => log_warn!(
                    &ctx,
                    "message_channel_producer",
                    "create inbound attachment failed (dropped): from={} channel={:?} name={} err={}",
                    msg.from_id,
                    msg.channel_type,
                    attachment.file_name,
                    e
                ),
            }
        }
        let content = compose_inbound_content(&msg.content, &inbound_attachments);
        // 声明清单随请求传给投递域做归属校验（正文引用客户端可伪造），同时兼容
        // 「只声明、没写进正文」的老调用方兜底补引用。
        let attachment_ids: Vec<String> = inbound_attachments
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        let cmd = SendToAgentCommand {
            from_id: &msg.from_id,
            from_role: msg.from_role,
            to_agent_id: &to_agent_id,
            content: &content,
            project_id: msg.project_id.as_deref(),
            task_id: msg.task_id.as_deref(),
            reply_to_id: msg.reply_to_id.as_deref(),
            external: msg.external,
            attachment_ids: if attachment_ids.is_empty() {
                None
            } else {
                Some(attachment_ids.as_slice())
            },
            // Mixed 归一收敛在投递域（正文含资源引用即升级），这里不必重复判定
            message_type: MessageType::Text,
        };

        let message = match self
            .message_domain
            .delivery()
            .send_to_agent(ctx.clone(), cmd)
            .await
        {
            Ok(message) => message,
            Err(e) => {
                // 投递失败会由 AOP 重投：清理本次刚落库的附件，避免重试累积孤儿资产
                for attachment_id in &attachment_ids {
                    if let Err(clean_err) = self
                        .finance_domain
                        .attachment_manage()
                        .delete_attachment(ctx.clone(), attachment_id)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "message_channel_producer",
                            "cleanup inbound attachment failed: id={} err={}",
                            attachment_id,
                            clean_err
                        );
                    }
                }
                return Err(err!(
                    Internal,
                    "message channel producer send_to_agent failed from={} to_agent={}: {}",
                    msg.from_id,
                    to_agent_id,
                    e
                ));
            }
        };

        // 渠道入站消息实时上屏：网页端对它**没有本地乐观气泡**（只有网页自发的
        // 消息才有），不推就永远看不到。出口只开 SSE —— 走渠道投递会把这条消息
        // 回灌回它来的那个渠道，形成回声（用户收到自己刚发的消息）。
        let deliver_cmd = DeliverMessageCommand {
            message: &message,
            user_id: &msg.from_id,
            options: DeliveryOptions::sse_only(),
        };
        if let Err(e) = self
            .message_domain
            .delivery()
            .deliver_message(ctx.clone(), deliver_cmd)
            .await
        {
            // 消息已落库、唤醒已入队，推送失败只留痕不回滚（否则重投会重复唤醒）
            log_warn!(
                &ctx,
                "message_channel_producer",
                "inbound sse push failed: message_id={} err={}",
                message.id(),
                e
            );
        }

        log_info!(
            &ctx,
            "message_channel_producer",
            "message dispatched: from={} to_agent={} channel={:?}",
            msg.from_id,
            to_agent_id,
            msg.channel_type
        );
        Ok(())
    }
}

pub async fn init() -> Result<()> {
    let registry = crate::pkg::adapter::message::registry();

    if registry.is_empty() {
        sys_info!("no message channel adapters registered, skip init");
        return Ok(());
    }

    let producer = Arc::new(MessageChannelProducer::new());
    registry.start_all(producer).await?;

    sys_info!(
        "message channel producers started, total adapters: {}",
        registry.len()
    );
    Ok(())
}

pub async fn shutdown() -> Result<()> {
    let registry = crate::pkg::adapter::message::registry();
    registry.stop_all().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::compose_inbound_content;

    fn one_image() -> Vec<(String, String)> {
        vec![("att_1".to_string(), "wx-image.png".to_string())]
    }

    #[test]
    fn no_attachments_keeps_base_verbatim() {
        // 无附件时连 `[图片]` 占位都不能改写：它可能就是用户手打的字面量
        assert_eq!(compose_inbound_content("[图片]", &[]), "[图片]");
        assert_eq!(compose_inbound_content("你好", &[]), "你好");
    }

    #[test]
    fn image_only_placeholder_is_replaced_by_ref() {
        assert_eq!(
            compose_inbound_content("[图片]", &one_image()),
            "[wx-image.png](attachment:att_1)"
        );
        assert_eq!(
            compose_inbound_content("  ", &one_image()),
            "[wx-image.png](attachment:att_1)"
        );
    }

    #[test]
    fn text_with_attachments_appends_ordered_refs() {
        let atts = vec![
            ("att_1".to_string(), "a.png".to_string()),
            ("att_2".to_string(), "b.pdf".to_string()),
        ];
        assert_eq!(
            compose_inbound_content("看这两份", &atts),
            "看这两份\n\n[a.png](attachment:att_1) [b.pdf](attachment:att_2)"
        );
    }

    #[test]
    fn empty_file_name_falls_back() {
        let atts = vec![("att_1".to_string(), "  ".to_string())];
        assert_eq!(
            compose_inbound_content("", &atts),
            "[附件](attachment:att_1)"
        );
    }

    /// 拼装结果必须能被唤醒侧 `extract_refs` 收回，否则附件进不了模型当前轮。
    #[test]
    fn composed_refs_are_extractable_by_awakening_side() {
        let composed = compose_inbound_content("看图", &one_image());
        let refs = common::markdown_protocol::extract_refs(&composed);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0.scheme, "attachment");
        assert_eq!(refs[0].0.id, "att_1");
    }
}
