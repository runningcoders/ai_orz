//! 决策形态：把「输入输出确定」的几类决策做成可直接复用的策略实现
//!
//! 策略层集中两件事：**骨架**（[`DecisionPolicy`]）与**形态**（本模块）。
//! 业务只决定「用哪个形态 + 填什么语义 + 结论怎么用」，不再重复写
//! 选项拼装、阈值判定、答案缺失处理。
//!
//! | 形态 | 题型 | 业务提供 | 结论 |
//! |---|---|---|---|
//! | [`RankPolicy`] | `Pick`（全选项权重） | 候选 [`Candidate`] | 有序保留集 `Vec<String>` |
//! | [`PickOnePolicy`] | `Pick` | 固定选项 + 上下文 | [`PickOne`]（选中项 + 置信度） |
//! | [`ScorePolicy`] | `Grade` | 等级表 + 业务上限 | [`Grade`]（缩放后的分数 + 置信度） |
//! | [`PresencePolicy`] | `Presence` | 上下文 | 0~1 概率 |
//!
//! 这四种覆盖了协议全部三种题型的「正常用法」：
//! - **排序**与**多选**是同一个形态（排序后按阈值截断就是多选，见 [`RankPolicy::ranked`]）；
//! - **单选**是「候选只有一个胜出」的排序特例，但答案的消费方式不同（要置信度门槛，不要全序）；
//! - **分级**用于「强度/质量」这类连续判断：服务端已把等级概率加权成一个分数，
//!   客户端只按等级数归一 + 乘业务上限（`ScorePolicy::scaled`），**不重复加权**；
//! - **概率**用于「是不是/要不要」这类二值判断。
//!
//! 需要额外后处理（把 id 映射回实体、按结论选文案）时，读 `Metrics` 里的答案即可；
//! 需要非同寻常的问法（多题、动态题型）时，自己实现 [`DecisionPolicy`]。

use super::decision::{
    DecisionAnswers, DecisionRequest, DecisionValue, Grade, Metrics, PickOne, QuestionSpec,
};
use super::{DecisionPolicy, Policy};
use common::api::cerebellum::{MAX_SCORE_LEVELS, MIN_SCORE_LEVELS, score_to_unit};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// 排序场景的问题 key（本轮要回答什么）
pub const KEY_QUESTION: &str = "question";

/// 排序场景的候选 key（`Vec<Candidate>`）
pub const KEY_CANDIDATES: &str = "candidates";

/// 排序候选：业务只需给「id + 判据文本」
///
/// 判据文本为空 = 不可判，`RankPolicy` 不会把它放进选项（避免占着坑位却无从比较）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// 业务侧标识（会原样回到保留集里）
    pub id: String,
    /// 进判据的文本（名称 + 摘录，别塞全文）
    pub text: String,
}

impl Candidate {
    /// 构造一个候选
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
        }
    }
}

/// 把 `Metrics` 里若干事实拼成请求的 state（形态类的通用做法）
fn collect_state(metrics: &Metrics, keys: &[&'static str]) -> Value {
    let mut map = serde_json::Map::new();
    for key in keys {
        if let Some(value) = metrics.get_value(key) {
            map.insert((*key).to_string(), value.clone());
        }
    }
    Value::Object(map)
}

/// 形态类的 `Policy` 实现是纯样板：描述 + 两行骨架转发
///
/// 形态类没有业务身份，全部由构造参数给出，所以可以一次生成。
macro_rules! impl_policy_for_form {
    ($ty:ty, $required:expr) => {
        impl Policy for $ty {
            fn id(&self) -> &str {
                self.id
            }
            fn name(&self) -> &str {
                self.name
            }
            fn condition_desc(&self) -> &str {
                self.condition_desc
            }
            fn required_metrics(&self) -> Vec<String> {
                $required
            }
            fn prepare(&self, metrics: &Metrics) -> Vec<DecisionRequest> {
                self.decision_prepare(metrics)
            }
            fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
                self.decision_evaluate(metrics)
            }
        }
    };
}

/// 排序形态：一次 `Pick` 拿到全部候选的相关性全序，再按阈值 + 上限截断
///
/// 与「每条候选各问一次」的区别：一次请求只写一份判据，token 不随候选数线性涨；
/// 而且模型看到的是**整批**候选，判断的是相对相关性——这才是排序，
/// 逐条独立打分是分类。
pub struct RankPolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    instructions: &'static str,
    threshold: f32,
    keep: usize,
}

