//! 协作压力组件：思考过程可复用的「链深度 + 信息疲劳度」观测与裁决
//!
//! # 为什么独立成组件
//!
//! 防 A↔B 无限互发原先只有一个编译期常量（链深 >= 5 → 直接掐断），
//! 模型侧又只有 `NO_REPLY` 这种**二值、靠自觉**的软停止，中间没有过渡带。
//! 本组件把「慢性红线」补上：重复往返累积疲劳度，压力带内先施压、到硬阈值才升级；
//! 新信息注入会**回血**并重置连续窗口，因此协作可以被新鲜感合法延长，而不是一刀切。
//!
//! 它被刻意做成**零 I/O、零上层依赖**的纯逻辑：
//! - 不认识 `Message` / `MessagePo`，调用方只把领域对象翻译成 [`ChainHop`] 语义标志；
//! - 不读数据库、不发消息、不写日志；记录与打印由 [`PressureSnapshot`] 交给调用方；
//! - 判定走 `pkg/policy` 策略引擎，命中的策略 id / 条件描述天然可记录。
//!
//! 因此消息回发链路与后续任何思考链路（工具循环、多 Agent 协商、任务收敛）
//! 都能用同一套口径，提示词与行为动作也在同一处维护。
//!
//! # 单一真相：一张档位规则表
//!
//! `policy.rs` 的 `STAGE_DEFS` 每一行**同时**声明：策略判据、命中档位、
//! 行为动作、给模型的配套提示词、回血话术。找「这个动作会让模型看到什么」
//! 只需要看那一行，不需要跨文件拼接。
//!
//! # 两个容易搞混的量
//!
//! - `depth`：绝对链深，单调递增，只用于展示与日志；
//! - `window_depth`：**自最近一次有效回血以来**的连续无新信息跳数，参与硬墙判定。
//!
//! 只让回血降疲劳度、不重置窗口的话，绝对深度仍会单调触顶，
//! 「有新鲜感就允许继续聊」就成了一句空话。

mod policy;

#[cfg(test)]
mod tests;

pub use policy::{PressureAction, PressureStage};

use std::fmt;

use serde::Serialize;

use crate::pkg::policy::Metrics;

/// 策略引擎输入键（全部由 [`ChainPressure::metrics`] 一处填充）
pub mod keys {
    /// 绝对链深（连续 Agent 来源跳数，含入口）
    pub const CHAIN_DEPTH: &str = "collab.chain_depth";
    /// 自最近回血以来的连续无新信息跳数
    pub const WINDOW_DEPTH: &str = "collab.window_depth";
    /// 信息疲劳度（0..=`max_fatigue`）
    pub const FATIGUE: &str = "collab.fatigue";
    /// 任意新信息出现次数
    pub const NOVELTY_COUNT: &str = "collab.novelty_count";
    /// 触发窗口重置的回血次数
    pub const RECOVERY_COUNT: &str = "collab.recovery_count";
    /// 软阈值（压力带起点，来自配置）
    pub const SOFT_FATIGUE: &str = "collab.soft_fatigue";
    /// 硬阈值（升级区起点，来自配置）
    pub const HARD_FATIGUE: &str = "collab.hard_fatigue";
    /// 连续无新信息窗口的硬上限（来自配置）
    pub const HARD_WINDOW_DEPTH: &str = "collab.hard_window_depth";
}

/// 一跳带来的新信息类型：**回血判据与回血力度的单一真相**
///
/// 调用方负责把领域字段翻译成这些语义标志（组件不认识消息类型枚举），
/// 「什么算新信息、回多少血、是否重置窗口、配哪句提示」全部留在本组件。
///
/// 刻意不做内容相似度 / embedding：那是落库后的检索能力，会带来模型调用、
/// 延迟与不确定性，而协作是否收敛用结构化锚点就够了（YAGNI）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum Novelty {
    /// 纯文本往返，没有引入新信息
    #[default]
    None,
    /// 正文与前序不同（弱新信息，只微量降疲劳度，不重置窗口）
    DistinctContent,
    /// 工具/任务结果、附件等非纯文本载荷（强新信息，回血并重置窗口）
    ToolResult,
    /// 链上出现第三个及以后的参与方（强新信息，回血并重置窗口）
    NewParticipant,
    /// 用户/系统外部输入：链的自然终止点，整条压力状态重置
    ExternalInput,
}

impl Novelty {
    /// 回血点数（从疲劳度里扣除）
    pub fn recovery_points(self) -> u64 {
        match self {
            Novelty::None => 0,
            Novelty::DistinctContent => 8,
            Novelty::ToolResult => 40,
            Novelty::NewParticipant => 30,
            Novelty::ExternalInput => 0,
        }
    }

