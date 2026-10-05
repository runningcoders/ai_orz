//! Cerebellum（小脑）System One 协议类型定义
//!
//! 本模块定义 cerebellum dao 与上层之间的数据契约（T1 报告 §3/§4）：
//! - `CerebellumQuestion` / `QuestionCriteria`: System One 类型化问题（noul/choice/score）
//! - `CerebellumAnswer`: 类型化决策输出（带概率）
//! - `CerebellumRequest` / `CerebellumResponse`: 请求/响应包络
//! - `CerebellumUsage` / `ThinkFastResult`: token 用量与快判断结果
//!
//! 协议口径（官方公开文档，多来源交叉实证；未经真实端点实测，
//! 连通性 Spike 顺延至凭据到位后二期启动前集中纠偏）：
//! - `POST {base}/v1/systemone`，Bearer 鉴权
//! - 请求 = `{"model", "state", "questions": {qid: {"type","instructions","criteria"}}}`
//! - 响应 = `{"model", "answers": {qid: {...}}, "usage": {"input_tokens","output_tokens"}}`
//! - choice：criteria 为选项对象（≤255 项），输出 {type,choice,confidence,probabilities}
//! - score：criteria 为有序等级数组（2~10 级），输出 {type,score,probabilities,legend,confidence}
//! - noul：仅 instructions，输出 {type,noul}（0~1 概率，无 confidence）

use common::error::{Result, err};
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

/// 类型化决策输出（System One 响应侧 answers 值）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CerebellumAnswer {
    /// choice：选中项 + 各选项概率
    Choice {
        choice: String,
        confidence: f32,
        probabilities: BTreeMap<String, f32>,
    },
    /// score：概率加权分数（可落在两级之间）
    Score {
        score: f32,
        probabilities: BTreeMap<String, f32>,
        #[serde(default)]
        legend: Vec<String>,
        confidence: f32,
    },
    /// noul：0~1 概率（无 confidence）
    Noul { noul: f32 },
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
    #[serde(default)]
    pub model: String,
    pub answers: BTreeMap<String, CerebellumAnswer>,
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
#[cfg(test)]
#[path = "cerebellum_types_tests.rs"]
mod cerebellum_protocol_tests;
