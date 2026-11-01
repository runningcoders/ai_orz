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
//! - `POST {base}/v1/systemone`，Bearer 鉴权
//! - 请求 = `{"model", "state", "questions": {qid: {"type","instructions","criteria"}}}`
//! - 响应 = `{"model", "answers": {qid: {...}}, "usage": {"input_tokens","output_tokens"}}`
//! - choice：criteria 为选项对象（≤255 项），输出 {type,choice,confidence,probabilities}
//! - score：criteria 为有序等级数组（2~10 级），输出 {type,score,probabilities,legend,confidence}
//! - noul：仅 instructions，输出 {type,noul}（0~1 概率，无 confidence）

use crate::error::{Result, err};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// choice 问题选项数上限（协议边界）
pub const MAX_CHOICE_OPTIONS: usize = 255;
/// score 问题等级数下限（协议边界）
pub const MIN_SCORE_LEVELS: usize = 2;
/// score 问题等级数上限（协议边界）
pub const MAX_SCORE_LEVELS: usize = 10;

/// System One 问题类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    /// 无选项判断：仅 instructions，输出 0~1 概率
    Noul,
    /// 单选：从 ≤255 个选项中选出一项，输出带各选项概率
    Choice,
    /// 打分：2~10 级有序等级，输出概率加权分数
    Score,
}

/// 问题评估准则（choice=选项对象 / score=有序等级数组；noul 不携带）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QuestionCriteria {
    /// choice：选项对象（键=选项标识，值=选项描述），≤255 项
    Choices(BTreeMap<String, String>),
    /// score：有序等级数组（从低到高），2~10 级
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
    /// choice=选项对象 / score=等级数组；noul 不携带
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<QuestionCriteria>,
}

impl CerebellumQuestion {
    /// 请求前置校验（协议边界）：类型与 criteria 形态匹配 + 数量边界
    pub fn validate(&self) -> Result<()> {
        match self.question_type {
            QuestionType::Noul => {
                if self.criteria.is_some() {
                    return Err(err!(ConfigInvalid, "noul question must not carry criteria"));
                }
            }
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
                            "score criteria must have 2..=10 levels, got {}",
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
    /// score：概率加权分数（可落在两级之间）
    Score {
        /// 概率加权分数（可落在两级之间，不一定是整数）
        score: f32,
        /// 各等级的概率分布
        probabilities: BTreeMap<String, f32>,
        /// 等级标签（低→高），响应可选回显
        #[serde(default)]
        legend: Vec<String>,
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
    /// 业务开关关闭（Domain 侧判定，如 `enable_cerebellum_route = false`）
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
        }
    }
}

#[cfg(test)]
#[path = "cerebellum_tests.rs"]
mod cerebellum_protocol_tests;
