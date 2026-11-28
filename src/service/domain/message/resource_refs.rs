//! final 正文的资源引用查漏补缺（拼装侧）
//!
//! 背景：System prompt §6 只以「软约束」要求 Agent 把创建出来的附件 / 产物写成
//! `[名](attachment:id)` / `[名](artifact:id)` 引用。但模型可能漏写，导致本轮辛苦
//! 生成的资源没进 final 正文、对方拿不到。本模块提供**工程兜底**的最后一环：
//! 把未被正文引用的资源按标准格式补到末尾（已引用的跳过）。
//!
//! ## 职责边界
//!
//! 本模块只做「拼进正文」这一件事（`common::markdown_protocol` 的判重与追加）。
//! 「本轮产出了什么」由执行层判定：`runtime::collected::collect_produced` 解析
//! 工具返回，容器是 `runtime::RoundCollected`；两侧共用的值对象在
//! [`crate::models::produced_resource`]（不落在任一域，避免域间调用）。
//!
//! ```text
//! dispatch 节点 ──collect_produced──▶ GraphRuntime.record_resources ──▶ AwakeningResult.collected
//!                                                                          │
//!                                       consumer：append_missing_resource_refs ◀──┘
//! ```
//!
//! 与 `delivery.rs::send_to_agent` 里基于 `attachment_ids` 声明清单的兜底是同一思路
//! （`extract_refs` 判重 + `format_ref`/`append_refs` 拼装），区别只在清单来源：
//! 那边是调用方显式声明，这边是本轮工具轨迹自动收集。
//!
//! ⚠️ 边界：不碰入站引用、不臆造 id。补引用发生在路由判定之后（NO_REPLY 全等判定
//! 不受影响）、落库之前（站内正文与外部渠道附件提取都读到补过的版本）。

use crate::models::produced_resource::ProducedResource;

/// 拼成标准 Markdown 引用 `[名](scheme:id)`，空名兜底避免空链接。
fn resource_ref(p: &ProducedResource) -> String {
    let safe_name = p.name.trim();
    let safe_name = if safe_name.is_empty() {
        if p.scheme == "artifact" {
            "产物"
        } else {
            "附件"
        }
    } else {
        safe_name
    };
    let r = common::markdown_protocol::ProtocolRef {
        scheme: p.scheme.to_string(),
        id: p.id.clone(),
        org: None,
    };
    common::markdown_protocol::format_ref(&r, safe_name)
}

/// 把本轮产出中**尚未在正文出现**的资源补到 final 末尾。
///
/// - 判重口径与 §6 标准格式一致：`extract_refs` 只认 `[名](scheme:id)`，按
///   `(scheme, id)` 去重；正文已有的（无论展示名）一律跳过、不重排；
/// - 同一资源被本轮多次产出时只补一次（保留首次出现顺序）；
/// - 全部已引用或产出为空时原样返回（零分配级别的最小改动）。
///
/// `content` 应传**路由判定之后**的 raw_output：NO_REPLY 等哨兵在此前已判完，
/// 本函数不影响投递决策，只补足正文事实。
pub fn append_missing_resource_refs(content: &str, produced: &[ProducedResource]) -> String {
    if produced.is_empty() {
        return content.to_string();
    }
    let mut seen: std::collections::HashSet<(String, String)> =
        common::markdown_protocol::extract_refs(content)
            .into_iter()
            .map(|(r, _)| (r.scheme, r.id))
            .collect();
    let mut missing = Vec::new();
    for p in produced {
        if !seen.insert((p.scheme.to_string(), p.id.clone())) {
            continue;
        }
        missing.push(resource_ref(p));
    }
    common::markdown_protocol::append_refs(content, &missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(id: &str, name: &str) -> ProducedResource {
        ProducedResource {
            scheme: "attachment",
            id: id.into(),
            name: name.into(),
        }
    }

    fn artifact(id: &str, name: &str) -> ProducedResource {
        ProducedResource {
            scheme: "artifact",
            id: id.into(),
            name: name.into(),
        }
    }

    #[test]
    fn backfill_appends_only_missing_refs() {
        let produced = vec![attachment("att_1", "图.png"), artifact("art_1", "报告")];
        // 正文已有 att_1（用了不同展示名）→ 跳过；art_1 缺失 → 补上
        let content = "结果见 [图](attachment:att_1)。";
        let out = append_missing_resource_refs(content, &produced);
        assert!(out.starts_with(content));
        assert_eq!(out.matches("attachment:att_1").count(), 1, "已引用不重复补");
        assert!(out.contains("[报告](artifact:art_1)"));
    }

    #[test]
    fn backfill_dedupes_repeated_production_and_keeps_order() {
        let produced = vec![
            artifact("art_1", "报告"),
            artifact("art_1", "报告"),
            attachment("att_1", "图.png"),
        ];
        let out = append_missing_resource_refs("done", &produced);
        assert_eq!(out.matches("artifact:art_1").count(), 1, "重复产出只补一次");
        let art_pos = out.find("artifact:art_1").unwrap();
        let att_pos = out.find("attachment:att_1").unwrap();
        assert!(art_pos < att_pos, "保留首次出现顺序");
    }

    #[test]
    fn backfill_empty_output_when_all_referenced() {
        let produced = vec![attachment("att_1", "图.png")];
        let content = "见 [随便起名](attachment:att_1)";
        assert_eq!(append_missing_resource_refs(content, &produced), content);
    }

    #[test]
    fn backfill_noop_for_empty_produced() {
        assert_eq!(append_missing_resource_refs("hello", &[]), "hello");
    }

    #[test]
    fn blank_name_falls_back_to_scheme_default() {
        let produced = vec![artifact("art_1", "  ")];
        let out = append_missing_resource_refs("done", &produced);
        assert!(out.contains("[产物](artifact:art_1)"), "空名兜底避免空链接");
    }
}
