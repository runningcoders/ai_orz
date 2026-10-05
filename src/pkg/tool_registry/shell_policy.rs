//! shell_exec 拦截层（策略引擎的 shell 领域落地）
//!
//! 复用 `pkg/policy` 引擎：每条拦截规则 = 一个 `ShellRulePolicy`（声明式结构，
//! 实现通用 `Policy` trait），通过 Or 组合 + 引擎级 `action()` 上浮首个阻断动作。
//!
//! 设计要点：
//! - 拦截层只做「策略」，不做「内容修补」——内容修补走 git hooks 这类原生扩展点
//! - Scope 类规则是结构性判定（读 Metrics 中的路径与身份），不解析命令字符串
//! - 命令类规则（Regex/Subcommand）是粗粒度匹配：Confirm 是护栏不是安全边界，
//!   真正的安全边界是路径 scope + Manual 控制模式审批，嵌套绕过（`sh -c` 等）
//!   漏拦可接受，YAGNI 不做完整 shell argv 解析
//! - inject-env（AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID）不是规则，是管线出口的固定步骤
//!   （见 `ENV_TASK_ID` / `ENV_AGENT_ID`，shell_exec 恒注入，供 git commit-msg hook 读取）

use crate::pkg::policy::{Metrics, Policy, PolicyAction, PolicyBuilder};
use crate::pkg::tool_registry::tool_security::fs::{
    crosses_agent_workspace, crosses_user_boundary,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// 结构性注入给子进程的环境变量键（git commit-msg hook 消费）
pub const ENV_TASK_ID: &str = "AI_ORZ_TASK_ID";
/// 结构性注入给子进程的环境变量键（git commit-msg hook 消费）
pub const ENV_AGENT_ID: &str = "AI_ORZ_AGENT_ID";

/// Metrics 键约定（适配层 `evaluate` 统一填充）
pub mod keys {
    /// 原始命令串
    pub const COMMAND: &str = "shell.command";
    /// 解析后的绝对工作目录
    pub const WORKING_DIR: &str = "shell.working_dir";
    /// base_data_path
    pub const BASE_ROOT: &str = "shell.base_root";
    /// 附加路径白名单（ShellExecConfig.additional_allowed_paths）
    pub const ADDITIONAL_ALLOWED_PATHS: &str = "shell.additional_allowed_paths";
    /// 调用者用户身份（缺失 = 系统直调）
    pub const USER_ID: &str = "shell.user_id";
    /// 调用者 Agent 身份（缺失 = 用户/系统直调）
    pub const AGENT_ID: &str = "shell.agent_id";
}

/// 命令匹配器
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandMatcher {
    /// 结构性判定：working_dir 不在 base_root 或附加白名单内
    ScopeOutsideAllowedPaths,
    /// 结构性判定：working_dir 越过用户树 / Agent 工作区身份边界
    ScopeIdentityBoundary,
    /// 任一正则命中原始命令串（模式编译失败 fail-fast，绝不静默跳过）
    Regex(&'static [&'static str]),
    /// 二进制 + 子命令粗粒度匹配：
    /// 语句级扫描命令 token，bin 名命中后向后扫描至首个 `-` 开头的
    /// flag 或语句边界（`&&`/`;`/`||`）为止，任一 token 命中 subs 即匹配。
    /// 已知局限（接受）：`git -C <path> push` 这类全局 flag 后置子命令会漏拦，
    /// `cd x&&git push`（无空格）会漏拦——Confirm 是护栏不是安全边界。
    Subcommand {
        bin: &'static str,
        subs: &'static [&'static str],
    },
}

/// 规则动作类别（规则声明用；具体文案在规则表的 `reason` 字段）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAction {
    /// 拒绝执行
    Deny,
    /// 需要用户确认后才可放行
    Confirm,
    /// 放行但记录审计
    Audit,
}

