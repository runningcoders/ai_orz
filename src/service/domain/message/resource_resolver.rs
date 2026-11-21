//! 消息资源引用预解析与出站降级（批2：Attachment/Artifact 组件 extract 实装）
//!
//! ## 分层（方案 §2.1 决策 A：common 协议层零 DAL 依赖）
//!
//! - **common 协议层**：同步纯函数——组件 `extract` / `render_text` 从预解析表取数；
//! - **domain resolver（本模块）**：async 批量预解析（DAL 逐条点查）→ 数据化组件
//!   实例（`with_resolved`）→ Registry 组装（「同 scheme 后注册者覆盖」批1 占位）；
//! - **delivery**：渠道分发前调 [`degrade_content_for_external`] 产出降级副本；
//!   SSE 站内保留协议原文（前端 chip 渲染，批1 语义边界随前端 render 接管消解）。
//!
//! ## 口径
//!
//! - 资源查询按 id 主键，`org` 段仅展示用途不参与匹配（Attachment / Artifact 均无
//!   org 维度，id 全局唯一）；
//! - 只收录 status 正常（=1）的资源；已删 / 未命中 → 组件快照兜底防悬空；
//! - JSON 结构化消息（ToolCall* / TaskAssignment）跳过 markdown 降级，防 JSON 重排。
//!
//! ## 批3/4 预留
//!
//! - [`build_resolved_registry`] 批3 混排主线复用（签名向后兼容承诺）；
//! - `AttachmentDal::read_file` bytes 通道：出站附件走 [`extract_outbound_attachments`]
//!   （图片 + 文档等，按 MIME 分派），vision 传参仍归批4。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use common::enums::FileType;
use common::markdown_protocol::{
    ArtifactComponent, AttachmentComponent, ProtocolRegistry, ResolvedPayload, TransformOptions,
    extract_refs, transform,
};
use common::markdown_render::{blocks_to_plain_text, parse_events};
use pulldown_cmark::{Event, Parser};

use crate::models::outbound_media::{OutboundAttachment, OutboundMediaKind};
use crate::pkg::RequestContext;
use crate::service::dal::artifact::ArtifactDal;
use crate::service::dal::attachment::AttachmentDal;

/// 产物摘要截断上限（≤64 字，方案 §3.2）
const ARTIFACT_SUMMARY_MAX_CHARS: usize = 64;

// ==================== 纯函数层（注入式单测，不依赖 sqlite） ====================

/// 从文本提取 attachment / artifact 引用 id 集合（其余 scheme 忽略）
pub fn collect_resource_ids(text: &str) -> (HashSet<String>, HashSet<String>) {
    let (mut att, mut art) = (HashSet::new(), HashSet::new());
    for (r, _) in extract_refs(text) {
        match r.scheme.as_str() {
            "attachment" => {
                att.insert(r.id);
            }
            "artifact" => {
                art.insert(r.id);
            }
            _ => {}
        }
    }
    (att, art)
}

/// content 是否含资源混排协议引用（Mixed=13 写入路径判定，投递域单一归一点）。
///
/// 仅认 attachment / artifact scheme（AMan 批3 决策点②：mention 族 agent/task/project/user
/// 系协作协议不算混排）；未注册 scheme（https 等）与歪格式不命中——[`extract_refs`]
/// 的 registry 校验链天然过滤。
pub fn content_has_resource_refs(text: &str) -> bool {
    extract_refs(text)
        .iter()
        .any(|(r, _)| r.scheme == "attachment" || r.scheme == "artifact")
}

/// 组装带预解析数据的 Registry：批1 default_registry + 数据化组件后注册覆盖
pub fn build_resolved_registry(
    attachments: HashMap<String, ResolvedPayload>,
    artifacts: HashMap<String, ResolvedPayload>,
) -> ProtocolRegistry {
    let mut reg = ProtocolRegistry::default_registry();
    reg.register(Arc::new(AttachmentComponent::with_resolved(attachments)));
    reg.register(Arc::new(ArtifactComponent::with_resolved(artifacts)));
    reg
}