impl RankPolicy {
    /// 构造排序形态
    ///
    /// - `instructions`：判据（放调用方，服务端不持有）
    /// - `threshold`：权重低于此值视为不相关
    /// - `keep`：保留上限
    pub fn new(
        id: &'static str,
        name: &'static str,
        condition_desc: &'static str,
        instructions: &'static str,
        threshold: f32,
        keep: usize,
    ) -> Self {
        Self {
            id,
            name,
            condition_desc,
            instructions,
            threshold,
            keep,
        }
    }

    /// 保留集（按相关性降序，已过阈值与上限）
    ///
    /// `None` = 答案不可用，调用方**必须**退回老路径，不得当成「全部不相关」。
    pub fn ranked(&self, metrics: &Metrics) -> Option<Vec<String>> {
        let candidates: Vec<Candidate> = metrics.get_as(KEY_CANDIDATES)?;
        let options: Vec<String> = candidates.into_iter().map(|c| c.id).collect();
        self.pick_ranked(metrics, &options, self.threshold, self.keep)
    }
}

impl_policy_for_form!(
    RankPolicy,
    vec![KEY_QUESTION.to_string(), KEY_CANDIDATES.to_string()]
);

impl DecisionPolicy for RankPolicy {
    fn scene(&self) -> &'static str {
        self.id
    }

    fn request(&self, metrics: &Metrics) -> Option<DecisionRequest> {
        let question = metrics.get_str(KEY_QUESTION).unwrap_or_default();
        let candidates: Vec<Candidate> = metrics.get_as(KEY_CANDIDATES)?;
        let options: BTreeMap<String, String> = candidates
            .into_iter()
            .filter(|c| !c.text.is_empty())
            .map(|c| (c.id, c.text))
            .collect();
        // 少于两个候选没有排序可言
        if options.len() < 2 {
            return None;
        }
        let candidate_count = options.len();
        Some(DecisionRequest {
            scene: self.id,
            state: json!({ KEY_QUESTION: question, "candidate_count": candidate_count }),
            questions: BTreeMap::from([(
                self.primary_qid(),
                QuestionSpec::Pick {
                    instructions: self.instructions.to_string(),
                    options,
                },
            )]),
            timeout_ms: None,
        })
    }

    fn judge(&self, _metrics: &Metrics, answers: &DecisionAnswers) -> Vec<String> {
        // 答案可用（Pick + 非空权重）才算命中本策略
        let usable = matches!(
            answers.get(&self.primary_qid()),
            Some(DecisionValue::Pick { weights, .. }) if !weights.is_empty()
        );
        if usable {
            vec![self.id.to_string()]
        } else {
            Vec::new()
        }
    }
}

/// 单选形态：从固定选项里选一个，置信度达标才算有结论
///
/// 典型用法是「分类 / 闸门 / 路由」：选项由业务固定，模型只负责选。
pub struct PickOnePolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    instructions: &'static str,
    options: BTreeMap<String, String>,
    state_keys: Vec<&'static str>,
    min_confidence: f32,
    /// 选项级置信门槛（覆盖 `min_confidence`）
    ///
    /// 收口的代价天然不对称：同一个场景里，某些选项的决定只是「多跑一跳」，
    /// 另一些则会改变下游形态。用一条全局门槛只能取交集（要么误伤低代价选项，
    /// 要么放过高代价选项），故允许按选项单列。
    option_min_confidence: BTreeMap<String, f32>,
    /// 场景级超时覆盖（毫秒）；`None` = 用 provider 配置 / 全局缺省
    timeout_ms: Option<u64>,
}

impl PickOnePolicy {
    /// 构造单选形态
    ///
    /// - `options`：选项 id → 判据描述（只写判断语义，不写实现细节）
    /// - `min_confidence`：低于此置信度视为「没结论」
    pub fn new(
        id: &'static str,
        name: &'static str,
        condition_desc: &'static str,
        instructions: &'static str,
        options: impl IntoIterator<Item = (&'static str, &'static str)>,
        min_confidence: f32,
    ) -> Self {
        Self {
            id,
            name,
            condition_desc,
            instructions,
            options: options
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            state_keys: Vec::new(),
            min_confidence,
            option_min_confidence: BTreeMap::new(),
            timeout_ms: None,
        }
    }

    /// 为单个选项声明置信门槛（不调用则用构造时的全局 `min_confidence`）
    pub fn with_option_min_confidence(mut self, option: &str, min_confidence: f32) -> Self {
        self.option_min_confidence
            .insert(option.to_string(), min_confidence);
        self
    }

