//! 决策契约（请求 → 答案）+ 本轮事实桶
//!
//! 策略引擎里「需要外部决策」的那一半：只描述数据形状，**不做 I/O、不感知协议**：
//! - [`DecisionRequest`] / [`QuestionSpec`]：请求侧契约
//! - [`DecisionValue`] / [`DecisionAnswers`]：答案侧契约
//! - [`Metrics`]：本轮事实桶；`decisions` 字段是答案的唯一写入口
//!
//! 消费方是上一层的 [`super::DecisionPolicy`]：它把「问什么 → 读答案 → 判命中」
//! 的骨架固定下来，场景策略只填 `scene` / `request` / `judge`。
//! 执行（真正的网络往返）统一在 DAL：`BrainDal::decide`。
//!
//! 分层约束：本模块零业务依赖，因此协议 DTO（`common::api::cerebellum`）
//! 与引擎契约（本模块）可以各自演化，映射只在 DAL 一处发生。

use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// 一次外部决策请求（场景约定输入的唯一出口）
///
/// 一个请求 = 一次网关调用（内部可含多题并行）。
/// 编排层把多个场景的请求合并成一个批次，避免每个场景各发一次网络请求。
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionRequest {
    /// 场景标识（计量与日志口径，同时用作 qid 命名空间前缀）
    pub scene: &'static str,
    /// 上下文（透传给决策模型；轻量，不塞全量候选）
    pub state: serde_json::Value,
    /// 题 id → 题型/指令/判据
    pub questions: BTreeMap<String, QuestionSpec>,
}

/// 决策题型（与外部协议三型对应，但引擎不引用协议 DTO）
#[derive(Debug, Clone, PartialEq)]
pub enum QuestionSpec {
    /// 多选一（对应 choice）：options 为「选项 id → 描述」
    ///
    /// ⚠️ 一次 Pick 的答案带**全部选项的概率**（[`DecisionValue::Pick::weights`]），
    /// 所以「N 个候选排序」也用它：把候选逐个放成选项，一次问完，
    /// 再按权重截断/阈值筛选。不要为每个候选各发一题。
    Pick {
        /// 判据说明（每批只写一次）
        instructions: String,
        /// 选项 id → 该选项的描述（模型据此比较）
        options: BTreeMap<String, String>,
    },
    /// 分级打分（对应 score）：levels 由低到高，2~10 级
    Grade {
        /// 判据说明
        instructions: String,
        /// 由低到高的等级描述（2~10 项）
        levels: Vec<String>,
    },
    /// 存在性概率（对应 noul）：0~1，**不带判据也不带 confidence**
    Presence {
        /// 判据说明
        instructions: String,
    },
}

/// 决策结论（引擎侧通用表示，不感知具体协议）
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionValue {
    /// 选中项 + 置信度 + 各选项概率（权重即「候选排序」的依据）
    Pick {
        /// 被选中的选项 id
        chosen: String,
        /// 选择置信度
        confidence: f32,
        /// 选项 id → 概率
        weights: BTreeMap<String, f32>,
    },
    /// 概率加权分数 + 置信度
    Grade {
        /// 加权分数
        score: f32,
        /// 打分置信度
        confidence: f32,
    },
    /// 0~1 概率（无置信度）
    Presence {
        /// 存在性概率
        value: f32,
    },
}

/// 单选结论（已过阈值过滤）
///
/// 与 [`DecisionValue::Pick`] 的区别：后者是**原始答案**（含全部选项概率），
/// 本类型是「业务真正要的结论」——置信度是否达标已判完，没达标就是 `None`。
#[derive(Debug, Clone, PartialEq)]
pub struct PickOne {
    /// 被选中的选项 id
    pub option: String,
    /// 选择置信度
    pub confidence: f32,
}

/// 分级结论（已过阈值过滤）
#[derive(Debug, Clone, PartialEq)]
pub struct Grade {
    /// 加权分数
    pub score: f32,
    /// 打分置信度
    pub confidence: f32,
}

