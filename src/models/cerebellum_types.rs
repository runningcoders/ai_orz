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
mod cerebellum_protocol_tests {
    use super::*;

    /// 官方协议 fixture：请求序列化形态（state=对象，三类型问题并存）
    #[test]
    fn request_serialization_matches_official_fixture() {
        let mut choices = BTreeMap::new();
        choices.insert("route_cortex".to_string(), "交由大脑深度思考".to_string());
        choices.insert("answer_directly".to_string(), "小脑直接回答".to_string());
        let mut questions = BTreeMap::new();
        questions.insert(
            "q_route".to_string(),
            CerebellumQuestion {
                question_type: QuestionType::Choice,
                instructions: "判断该消息的路由去向".to_string(),
                criteria: Some(QuestionCriteria::Choices(choices)),
            },
        );
        questions.insert(
            "q_urgency".to_string(),
            CerebellumQuestion {
                question_type: QuestionType::Score,
                instructions: "为消息紧急程度打分".to_string(),
                criteria: Some(QuestionCriteria::Levels(vec![
                    "无关".to_string(),
                    "一般".to_string(),
                    "紧急".to_string(),
                ])),
            },
        );
        questions.insert(
            "q_trivial".to_string(),
            CerebellumQuestion {
                question_type: QuestionType::Noul,
                instructions: "该消息是否为寒暄闲聊".to_string(),
                criteria: None,
            },
        );
        let req = CerebellumRequest {
            model: "jev-latest".to_string(),
            state: serde_json::json!({"conversation_id": "c1", "last_user_message": "你好"}),
            questions,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["model"], "jev-latest");
        assert!(v["state"].is_object());
        let qs = v["questions"].as_object().unwrap();
        assert_eq!(qs.len(), 3);
        let q_route = &qs["q_route"];
        assert_eq!(q_route["type"], "choice");
        assert!(q_route["criteria"].is_object());
        let q_urgency = &qs["q_urgency"];
        assert_eq!(q_urgency["type"], "score");
        assert!(q_urgency["criteria"].is_array());
        assert_eq!(q_urgency["criteria"].as_array().unwrap().len(), 3);
        let q_trivial = &qs["q_trivial"];
        assert_eq!(q_trivial["type"], "noul");
        assert!(
            q_trivial.get("criteria").is_none(),
            "noul 不得序列化 criteria 字段"
        );
    }

    /// 官方协议 fixture：响应解析（choice/score/noul 三型答案 + usage）
    #[test]
    fn response_parse_matches_official_fixture() {
        let fixture = r#"{
            "model": "jev-1.13.0",
            "answers": {
                "q_route": {"type": "choice", "choice": "answer_directly", "confidence": 0.87, "probabilities": {"answer_directly": 0.87, "route_cortex": 0.13}},
                "q_urgency": {"type": "score", "score": 2.4, "probabilities": {"无关": 0.1, "一般": 0.8, "紧急": 0.1}, "legend": ["无关", "一般", "紧急"], "confidence": 0.8},
                "q_trivial": {"type": "noul", "noul": 0.92}
            },
            "usage": {"input_tokens": 128, "output_tokens": 16}
        }"#;
        let resp: CerebellumResponse = serde_json::from_str(fixture).unwrap();
        assert_eq!(resp.model, "jev-1.13.0");
        assert_eq!(resp.usage.input_tokens, 128);
        assert_eq!(resp.usage.output_tokens, 16);
        let (choice, conf) = resp.answers["q_route"].as_choice().unwrap();
        assert_eq!(choice, "answer_directly");
        assert!((conf - 0.87).abs() < 1e-6);
        assert!((resp.answers["q_urgency"].as_score().unwrap() - 2.4).abs() < 1e-6);
        assert!((resp.answers["q_trivial"].as_noul().unwrap() - 0.92).abs() < 1e-6);
    }

    /// 响应缺 usage 字段时容错为零值（协议版本演进容忍）
    #[test]
    fn response_without_usage_defaults_to_zero() {
        let fixture = r#"{"model": "jev-1.13.0", "answers": {"q": {"type": "noul", "noul": 0.5}}}"#;
        let resp: CerebellumResponse = serde_json::from_str(fixture).unwrap();
        assert_eq!(resp.usage.input_tokens, 0);
        assert_eq!(resp.usage.output_tokens, 0);
    }

    /// 边界校验：choice ≤255 / score 2~10 / noul 无 criteria
    #[test]
    fn question_validate_enforces_protocol_boundaries() {
        // 合法用例
        let mut choices = BTreeMap::new();
        choices.insert("a".to_string(), "A".to_string());
        let ok_choice = CerebellumQuestion {
            question_type: QuestionType::Choice,
            instructions: "i".into(),
            criteria: Some(QuestionCriteria::Choices(choices)),
        };
        assert!(ok_choice.validate().is_ok());
        let ok_score = CerebellumQuestion {
            question_type: QuestionType::Score,
            instructions: "i".into(),
            criteria: Some(QuestionCriteria::Levels(vec!["低".into(), "高".into()])),
        };
        assert!(ok_score.validate().is_ok());
        let ok_noul = CerebellumQuestion {
            question_type: QuestionType::Noul,
            instructions: "i".into(),
            criteria: None,
        };
        assert!(ok_noul.validate().is_ok());

        // choice 256 选项越界
        let mut too_many = BTreeMap::new();
        for i in 0..256 {
            too_many.insert(format!("o{i}"), "x".to_string());
        }
        let bad_choice = CerebellumQuestion {
            question_type: QuestionType::Choice,
            instructions: "i".into(),
            criteria: Some(QuestionCriteria::Choices(too_many)),
        };
        assert!(bad_choice.validate().is_err());

        // score 1 级 / 11 级越界
        let bad_score_low = CerebellumQuestion {
            question_type: QuestionType::Score,
            instructions: "i".into(),
            criteria: Some(QuestionCriteria::Levels(vec!["唯一".into()])),
        };
        assert!(bad_score_low.validate().is_err());
        let bad_score_high = CerebellumQuestion {
            question_type: QuestionType::Score,
            instructions: "i".into(),
            criteria: Some(QuestionCriteria::Levels(
                (0..11).map(|i| format!("L{i}")).collect(),
            )),
        };
        assert!(bad_score_high.validate().is_err());

        // noul 携带 criteria 非法
        let bad_noul = CerebellumQuestion {
            question_type: QuestionType::Noul,
            instructions: "i".into(),
            criteria: Some(QuestionCriteria::Levels(vec!["低".into(), "高".into()])),
        };
        assert!(bad_noul.validate().is_err());
    }
}