/// 声明式 shell 拦截规则（通用 `Policy` 的 shell 领域实现）
#[derive(Debug, Clone, Copy)]
pub struct ShellRulePolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    matcher: CommandMatcher,
    action: RuleAction,
    reason: &'static str,
    /// 幂等性声明（§14.2 定案）：决定授权签发默认次数上限（非幂等=1 次）
    pub idempotent: bool,
}

impl ShellRulePolicy {
    /// 命中判定（供 action 与 evaluate 复用）
    fn matches(&self, metrics: &Metrics) -> bool {
        match self.matcher {
            CommandMatcher::ScopeOutsideAllowedPaths => {
                let (Some(wd), Some(base)) = (
                    metrics.get_str(keys::WORKING_DIR),
                    metrics.get_str(keys::BASE_ROOT),
                ) else {
                    return false;
                };
                let wd = Path::new(wd);
                if wd.starts_with(base) {
                    return false;
                }
                !metrics
                    .get_str_list(keys::ADDITIONAL_ALLOWED_PATHS)
                    .iter()
                    .any(|allowed| wd.starts_with(allowed))
            }
            CommandMatcher::ScopeIdentityBoundary => {
                let (Some(base), Some(wd)) = (
                    metrics.get_str(keys::BASE_ROOT),
                    metrics.get_str(keys::WORKING_DIR),
                ) else {
                    return false;
                };
                let (base, wd) = (Path::new(base), Path::new(wd));
                crosses_user_boundary(base, wd, metrics.get_str(keys::USER_ID))
                    || crosses_agent_workspace(base, wd, metrics.get_str(keys::AGENT_ID))
            }
            CommandMatcher::Regex(patterns) => {
                let cmd = metrics.get_str(keys::COMMAND).unwrap_or("");
                patterns.iter().any(|p| compiled_regex(p).is_match(cmd))
            }
            CommandMatcher::Subcommand { bin, subs } => {
                subcommand_matches(metrics.get_str(keys::COMMAND).unwrap_or(""), bin, subs)
            }
        }
    }
}

impl Policy for ShellRulePolicy {
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
            CommandMatcher::ScopeOutsideAllowedPaths => vec![
                keys::WORKING_DIR.to_string(),
                keys::BASE_ROOT.to_string(),
                keys::ADDITIONAL_ALLOWED_PATHS.to_string(),
            ],
            CommandMatcher::ScopeIdentityBoundary => vec![
                keys::WORKING_DIR.to_string(),
                keys::BASE_ROOT.to_string(),
                keys::USER_ID.to_string(),
                keys::AGENT_ID.to_string(),
            ],
            _ => vec![keys::COMMAND.to_string()],
        }
    }

    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        if self.matches(metrics) {
            vec![self.id.to_string()]
        } else {
            Vec::new()
        }
    }

    fn action(&self, metrics: &Metrics) -> Option<PolicyAction> {
        if !self.matches(metrics) {
            return None;
        }
        let reason = self.reason.to_string();
        Some(match self.action {
            RuleAction::Deny => PolicyAction::Deny(reason),
            RuleAction::Confirm => PolicyAction::Confirm(reason),
            RuleAction::Audit => PolicyAction::Audit(reason),
        })
    }
}