/// 产物摘要：file_type 标签 · description（≤64 字截断）
fn artifact_summary(file_type: FileType, description: &str) -> Option<String> {
    let label = match file_type {
        FileType::Document => "文档",
        FileType::Image => "图片",
        FileType::Audio => "音频",
        FileType::Video => "视频",
        FileType::Binary => "文件",
    };
    let desc = description.trim();
    let desc = if desc.chars().count() > ARTIFACT_SUMMARY_MAX_CHARS {
        let truncated: String = desc.chars().take(ARTIFACT_SUMMARY_MAX_CHARS).collect();
        format!("{truncated}…")
    } else {
        desc.to_string()
    };
    if desc.is_empty() {
        Some(label.to_string())
    } else {
        Some(format!("{label} · {desc}"))
    }
}

/// 出站降级：协议引用 → `[附件：名]` / `[产物：名]`（未命中快照兜底防悬空）
///
/// - 无已注册协议引用：原文**零改动**返回（幂等；存量无协议消息零影响）；
/// - 有引用：transform 管线（后端口径 `demote_raw_html=false`）+ 组件
///   `render_text` 统一出口（已解析实名 / 未解析快照兜底由组件内决策）+
///   `common::markdown_render` 的事件流→IR→纯文本降级（前后端口径统一）。
pub fn degrade_content_for_external(content: &str, registry: &ProtocolRegistry) -> String {
    let has_registered_refs = extract_refs(content)
        .iter()
        .any(|(r, _)| registry.contains(&r.scheme));
    if !has_registered_refs {
        return content.to_string();
    }
    let parser = Parser::new(content);
    let opts = TransformOptions {
        demote_raw_html: false,
    };
    let events = transform(parser, registry, &opts, |r, snapshot, _| {
        // transform 只拦截已注册 scheme，get 必命中；防御性回退快照文本
        let text = match registry.get(&r.scheme) {
            Some(c) => c.render_text(r, snapshot),
            None => snapshot.to_string(),
        };
        Some(Event::Text(text.into()))
    });
    // 事件流 → 渠道无关 IR → 纯文本（降级逻辑统一在 common，前后端口径一致）
    blocks_to_plain_text(&parse_events(events))
}

// ==================== IO 薄壳层（DAL 查询，async） ====================

/// 预解析消息中的资源引用 → 载荷表（attachment 表, artifact 表）
///
/// 引用量为人工 / Agent 撰写场景的个位数，逐条 DAL 点查够用（IN 批量优化归批3，
/// 方案 §2.1 简单够用口径）。
pub async fn resolve_message_resources(
    ctx: &RequestContext,
    attachment_dal: &Arc<dyn AttachmentDal>,
    artifact_dal: &Arc<dyn ArtifactDal>,
    content: &str,
) -> (
    HashMap<String, ResolvedPayload>,
    HashMap<String, ResolvedPayload>,
) {
    let (att_ids, art_ids) = collect_resource_ids(content);

    let mut attachments = HashMap::new();
    for id in att_ids {
        // status == 1 = 启用态；软删资源不参与解析（let-chain 展平，避免嵌套 if）
        if let Ok(Some(a)) = attachment_dal.get_by_id(ctx.clone(), &id).await
            && a.po.status == 1
        {
            // 归一化后再拼摘要：历史泛型 mime 会让模型把图片误判成二进制附件。
            // 先算完借用值，再移动 `original_name`（字段移动后不能整体借用 a）。
            let mime = a.po.effective_mime_type();
            let size = a.po.size;
            attachments.insert(
                id,
                ResolvedPayload {
                    name: a.po.original_name,
                    summary: Some(format!("{mime} · {size}")),
                },
            );
        }
    }

    let mut artifacts = HashMap::new();
    for id in art_ids {
        if let Ok(Some(a)) = artifact_dal.find_by_id(ctx.clone(), &id).await
            && a.po.status == 1
        {
            artifacts.insert(
                id,
                ResolvedPayload {
                    name: a.po.name,
                    summary: artifact_summary(a.po.file_type, &a.po.description),
                },
            );
        }
    }
    (attachments, artifacts)
}