    /// 是否重置「连续无新信息窗口」（新鲜感能延长协作的关键）
    pub fn resets_window(self) -> bool {
        matches!(
            self,
            Novelty::ToolResult | Novelty::NewParticipant | Novelty::ExternalInput
        )
    }

    /// 是否外部输入（整条链重置）
    pub fn is_external_input(self) -> bool {
        matches!(self, Novelty::ExternalInput)
    }

    /// 日志/提示词里的可读标签
    pub fn label(self) -> &'static str {
        match self {
            Novelty::None => "none",
            Novelty::DistinctContent => "distinct_content",
            Novelty::ToolResult => "tool_result",
            Novelty::NewParticipant => "new_participant",
            Novelty::ExternalInput => "external_input",
        }
    }
}

/// 链上一跳的观测（调用方沿 `reply_to_id` 回溯时逐条构造）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainHop {
    /// 该跳是否为 Agent 来源；非 Agent 来源是链的自然断点
    pub from_agent: bool,
    /// 该跳带来的新信息类型
    pub novelty: Novelty,
}

impl ChainHop {
    /// 纯文本、无新信息的一跳（最常见的乒乓形态）
    pub fn plain() -> Self {
        Self {
            from_agent: true,
            novelty: Novelty::None,
        }
    }
}

/// 压力参数（全部整数，可直接塞进 [`Metrics`]，阈值不固化在策略对象里）
///
/// 默认值对应「硬墙 8 跳、第 3 跳开始施压」：
/// 每跳 +12，纯往返时第 3 跳疲劳度 36 进入压力带，第 8 跳撞窗口硬墙。
/// 有工具结果 / 新参与方回血时窗口重置，协作可以合法超过 8 跳。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PressureConfig {
    /// 每一跳累积的疲劳度
    pub per_hop_fatigue: u64,
    /// 压力带起点（>= 该值触发 Nudge）
    pub soft_fatigue: u64,
    /// 升级区起点（>= 该值触发 Escalate）
    pub hard_fatigue: u64,
    /// 连续无新信息窗口的硬上限
    pub hard_window_depth: u64,
    /// 疲劳度上限（防止无界增长）
    pub max_fatigue: u64,
}

impl Default for PressureConfig {
    fn default() -> Self {
        Self {
            per_hop_fatigue: 12,
            soft_fatigue: 36,
            hard_fatigue: 100,
            hard_window_depth: 8,
            max_fatigue: 100,
        }
    }
}

impl PressureConfig {
    /// 系统级配置入口（`[agent].collab_*`），供消息链路等默认路径使用
    ///
    /// 组件核心仍是纯的：需要自定义口径的思考链路直接传显式构造的
    /// [`PressureConfig`]，不要走这里。
    pub fn from_system() -> Self {
        let agent = &crate::config::get().agent;
        Self {
            soft_fatigue: agent.collab_soft_fatigue,
            hard_fatigue: agent.collab_hard_fatigue,
            hard_window_depth: agent.collab_hard_window_depth as u64,
            ..Self::default()
        }
    }

    /// 阈值合法性兜底：配置写错（软 >= 硬、窗口 0）时退回默认值，
    /// 避免一条坏配置把所有协作永久钉死在升级区
    pub fn sanitized(mut self) -> Self {
        let default = Self::default();
        if self.per_hop_fatigue == 0
            || self.soft_fatigue == 0
            || self.soft_fatigue >= self.hard_fatigue
            || self.hard_window_depth == 0
        {
            self = default;
        }
        // `hard_fatigue` 是可配的，默认 `max_fatigue` 只有 100。若照旧用
        // `hard_fatigue > max_fatigue` 判非法，配 150 会把整份配置静默退回
        // 默认值（连软阈值与窗口也一起回退），等于「放宽硬墙」永远不生效。
        // 这里让疲劳度上限自动跟随硬阈值。
        self.max_fatigue = self.max_fatigue.max(self.hard_fatigue);
        self
    }
}

/// 一次协作链的观测累加器（无状态持久化，每次回发/思考前重建）
#[derive(Debug, Clone)]
pub struct ChainPressure {
    config: PressureConfig,
    depth: usize,
    window_depth: usize,
    fatigue: u64,
    novelty_count: u64,
    recovery_count: u64,
    reset_count: u64,
    last_novelty: Novelty,
}