    /// 某选项的置信门槛；`None` = 不在本轮选项集内（模型编造的选项）
    ///
    /// 与 [`Self::chosen`] 的分工：那个用一条全局门槛给「有没有结论」做总闸；
    /// 本方法把门槛判断交给调用方，让同一场景的不同选项各自设档。
    pub fn option_threshold(&self, option: &str) -> Option<f32> {
        self.options.contains_key(option).then(|| {
            self.option_min_confidence
                .get(option)
                .copied()
                .unwrap_or(self.min_confidence)
        })
    }

    /// 声明哪些事实要作为 `state` 透传给模型（按给定顺序）
    pub fn with_state_keys(mut self, keys: &[&'static str]) -> Self {
        self.state_keys = keys.to_vec();
        self
    }

    /// 声明本场景的请求超时预算（毫秒），覆盖 provider 配置与全局缺省
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// 选中项（置信度达标，且确实在本轮选项集内）
    ///
    /// 模型可能编造选项：不在选项集内视为「没有结论」，不返回幻觉值。
    /// 需要自己处理未知选项（例如把它记成协议错误）时用 `pick_one`。
    pub fn chosen(&self, metrics: &Metrics) -> Option<PickOne> {
        let picked = self.pick_one(metrics, self.min_confidence)?;
        self.options.contains_key(&picked.option).then_some(picked)
    }
}

impl_policy_for_form!(PickOnePolicy, Vec::new());

impl DecisionPolicy for PickOnePolicy {
    fn scene(&self) -> &'static str {
        self.id
    }

    fn request(&self, metrics: &Metrics) -> Option<DecisionRequest> {
        if self.options.is_empty() {
            return None;
        }
        Some(DecisionRequest {
            scene: self.id,
            state: collect_state(metrics, &self.state_keys),
            questions: BTreeMap::from([(
                self.primary_qid(),
                QuestionSpec::Pick {
                    instructions: self.instructions.to_string(),
                    options: self.options.clone(),
                },
            )]),
            timeout_ms: self.timeout_ms,
        })
    }

    fn judge(&self, metrics: &Metrics, _answers: &DecisionAnswers) -> Vec<String> {
        if self.chosen(metrics).is_some() {
            vec![self.id.to_string()]
        } else {
            Vec::new()
        }
    }
}

/// 分级形态：把「强度 / 质量」判成一个分数，按业务上限缩放后判命中
///
/// ⚠️ 协议 `score` 题的返回值**已经是服务端判断好的连续值**（各等级概率由
/// 服务端加权合成），客户端不再加权。它的量纲是「0-based 等级序号」，
/// 值域 `[0, levels.len()-1]` 随本轮等级数变化，所以读侧只做两步：
/// ① [`score_to_unit`] 归一到 0~1；② 乘业务上限 `upper`。
///
/// 于是**等级数怎么选**（2~255，协议上限）与**业务上限定多少**（0~100 /
/// 0~10 / 0~1）完全解耦：同一道 11 级题既能读成 100 分制，也能读成 0~1，
/// 不必为了凑业务上限去人为切档，输出也不再被档位离散化。
pub struct ScorePolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    instructions: &'static str,
    levels: Vec<String>,
    state_keys: Vec<&'static str>,
    upper: f32,
    min_score: f32,
}

impl ScorePolicy {
    /// 构造分级形态
    ///
    /// - `levels`：由低到高的等级描述，数量在协议边界
    ///   [`MIN_SCORE_LEVELS`]~[`MAX_SCORE_LEVELS`]（2~255）内
    /// - `min_score`：**业务刻度**下的命中阈值（配合 [`Self::with_upper`] 理解）
    pub fn new(
        id: &'static str,
        name: &'static str,
        condition_desc: &'static str,
        instructions: &'static str,
        levels: impl IntoIterator<Item = &'static str>,
        min_score: f32,
    ) -> Self {
        Self {
            id,
            name,
            condition_desc,
            instructions,
            levels: levels.into_iter().map(str::to_string).collect(),
            state_keys: Vec::new(),
            upper: 1.0,
            min_score,
        }
    }

    /// 声明业务上限：读到的分数落在 `[0, upper]`
    ///
    /// 例：11 级题、`upper=100`，服务端判断出 6.95 → `6.95 / 10 × 100 = 69.5`。
    /// 底层等级数保持原样，不需要为了 100 分制去凑 101 个档位。
    pub fn with_upper(mut self, upper: f32) -> Self {
        self.upper = upper;
        self
    }

