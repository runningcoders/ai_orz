use serde::{Deserialize, Serialize};

/// 思考场景类型
///
/// 用于区分唤醒（awaken）、沉睡（sleep_and_settle）、总结退出（summary）
/// 和意图识别（intent-analyze）四种场景，不同场景根据 tag 过滤可用工具和技能。
///
/// 放在 common 层，供 pkg / domain / handler / DTO 共用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingScene {
    /// 唤醒场景：加载业务工具，但**裁剪「管平台」而不是「干活」的管理面**
    ///
    /// 实测 Reception Agent 绑了 97 个工具，其中 tool_management 14、skill_management 14、
    /// ontology_management 11，近一半是运维/管理面。全量注入既吃 token、拉高首字延迟，
    /// 也会诱导模型在闲聊时去调管理工具。
    ///
    /// 规则：**所有** tag 都落在管理面时才排除（无 tag 的一律保留）。这样
    /// `["neural","skill_management"]` 这类常驻工具、`["collaboration","hr_specialist"]`
    /// 这类业务协作工具都不会被误伤。
    #[default]
    Awaken,
    /// 沉睡场景：沉淀记忆，加载记忆相关工具（neural/memory tag）+ 自进化工具
    /// （self_evolution tag：create_skill / update_skill）
    ///
    /// 放通自进化的理由：沉淀的产出不只知识图谱，还有「怎么做」的技能。
    /// 图谱存抽象知识与关系，技能存可复用的操作路径；二者在沉淀里一起产出，
    /// 并通过 `node_type=skill` 的知识节点 + `distilled_from` 边互相索引。
    ///
    /// ⚠️ **放通的是 self_evolution 而不是整个 skill_management**：后者含
    /// Remove Skill（无归属校验、不可撤销）、Uninstall Skill Pack 等运维/破坏性工具，
    /// 沉淀这种无人值守的内循环不该拿到它们。需要哪个写能力就给哪个打
    /// self_evolution tag，白名单不按"整个管理面"开放。
    ///
    /// ⚠️ 与 Compact 不同：Compact 只压缩出一条短期记忆，不放通技能写工具。
    Settle,
    /// 总结退出场景：思考轮次耗尽后总结当前工作，允许消息和任务管理工具
    /// （neural + memory + messaging + project_management tag）
    Summary,
    /// 上下文压缩场景：主循环上下文接近上限时，把本次对话压缩成一条短期记忆
    ///
    /// 与 Settle 的关键差异：
    /// - **不重新拼装 Prompt**：直接复用主循环已有的完整 `messages` 数组，
    ///   仅在尾部追加一条「压缩指令」伪 User 消息。前缀与上一次模型调用完全一致，
    ///   可命中 provider 侧 prefix caching，省去重新拼装 System/技能/通用上下文的开销。
    /// - **不操作 Agent 运行时状态**：既不 set_resting 也不 set_idle，
    ///   压缩发生在 awaken 主循环内部，Agent 始终保持 Busy。
    /// - **目标是「一条」短期记忆**，不是知识图谱沉淀：不查图谱、不建关系、不改记忆状态。
    ///
    /// 工具策略比 Settle 更窄（仅 neural/memory，不放通 skill_management）：
    /// 压缩的产出是「一条短期记忆」，不该在压缩途中创建或修改技能。
    /// 目的由提示词说明；`settle_memory` 的递归调用由 think loop 运行时拦截。
    Compact,
    /// 意图识别 + 上下文补充阶段
    ///
    /// 思考目标：只理解，不执行任何业务动作
    /// 工具约束：严格禁止执行类工具
    /// 最终输出：IntentAnalysis 结构化 JSON
    IntentAnalyze,
}

impl ThinkingScene {
    /// 转为场景字符串标识（用于 trace / event / prompt 等字符串场景）
    pub fn as_str(&self) -> &'static str {
        match self {
            ThinkingScene::Awaken => "awaken",
            ThinkingScene::Settle => "settle",
            ThinkingScene::Summary => "summary",
            ThinkingScene::Compact => "compact",
            ThinkingScene::IntentAnalyze => "intent-analyze",
        }
    }

    /// 判断工具是否在此场景可用
    ///
    /// - Awaken 场景：排除**纯管理面**工具（见 `is_management_only`），业务/常驻工具全放通
    /// - Settle 场景：tags 含 "neural" / "memory" / "self_evolution" 的工具可用
    ///   （自进化用于沉淀期产出技能，见 Settle 变体注释）
    /// - Compact 场景：只有 tags 含 "neural" 或 "memory" 的工具可用（比 Settle 更窄）
    /// - Summary 场景：允许 neural / memory / messaging / project_management
    /// - IntentAnalyze 场景：允许 tags 包含 neural/memory/query/search/analyze（理解类工具）
    ///
    /// ⚠️ 本函数同时被用于**技能过滤**（`legacy_settle/context.rs` 的 build_providers）：
    /// 给 Settle 加 tag 会连带把同 tag 的技能加载进沉淀 Prompt。当前库内
    /// self_evolution 的根技能只有 TEMPLATE_SELF_EVOLUTION（自带 neural），
    /// 即过滤前后无差异；若将来新增纯 self_evolution 的父技能，需重新评估
    /// 沉淀 Prompt 的体量。
    pub fn is_tool_allowed(&self, tags: &[String]) -> bool {
        match self {
            ThinkingScene::Awaken => !is_management_only(tags),
            ThinkingScene::Settle => tags
                .iter()
                .any(|t| t == "neural" || t == "memory" || t == "self_evolution"),
            ThinkingScene::Compact => tags.iter().any(|t| t == "neural" || t == "memory"),
            ThinkingScene::Summary => tags.iter().any(|t| {
                t == "neural" || t == "memory" || t == "messaging" || t == "project_management"
            }),
            ThinkingScene::IntentAnalyze => tags.iter().any(|t| {
                t.contains("neural")
                    || t.contains("memory")
                    || t.contains("query")
                    || t.contains("search")
                    || t.contains("analyze")
            }),
        }
    }
}

