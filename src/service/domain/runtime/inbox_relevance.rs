//! 收件箱新输入的相关性策略（每轮 poll 的语义判据）
//!
//! 结构判据（`inbox::accepts`：同源 / 同任务 / 同项目 / 同线程 / 同类型）
//! 先筛出「同上下文」的候选，数量本来就少；本模块只回答剩下的那个问题：
//! **这些候选是不是在补当前正在处理的同一件事**。
//!
//! - **一次多选**：全部候选作为一次 `Choice` 的选项交给小脑，额外放一个
//!   「都不相关」哨兵承接无关概率。只有一条候选时，多选题自然退化成
//!   「相关 / 不相关」判断题，无需单独分支；
//! - **不设时间窗**：不再按到达间隔分流。同上下文的补充消息哪怕隔了很久，
//!   只要语义相关就该并进本轮；反之刚到的新话题也不该混进来。
//! - **双向缓存**：判定结果按 `(anchor_id, message_id, 现场语义指纹)` 缓存，
//!   相关与不相关都写。指纹只取任务 / 项目 / 输入理解这类稳定区块，
//!   **不取**随轮次变化的「消息条数 / 最近动作」，否则缓存永远不命中；
//!   任务上下文变了则指纹变，旧判定自然失效。降级（无 Brain / 无小脑 /
//!   调用失败）时按「本次已判定为相关」放行并**同样写缓存**——结构判据
//!   已筛过一轮，既不重复问小脑，也不重复把同一批消息拼进后续轮次。
//!
//! 决策走统一的 [`crate::service::dal::brain::BrainDal::decide`]，Brain
//! 由思考节点从 Agent 携带的实例传入（优先 `Agent.brain`），不再另起
//! 「默认小脑」旁路：Agent 以后绑定自己的小脑时，这条链路自动跟随。
//! 缓存放 `pkg::cache` 基础设施层，不在域里新造全局表。

use crate::models::brain::Brain;
use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::pkg::policy::decision::{DecisionRequest, DecisionValue, QuestionSpec};
use crate::service::dal::brain::BrainDal;
use crate::service::domain::runtime::inbox::InboxAnchor;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::time::Duration;

/// 单批扫描条数上限：去掉时间窗后用它兜住每次拉取量，按时间正序分批
pub const INBOX_SCAN_LIMIT: usize = 10;

/// 排序权重的「噪声地板」：低于此值一律不相关
///
/// Choice 返回归一化概率分布，多个相关候选会互相稀释（5 个相关候选时
/// 每个可能只有 0.15），所以这里只挡极低噪声；真正的判据是「比哨兵
/// （都不相关）更可能」，见 [`related_ids_from_weights`]。
pub const INBOX_RELEVANCE_MIN_PROBABILITY: f32 = 0.1;

/// 单个候选进判据的最大字符数（控制小脑 token，超出截断）
const CANDIDATE_SNIPPET_CHARS: usize = 400;
/// 锚点正文进判据的最大字符数
const ANCHOR_SNIPPET_CHARS: usize = 1000;

/// 哨兵选项：所有候选都不相关时，概率落在这里，避免被迫分给某个候选
pub const NONE_OPTION_ID: &str = "__inbox_none__";

/// 决策场景标识（计量 / 日志口径，全局唯一）
pub const DECISION_SCENE: &str = "inbox_relevance";

/// 判定缓存名（系统缓存页可见可清）
const CACHE_NAME: &str = "runtime.inbox_relevance";
/// 缓存 TTL：覆盖一次长唤醒；判定输入（锚点 + 候选正文 + 稳定现场）由键覆盖
const CACHE_TTL: Duration = Duration::from_secs(30 * 60);

/// 参与现场指纹的稳定摘要标题（与 `cerebellum_context::GOAL_SECTIONS` 同口径）
const STABLE_FACT_PREFIXES: [&str; 3] = ["【任务上下文】", "【项目上下文】", "【输入理解结果】"];

/// Choice 判定的固定问题 ID（独立命名空间，避免覆盖排序 / 入站过滤场景）
const QUESTION_ID: &str = "inbox_relevance_ranking.primary";

/// 多选判据：找出所有在补同一件事的候选，哨兵承接「都不相关」
const INSTRUCTIONS: &str = "候选消息是同一发送者在同一个任务/项目上下文中、于当前消息之后发来的新消息。\
请选出每一条在补充、追问、修正「当前消息」正在处理的同一件事的候选。\
这是多选题：相关的候选请都选上；无关的新话题或独立请求不要选。\
如果所有候选都与当前消息无关，请选择「以上都与当前消息无关」这一项。";

/// 判定结果缓存（相关与不相关都缓存，降级放行按相关缓存）
fn verdict_cache() -> &'static crate::pkg::cache::TtlCache<String, bool> {
    static CACHE: std::sync::OnceLock<&'static crate::pkg::cache::TtlCache<String, bool>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| crate::pkg::cache::register(CACHE_NAME, CACHE_TTL))
}