    /// 声明哪些事实要作为 `state` 透传给模型
    pub fn with_state_keys(mut self, keys: &[&'static str]) -> Self {
        self.state_keys = keys.to_vec();
        self
    }

    /// 协议原值（未缩放，`[0, levels.len()-1]` 刻度）；生产请用 [`Self::scaled`]
    pub fn score(&self, metrics: &Metrics) -> Option<Grade> {
        self.grade(metrics, 0.0)
    }

    /// 映射到 `[0, upper]` 的分数（**生产读法**）
    ///
    /// `confidence` 原样透传、只作观测：连续量的置信度不是「有没有结论」的
    /// 门禁（11 档 choice 题硬套单档阈值曾导致强度决策 79 次全量降级）。
    ///
    /// `None` = 没答 / 形态不符 / 量纲非法 / upper 非法，一律当「没结论」，
    /// 不得回退成 0 分（0 分是明确判断，不是未知）。
    pub fn scaled(&self, metrics: &Metrics) -> Option<Grade> {
        if !self.upper.is_finite() || self.upper <= 0.0 {
            return None;
        }
        let raw = self.grade(metrics, 0.0)?;
        let unit = score_to_unit(raw.score, self.levels.len())?;
        Some(Grade {
            score: unit * self.upper,
            confidence: raw.confidence,
        })
    }
}

impl_policy_for_form!(ScorePolicy, Vec::new());

impl DecisionPolicy for ScorePolicy {
    fn scene(&self) -> &'static str {
        self.id
    }

    fn request(&self, metrics: &Metrics) -> Option<DecisionRequest> {
        // 数量边界的唯一真相在协议层（2~255）。写死 10 是 jev 早期口径残留，
        // 会让合法的 11 级题零调用，也让平台永远用不上协议的真实上限。
        if !(MIN_SCORE_LEVELS..=MAX_SCORE_LEVELS).contains(&self.levels.len()) {
            return None;
        }
        Some(DecisionRequest {
            scene: self.id,
            state: collect_state(metrics, &self.state_keys),
            questions: BTreeMap::from([(
                self.primary_qid(),
                QuestionSpec::Grade {
                    instructions: self.instructions.to_string(),
                    levels: self.levels.clone(),
                },
            )]),
            timeout_ms: None,
        })
    }

    fn judge(&self, metrics: &Metrics, _answers: &DecisionAnswers) -> Vec<String> {
        let hit = self
            .scaled(metrics)
            .is_some_and(|g| g.score >= self.min_score);
        if hit {
            vec![self.id.to_string()]
        } else {
            Vec::new()
        }
    }
}

/// 概率形态：问一个 0~1 的存在性概率（`noul`，不带置信度）
///
/// 适合「是不是 / 要不要」这类二值判断，比单选少一层选项构造。
pub struct PresencePolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    instructions: &'static str,
    state_keys: Vec<&'static str>,
    min_probability: f32,
}

impl PresencePolicy {
    /// 构造概率形态
    ///
    /// - `min_probability`：命中阈值（0~1）
    pub fn new(
        id: &'static str,
        name: &'static str,
        condition_desc: &'static str,
        instructions: &'static str,
        min_probability: f32,
    ) -> Self {
        Self {
            id,
            name,
            condition_desc,
            instructions,
            state_keys: Vec::new(),
            min_probability,
        }
    }

    /// 声明哪些事实要作为 `state` 透传给模型
    pub fn with_state_keys(mut self, keys: &[&'static str]) -> Self {
        self.state_keys = keys.to_vec();
        self
    }

    /// 概率（0~1）
    pub fn probability(&self, metrics: &Metrics) -> Option<f32> {
        self.presence(metrics)
    }
}

impl_policy_for_form!(PresencePolicy, Vec::new());

impl DecisionPolicy for PresencePolicy {
    fn scene(&self) -> &'static str {
        self.id
    }

    fn request(&self, metrics: &Metrics) -> Option<DecisionRequest> {
        Some(DecisionRequest {
            scene: self.id,
            state: collect_state(metrics, &self.state_keys),
            questions: BTreeMap::from([(
                self.primary_qid(),
                QuestionSpec::Presence {
                    instructions: self.instructions.to_string(),
                },
            )]),
            timeout_ms: None,
        })
    }

    fn judge(&self, metrics: &Metrics, _answers: &DecisionAnswers) -> Vec<String> {
        let hit = self
            .probability(metrics)
            .is_some_and(|p| p >= self.min_probability);
        if hit {
            vec![self.id.to_string()]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests;
