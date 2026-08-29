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
    /// 唤醒场景：响应外部消息，加载全部工具
    #[default]
    Awaken,
    /// 沉睡场景：沉淀记忆，只加载记忆相关工具（neural/memory tag）
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
    /// 工具策略与 Settle 一致（neural/memory 放行，不额外收窄），
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
    /// - Awaken 场景：全部可用
    /// - Settle 场景：只有 tags 含 "neural" 或 "memory" 的工具可用
    /// - Compact 场景：与 Settle 相同（不额外收窄，目的由提示词说明）
    /// - Summary 场景：允许 neural / memory / messaging / project_management
    /// - IntentAnalyze 场景：允许 tags 包含 neural/memory/query/search/analyze（理解类工具）
    pub fn is_tool_allowed(&self, tags: &[String]) -> bool {
        match self {
            ThinkingScene::Awaken => true,
            ThinkingScene::Settle | ThinkingScene::Compact => {
                tags.iter().any(|t| t == "neural" || t == "memory")
            }
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
