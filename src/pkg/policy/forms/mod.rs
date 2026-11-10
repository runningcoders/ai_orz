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
//! | [`ScorePolicy`] | `Grade` | 等级表 + 上下文 | [`Grade`]（分数 + 置信度） |
//! | [`PresencePolicy`] | `Presence` | 上下文 | 0~1 概率 |
//!
//! 这四种覆盖了协议全部三种题型的「正常用法」：
//! - **排序**与**多选**是同一个形态（排序后按阈值截断就是多选，见 [`RankPolicy::ranked`]）；
//! - **单选**是「候选只有一个胜出」的排序特例，但答案的消费方式不同（要置信度门槛，不要全序）；
//! - **分级**用于「强度/质量」这类连续判断，**概率**用于「是不是/要不要」这类二值判断。
//!
//! 需要额外后处理（把 id 映射回实体、按结论选文案）时，读 `Metrics` 里的答案即可；
//! 需要非同寻常的问法（多题、动态题型）时，自己实现 [`DecisionPolicy`]。

use super::decision::{
    DecisionAnswers, DecisionRequest, DecisionValue, Grade, Metrics, PickOne, QuestionSpec,
};
use super::{DecisionPolicy, Policy};
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
            timeout_ms: None,
        }
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

/// 分级形态：把「强度 / 质量」判成一个分数，分数达标才算命中
pub struct ScorePolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    instructions: &'static str,
    levels: Vec<&'static str>,
    state_keys: Vec<&'static str>,
    min_score: f32,
}

impl ScorePolicy {
    /// 构造分级形态
    ///
    /// - `levels`：由低到高的等级描述（协议要求 2~10 项）
    /// - `min_score`：命中阈值（0~1）
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
            levels: levels.into_iter().collect(),
            state_keys: Vec::new(),
            min_score,
        }
    }

    /// 声明哪些事实要作为 `state` 透传给模型
    pub fn with_state_keys(mut self, keys: &[&'static str]) -> Self {
        self.state_keys = keys.to_vec();
        self
    }

    /// 分数（带置信度）
    pub fn score(&self, metrics: &Metrics) -> Option<Grade> {
        self.grade(metrics, 0.0)
    }
}

impl_policy_for_form!(ScorePolicy, Vec::new());

impl DecisionPolicy for ScorePolicy {
    fn scene(&self) -> &'static str {
        self.id
    }

    fn request(&self, metrics: &Metrics) -> Option<DecisionRequest> {
        // 等级数量是协议硬约束：不在 2~10 会被 client 整批拒
        if !(2..=10).contains(&self.levels.len()) {
            return None;
        }
        Some(DecisionRequest {
            scene: self.id,
            state: collect_state(metrics, &self.state_keys),
            questions: BTreeMap::from([(
                self.primary_qid(),
                QuestionSpec::Grade {
                    instructions: self.instructions.to_string(),
                    levels: self.levels.iter().map(|l| (*l).to_string()).collect(),
                },
            )]),
            timeout_ms: None,
        })
    }

    fn judge(&self, metrics: &Metrics, _answers: &DecisionAnswers) -> Vec<String> {
        let hit = self
            .score(metrics)
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
