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
    /// 唤醒场景：**绑定即权限**，绑定到该 Agent 的工具全量注入，不做 Prompt 层裁剪
    ///
    /// 早期曾按 tag 裁掉「管理面」工具以降 token，但工具的执行授权本身来自
    /// 绑定（`agent_tools` + `installed_tags`）。在 Prompt 层再裁一遍会造成
    /// 「已授权却看不见」的信息差：招聘官拿不到 create_agent、前台看到技能管理
    /// 指令却调不到对应工具，模型只能干看着或编造。
    ///
    /// 因此这里一律放通。控制 Prompt 体积应在上游做——精细化工具包 tag、
    /// 按角色推荐绑定，而不是在运行时把已装好的工具藏起来。
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

    /// 思考强度场景偏移量（分数，在 Agent / Provider 基准分数上做 ±N 分调整）
    ///
    /// 未特别配置的场景返回 0（走基准值）。偏移由调用方叠加到基准分数后
    /// 再饱和钳制（见 `ThinkingIntensity::offset`），本方法不关心基准值本身。
    ///
    /// - Awaken：0（主循环需要完整推理能力，保持基准）
    /// - IntentAnalyze：-20（只做意图理解，降低思考深度更快更省）
    /// - Settle：-20（沉淀以归纳为主，降低思考深度）
    /// - Summary：0（总结质量直接影响用户可见结果，保持基准）
    /// - Compact：0（压缩要保持信息完整，保持基准）
    pub fn thinking_intensity_offset(&self) -> i32 {
        match self {
            ThinkingScene::Awaken => 0,
            ThinkingScene::IntentAnalyze => -20,
            ThinkingScene::Settle => -20,
            ThinkingScene::Summary => 0,
            ThinkingScene::Compact => 0,
        }
    }

    /// 判断工具是否在此场景可用
    ///
    /// - Awaken 场景：**全量放通**（绑定即权限，见 `Awaken` 变体注释）
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
            ThinkingScene::Awaken => true,
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

    /// Awaken 场景**绑定即权限**：无论 tag 是什么（管理面 / 业务面 / 空 tag）都放通。
    ///
    /// 护栏意义：曾在 Prompt 层裁掉「纯管理面」工具，结果招聘官拿不到
    /// create_agent / 技能与工具管理工具，本职工作做不了；前台的技能管理指令
    /// 也变成「有指令、无工具」。工具的执行授权本来就来自绑定，再在 Prompt 层
    /// 藏一遍只是人为制造信息差。控制 Prompt 体积靠上游精细化工具包 tag，
    /// 不靠运行时裁剪。这条断言把「重新加回裁剪」的退路堵死。
    #[test]
    fn awaken_allows_all_bound_tools() {
        let scene = ThinkingScene::Awaken;
        for tags in [
            vec![],
            vec!["neural".to_string()],
            vec!["tool_management".to_string()],
            vec!["skill_management".to_string()],
            vec!["ontology_management".to_string()],
            vec!["tool_management".to_string(), "internal".to_string()],
            vec![
                "collaboration".to_string(),
                "agent_management".to_string(),
                "hr_specialist".to_string(),
            ],
        ] {
            assert!(scene.is_tool_allowed(&tags), "Awaken 应全量放通: {tags:?}");
        }
    }

    /// 场景偏移口径：轻量理解/沉淀降 20 分，其余保持基准（未配置 = 0）
    #[test]
    fn thinking_intensity_offset_matches_scene_intent() {
        assert_eq!(ThinkingScene::Awaken.thinking_intensity_offset(), 0);
        assert_eq!(
            ThinkingScene::IntentAnalyze.thinking_intensity_offset(),
            -20
        );
        assert_eq!(ThinkingScene::Settle.thinking_intensity_offset(), -20);
        assert_eq!(ThinkingScene::Summary.thinking_intensity_offset(), 0);
        assert_eq!(ThinkingScene::Compact.thinking_intensity_offset(), 0);
    }
}