/// 静态规则表（声明顺序 = 阻断优先级：首个命中即上浮；审计规则置于末尾）
static RULE_DEFS: &[ShellRulePolicy] = &[
    // 灾难级 Deny 规则必须置于 RULE_DEFS 最前：声明序=裁决优先级，双命中时保证 Deny 先上浮（无解锁通路）
    ShellRulePolicy {
        id: "destructive_fs_catastrophic",
        name: "灾难级文件系统命令",
        condition_desc: "格式化块设备或直写块设备（源头不可执行，任何审批不可解锁）",
        matcher: CommandMatcher::Regex(&[r"mkfs", r"dd\s+[^;&]*of=/dev/"]),
        action: RuleAction::Deny,
        reason: "Catastrophic filesystem operation (formatting or raw-write to a block device). Denied at source; cannot be unlocked by any approval.",
        idempotent: false,
    },
    ShellRulePolicy {
        id: "workspace_outside_allowed_paths",
        name: "工作目录越界",
        condition_desc: "working_dir 不在 base_data_path 或附加白名单内",
        matcher: CommandMatcher::ScopeOutsideAllowedPaths,
        action: RuleAction::Confirm,
        reason: "Working directory is not in allowed paths. Execution requires explicit user confirmation.",
        idempotent: false,
    },
    ShellRulePolicy {
        id: "workspace_identity_boundary",
        name: "身份边界越界",
        condition_desc: "working_dir 越过用户树或 Agent 工作区身份边界",
        matcher: CommandMatcher::ScopeIdentityBoundary,
        action: RuleAction::Confirm,
        reason: "Working directory belongs to another user/agent workspace. You MUST STOP and ask the user for explicit confirmation before using it.",
        idempotent: false,
    },
    ShellRulePolicy {
        id: "destructive_fs",
        name: "破坏性文件系统命令",
        condition_desc: "rm 递归/强制删除宽泛目标（/ ~ * ..）",
        matcher: CommandMatcher::Regex(&[
            r"(^|[;&|]\s*)(sudo\s+)?rm\s+(-\w+\s+)*(--\s+)?(/|~|\*|\.\.?)(\s|$)",
        ]),
        action: RuleAction::Confirm,
        reason: "Command matches a broad-target destructive deletion pattern. Explicit user confirmation is required.",
        idempotent: false,
    },
    ShellRulePolicy {
        id: "git_dangerous_subcommand",
        name: "受限 git 子命令",
        condition_desc: "git push / reset / clean",
        matcher: CommandMatcher::Subcommand {
            bin: "git",
            subs: &["push", "reset", "clean"],
        },
        action: RuleAction::Confirm,
        reason: "Restricted git subcommand (push/reset/clean) requires explicit user confirmation.",
        idempotent: false,
    },
    ShellRulePolicy {
        id: "git_commit_audit",
        name: "git 提交审计",
        condition_desc: "git commit（产物锚点产生时刻）",
        matcher: CommandMatcher::Subcommand {
            bin: "git",
            subs: &["commit"],
        },
        action: RuleAction::Audit,
        reason: "git commit executed; task-scoped trailer is injected via commit-msg hook",
        idempotent: true,
    },
];

/// 规则集：静态规则表按 Or 组合（等价 policy_set!(OR { .. }) 的展开结果）
fn ruleset() -> &'static dyn Policy {
    static RULESET: OnceLock<Box<dyn Policy>> = OnceLock::new();
    RULESET
        .get_or_init(|| {
            let mut builder = PolicyBuilder::new();
            for rule in RULE_DEFS {
                builder = builder.with_policy(*rule);
            }
            builder.or()
        })
        .as_ref()
}

