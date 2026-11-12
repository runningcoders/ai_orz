//! Cerebellum（小脑）System One 协议类型定义
//!
//! 本模块定义 cerebellum dao 与上层之间的数据契约（T1 报告 §3/§4）：
//! - `CerebellumQuestion` / `QuestionCriteria`: System One 类型化问题（noul/choice/score）
//! - `CerebellumAnswer`: 类型化决策输出（带概率）
//! - `CerebellumRequest` / `CerebellumResponse`: 请求/响应包络
//! - `CerebellumUsage` / `ThinkFastResult`: token 用量与快判断结果
//! - `DegradedReason`: 降级口径（跨 DAL / Domain 共享，前端可展示）——
//!   非线路格式，但同样需要跨层与跨端同一份词表，故一并放这里
//!
//! 📍 **为什么在 common**：这是**跨进程共享的协议线格式**（小脑服务商的
//! REST 请求/响应），与服务端实现无关。放 common 使前端可复用同一套结构
//! 渲染决策摘要与调试面板，避免前后端各自维护一份必然漂移的口径。
//! ⚠️ 与本文件区分：`pkg::policy` 的 `DecisionRequest` / `DecisionValue` 是
//! **进程内策略引擎抽象**，不是协议 DTO，**不要**搬到这里。
//!
//! 协议口径（官方公开文档，多来源交叉实证；未经真实端点实测，
//! 连通性 Spike 顺延至凭据到位后二期启动前集中纠偏）：
//! - `POST {base}/systemone`（`base` 自带版本前缀，如 `https://api.typesafe.ai/v1`），Bearer 鉴权
//! - 请求 = `{"model", "state", "questions": {qid: {"type","instructions","criteria"}}}`
//! - 响应 = `{"model", "answers": {qid: {...}}, "usage": {"input_tokens","output_tokens"}}`
//! - choice：criteria 为选项对象（≤255 项），输出 {type,choice,confidence,probabilities}
//! - score：criteria 为有序等级数组（2~255 级，建议 3~7 级），输出
//!   {type,score,probabilities,legend,confidence}；legend 为「等级序号 → 描述」对象
//! - noul：instructions + 可选 {true,false} 判据对象，输出 {type,noul}（0~1 概率，无 confidence）

use crate::error::{Result, err};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// choice 问题选项数上限（协议边界）
pub const MAX_CHOICE_OPTIONS: usize = 255;
/// score 问题等级数下限（协议边界）
pub const MIN_SCORE_LEVELS: usize = 2;
/// score 问题等级数上限（协议边界）
///
/// **官方口径是 2–255 级**（阿里百炼《决策模型 API》原文：「score：从低到高的
/// 等级描述数组（2–255 级，建议 3–7 级且每级可清晰区分）」）。旧值 10 是 jev
/// 早期公开口径的残留，接入百炼后已对齐到 255；平台自身不校验「建议 3–7 级」
/// 这条软建议（那是给用户的提示，不是协议边界）。
pub const MAX_SCORE_LEVELS: usize = 255;

/// score 题协议原值 → 0~1 单位刻度（**唯一量纲换算点**）
///
/// ⚠️ `score` 是**服务端已经判断完的连续值**（官方称「概率加权分数」，即各等级
/// 概率已由服务端合成一个数，可落在两级之间）。实测 11 级题返回 `score=6.95`，
/// 用响应里的 `probabilities` 手算期望 = 6.90（差 0.05 纯舍入）——
/// **客户端不要再加权一次**，否则等于把同一个判断算两遍。
///
/// 但这个值**不是 0~1，也不是 0~255**，而是 **0-based 等级序号**的加权期望：
///
/// | 等级数 | 同一输入返回的 score | 值域 |
/// |---|---|---|
/// | 3 | 1.27 | 0~2 |
/// | 11 | 6.95（语义等级）/ 3.98（裸数字等级） | 0~10 |
/// | 101 | 31.38 | 0~100 |
///
/// 量纲由**本轮等级数**决定，分母必须是 `level_count - 1`（最大序号），
/// 不能写死 255 —— 255 是**等级数上限**（[`MAX_SCORE_LEVELS`]），不是分数量纲。
/// 固定 /255 会让 11 级题的满分只换算出 3.9 分，业务上限也永远差 1/255。
///
/// 映射到任意业务上限 `upper` 只需再乘一次：`score_to_unit(..)? * upper`。
/// 例：255 级判断出 150 → 150/254 → `upper=100` 得 59.1；
/// 11 级判断出 6.95 → 6.95/10 → 得 69.5。**等级数怎么选、业务上限定多少，
/// 两边靠这一次线性缩放解耦**，不必人为凑档位。
///
/// 越界（服务端偶发 `probabilities` 之和 ≠ 1 导致 score 略超值域）夹紧到 0~1；
/// 等级数非法 / 非有限值返回 `None`，调用方必须当「没结论」，
/// 禁止 `unwrap_or_default()` 压成 0 分。
pub fn score_to_unit(score: f32, level_count: usize) -> Option<f32> {
    if !(MIN_SCORE_LEVELS..=MAX_SCORE_LEVELS).contains(&level_count) || !score.is_finite() {
        return None;
    }
    Some((score / (level_count - 1) as f32).clamp(0.0, 1.0))
}

