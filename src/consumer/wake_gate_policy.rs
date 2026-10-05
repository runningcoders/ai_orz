//! 唤醒前置门闩策略（策略引擎的消息消费领域落地）
//!
//! 复用 `pkg/policy` 引擎组合「这次消息要不要唤醒 Agent」的前置校验规则组，
//! 照 `message_route_policy.rs` 先例：
//! - 领域判定走专用匹配器 + 声明式规则表，按 Or 组合
//! - 声明顺序 = 判定优先级：首个命中即产出跳过决策
//! - 策略只回答「要不要唤醒」与「为什么」，**不碰副作用**
//!   （释放 Busy / 通知来源方的副作用留在 consumer 侧）
//!
//! 为什么要有这一层：
//! 项目管理场景下 Owner 会把整张 DAG 一次性全部分派出去，后继任务的
//! TaskAssignment 会先于它的前置任务完成而到达。若无门闩，Agent 会被
//! 唤醒去做一个还轮不到它的任务 —— 纯资源浪费（LLM 往返 + 工具调用），
//! 且大概率产出错误的中间结果。门闩的处理详见 `TaskEventConsumer` 的
//! 依赖就绪补发：跳过不是丢弃，是把这次唤醒推迟到前置完成时重发。
//!
//! 适配器（`MessageConsumer::handle_agent_message`）负责把查库得到的
//! 事实填进 Metrics，策略表本身零查库。

use common::enums::TaskStatus;

use crate::pkg::policy::{Metrics, Policy, PolicyBuilder};
use std::sync::OnceLock;

/// Metrics 键约定（入口统一填充）
pub mod keys {
    /// 任务状态（`TaskStatus::to_i32` 数值；未关联任务时不填）
    pub const TASK_STATUS: &str = "wake.task_status";
    /// 尚未完成的前置任务 id 列表（未就绪即阻塞）
    pub const PENDING_DEPS: &str = "wake.pending_dependencies";
    /// 已终结但非 Completed 的前置任务 id 列表（永不可能就绪）
    pub const STALE_DEPS: &str = "wake.stale_dependencies";
    /// 该 Agent 在该任务上已累计的唤醒次数
    pub const WAKEUP_COUNT: &str = "wake.wakeup_count";
    /// 单任务唤醒次数上限（Agent 级 `max_thinking_depth`；0 = 未启用）
    pub const MAX_WAKEUPS: &str = "wake.max_wakeups";
}

/// 「已达单任务唤醒上限」策略 id（consumer 侧据此决定是否通知来源方）
pub const WAKEUP_BUDGET_POLICY_ID: &str = "wakeup_budget_exhausted";
/// 「前置依赖已失效」策略 id（consumer 侧据此把日志升级为 warn —— 这是死锁信号）
pub const STALE_DEPENDENCY_POLICY_ID: &str = "dependency_stale";

/// 唤醒跳过决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeGateSkip {
    /// 命中的策略 id（`WAKEUP_BUDGET_POLICY_ID` 等）
    pub policy_id: &'static str,
    /// 跳过原因（人类可读，直接进日志）
    pub reason: String,
}

/// 唤醒门闩的判定输入（适配器查库后填充，策略侧零查询）
pub struct WakeGateInput {
    /// 消息关联的任务状态（普通对话消息为 `None`）
    pub task_status: Option<TaskStatus>,
    /// 尚未完成的前置任务 id
    pub pending_dependencies: Vec<String>,
    /// 已终结但非 Completed 的前置任务 id（撤销 / 归档）
    pub stale_dependencies: Vec<String>,
    /// 该 Agent 在该任务上已累计的唤醒次数（无统计数据时为 `None`）
    pub wakeup_count: Option<u64>,
    /// 单任务唤醒次数上限（0 = 未启用该护栏）
    pub max_wakeups: u64,
}