impl Default for ChainPressure {
    fn default() -> Self {
        Self::new(PressureConfig::default())
    }
}

impl ChainPressure {
    /// 用指定口径创建（会自动做阈值合法性兜底）
    pub fn new(config: PressureConfig) -> Self {
        Self {
            config: config.sanitized(),
            depth: 0,
            window_depth: 0,
            fatigue: 0,
            novelty_count: 0,
            recovery_count: 0,
            reset_count: 0,
            last_novelty: Novelty::None,
        }
    }

    /// 观测一跳，返回 `false` 表示链在此断开（非 Agent 来源）
    ///
    /// 调用方应在 `false` 时停止回溯——用户/系统介入是链的自然终止点。
    pub fn observe(&mut self, hop: ChainHop) -> bool {
        if !hop.from_agent {
            return false;
        }
        self.depth += 1;
        self.window_depth += 1;
        self.fatigue = (self.fatigue + self.config.per_hop_fatigue).min(self.config.max_fatigue);
        self.last_novelty = hop.novelty;

        if hop.novelty.is_external_input() {
            self.reset_chain();
            return true;
        }

        let points = hop.novelty.recovery_points();
        if points > 0 {
            self.fatigue = self.fatigue.saturating_sub(points);
            self.novelty_count += 1;
        }
        if hop.novelty.resets_window() {
            self.window_depth = 0;
            self.recovery_count += 1;
        }
        true
    }

    /// 观测整条链：输入**必须按时间正序**（链头 → 当前消息），返回观测跳数
    ///
    /// 回血会清零 `window_depth` 并扣减 `fatigue`，是**有状态且有方向**的运算：
    /// 倒序喂入会把「回血之前的老跳」误算成「最近回血之后的连续窗口」，
    /// 既可能让硬墙永不触发（浪费），也可能对刚注入新信息的链误施压。
    pub fn observe_chain(&mut self, hops: impl IntoIterator<Item = ChainHop>) -> usize {
        let mut observed = 0;
        for hop in hops {
            if !self.observe(hop) {
                break;
            }
            observed += 1;
        }
        observed
    }

    /// 沿 `reply_to_id` 回溯（**新 → 旧**）得到的链：归一化为时间正序后观测
    ///
    /// 方向归一化留在组件内，调用方按查库顺序喂入即可，避免每个复用方各自
    /// `reverse` 而漏掉。⚠️ 链断点（`from_agent=false`）**不要喂进来**：归一化后
    /// 它会落在链头，`observe` 返回 false 会让整条链被误截断；调用方应在回溯
    /// 遇到 User/System 来源时直接停止收集。
    pub fn observe_backward_chain(
        &mut self,
        newest_first: impl IntoIterator<Item = ChainHop>,
    ) -> usize {
        let mut hops: Vec<ChainHop> = newest_first.into_iter().collect();
        hops.reverse();
        self.observe_chain(hops)
    }

    /// 外部输入注入：整条压力状态重置，但保留「新链第一跳」的深度
    fn reset_chain(&mut self) {
        self.depth = 1;
        self.window_depth = 0;
        self.fatigue = 0;
        self.reset_count += 1;
    }