/// System One 问题类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    /// 无选项判断：instructions（可选 true/false 判据），输出 0~1 概率
    Noul,
    /// 单选：从 ≤255 个选项中选出一项，输出带各选项概率
    Choice,
    /// 打分：2~255 级有序等级，输出等级序号的概率加权分数（见 [`score_to_unit`]）
    Score,
}

/// 问题评估准则（choice=选项对象 / score=有序等级数组 / noul=可选 {true,false} 判据对象）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QuestionCriteria {
    /// 对象形态：choice 为选项映射（键=选项标识，值=描述，≤255 项）；
    /// noul 为判据映射（键仅限 `true` / `false`）
    Choices(BTreeMap<String, String>),
    /// score：有序等级数组（从低到高），2~255 级（官方口径；建议 3~7 级）
    Levels(Vec<String>),
}

/// 单个类型化问题（System One 请求侧）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CerebellumQuestion {
    /// 题型（noul / choice / score），决定 criteria 的合法形态
    #[serde(rename = "type")]
    pub question_type: QuestionType,
    /// 问题指令（三类问题必有）
    pub instructions: String,
    /// choice=选项对象 / score=等级数组 / noul=可选 {true,false} 判据对象
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<QuestionCriteria>,
}

impl CerebellumQuestion {
    /// 请求前置校验（协议边界）：类型与 criteria 形态匹配 + 数量边界
    pub fn validate(&self) -> Result<()> {
        match self.question_type {
            QuestionType::Noul => match &self.criteria {
                // noul 判据可选：官方 NoulCriteria = {true?, false?}
                None => {}
                Some(QuestionCriteria::Choices(m)) => {
                    if m.is_empty() || m.keys().any(|k| k != "true" && k != "false") {
                        return Err(err!(
                            ConfigInvalid,
                            "noul criteria must be a non-empty object with only 'true' / 'false' keys"
                        ));
                    }
                }
                Some(QuestionCriteria::Levels(_)) => {
                    return Err(err!(
                        ConfigInvalid,
                        "noul criteria must be an object with 'true' / 'false', not an array"
                    ));
                }
            },
            QuestionType::Choice => match &self.criteria {
                Some(QuestionCriteria::Choices(m)) => {
                    if m.is_empty() || m.len() > MAX_CHOICE_OPTIONS {
                        return Err(err!(
                            ConfigInvalid,
                            "choice criteria must have 1..=255 options, got {}",
                            m.len()
                        ));
                    }
                }
                _ => {
                    return Err(err!(
                        ConfigInvalid,
                        "choice question requires object criteria"
                    ));
                }
            },
            QuestionType::Score => match &self.criteria {
                Some(QuestionCriteria::Levels(v)) => {
                    if v.len() < MIN_SCORE_LEVELS || v.len() > MAX_SCORE_LEVELS {
                        return Err(err!(
                            ConfigInvalid,
                            "score criteria must have 2..=255 levels, got {}",
                            v.len()
                        ));
                    }
                }
                _ => {
                    return Err(err!(
                        ConfigInvalid,
                        "score question requires array criteria"
                    ));
                }
            },
        }
        Ok(())
    }
}

/// 快判断请求包络（System One wire format）
#[derive(Debug, Clone, Serialize)]
pub struct CerebellumRequest {
    /// 模型标识（请求 model 取 provider.model_name，默认小脑不内建标识）
    pub model: String,
    /// 状态上下文：字符串 / JSON 对象 / 数组（透传给决策模型）
    pub state: Value,
    /// 问题集合（qid → 问题），服务端并行独立评估
    pub questions: BTreeMap<String, CerebellumQuestion>,
}

/// token 用量
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CerebellumUsage {
    /// 输入 token 数
    #[serde(default)]
    pub input_tokens: u64,
    /// 输出 token 数
    #[serde(default)]
    pub output_tokens: u64,
}