/// 唤醒门闩匹配器（本领域的判定形态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WakeGateMatcher {
    /// 任务状态落在给定集合内（任务已终结的三种形态）
    TaskStatusIn(&'static [TaskStatus]),
    /// 给定键承载的字符串列表非空（还有依赖没满足）
    NonEmptyList(&'static str),
    /// 双键数值比较：`wakeup_count >= max_wakeups`
    /// （上限来自 Metrics 而非构造期常量 —— 它是 Agent 级配置，与 builtin
    /// `MaxRoundsPolicy` 保留专用实现的理由一致；`max_wakeups = 0` 未启用）
    WakeupBudget,
}

impl WakeGateMatcher {
    fn matches(&self, metrics: &Metrics) -> bool {
        match self {
            WakeGateMatcher::TaskStatusIn(statuses) => {
                // 未关联任务时该键缺失 → 不匹配任何状态
                let Some(actual) = metrics.get_u64(keys::TASK_STATUS) else {
                    return false;
                };
                statuses.iter().any(|s| s.to_i32() as u64 == actual)
            }
            WakeGateMatcher::NonEmptyList(key) => !metrics.get_str_list(key).is_empty(),
            WakeGateMatcher::WakeupBudget => {
                let max = metrics.get_u64(keys::MAX_WAKEUPS).unwrap_or(0);
                if max == 0 {
                    return false;
                }
                metrics.get_u64(keys::WAKEUP_COUNT).unwrap_or(0) >= max
            }
        }
    }
}

/// 唤醒门闩规则（通用 `Policy` 的消息消费领域实现）
#[derive(Debug, Clone, Copy)]
struct StaticWakeGatePolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    matcher: WakeGateMatcher,
    /// 命中后的原因文案生成器（携带具体任务 id / 计数，直接可读）
    describe: fn(&Metrics) -> String,
}

impl Policy for StaticWakeGatePolicy {
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
        match self.matcher {
            WakeGateMatcher::TaskStatusIn(_) => vec![keys::TASK_STATUS.to_string()],
            WakeGateMatcher::NonEmptyList(key) => vec![key.to_string()],
            WakeGateMatcher::WakeupBudget => {
                vec![
                    keys::WAKEUP_COUNT.to_string(),
                    keys::MAX_WAKEUPS.to_string(),
                ]
            }
        }
    }

    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        if self.matcher.matches(metrics) {
            vec![self.id.to_string()]
        } else {
            Vec::new()
        }
    }
}

/// 任务终态集合（Completed / Cancelled / Archived）
const TERMINAL_STATUSES: &[TaskStatus] = &[
    TaskStatus::Completed,
    TaskStatus::Cancelled,
    TaskStatus::Archived,
];

/// 静态规则表（声明顺序 = 判定优先级：首个命中即产出跳过）
static WAKE_GATE_DEFS: &[StaticWakeGatePolicy] = &[
    // 任务已终结 → 跳过：任务是一次性的，再唤醒只会做重复劳动
    StaticWakeGatePolicy {
        id: "task_terminal",
        name: "任务已终结",
        condition_desc: "关联任务处于 Completed / Cancelled / Archived",
        matcher: WakeGateMatcher::TaskStatusIn(TERMINAL_STATUSES),
        describe: |m| {
            format!(
                "task status is terminal (status={})",
                m.get_u64(keys::TASK_STATUS).unwrap_or_default()
            )
        },
    },
    // 前置依赖未就绪 → 跳过：这是 DAG 乱序派发的正常现象，不是错误。
    // 后继任务会在前置完成时由 `TaskEventConsumer` 重发 TaskAssignment
    StaticWakeGatePolicy {
        id: "dependency_unmet",
        name: "前置依赖未就绪",
        condition_desc: "存在尚未完成的前置任务",
        matcher: WakeGateMatcher::NonEmptyList(keys::PENDING_DEPS),
        describe: |m| {
            format!(
                "pending prerequisites: {}",
                m.get_str_list(keys::PENDING_DEPS).join(", ")
            )
        },
    },
    // 前置依赖已失效 → 跳过：前置被撤销 / 归档，等待永远不会结束。
    // 与 dependency_unmet 分开是为了让这条单独可观测 —— 它是死锁信号，
    // 需要人工介入，而不只是「还没轮到」
    StaticWakeGatePolicy {
        id: STALE_DEPENDENCY_POLICY_ID,
        name: "前置依赖已失效",
        condition_desc: "存在已撤销或已归档的前置任务",
        matcher: WakeGateMatcher::NonEmptyList(keys::STALE_DEPS),
        describe: |m| {
            format!(
                "stale prerequisites (cancelled or archived, will never be satisfied): {}",
                m.get_str_list(keys::STALE_DEPS).join(", ")
            )
        },
    },
    // 单任务唤醒预算耗尽 → 跳过：口径是「该 Agent 在该任务上被唤醒的累计次数」，
    // 不是工具调用数、不是思考轮次
    StaticWakeGatePolicy {
        id: WAKEUP_BUDGET_POLICY_ID,
        name: "单任务唤醒预算耗尽",
        condition_desc: "该任务上的累计唤醒次数已达 Agent 级上限",
        matcher: WakeGateMatcher::WakeupBudget,
        describe: |m| {
            format!(
                "wakeup budget exhausted for this task ({}/{})",
                m.get_u64(keys::WAKEUP_COUNT).unwrap_or(0),
                m.get_u64(keys::MAX_WAKEUPS).unwrap_or(0)
            )
        },
    },
];

