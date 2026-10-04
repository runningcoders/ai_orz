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
//! - `AttachmentDal::read_file` bytes 通道归批4 vision 传参（本批不预留代码）。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use common::enums::FileType;
use common::markdown_protocol::{
    ArtifactComponent, AttachmentComponent, ProtocolRegistry, ResolvedPayload, TransformOptions,
    extract_refs, transform,
};
use pulldown_cmark::{Event, Parser, TagEnd};

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

/// content 是否含资源混排协议引用（批3：Mixed=13 写入路径判定，handler 单点调用）。
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

/// 事件流 → 纯文本（出站降级形态）
///
/// Text/Code 保留、软硬换行还原为换行、段落 / 标题 / 列表项 / 表格行等结构边界补
/// 换行；结构标记本身（#、-、表格线）不还原——外部渠道为纯文本阅读场景，降级形态
/// 可读即可（方案 R5：不追求完美还原，不做裁剪）。
fn events_to_plain_text(events: Vec<Event<'_>>) -> String {
    let mut out = String::new();
    for ev in events {
        match ev {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::BlockQuote(_)
                | TagEnd::Table
                | TagEnd::TableRow,
            ) => out.push('\n'),
            Event::End(TagEnd::TableCell) => out.push(' '),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// 出站降级：协议引用 → `[附件：名]` / `[产物：名]`（未命中快照兜底防悬空）
///
/// - 无已注册协议引用：原文**零改动**返回（幂等；存量无协议消息零影响）；
/// - 有引用：transform 管线（后端口径 `demote_raw_html=false`）+ 组件
///   `render_text` 统一出口（已解析实名 / 未解析快照兜底由组件内决策）+
///   事件流纯文本还原。
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
    events_to_plain_text(events)
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
        if let Ok(Some(a)) = attachment_dal.get_by_id(ctx.clone(), &id).await {
            if a.po.status == 1 {
                attachments.insert(
                    id,
                    ResolvedPayload {
                        name: a.po.original_name,
                        summary: Some(format!("{} · {}", a.po.mime_type, a.po.size)),
                    },
                );
            }
        }
    }

    let mut artifacts = HashMap::new();
    for id in art_ids {
        if let Ok(Some(a)) = artifact_dal.find_by_id(ctx.clone(), &id).await {
            if a.po.status == 1 {
                artifacts.insert(
                    id,
                    ResolvedPayload {
                        name: a.po.name,
                        summary: artifact_summary(a.po.file_type, &a.po.description),
                    },
                );
            }
        }
    }
    (attachments, artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_ids_scopes_to_resource_schemes() {
        let text = "看 [@张伟](agent:agt_1) 的 [设计稿](attachment:att_1) 与 [报告](artifact:art_2)，\
                    未注册 [x](user:u1) 与普通 [链接](https://e.com) 忽略";
        let (att, art) = collect_resource_ids(text);
        assert_eq!(att, HashSet::from(["att_1".to_string()]));
        assert_eq!(art, HashSet::from(["art_2".to_string()]));
    }

    #[test]
    fn degrade_passthrough_without_registered_refs() {
        // 幂等：无已注册协议引用 → 原文零改动
        let reg = ProtocolRegistry::default_registry();
        let src = "# 标题\n\n普通段落，含 [链接](https://e.com) 与 [x](user:u1)。\n";
        assert_eq!(degrade_content_for_external(src, &reg), src);
    }

    #[test]
    fn degrade_replaces_resolved_and_falls_back_on_miss() {
        let mut att = HashMap::new();
        att.insert(
            "att_1".to_string(),
            ResolvedPayload {
                name: "设计稿.png".to_string(),
                summary: Some("image/png · 1".to_string()),
            },
        );
        let reg = build_resolved_registry(att, HashMap::new());
        let out = degrade_content_for_external(
            "请查收 [设计稿](attachment:att_1)，另有 [丢失](attachment:att_x)。",
            &reg,
        );
        assert!(out.contains("[附件：设计稿.png]"), "命中实名：{out}");
        assert!(out.contains("[附件：丢失]"), "未命中快照兜底：{out}");
        assert!(!out.contains("attachment:"));
    }

    #[test]
    fn degrade_resolves_batch1_semantic_boundary_nested_emphasis() {
        // 批1 登记语义边界：结构化内层（加粗）链接——批2 组件 render 接管后消解
        let reg = ProtocolRegistry::default_registry();
        let out = degrade_content_for_external("[**张伟**](attachment:att_1) 看看", &reg);
        assert!(out.contains("[附件：张伟]"), "消解成立：{out}");
        assert!(!out.contains("**"));
    }

    #[test]
    fn degrade_mention_renders_via_component() {
        let reg = ProtocolRegistry::default_registry();
        let out = degrade_content_for_external("[@张伟](agent:agt_1) 进度如何", &reg);
        assert!(out.contains("@张伟"));
        assert!(!out.contains("agent:"));
    }

    #[test]
    fn artifact_summary_truncates_long_description() {
        let long = "很".repeat(100);
        let s = artifact_summary(FileType::Document, &long).unwrap();
        assert!(s.starts_with("文档 · "));
        assert!(s.ends_with('…'));
        assert!(s.chars().count() < 100, "截断生效：{s}");

        // 空 description：仅文件类型标签
        assert_eq!(artifact_summary(FileType::Image, "  ").unwrap(), "图片");
    }

    #[test]
    fn has_resource_refs_true_when_resource_ref_present() {
        assert!(content_has_resource_refs("看 [设计稿](attachment:att_1)"));
        assert!(content_has_resource_refs("报告见 [报告](artifact:art_2)"));
        assert!(content_has_resource_refs(
            "混排 [@张伟](agent:agt_1) + [附件](attachment:a1)"
        ));
    }

    #[test]
    fn has_resource_refs_false_for_mention_and_plain_text() {
        assert!(!content_has_resource_refs(
            "[@张伟](agent:agt_1) 请跟进 [@任务](task:t_9)"
        ));
        assert!(!content_has_resource_refs("普通文本，无任何协议引用"));
    }

    #[test]
    fn has_resource_refs_false_for_unregistered_or_malformed() {
        assert!(!content_has_resource_refs("普通 [链接](https://e.com)"));
        assert!(!content_has_resource_refs("[x](user:u1) 未注册不算混排"));
        assert!(!content_has_resource_refs(
            "歪格式 [x](attachment) 缺 id 段"
        ));
    }
}