/// 类型化决策输出（System One 响应侧 answers 值）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CerebellumAnswer {
    /// choice：选中项 + 各选项概率
    Choice {
        /// 选中的选项标识（必须出现在请求 criteria 的键集内）
        choice: String,
        /// 该选择的置信度（0~1）
        confidence: f32,
        /// 各选项的概率分布（键 = 选项标识）
        probabilities: BTreeMap<String, f32>,
    },
    /// score：服务端已判断完的连续分数（可落在两级之间）
    Score {
        /// **0-based 等级序号**的概率加权期望，值域 `[0, levels.len()-1]`，
        /// 不是 0~1、也不是 0~255。换算口径见 [`score_to_unit`]。
        ///
        /// 加权已由服务端完成，客户端**不要**拿 `probabilities` 再算一遍。
        score: f32,
        /// 各等级的概率分布
        probabilities: BTreeMap<String, f32>,
        /// 等级标签（等级序号 → 描述），响应可选回显
        #[serde(default)]
        legend: BTreeMap<String, String>,
        /// 该评分的置信度（0~1）
        confidence: f32,
    },
    /// noul：0~1 概率（无 confidence）
    Noul {
        /// 0~1 概率（**无 confidence**：要带置信度的二值判断请用 choice）
        noul: f32,
    },
}

impl CerebellumAnswer {
    /// noul 概率（非 noul 答案返回 None）
    pub fn as_noul(&self) -> Option<f32> {
        match self {
            CerebellumAnswer::Noul { noul } => Some(*noul),
            _ => None,
        }
    }

    /// choice 结果（选项标识 + 置信度）
    pub fn as_choice(&self) -> Option<(&str, f32)> {
        match self {
            CerebellumAnswer::Choice {
                choice, confidence, ..
            } => Some((choice.as_str(), *confidence)),
            _ => None,
        }
    }

    /// score 分数
    pub fn as_score(&self) -> Option<f32> {
        match self {
            CerebellumAnswer::Score { score, .. } => Some(*score),
            _ => None,
        }
    }
}

/// 快判断响应包络（System One wire format）
///
/// usage 缺失时容错为零值（协议版本演进容忍）；answers 逐项解析失败
/// 由 client 层容错跳过，不拖垮整批结果。
#[derive(Debug, Clone, Deserialize)]
pub struct CerebellumResponse {
    /// 实际执行的模型标识（服务端回显，可缺省）
    #[serde(default)]
    pub model: String,
    /// qid → 类型化答案（单题解析失败由 client 层容错跳过）
    pub answers: BTreeMap<String, CerebellumAnswer>,
    /// token 用量（缺失时容错为零值）
    #[serde(default)]
    pub usage: CerebellumUsage,
}

/// 快判断结果：各问题独立答案 + token 用量
#[derive(Debug, Clone, Default)]
pub struct ThinkFastResult {
    /// qid → 类型化答案
    pub answers: BTreeMap<String, CerebellumAnswer>,
    /// 本次调用 token 用量
    pub usage: CerebellumUsage,
}
/// 小脑链路降级原因
///
/// 全链路共享：DAL 侧判定（provider / 调用失败 / 超时）与 Domain 侧判定
/// （业务开关 / 上下文 / 判据）产出不同变体。**「没配小脑」与「被开关关掉」
/// 必须是两个变体**——否则日志与前端无法区分配置缺失和人为关闭，排查时
/// 会把「有人关了开关」误读成「环境没配」。
///
/// ⚠️ 降级 ≠ 判断为否：任何变体都只能让决策策略走「未命中」，
/// 不允许压成「否」（与 `pkg::policy::Metrics::decision` 的约定同源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradedReason {
    /// 小脑 provider 未装配，或已装配但 api_key 为空（DAL 侧判定）
    NoProvider,
    /// 业务开关关闭（**已无生产方**：2026-11-09 小脑开关整体取消，
    /// 保留变体仅用于解释历史日志）
    Disabled,
    /// 上下文不足，无法构造请求（如没有用户消息）
    NoUserMessage,
    /// 调用失败：网络 / 协议 / 服务端错误
    CallError,
    /// 超时（超出 provider 配置的 timeout_ms，缺省 800ms）
    Timeout,
    /// 低置信：低于该场景声明的置信阈值
    LowConfidence,
    /// 答案形态不符合预期（题型不符 / 未知选项）
    UnsupportedAnswer,
    /// 本 run 的小脑调用配额已耗尽（上限 = resolved 大脑轮次）
    ///
    /// 唯一生产方是各决策点入口的 `CerebellumQuota::try_acquire`：超限即
    /// **静默走原逻辑**（与超时 / 失败同一降级语义）。
    BudgetExhausted,
}

impl DegradedReason {
    /// 稳定字符串口径（日志与前端展示用，禁止作为业务分支判断依据）
    pub fn as_str(&self) -> &'static str {
        match self {
            DegradedReason::NoProvider => "no_provider",
            DegradedReason::Disabled => "disabled",
            DegradedReason::NoUserMessage => "no_user_message",
            DegradedReason::CallError => "call_error",
            DegradedReason::Timeout => "timeout",
            DegradedReason::LowConfidence => "low_confidence",
            DegradedReason::UnsupportedAnswer => "unsupported_answer",
            DegradedReason::BudgetExhausted => "budget_exhausted",
        }
    }
}

#[cfg(test)]
#[path = "cerebellum_tests.rs"]
mod cerebellum_protocol_tests;