/// 按正文出现顺序收集 `attachment:` 引用 id（首次出现为准，重复忽略）。
///
/// 出站图片逐张独立发送，顺序必须稳定——`collect_resource_ids` 返回 `HashSet`
/// 只适合无顺序语义的解析表，这里单独提供有序版本。
fn collect_attachment_ids_in_order(content: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for (r, _) in extract_refs(content) {
        if r.scheme == "attachment" && seen.insert(r.id.clone()) {
            ids.push(r.id);
        }
    }
    ids
}

/// 出站大小判定（纯函数，锁死「先按声明值预拦、再按实测值复核」两段口径）
///
/// `actual=None` 表示尚未读取字节：只在声明值超限时返回跳过；读后传入实测值，
/// 以实测为准（meta 滞后 / 文件在两次检查之间变化都靠这一步兜底）。
/// 返回 `Some(size)` 表示应跳过，数值用于正文提示。
pub fn outbound_oversize(declared: usize, actual: Option<usize>, limit: usize) -> Option<usize> {
    if declared > limit {
        return Some(declared);
    }
    actual.filter(|size| *size > limit)
}

/// 因渠道上限或读取失败未随本条消息发送的附件
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkippedAttachmentReason {
    /// 超过渠道大小上限（资产本身没问題，站内可看）
    #[default]
    OverLimit,
    /// 读取字节失败（库里有记录但读不出来）
    ReadFailed,
}

/// 因渠道上限或读取失败未随本条消息发送的附件
///
/// 附件是站内资产，创建环节不设渠道上限；能不能随消息发出去是**推送时**的事。
/// 这里只记录「被跳过的是谁、多大、为什么」，由调用方拼进正文提示，
/// 保证信息不丢（渠道文本里仍能看到 `[附件：名]` 占位 + 这句说明）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedAttachment {
    /// 附件原名（与正文占位一致）
    pub name: String,
    /// 实际字节数（超限为实测值；读取失败为库里的声明值）
    pub size: usize,
    /// 该类媒体的渠道上限（字节）
    pub limit: usize,
    /// 跳过原因
    pub reason: SkippedAttachmentReason,
}

/// 出站附件批次：可发送的载荷 + 因超限被跳过的清单
#[derive(Debug, Default)]
pub struct OutboundAttachmentBatch {
    /// 随渠道 payload 透传的附件（按正文引用顺序）
    pub attachments: Vec<OutboundAttachment>,
    /// 超限被跳过的附件（按正文引用顺序）
    pub skipped: Vec<SkippedAttachment>,
}