/// 一批决策结论（题 id → 结论）
///
/// 缺失的题不会出现在表里："没问出来" 与 "判断为否" 必须可区分。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecisionAnswers {
    values: BTreeMap<String, DecisionValue>,
}
impl DecisionAnswers {
    /// 空答案集
    pub fn new() -> Self {
        Self::default()
    }

    /// 链式添加一题结论
    pub fn with(mut self, qid: impl Into<String>, value: DecisionValue) -> Self {
        self.values.insert(qid.into(), value);
        self
    }

    /// 读取某题结论（缺失 = None，调用方不得当作「否」）
    pub fn get(&self, qid: &str) -> Option<&DecisionValue> {
        self.values.get(qid)
    }

    /// 是否一题答案都没有
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// 答案题数
    pub fn len(&self) -> usize {
        self.values.len()
    }
}

/// 运行时算子集合（HashMap 封装，灵活传递）
///
/// 每轮构造，传给策略。调用方通过 `with` 放入事实，
/// 策略通过 `required_metrics` 声明依赖、从桶里取值。
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    data: HashMap<String, serde_json::Value>,
    /// 本轮可用的外部决策结论
    /// None = 没有决策可用（未声明请求 / 网关不可用 / 失败降级），
    /// 决策型策略据此走「未命中」而非「判断为否」
    decisions: Option<Arc<DecisionAnswers>>,
}

impl Metrics {
    /// 空算子集
    pub fn new() -> Self {
        Self::default()
    }

    /// 链式添加算子
    pub fn with(mut self, key: &str, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.data.insert(key.to_string(), v);
        }
        self
    }

    /// 读取 u64 算子
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.data.get(key).and_then(|v| v.as_u64())
    }

    /// 读取 bool 算子
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.data.get(key).and_then(|v| v.as_bool())
    }

    /// 读取 f64 算子
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.data.get(key).and_then(|v| v.as_f64())
    }

    /// 读取字符串算子（shell 拦截等文本类策略使用）
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.data.get(key).and_then(|v| v.as_str())
    }

    /// 读取字符串列表算子（如路径白名单）
    pub fn get_str_list(&self, key: &str) -> Vec<String> {
        self.data
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 读取任意可反序列化算子（场景自定义的复合结构）
    ///
    /// 决策场景的输入统一走 `Metrics`，复合输入（如候选列表）
    /// 由场景自己定义视图结构，用本方法取回；解析失败按「没有这个事实」处理。
    pub fn get_as<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.data
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// 读取原始 JSON 值（形态类透传 state 用，不做类型假设）
    pub fn get_value(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    /// 注入外部决策结论（编排层唯一写入口）
    ///
    /// 只有确实拿到结论时才调用；降级路径不调用，`decisions` 保持 None。
    pub fn with_decisions(mut self, answers: Arc<DecisionAnswers>) -> Self {
        self.decisions = Some(answers);
        self
    }

    /// 读取某题决策结论
    ///
    /// None = 没问过 / 问了但答案缺失 / 形态不符，一律视为「没有判断依据」。
    /// ⚠️ 调用方禁止 `unwrap_or(false)` 或 `unwrap_or_default()` 把它压成「否」。
    pub fn decision(&self, qid: &str) -> Option<&DecisionValue> {
        self.decisions.as_ref().and_then(|a| a.get(qid))
    }

    /// 本轮全部决策结论（None = 一次决策都没拿到）
    ///
    /// 供 `DecisionPolicy` 的默认骨架区分「没问」与「问了但没答」：
    /// 前者直接未命中，后者才交给场景判据。
    pub fn decisions(&self) -> Option<&DecisionAnswers> {
        self.decisions.as_deref()
    }

    /// 本轮是否带了任何决策结论
    pub fn has_decisions(&self) -> bool {
        self.decisions.is_some()
    }
}

#[cfg(test)]
mod tests;
