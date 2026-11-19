//! 意图分析结果（跨层共享的纯数据）
//!
//! 由 `RuntimeAwakening::analyze_input_intent()` 产出，需要被三个方向消费：
//! - 大脑 prompt 组装（`dal/agent/builder`）渲染【输入理解结果】区块；
//! - 外部入站适配器（飞书 / WS / HTTP 回调）路由消息前的预分析；
//! - 运行时图本身：`direct_answer` 非空时从 `intent` 直连 `consolidate`。
//!
//! ## 为什么在 models 而不是 runtime
//!
//! 它没有任何运行时行为依赖（既不含 DAL，也不含图上下文），但 `PromptBuilder`
//! 的签名要吃它 —— `PromptBuilder` 在 `models`、实现在 `dal`。放 runtime 会让
//! `models → domain`、`dal → domain` 两条反向边成立。这里是它唯一合法的位置：
//! models 在依赖链最底端，谁都能用。

/// 结构化意图分析结果
///
/// 说明：除了 confidence 用 f32，其余均为自由文本/数组，不做强枚举约束，
/// 避免未来新意图类型导致编译期改动；解析失败时降级为 Default::default()
/// 空结构，保证不阻塞主流程。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct IntentAnalysis {
    /// 主意图类型（推荐取值：Question / TaskRequest / Confirm /
    /// FollowUp / ClarificationResponse / Chat / Mixed）
    /// Agent 自主判断，不做强枚举
    pub intent_type: String,
    /// 意图置信度 0.0~1.0（Agent 自己打分）
    #[serde(default)]
    pub confidence: f32,
    /// 关键词/关键实体抽取（直接可复用于 search_memory 的 query）
    #[serde(default)]
    pub key_terms: Vec<String>,
    /// 指代消歧结果（自由文本数组，每条 Agent 写清楚"X → Y"）
    /// 例如：["\"上次那个方案\" → project=proj_123, task=task_456"]
    #[serde(default)]
    pub resolutions: Vec<String>,
    /// 检索补充上下文摘要（search_memory/recommend_seed_nodes 结果）
    #[serde(default)]
    pub retrieved_context: Vec<String>,
    /// 需要进一步追问澄清的问题（空列表 = 理解充分）
    #[serde(default)]
    pub need_clarification: Vec<String>,
    /// 简单问题直答：给用户的最终答复（空串 = 必须走完整唤醒链路）
    ///
    /// 意图阶段判定「无需检索 / 工具 / 澄清即可作答」时填写。非空即视为直答，
    /// 图会从 `intent` 直接进 `consolidate`；`summary` 仍只放理解结论，不混入答复。
    #[serde(default)]
    pub direct_answer: String,
    /// 一句话总结：Agent 最终确认自己理解用户想要什么
    #[serde(default)]
    pub summary: String,
}

impl IntentAnalysis {
    /// 是否在意图阶段就给出了最终答复（非空即直答）
    pub fn has_direct_answer(&self) -> bool {
        !self.direct_answer.trim().is_empty()
    }

    /// 意图识别产出的「后续阶段思考强度动态偏移」（分）
    ///
    /// 目标：让「这一步要花多少思考」由意图本身决定，而不是所有会话都用同一个分数。
    ///
    /// 规则（确定性启发式，先覆盖能明确判断的三种情形，其余保持 0）：
    /// - **低置信（< 0.5）** → `+20`：连意图都没把握，后续更需要深度推理，
    ///   提高预算避免「没想明白就草率作答」
    /// - **任务 / 代码 / 计划类** → `+10`：多步执行、易出错，适度提强度
    /// - **简单且高置信（≥ 0.8）** → `-10`：闲聊/问答/确认，降强度更快更省
    /// - **其余** → `0`
    ///
    /// 结果与场景固定偏移一起构成 `strength_fallback`（由
    /// `ThinkingIntensity::offset` 饱和钳制到 [0, 100]）：作为「场景重要度先验」
    /// 的数值部分写进题面/state，**只在小脑无结论时**才硬兑现；判断有结论时
    /// 完全以它为准。因此这里不需要自带边界；但总幅度刻意压在 ±20 内 ——
    /// 动态偏移是「微调」，不该盖过用户在 Provider/Agent 上的显式配置。
    pub fn thinking_intensity_offset(&self) -> i32 {
        if self.confidence < 0.5 {
            return 20;
        }

        let ty = self.intent_type.trim().to_ascii_lowercase();
        let is_task_like = [
            "task",
            "task_request",
            "code",
            "coding",
            "plan",
            "planning",
            "mixed",
            "任务",
            "代码",
            "编程",
            "计划",
            "混合",
        ]
        .iter()
        .any(|k| ty.contains(k));
        if is_task_like {
            return 10;
        }

        let is_simple_like = [
            "chat",
            "question",
            "followup",
            "follow_up",
            "confirm",
            "闲聊",
            "聊天",
            "问答",
            "确认",
            "追问",
        ]
        .iter()
        .any(|k| ty.contains(k));
        if self.confidence >= 0.8 && is_simple_like {
            return -10;
        }

        0
    }
}

#[cfg(test)]
mod tests {
    use super::IntentAnalysis;

    fn intent(intent_type: &str, confidence: f32) -> IntentAnalysis {
        IntentAnalysis {
            intent_type: intent_type.to_string(),
            confidence,
            ..Default::default()
        }
    }

    /// 低置信优先：连意图都没把握时一律提强度（即使类型看着像闲聊）
    #[test]
    fn low_confidence_raises_intensity() {
        assert_eq!(intent("Chat", 0.3).thinking_intensity_offset(), 20);
        assert_eq!(intent("", 0.0).thinking_intensity_offset(), 20);
    }

    /// 任务/代码/计划类：多步执行，适度提强度
    #[test]
    fn task_like_raises_intensity() {
        for ty in [
            "TaskRequest",
            "Coding",
            "Plan",
            "Mixed",
            "任务",
            "代码",
            "编程",
        ] {
            assert_eq!(
                intent(ty, 0.9).thinking_intensity_offset(),
                10,
                "intent_type={ty} 应命中任务类"
            );
        }
    }

    /// 简单且高置信：降强度更快更省
    #[test]
    fn simple_high_confidence_lowers_intensity() {
        for ty in [
            "Chat", "Question", "FollowUp", "Confirm", "闲聊", "问答", "确认",
        ] {
            assert_eq!(
                intent(ty, 0.9).thinking_intensity_offset(),
                -10,
                "intent_type={ty} 应命中简单类"
            );
        }
    }

    /// 简单类但置信不足，或不认识的类型：保持基准
    #[test]
    fn ambiguous_or_unknown_intent_keeps_base() {
        assert_eq!(intent("Chat", 0.6).thinking_intensity_offset(), 0);
        assert_eq!(intent("Unknown", 0.9).thinking_intensity_offset(), 0);
        assert_eq!(intent("", 0.9).thinking_intensity_offset(), 0);
    }
}