/// 从正文 `attachment:` 引用提取可出站的附件字节（按引用出现顺序，去重）。
///
/// - 仅认 status==1 的附件；图片走图片通道（MIME 白名单），其余走文件通道；
///   artifact 引用不动；
/// - 查询失败 / 读取失败 → WARN 跳过该条（不算「超限」，不写进正文提示）；
/// - 超过该类媒体的渠道上限 → 记入 [`OutboundAttachmentBatch::skipped`]，由调用方
///   在正文里说明「未随本条消息发送」（不静默丢信息）；
/// - 无引用时零 IO，存量消息行为逐字节不变。
pub async fn extract_outbound_attachments(
    ctx: &RequestContext,
    attachment_dal: &Arc<dyn AttachmentDal>,
    content: &str,
    // 纯附件消息的正文即附件 id（file_meta 形态），不走 markdown 引用；
    // 由调用方按消息形态传入，避免把「消息结构」知识塞进文本解析器。
    direct_attachment_id: Option<&str>,
) -> OutboundAttachmentBatch {
    let mut batch = OutboundAttachmentBatch::default();
    let mut ids = collect_attachment_ids_in_order(content);
    if let Some(id) = direct_attachment_id
        && !ids.iter().any(|existing| existing == id)
    {
        ids.insert(0, id.to_string());
    }
    for id in ids {
        let Some(attachment) = attachment_dal
            .get_by_id(ctx.clone(), &id)
            .await
            .ok()
            .flatten()
            .filter(|a| a.po.status == 1)
        else {
            continue;
        };
        // 先算分类（借用值），再把 original_name 移进载荷
        let mime = attachment.po.effective_mime_type();
        let kind = OutboundMediaKind::from_mime_and_name(&mime, &attachment.po.original_name);
        let limit = kind.max_bytes();
        // 先用库里的 size 拦一次：超限就不必把字节读进内存（附件可达数百 MB）
        let declared = attachment.po.size.max(0) as usize;
        if declared > limit {
            log_warn!(
                ctx,
                "outbound_attachment",
                "附件超出站上限，跳过: id={} size={} limit={} kind={:?}",
                id,
                declared,
                limit,
                kind
            );
            batch.skipped.push(SkippedAttachment {
                name: attachment.po.original_name,
                size: declared,
                limit,
                reason: SkippedAttachmentReason::OverLimit,
            });
            continue;
        }
        let bytes = match attachment_dal.read_file(&attachment) {
            Ok(bytes) => bytes,
            Err(e) => {
                log_warn!(
                    ctx,
                    "outbound_attachment",
                    "读取附件失败，跳过出站: id={} err={}",
                    id,
                    e
                );
                // 读取失败也要正文提示：只用占位符会让对方以为附件丢了
                batch.skipped.push(SkippedAttachment {
                    name: attachment.po.original_name,
                    size: declared,
                    limit,
                    reason: SkippedAttachmentReason::ReadFailed,
                });
                continue;
            }
        };
        // meta 可能滞后于实际内容（历史数据 / 写入竞态），以真实字节为准
        if let Some(size) = outbound_oversize(declared, Some(bytes.len()), limit) {
            log_warn!(
                ctx,
                "outbound_attachment",
                "附件超出站上限，跳过: id={} size={} limit={} kind={:?}",
                id,
                size,
                limit,
                kind
            );
            batch.skipped.push(SkippedAttachment {
                name: attachment.po.original_name,
                size,
                limit,
                reason: SkippedAttachmentReason::OverLimit,
            });
            continue;
        }
        batch.attachments.push(OutboundAttachment {
            name: attachment.po.original_name,
            mime_type: mime,
            bytes,
        });
    }
    batch
}

/// 生成「附件未随本条消息发送」的正文提示（无跳过项时返回空串）
///
/// 渠道文本是纯文本语境（各渠道 markdown 支持程度不一），故用一句自然语言
/// 追加在末尾，不依赖任何标记语法；`degrade_content_for_external` 产出的
/// `[附件：名]` 占位仍在原位，这里只补「为什么没发」。
pub fn skipped_attachment_notice(skipped: &[SkippedAttachment]) -> String {
    if skipped.is_empty() {
        return String::new();
    }
    let over_limit: Vec<String> = skipped
        .iter()
        .filter(|s| s.reason == SkippedAttachmentReason::OverLimit)
        .map(|s| {
            format!(
                "「{}」{}（渠道上限 {}）",
                s.name,
                human_size(s.size),
                human_size(s.limit)
            )
        })
        .collect();
    let read_failed: Vec<String> = skipped
        .iter()
        .filter(|s| s.reason == SkippedAttachmentReason::ReadFailed)
        .map(|s| format!("「{}」", s.name))
        .collect();
    let mut reasons = Vec::new();
    if !over_limit.is_empty() {
        reasons.push(format!(
            "以下附件超过渠道大小上限，未随本条消息发送，可在站内查看：{}",
            over_limit.join("；")
        ));
    }
    if !read_failed.is_empty() {
        reasons.push(format!(
            "以下附件读取失败，未随本条消息发送，可在站内查看：{}",
            read_failed.join("；")
        ));
    }
    format!("\n\n（{}）", reasons.join("；"))
}

/// 人类可读的字节数（提示文案用）
fn human_size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}
#[cfg(test)]
#[path = "resource_resolver_tests.rs"]
mod tests;
