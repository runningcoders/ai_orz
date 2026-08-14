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
            ThinkingScene::IntentAnalyze => "intent-analyze",
        }
    }

    /// 判断工具是否在此场景可用
    ///
    /// - Awaken 场景：全部可用
    /// - Settle 场景：只有 tags 含 "neural" 或 "memory" 的工具可用
    /// - Summary 场景：允许 neural / memory / messaging / project_management
    /// - IntentAnalyze 场景：允许 tags 包含 neural/memory/query/search/analyze（理解类工具）
    pub fn is_tool_allowed(&self, tags: &[String]) -> bool {
        match self {
            ThinkingScene::Awaken => true,
            ThinkingScene::Settle => tags.iter().any(|t| t == "neural" || t == "memory"),
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