/// tag **全部**落在平台管理面（运维/配置类），用于 Awaken 场景裁剪
///
/// 无 tag 视为「没分类的业务工具」保留——误删业务工具的代价远大于多注入几个描述。
fn is_management_only(tags: &[String]) -> bool {
    !tags.is_empty()
        && tags.iter().all(|t| {
            matches!(
                t.as_str(),
                "admin"
                    | "tool_management"
                    | "skill_management"
                    | "agent_management"
                    | "hr_specialist"
                    | "ontology_management"
                    // `internal` 在当前库里只与 tool_management 同时出现（Send Tool Call
                    // Message / Request Tool Execution）。它表示「不直接面客」，作为
                    // 管理面共同标记参与判定；带任一业务 tag 的工具仍会保留。
                    | "internal"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::ThinkingScene;

    /// 沉淀场景必须能写技能：沉淀的产出既有知识图谱节点，也有「怎么做」的技能。
    ///
    /// 护栏意义：白名单一旦被误收窄回 neural/memory，沉淀 Prompt 里的技能沉淀步骤
    /// 会变成「有指令、无工具」，模型只能干看着，静默降级为只沉淀图谱。
    /// ⚠️ 放通面是 self_evolution（写能力最小集），**不是**整个 skill_management：
    /// 后者含 Remove Skill（无归属校验、不可撤销）等破坏性工具，沉淀这种无人值守
    /// 的内循环不该拿到。这条断言同时把「按整个管理面放通」这条退路堵死。
    #[test]
    fn settle_allows_self_evolution_but_not_plain_skill_management() {
        let scene = ThinkingScene::Settle;
        for tag in ["self_evolution", "neural", "memory"] {
            assert!(
                scene.is_tool_allowed(&[tag.to_string()]),
                "Settle 场景应放通 {tag}"
            );
        }
        assert!(
            !scene.is_tool_allowed(&["skill_management".to_string()]),
            "Settle 场景不应放通整个 skill_management 面（含删除/卸载类工具）"
        );
    }

    /// 压缩场景比沉淀更窄：压缩的产出是一条短期记忆，不该在压缩途中建/改技能。
    ///
    /// 护栏意义：Compact 与 Settle 曾共用同一 match 分支，给 Settle 加 tag 会连带
    /// 放通 Compact；两者必须保持可分别演进。
    #[test]
    fn compact_does_not_allow_self_evolution_tools() {
        let scene = ThinkingScene::Compact;
        assert!(
            !scene.is_tool_allowed(&["self_evolution".to_string()]),
            "Compact 场景不应放通 self_evolution"
        );
        for tag in ["neural", "memory"] {
            assert!(
                scene.is_tool_allowed(&[tag.to_string()]),
                "Compact 场景应放通 {tag}"
            );
        }
    }

    /// Awaken 只裁剪**纯管理面**，业务工具与常驻工具必须全留。
    ///
    /// 护栏意义：曾把 `agent_management` / `hr_specialist` 也写进「一律排除」，
    /// 会让协作类工具（同时带 collaboration/reception tag）被误伤；判据必须是
    /// 「全部 tag 都在管理面」而不是「命中任一管理 tag」。
    #[test]
    fn awaken_trims_management_plane_only() {
        let scene = ThinkingScene::Awaken;
        for tags in [
            vec!["tool_management".to_string()],
            vec!["skill_management".to_string()],
            vec!["ontology_management".to_string()],
            vec!["tool_management".to_string(), "internal".to_string()],
            vec!["admin".to_string(), "tool_management".to_string()],
        ] {
            assert!(!scene.is_tool_allowed(&tags), "应裁剪管理面: {tags:?}");
        }
        for tags in [
            vec![],
            vec!["neural".to_string()],
            vec!["neural".to_string(), "skill_management".to_string()],
            vec!["project_management".to_string()],
            vec![
                "collaboration".to_string(),
                "agent_management".to_string(),
                "hr_specialist".to_string(),
                "reception".to_string(),
            ],
        ] {
            assert!(scene.is_tool_allowed(&tags), "不应误伤: {tags:?}");
        }
    }
}