    pub fn config(&self) -> PressureConfig {
        self.config
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn window_depth(&self) -> usize {
        self.window_depth
    }

    pub fn fatigue(&self) -> u64 {
        self.fatigue
    }

    pub fn novelty_count(&self) -> u64 {
        self.novelty_count
    }

    pub fn recovery_count(&self) -> u64 {
        self.recovery_count
    }

    pub fn reset_count(&self) -> u64 {
        self.reset_count
    }

    pub fn last_novelty(&self) -> Novelty {
        self.last_novelty
    }

    /// 供策略引擎消费的事实桶（阈值也作为双键注入，策略本身无状态）
    pub fn metrics(&self) -> Metrics {
        Metrics::new()
            .with(keys::CHAIN_DEPTH, self.depth as u64)
            .with(keys::WINDOW_DEPTH, self.window_depth as u64)
            .with(keys::FATIGUE, self.fatigue)
            .with(keys::NOVELTY_COUNT, self.novelty_count)
            .with(keys::RECOVERY_COUNT, self.recovery_count)
            .with(keys::SOFT_FATIGUE, self.config.soft_fatigue)
            .with(keys::HARD_FATIGUE, self.config.hard_fatigue)
            .with(keys::HARD_WINDOW_DEPTH, self.config.hard_window_depth)
    }

    /// 用策略引擎裁决档位，产出「行为动作 + 配套提示词 + 可记录快照」
    pub fn judge(&self) -> PressureDecision {
        let outcome = policy::judge_stage(&self.metrics());
        let snapshot = PressureSnapshot {
            chain_depth: self.depth,
            window_depth: self.window_depth,
            fatigue: self.fatigue,
            soft_fatigue: self.config.soft_fatigue,
            hard_fatigue: self.config.hard_fatigue,
            hard_window_depth: self.config.hard_window_depth,
            novelty_count: self.novelty_count,
            recovery_count: self.recovery_count,
            reset_count: self.reset_count,
            last_novelty: self.last_novelty.label(),
            stage: outcome.stage,
            action: outcome.action,
            policy_hits: outcome.policy_hits.clone(),
        };
        PressureDecision {
            stage: outcome.stage,
            action: outcome.action,
            policy_id: outcome.policy_id,
            condition_desc: outcome.condition_desc,
            prompt_hint: outcome.prompt_hint,
            recovery_hint: outcome.recovery_hint,
            policy_hits: outcome.policy_hits,
            snapshot,
        }
    }
}

/// 一次裁决的完整产出：动作、配套提示词、可记录快照在同一个对象里
#[derive(Debug, Clone)]
pub struct PressureDecision {
    pub stage: PressureStage,
    pub action: PressureAction,
    /// 命中的最高优先级策略 id（fallback 为 `pressure_fresh`）
    pub policy_id: &'static str,
    /// 策略条件描述（日志/前端可直接展示）
    pub condition_desc: &'static str,
    /// 本档位给模型的配套提示词
    pub prompt_hint: &'static str,
    /// 本档位的回血话术（与提示词同表声明）
    pub recovery_hint: &'static str,
    /// 本轮全部命中（声明顺序即优先级，便于统计「哪条规则最常触发」）
    pub policy_hits: Vec<String>,
    /// 可序列化 / 可打印的事实快照
    pub snapshot: PressureSnapshot,
}

impl PressureDecision {
    /// 注入到思考 prompt 的配套提示词（信号 + 动作 + 本轮指引 + 回血规则）
    ///
    /// 这是「每次回复都带上深度等信号」的落点：调用方把这组行追加到
    /// `ThinkingOptions.turn_guidance_lines`（与回复仲裁行同区块），当前正在
    /// 思考的 Agent 就能看到压力并主动收敛，而不是等到硬墙被机械掐断。
    pub fn prompt_lines(&self) -> Vec<String> {
        let s = &self.snapshot;
        vec![
            format!(
                "【协作压力】链深 {}（连续无新信息窗口 {}），疲劳度 {}/{}，回血 {} 次，最近新信息：{}。",
                s.chain_depth,
                s.window_depth,
                s.fatigue,
                s.hard_fatigue,
                s.recovery_count,
                s.last_novelty,
            ),
            format!(
                "- 当前档位：{} / 动作：{}（{}）",
                s.stage.label(),
                s.action.label(),
                self.policy_id
            ),
            format!("- 本轮指引：{}", self.prompt_hint),
            format!("- 回血规则：{}", self.recovery_hint),
        ]
    }

    /// 一行结构化日志文本（记录/打印的唯一出口）
    pub fn log_line(&self) -> String {
        format!("{} condition={}", self.snapshot, self.condition_desc)
    }
}

/// 可记录、可打印的压力事实快照
///
/// `Serialize` 供统计/轨迹落库，`Display` 供 `log_info!` 一行打印；
/// 组件自身不做任何 I/O，是否记录由调用方决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PressureSnapshot {
    pub chain_depth: usize,
    pub window_depth: usize,
    pub fatigue: u64,
    pub soft_fatigue: u64,
    pub hard_fatigue: u64,
    pub hard_window_depth: u64,
    pub novelty_count: u64,
    pub recovery_count: u64,
    pub reset_count: u64,
    pub last_novelty: &'static str,
    pub stage: PressureStage,
    pub action: PressureAction,
    pub policy_hits: Vec<String>,
}

impl fmt::Display for PressureSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "collab_pressure depth={} window={}/{} fatigue={}/{} (soft={} hard={}) novelty={} recovery={} reset={} novelty_last={} stage={} action={} hits=[{}]",
            self.chain_depth,
            self.window_depth,
            self.hard_window_depth,
            self.fatigue,
            self.hard_fatigue,
            self.soft_fatigue,
            self.hard_fatigue,
            self.novelty_count,
            self.recovery_count,
            self.reset_count,
            self.last_novelty,
            self.stage.label(),
            self.action.label(),
            self.policy_hits.join(","),
        )
    }
}