/// 正则缓存：所有模式在首次使用时一次性编译（编译失败 fail-fast）
fn compiled_regex(pattern: &'static str) -> &'static regex::Regex {
    static REGEXES: OnceLock<HashMap<&'static str, regex::Regex>> = OnceLock::new();
    REGEXES
        .get_or_init(|| {
            let mut map = HashMap::new();
            for rule in RULE_DEFS {
                if let CommandMatcher::Regex(patterns) = rule.matcher {
                    for p in patterns {
                        map.insert(
                            *p,
                            regex::Regex::new(p).unwrap_or_else(|e| {
                                panic!("invalid shell policy regex {p:?}: {e}")
                            }),
                        );
                    }
                }
            }
            map
        })
        .get(pattern)
        .expect("pattern must be precompiled from RULE_DEFS")
}

/// 子命令粗粒度匹配：语句级扫描 bin 名，命中后向后扫描至首个 flag 或语句边界
fn subcommand_matches(command: &str, bin: &str, subs: &[&str]) -> bool {
    let tokens: Vec<&str> = command
        .split_whitespace()
        .map(|t| t.trim_matches(|c| c == '"' || c == '\''))
        .collect();
    for (i, tok) in tokens.iter().enumerate() {
        let name = tok
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(tok)
            .trim_end_matches(".exe");
        if name != bin {
            continue;
        }
        for next in &tokens[i + 1..] {
            if next.starts_with('-') || *next == "&&" || *next == ";" || *next == "||" {
                break;
            }
            if subs.contains(next) {
                return true;
            }
        }
    }
    false
}

/// 适配层输入（shell_exec 从 ctx/config 解析后填充）
pub struct ShellPolicyInput<'a> {
    /// 原始命令串
    pub command: &'a str,
    /// 解析后的绝对工作目录
    pub working_dir: &'a Path,
    /// base_data_path
    pub base_root: &'a str,
    /// 附加路径白名单
    pub additional_allowed_paths: &'a [String],
    /// 调用者用户身份
    pub user_id: Option<&'a str>,
    /// 调用者 Agent 身份
    pub agent_id: Option<&'a str>,
}

/// 管线裁决结果
#[derive(Debug)]
pub struct ShellPolicyVerdict {
    /// 阻断动作（Deny/Confirm）：命中即短路，不执行命令
    pub blocking: Option<PolicyAction>,
    /// 阻断命中的规则 id（授权白名单按 rule_id 判定；None=未阻断）
    pub blocking_rule: Option<&'static str>,
    /// 放行场景下命中的审计规则（rule_id, reason）
    pub audits: Vec<(&'static str, &'static str)>,
}

/// 拦截层入口：构造 Metrics → 引擎评估 → 汇总阻断与审计
pub fn evaluate(input: ShellPolicyInput<'_>) -> ShellPolicyVerdict {
    let mut metrics = Metrics::new()
        .with(keys::COMMAND, input.command)
        .with(
            keys::WORKING_DIR,
            input.working_dir.to_string_lossy().into_owned(),
        )
        .with(keys::BASE_ROOT, input.base_root)
        .with(
            keys::ADDITIONAL_ALLOWED_PATHS,
            input.additional_allowed_paths.to_vec(),
        );
    if let Some(uid) = input.user_id {
        metrics = metrics.with(keys::USER_ID, uid);
    }
    if let Some(aid) = input.agent_id {
        metrics = metrics.with(keys::AGENT_ID, aid);
    }

    // 引擎 Or 组：按规则表声明顺序上浮首个 Some（阻断规则在前 → 阻断优先）
    let group_action = ruleset().action(&metrics);
    let blocking = group_action.filter(PolicyAction::is_blocking);

    let audits = if blocking.is_some() {
        Vec::new()
    } else {
        let hit_ids = ruleset().evaluate(&metrics);
        RULE_DEFS
            .iter()
            .filter(|r| r.action == RuleAction::Audit && hit_ids.iter().any(|id| id == r.id))
            .map(|r| (r.id, r.reason))
            .collect()
    };

    // 阻断规则 id：按声明序（=引擎 Or 优先级）找首个命中的阻断级规则
    let blocking_rule = if blocking.is_some() {
        RULE_DEFS
            .iter()
            .find(|r| {
                matches!(r.action, RuleAction::Deny | RuleAction::Confirm) && r.matches(&metrics)
            })
            .map(|r| r.id)
    } else {
        None
    };

    ShellPolicyVerdict {
        blocking,
        blocking_rule,
        audits,
    }
}

/// 按规则 id 查规则定义（授权融合层取幂等属性等）
pub fn rule_def(id: &str) -> Option<&'static ShellRulePolicy> {
    RULE_DEFS.iter().find(|r| r.id == id)
}
#[cfg(test)]
#[path = "shell_policy_tests.rs"]
mod tests;