/// 现场指纹：只取任务 / 项目 / 输入理解三类稳定摘要。
///
/// `situation` 还含「消息序列条数」「最近动作」等每轮都变的内容，若一并进键，
/// 缓存永远不命中；任务上下文真变了则指纹随之变化，旧判定自然失效。
fn situation_fingerprint(judge: &InboxJudgeContext) -> u64 {
    let mut hasher = DefaultHasher::new();
    judge.agent_id.hash(&mut hasher);
    judge.scene.hash(&mut hasher);
    for line in &judge.situation {
        if STABLE_FACT_PREFIXES
            .iter()
            .any(|prefix| line.starts_with(prefix))
        {
            line.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// 缓存键：锚点 + 候选 ID + 稳定现场指纹
fn cache_key(anchor_id: &str, message_id: &str, fingerprint: u64) -> String {
    format!("{anchor_id}|{message_id}|{fingerprint}")
}

/// 把一批候选按「相关」写入缓存，作为降级放行的终局标记
///
/// 结构判据已经筛过同源 / 同任务 / 同项目 / 同线程，筛不出更细的语义分辨
/// 时才放行；写缓存是为了不让同一批消息在后续轮次被反复判定、反复拼入。
fn cache_as_related(anchor_id: &str, ids: &BTreeSet<String>, fingerprint: u64) -> BTreeSet<String> {
    for id in ids {
        verdict_cache().insert(cache_key(anchor_id, id, fingerprint), true);
    }
    ids.clone()
}

/// 排序权重是否达到「相关」绝对阈值
pub fn is_relevant_probability(probability: f32) -> bool {
    probability >= INBOX_RELEVANCE_MIN_PROBABILITY
}

/// 截断正文为判据片段（按字符计，避免中英混排按字节切出半个汉字）
fn snippet(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{head}…（已截断）")
}

/// 小脑判定用的现场上下文
///
/// 由轮边界 `poll_round` 从当前思考现场组装，经 `collect` 透传给 `rank_related`。
/// 目的：候选之间未必直接相关，但可能都和**当前正在处理的那件事**有关；
/// 只给锚点正文 + 裸 ID，小脑无从判断这种间接关联。
#[derive(Debug, Clone, Default)]
pub struct InboxJudgeContext {
    /// 会话身份（Agent ID / 名称）
    pub agent_id: String,
    pub agent_name: String,
    /// 当前场景标识
    pub scene: String,
    /// 当前思考现场要点：任务/项目/输入理解摘要 + 最近动作 + 消息序列规模
    pub situation: Vec<String>,
}

/// 从思考现场组装判定上下文（供轮边界 poll 节点调用）
///
/// 身份与 Brain 都取自 Agent —— 与思考循环「四个场景统一从 `agent.brain`
/// 解析」的口径一致；Agent 日后绑定自己的小脑时，本链路无需再改。
/// 场景取自 `ThinkingScene`，现场要点由 `node_message_facts` 从本轮 Prompt
/// 抽取（任务上下文 / 项目上下文 / 输入理解摘要 + 最近动作）。全部在
/// domain/runtime 内部组装，不引入跨层依赖。
pub fn build_judge_context(
    agent: &crate::models::agent::Agent,
    scene: common::enums::ThinkingScene,
    messages: &[crate::models::cortex_types::ChatMessage],
) -> InboxJudgeContext {
    use crate::service::domain::runtime::cerebellum_context as context;
    let snippet_tokens = agent
        .brain
        .as_ref()
        .and_then(context::dialogue_snippet_tokens);
    InboxJudgeContext {
        agent_id: agent.po.id.clone(),
        agent_name: agent.po.name.clone(),
        scene: format!("{scene:?}"),
        situation: context::node_message_facts(messages, snippet_tokens),
    }
}

/// 组装小脑 state：会话身份 + 场景 + 当前消息 + 思考现场 + 结构上下文
///
/// 复用 `ContextBundle`，与其他决策场景（`thinking_strength` / `chain_reply` 等）
/// 同一套分块口径，避免小脑看到各不相同的上下文格式。
fn build_context(anchor: &InboxAnchor, judge: &InboxJudgeContext) -> String {
    let mut bundle = crate::service::domain::runtime::cerebellum_context::ContextBundle::new()
        .with_identity(&judge.agent_id, &judge.agent_name)
        .with_scene(&judge.scene)
        .with_user_message(&snippet(&anchor.content, ANCHOR_SNIPPET_CHARS));
    if !judge.situation.is_empty() {
        bundle.push("当前思考现场", judge.situation.join("\n"));
    }
    bundle.push(
        "消息结构",
        serde_json::json!({
            "from_id": anchor.from_id,
            "from_role": format!("{:?}", anchor.from_role),
            "project_id": anchor.project_id,
            "task_id": anchor.task_id,
            "thread_id": anchor.thread_id,
            "message_type": format!("{:?}", anchor.message_type),
        })
        .to_string(),
    );
    bundle.render()
}

/// 从一次 Choice 的权重表里取「相关」的候选 ID
///
/// 两道判据同时满足才算相关：
/// 1. 权重达到绝对阈值 [`INBOX_RELEVANCE_MIN_PROBABILITY`]；
/// 2. 权重高于「都不相关」哨兵——Choice 是归一化分布，多个候选会互相稀释，
///    只有比哨兵更可能，才说明模型真的认为它是同一件事。
///
/// 权重缺失与哨兵缺失同为「答案不完整」的降级态：一律放行，不因模型
/// 少回一个选项就把本该合并的消息拒掉（fail-open，与整体降级口径一致）。
pub fn related_ids_from_weights(
    weights: &BTreeMap<String, f32>,
    candidate_ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    let none_weight = weights.get(NONE_OPTION_ID).copied();
    candidate_ids
        .iter()
        .filter(|id| match weights.get(id.as_str()).copied() {
            Some(w) => is_relevant_probability(w) && none_weight.is_none_or(|none| w > none),
            None => true,
        })
        .cloned()
        .collect()
}

/// 判定一批同上下文候选，返回其中「与锚点同一件事」的消息 ID
///
/// 先查缓存，只有未命中的才进一次 Pick 多选；单条候选时退化为判断题
/// （一个候选 + 哨兵）。Brain / 小脑缺失、配额耗尽、调用失败或无答案时
/// 全部放行，并按「已判定为相关」写缓存。
pub async fn rank_related(
    ctx: &RequestContext,
    anchor: &InboxAnchor,
    candidates: &[Message],
    judge: &InboxJudgeContext,
    brain: Option<&Brain>,
    dal: &dyn BrainDal,
) -> BTreeSet<String> {
    let mut related = BTreeSet::new();
    let fingerprint = situation_fingerprint(judge);
    // 待判定候选：(id, 正文片段)
    let mut to_judge: Vec<(String, String)> = Vec::new();

    for message in candidates {
        let id = &message.po.id;
        match verdict_cache().get(&cache_key(&anchor.id, id, fingerprint)) {
            Some(true) => {
                related.insert(id.clone());
            }
            Some(false) => {}
            None => to_judge.push((
                id.clone(),
                snippet(&message.po.content, CANDIDATE_SNIPPET_CHARS),
            )),
        }
    }

    if to_judge.is_empty() {
        return related;
    }

    let judged_ids: BTreeSet<String> = to_judge.iter().map(|(id, _)| id.clone()).collect();
    // Brain 未装配（外部 Agent / 尚未唤醒）或没有小脑 ⇒ 零调用降级
    if brain.and_then(|brain| brain.cerebellum.as_ref()).is_none()
        || !brain.is_some_and(|brain| brain.cerebellum_quota.try_acquire())
    {
        return cache_as_related(&anchor.id, &judged_ids, fingerprint);
    }
    let brain = brain.expect("上面已确认 Brain 存在");

    let mut options: BTreeMap<String, String> = to_judge
        .iter()
        .map(|(id, text)| (id.clone(), text.clone()))
        .collect();
    options.insert(
        NONE_OPTION_ID.to_string(),
        "以上候选都与当前消息无关".to_string(),
    );
    let mut questions = BTreeMap::new();
    questions.insert(
        QUESTION_ID.to_string(),
        QuestionSpec::Pick {
            instructions: INSTRUCTIONS.to_string(),
            options,
        },
    );
    let request = DecisionRequest {
        scene: DECISION_SCENE,
        state: serde_json::json!({
            crate::service::domain::runtime::cerebellum_context::STATE_KEY: build_context(anchor, judge)
        }),
        questions,
        timeout_ms: None,
    };

    // 与其他小脑场景同一入口：provider / 超时 / 协议 / 降级全在 DAL。
    // DAL 由调用方注入，便于测试替换成 mock 覆盖各降级路径。
    let outcome = dal.decide(ctx.clone(), brain, vec![request]).await;
    if outcome.degraded.is_some() {
        return cache_as_related(&anchor.id, &judged_ids, fingerprint);
    }
    let Some(DecisionValue::Pick { weights, .. }) = outcome.answers.get(QUESTION_ID) else {
        // 有应答但没结论：同样视为本次已判定并放行
        return cache_as_related(&anchor.id, &judged_ids, fingerprint);
    };

    let hit = related_ids_from_weights(weights, &judged_ids);
    // 相关与不相关都写缓存，避免「判定过的消息」每轮重判
    for id in &judged_ids {
        verdict_cache().insert(cache_key(&anchor.id, id, fingerprint), hit.contains(id));
    }
    related.extend(hit);
    related
}

/// 生产入口：注入默认 Brain DAL（与 `thinking_strength::select_with_default_dao` 同范式）
pub async fn rank_related_with_default_dao(
    ctx: &RequestContext,
    anchor: &InboxAnchor,
    candidates: &[Message],
    judge: &InboxJudgeContext,
    brain: Option<&Brain>,
) -> BTreeSet<String> {
    let dal = crate::service::dal::brain::dal();
    rank_related(ctx, anchor, candidates, judge, brain, dal.as_ref()).await
}

#[cfg(test)]
#[path = "inbox_relevance_tests.rs"]
mod tests;