/// 门闩策略组：领域规则表按 Or 组合
/// （等价 `policy_set!(OR { .. })` 的展开结果，声明顺序即优先级）
fn wake_gate_ruleset() -> &'static dyn Policy {
    static RULESET: OnceLock<Box<dyn Policy>> = OnceLock::new();
    RULESET
        .get_or_init(|| {
            let mut builder = PolicyBuilder::new();
            for rule in WAKE_GATE_DEFS {
                builder = builder.with_policy(*rule);
            }
            builder.or()
        })
        .as_ref()
}

/// 判断本次消息是否应当跳过唤醒
///
/// **单一扩展点**：所有「醒了也没意义 / 醒了有害」的前置条件都在 `WAKE_GATE_DEFS`
/// 里声明一条，新增判据只加规则、不动 consumer。返回 `None` 表示放行
/// （正常唤醒），返回 `Some(skip)` 表示应当跳过。
///
/// | 场景 | 规则 | 理由 |
/// |------|------|------|
/// | 任务已完结 | `task_terminal` | 任务是一次性的，重复唤醒只会重做 |
/// | 前置未完成 | `dependency_unmet` | DAG 后继还没轮到，等前置完成时重发任务分配 |
/// | 前置已失效 | `dependency_stale` | 前置被撤销/归档，等待不会结束，需人工介入 |
/// | 唤醒预算耗尽 | `wakeup_budget_exhausted` | 单任务唤醒次数上限，防无限自唤醒 |
pub fn judge_wake_gate(input: WakeGateInput) -> Option<WakeGateSkip> {
    let mut metrics = Metrics::new();
    if let Some(status) = input.task_status {
        metrics = metrics.with(keys::TASK_STATUS, status.to_i32() as u64);
    }
    if !input.pending_dependencies.is_empty() {
        metrics = metrics.with(keys::PENDING_DEPS, input.pending_dependencies);
    }
    if !input.stale_dependencies.is_empty() {
        metrics = metrics.with(keys::STALE_DEPS, input.stale_dependencies);
    }
    if let Some(count) = input.wakeup_count {
        metrics = metrics.with(keys::WAKEUP_COUNT, count);
    }
    metrics = metrics.with(keys::MAX_WAKEUPS, input.max_wakeups);

    // Or 组：按声明顺序上浮首个命中
    let policy_id = wake_gate_ruleset().evaluate(&metrics).first()?.clone();
    let rule = WAKE_GATE_DEFS.iter().find(|r| r.id == policy_id)?;
    Some(WakeGateSkip {
        policy_id: rule.id,
        reason: (rule.describe)(&metrics),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(task_status: Option<TaskStatus>) -> WakeGateInput {
        WakeGateInput {
            task_status,
            pending_dependencies: Vec::new(),
            stale_dependencies: Vec::new(),
            wakeup_count: None,
            max_wakeups: 0,
        }
    }

    #[test]
    fn no_task_and_no_signals_passes_gate() {
        assert_eq!(judge_wake_gate(gate(None)), None);
    }

    #[test]
    fn active_task_status_passes_gate() {
        for status in [TaskStatus::Pending, TaskStatus::InProgress] {
            assert_eq!(
                judge_wake_gate(gate(Some(status))),
                None,
                "{status:?} 不应被拦下"
            );
        }
    }

    #[test]
    fn terminal_task_status_skips_wake() {
        for status in [
            TaskStatus::Completed,
            TaskStatus::Cancelled,
            TaskStatus::Archived,
        ] {
            let skip = judge_wake_gate(gate(Some(status)))
                .unwrap_or_else(|| panic!("{status:?} 应被拦下"));
            assert_eq!(skip.policy_id, "task_terminal");
        }
    }

    #[test]
    fn unmet_dependency_skips_wake_with_ids() {
        let input = WakeGateInput {
            task_status: Some(TaskStatus::Pending),
            pending_dependencies: vec!["task-A".to_string(), "task-B".to_string()],
            stale_dependencies: Vec::new(),
            wakeup_count: Some(1),
            max_wakeups: 0,
        };
        let skip = judge_wake_gate(input).expect("未就绪依赖应跳过唤醒");
        assert_eq!(skip.policy_id, "dependency_unmet");
        assert!(skip.reason.contains("task-A"));
        assert!(skip.reason.contains("task-B"));
    }

    #[test]
    fn terminal_task_takes_priority_over_unmet_dependency() {
        // 任务已终结时不必再去算依赖：声明顺序决定 task_terminal 优先
        let input = WakeGateInput {
            task_status: Some(TaskStatus::Completed),
            pending_dependencies: vec!["task-A".to_string()],
            stale_dependencies: Vec::new(),
            wakeup_count: None,
            max_wakeups: 0,
        };
        assert_eq!(judge_wake_gate(input).unwrap().policy_id, "task_terminal");
    }

    #[test]
    fn stale_dependency_is_reported_separately_from_unmet() {
        let input = WakeGateInput {
            task_status: Some(TaskStatus::Pending),
            pending_dependencies: vec!["task-A".to_string()],
            stale_dependencies: vec!["task-Z".to_string()],
            wakeup_count: None,
            max_wakeups: 0,
        };
        // dependency_unmet 声明在前，两者同时成立时报前者；
        // 仅失效依赖成立时才显式暴露 dependency_stale
        assert_eq!(
            judge_wake_gate(input).unwrap().policy_id,
            "dependency_unmet"
        );

        let input = WakeGateInput {
            task_status: Some(TaskStatus::Pending),
            stale_dependencies: vec!["task-Z".to_string()],
            ..gate(None)
        };
        let skip = judge_wake_gate(input).expect("失效依赖应跳过唤醒");
        assert_eq!(skip.policy_id, "dependency_stale");
        assert!(skip.reason.contains("task-Z"));
    }

    #[test]
    fn wakeup_budget_zero_means_disabled() {
        let input = WakeGateInput {
            task_status: None,
            pending_dependencies: Vec::new(),
            stale_dependencies: Vec::new(),
            wakeup_count: Some(999),
            max_wakeups: 0,
        };
        assert_eq!(judge_wake_gate(input), None);
    }

    #[test]
    fn wakeup_budget_exhausted_skips_wake() {
        let input = WakeGateInput {
            task_status: Some(TaskStatus::InProgress),
            pending_dependencies: Vec::new(),
            stale_dependencies: Vec::new(),
            wakeup_count: Some(5),
            max_wakeups: 5,
        };
        let skip = judge_wake_gate(input).expect("达到唤醒上限应跳过");
        assert_eq!(skip.policy_id, WAKEUP_BUDGET_POLICY_ID);
        assert!(skip.reason.contains("5/5"));
    }

    #[test]
    fn wakeup_budget_not_reached_passes_gate() {
        let input = WakeGateInput {
            task_status: Some(TaskStatus::InProgress),
            pending_dependencies: Vec::new(),
            stale_dependencies: Vec::new(),
            wakeup_count: Some(4),
            max_wakeups: 5,
        };
        assert_eq!(judge_wake_gate(input), None);
    }

    #[test]
    fn wakeup_count_missing_passes_gate() {
        // 无统计数据时不能误判为 0 次唤醒之外的任何结论 —— 缺数据即放行，
        // 宁可多唤醒一次也不要静默吞掉任务
        let input = WakeGateInput {
            task_status: None,
            pending_dependencies: Vec::new(),
            stale_dependencies: Vec::new(),
            wakeup_count: None,
            max_wakeups: 1,
        };
        assert_eq!(judge_wake_gate(input), None);
    }
}
