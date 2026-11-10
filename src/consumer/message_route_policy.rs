//! Final 自动回发路由策略（策略引擎的消息路由领域落地）
//!
//! 复用 `pkg/policy` 引擎组合路由策略组（照 shell_policy 先例）：
//! - 静态判定段（零查库）：领域匹配器规则（哨兵 / 来源角色 / 自触发 /
//!   知会类型）按 Or 组合，声明顺序即判定优先级（首个命中即产出路由）
//! - 协作压力段：`pkg/collaboration_pressure` 组件的策略引擎裁决档位，
//!   仅 `PressureAction::Escalate` 收口——把 Final 作为知会投给来源方
//!   （最后一轮信息照常送达，对端不会再自动回发）并通知归属用户，
//!   其余档位仍 Peer；压力带内的施压提示词在唤醒前由该组件注入 `ThinkingOptions`。
//!   仅对 Agent 来源消息执行（`ReplyObligation::Some`）：`Expected` 用链事实做
//!   施压与硬墙裁决，`NotifyOnly` 只用它识别「上游触顶收口的那一跳」；
//!   用户/系统来源、自触发零额外查询
//!
//! # 回复决策权归 Agent，Final 是唯一权威
//!
//! 框架不根据「本轮是否调用过发送工具」反推该不该回——那是**副作用推断**，
//! 既覆盖不了「发给了别人」「发了但对方没收到」，也会把「Agent 已经交代过」
//! 与「还欠一句答复」混为一谈。判定权在 Agent：思考中自决，Final 只有两种
//! 合法形态（[`ReplyDecision`]）——正文即答复，或恰好输出 [`NO_REPLY_SENTINEL`]。
//! 框架的职责是把「本条要不要回」与「已经来回多少轮」写进提示词
//! （[`ReplyObligation::prompt_lines`]），再按 Final 确定性执行。
//!
//! 领域特有判定保留专用匹配器（不具备跨领域共性，抽象边界参照 builtin 模块文档）：
//! - 来源角色 / 知会类型是枚举等值（Metrics 侧以数值承载）
//! - 自触发是双键等值（`from_id == agent_id`），通用 FieldEqualsPolicy 只支持
//!   「单键 == 常量」，跨键比较属路由特有形态

use std::sync::OnceLock;

use common::enums::{MessageRole, MessageType};

use crate::pkg::collaboration_pressure::{ChainPressure, PressureAction};
use crate::pkg::policy::{Metrics, Policy, PolicyBuilder};

// 历史注记：哨兵曾由 builtin `FieldEqualsPolicy`（单键 == 单常量）承载。
// 2026-10-18 起改为领域 matcher `RouteMatcher::NoReplySentinel`——需要一次
// 比对多个等价写法 + 归一化，属路由特有形态（同 `SelfTrigger` 的双键比较）。

/// Metrics 键约定（入口统一填充）
pub mod keys {
    /// 消息来源角色（`MessageRole::to_i32` 数值）
    pub const FROM_ROLE: &str = "route.from_role";
    /// 来源 id（用户 id / Agent id / 触发器标识）
    pub const FROM_ID: &str = "route.from_id";
    /// 本次被唤醒的 Agent id（自触发判定的对侧键）
    pub const AGENT_ID: &str = "route.agent_id";
    /// 消息类型（`MessageType::to_i32` 数值）
    pub const MESSAGE_TYPE: &str = "route.message_type";
    /// 唤醒 Final 原始输出（trim 后）
    pub const RAW_OUTPUT: &str = "route.raw_output";
}

/// 无需回复哨兵：Agent 判断「后续工作与来源方无关」时，让 Final 恰好输出该词，
/// Framework 检测到后不再把 Final 回发给任何对象。保留协议字，路由判定中
/// 优先级高于一切来源角色规则（见 `static_ruleset`）——任务派发经用户身份
/// 中继后 from_role=User，若让 `user_origin` 先命中会把哨兵原文发给用户
pub const NO_REPLY_SENTINEL: &str = "NO_REPLY";

/// 哨兵的等价写法（**封闭集合**，判定见 [`is_no_reply_sentinel`]）
///
/// 除英文主哨兵外，收录模型自作主张写出的的表达。2026-10-18 实证：前台
/// Agent 把同一个意图写成「未回复」（全库 grep / git 历史 / seeds / soul
/// 均无该字面量 ⇒ 只能来自模型），英文全等不命中，该输出被当作真实回复
/// 投给财务总监，引发两轮无意义往返（一次确认 + 一次澄清）。
/// prompt 只能降低发生率，**确定性防线必须在路由层**。
pub const NO_REPLY_SENTINEL_VARIANTS: &[&str] = &[
    NO_REPLY_SENTINEL,
    "未回复",
    "无需回复",
    "无须回复",
    "勿需回复",
    "不必回复",
    "不需要回复",
    "无需跟进",
    "无回复",
    "不回复",
];

/// 哨兵判定时从首尾剥离的字符：模型常给整段输出套句号 / 引号 / markdown 强调
const SENTINEL_TRIM_CHARS: &[char] = &[
    '.', '。', '!', '！', ',', '，', ';', '；', '"', '\'', '`', '*',
];

/// Final 是否命中无需回复哨兵
///
/// 一律「归一化后**全等**」，绝不做包含判断——正文中提到「无需回复」是合法内容
/// （如「这条消息无需回复，但我还有个问题…」），只有独占全文才算哨兵。
/// 归一化两步：① trim；② 剥离收尾标点/引号；③ 纯 ASCII 时统一大写（容忍
/// `no_reply` / `No_Reply`）。
pub fn is_no_reply_sentinel(raw_output: &str) -> bool {
    let normalized = raw_output
        .trim()
        .trim_matches(|c| SENTINEL_TRIM_CHARS.contains(&c));
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return false;
    }
    if NO_REPLY_SENTINEL_VARIANTS.contains(&normalized) {
        return true;
    }
    normalized.is_ascii() && normalized.to_ascii_uppercase() == NO_REPLY_SENTINEL
}

/// 本轮消息对「是否需要回复」的要求（由入口消息语义决定，不参与路由判定）
///
/// 只回答「这条消息在不在乎答复」，**不看本轮调用过哪些工具**：是否回复是
/// Agent 在思考中自决的结果，框架只负责把要求与证据摆到决策点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyObligation {
    /// 对端在等答复（跨 Agent 对等协作 / 任务交接）
    Expected,
    /// 知会类：发送方已声明无需回复（`MessageType::AgentNotify`）
    NotifyOnly,
}

impl ReplyObligation {
    /// 从入口消息推导；`None` = 不需要回复仲裁提示
    ///
    /// 只对 Agent 来源且非自触发的消息生效：用户/系统来源由来源规则决定
    /// 投递方向（用户必须收到 Final），自触发回给自己则本来就是 Discard。
    pub fn of(
        from_role: MessageRole,
        from_id: &str,
        agent_id: &str,
        message_type: MessageType,
    ) -> Option<Self> {
        if from_role != MessageRole::Agent || from_id == agent_id {
            return None;
        }
        Some(if message_type == MessageType::AgentNotify {
            Self::NotifyOnly
        } else {
            Self::Expected
        })
    }

    /// 注入提示词的回复仲裁区块（与 [`NO_REPLY_SENTINEL`] 的协议成对维护）
    ///
    /// ⚠️ 文案必须与路由行为**逐条对应**：这里承诺「Final 正文即答复」，
    /// 路由就必须把正文回发给来源方；这里承诺「NO_REPLY 不回发」，路由
    /// 就必须在哨兵分支 Discard。改任何一侧都要同步另一侧。
    pub fn prompt_lines(self) -> Vec<String> {
        match self {
            ReplyObligation::Expected => vec![
                "【回复仲裁】本条来自另一个 Agent，在等你答复；是否回复由你决定。".to_string(),
                "- 回复：思考完成后由 Final 统一答复，正文即答复全文——不要再用发送消息工具重复投递同一份内容。"
                    .to_string(),
                "- 不回复：Final 恰好只输出 NO_REPLY（独占全文，不加引号、标点或解释），框架不会回发。"
                    .to_string(),
                "- 判据：有结论/交付物、需要对方决策或补充信息 → 回复；只是确认收到、复述上一轮、已完成且无补充、后续与对方无关 → NO_REPLY。"
                    .to_string(),
            ],
            ReplyObligation::NotifyOnly => vec![
                "【回复仲裁】本条是知会消息（发送方已声明无需回复），框架不会回发你的 Final。"
                    .to_string(),
                "- 默认不回复：Final 恰好只输出 NO_REPLY 收尾。".to_string(),
                "- 确需回应：用发送消息工具主动发给来源方，不要依赖 Final。".to_string(),
            ],
        }
    }
}

/// Agent 对「本轮是否回复来源方」的自决结果（Final 的唯一权威形态）
///
/// 只有两个取值：正文即答复，或恰好输出哨兵。解析是**归一化后全等**的
/// 封闭词表（见 [`is_no_reply_sentinel`]），正文里提到「无需回复」仍是正常
/// 答复；任何无法判定的输出都按 `Reply` 处理（宁可多发，也绝不吞答复）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyDecision {
    /// 回复：Final 正文投递给来源方
    Reply,
    /// 不回复：Final 是哨兵，丢弃
    NoReply,
}

/// 解析 Final 的回复决策（路由层与日志共用同一判定，避免两处口径漂移）
pub fn classify_reply_decision(raw_output: &str) -> ReplyDecision {
    if is_no_reply_sentinel(raw_output) {
        ReplyDecision::NoReply
    } else {
        ReplyDecision::Reply
    }
}

/// Final 自动回发的路由决策
///
/// `handle_agent_message` 唤醒完成后按此决定 Final 输出的去向：
/// - `Peer`                → 回发给来源方（用户或对端 Agent）
/// - `Discard`             → 丢弃 Final（无对等回复对象 / 发送方声明无需回复）
/// - `CloseChainAsNotify`  → 硬墙收口：把 Final 作为**知会**投给来源方（信息不丢）
///   并通知归属用户；知会语义保证对端不会再自动回发，链在此终止
///
/// 硬墙不采用「直接丢弃」：那样既不通知归属方，也让最后一轮结论白白消失；
/// 也不采用「照常回发」，那只会把乒乓推高一跳。收口成知会是唯一既完成
/// 「最后一轮信息传递」又必然终止链条的形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoReplyRoute {
    Peer,
    Discard { reason: &'static str },
    CloseChainAsNotify { reason: &'static str },
}

/// 静态路由判定的输入（consumer 从 Message / 唤醒结果解析后填充）
pub struct RouteInput<'a> {
    /// 消息来源角色
    pub from_role: MessageRole,
    /// 来源 id（用户 id / Agent id / 触发器标识）
    pub from_id: &'a str,
    /// 本次被唤醒的 Agent id
    pub agent_id: &'a str,
    /// 消息类型
    pub message_type: MessageType,
    /// 唤醒 Final 原始输出（trim 后）
    pub raw_output: &'a str,
}

/// 路由匹配器（路由领域特有的判定形态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteMatcher {
    /// Final 命中无需回复哨兵（归一化后全等词表，见 `is_no_reply_sentinel`）
    NoReplySentinel,
    /// 来源角色等值（用户 / 系统来源的粗分类）
    RoleEquals(MessageRole),
    /// 双键等值：`from_id == agent_id`（自触发守卫）
    SelfTrigger,
    /// 消息类型等值（知会声明）
    MessageTypeEquals(MessageType),
}

impl RouteMatcher {
    fn matches(&self, metrics: &Metrics) -> bool {
        match self {
            RouteMatcher::NoReplySentinel => metrics
                .get_str(keys::RAW_OUTPUT)
                .is_some_and(is_no_reply_sentinel),
            RouteMatcher::RoleEquals(role) => {
                metrics.get_u64(keys::FROM_ROLE) == Some(role.to_i32() as u64)
            }
            RouteMatcher::SelfTrigger => {
                metrics.get_str(keys::FROM_ID) == metrics.get_str(keys::AGENT_ID)
            }
            RouteMatcher::MessageTypeEquals(message_type) => {
                metrics.get_u64(keys::MESSAGE_TYPE) == Some(message_type.to_i32() as u64)
            }
        }
    }
}

/// 静态路由规则（通用 `Policy` 的消息路由领域实现）
#[derive(Debug, Clone, Copy)]
struct StaticRoutePolicy {
    id: &'static str,
    name: &'static str,
    condition_desc: &'static str,
    matcher: RouteMatcher,
    /// 命中产出：路由决策由规则表声明式携带（不走引擎 PolicyAction——
    /// Peer/Discard/Escalate 与 Deny/Confirm/Audit 是两套语义）
    outcome: AutoReplyRoute,
}

impl Policy for StaticRoutePolicy {
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
            RouteMatcher::NoReplySentinel => vec![keys::RAW_OUTPUT.to_string()],
            RouteMatcher::RoleEquals(_) => vec![keys::FROM_ROLE.to_string()],
            RouteMatcher::SelfTrigger => {
                vec![keys::FROM_ID.to_string(), keys::AGENT_ID.to_string()]
            }
            RouteMatcher::MessageTypeEquals(_) => vec![keys::MESSAGE_TYPE.to_string()],
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

/// 静态规则表（声明顺序 = 判定优先级：首个命中即产出路由）
static STATIC_ROUTE_DEFS: &[StaticRoutePolicy] = &[
    // 无需回复哨兵 → Discard：保留字，**恒声明在最前**（详见 `static_ruleset`）
    StaticRoutePolicy {
        id: SENTINEL_POLICY_ID,
        name: "无需回复哨兵",
        condition_desc: "Final 全等 NO_REPLY 或其等价写法",
        matcher: RouteMatcher::NoReplySentinel,
        outcome: AutoReplyRoute::Discard {
            reason: SENTINEL_HIT_REASON,
        },
    },
    // 用户来源 → Peer：用户身份中继后 Final 自然回到该用户
    StaticRoutePolicy {
        id: "user_origin",
        name: "用户来源",
        condition_desc: "from_role == User",
        matcher: RouteMatcher::RoleEquals(MessageRole::User),
        outcome: AutoReplyRoute::Peer,
    },
    // 系统来源 → Discard：触发器消息（纯系统指令、无归属用户的 A2A 触达）没有对等回复对象
    StaticRoutePolicy {
        id: "system_origin",
        name: "系统来源",
        condition_desc: "from_role == System",
        matcher: RouteMatcher::RoleEquals(MessageRole::System),
        outcome: AutoReplyRoute::Discard {
            reason: "system-originated message has no peer reply target",
        },
    },
    // 自触发 → Discard：触发器以 Owner Agent 名义发给自己的消息，回给自己会无限自唤醒循环
    StaticRoutePolicy {
        id: "self_trigger",
        name: "自触发守卫",
        condition_desc: "from_id == agent_id",
        matcher: RouteMatcher::SelfTrigger,
        outcome: AutoReplyRoute::Discard {
            reason: "agent self-triggered message (from == to)",
        },
    },
    // 知会 → Discard：发送方已声明无需回复。仅 Agent 来源会命中该规则——
    // 用户来源已被声明在前的 user_origin 拦下，用户对话主干不受影响
    StaticRoutePolicy {
        id: "agent_notify",
        name: "知会消息",
        condition_desc: "message_type == AgentNotify",
        matcher: RouteMatcher::MessageTypeEquals(MessageType::AgentNotify),
        outcome: AutoReplyRoute::Discard {
            reason: "agent-notify message (sender declared no reply needed)",
        },
    },
];

/// 通用哨兵策略 id（领域规则表成员，见下条注释）
const SENTINEL_POLICY_ID: &str = "no_reply_sentinel";
/// 哨兵命中的路由产出 reason
const SENTINEL_HIT_REASON: &str = "agent final output is the NO_REPLY sentinel";

/// 静态规则集：领域规则表按 Or 组合
/// （等价 policy_set!(OR { .. }) 的展开结果，声明顺序即优先级）
///
/// **哨兵必须声明在最前**（`STATIC_ROUTE_DEFS[0]`）：哨兵是保留协议字
/// （归一化后全等才命中），任何来源的 Final 命中即 Discard。若排在
/// `user_origin` 之后，任务派发经「用户身份中继」后 from_role=User，
/// 会被 Peer 短路，把哨兵原文当回复发给用户——2026-10-16 实证：全库 29 条
/// 泄漏、25+ 条直达用户。保留字不存在「合法正文」歧义，优先级高于一切
/// 来源角色规则。
fn static_ruleset() -> &'static dyn Policy {
    static RULESET: OnceLock<Box<dyn Policy>> = OnceLock::new();
    RULESET
        .get_or_init(|| {
            let mut builder = PolicyBuilder::new();
            for rule in STATIC_ROUTE_DEFS {
                builder = builder.with_policy(*rule);
            }
            builder.or()
        })
        .as_ref()
}

/// 静态命中 id → 路由产出：全部内嵌在规则表（含哨兵），由 Or 组的首个命中
/// 上浮。fallback 仅作防御——表新增成员忘了写 outcome 时，退化为「不投递」
/// 而非「发给来源方」，失败的永远是噪声最少的一侧
fn static_outcome(policy_id: &str) -> AutoReplyRoute {
    STATIC_ROUTE_DEFS
        .iter()
        .find(|rule| rule.id == policy_id)
        .map(|rule| rule.outcome)
        .unwrap_or(AutoReplyRoute::Discard {
            reason: SENTINEL_HIT_REASON,
        })
}

/// Final 回发路由的静态判定段（无需查库）
///
/// **单一扩展点**：所有「没有对等回复对象 / 回复会造成自唤醒 / 发送方声明无需
/// 回复」的场景都在静态规则表里声明，新增场景只加一条规则。返回 `None` 表示
/// 「跨 Agent 对等回复候选」，需继续做回复链深度机械判定
/// （`judge_chain_reply_route`）。
///
/// | 场景 | 规则 | 路由 | 理由 |
/// |------|------|------|------|
/// | 无需回复哨兵（**任何来源，优先级最高**） | `no_reply_sentinel`（领域 matcher） | Discard | Agent 自判后续工作与来源方无关，回发只会制造乒乓；判定是归一化后全等的**封闭词表**（含模型自造的「未回复」等中文写法），保留字先于下表所有来源规则判定，用户身份中继的任务派发同样生效 |
/// | 用户消息 | `user_origin` | Peer | 用户身份中继后 Final 自然回到该用户 |
/// | System 来源 | `system_origin` | Discard | 触发器消息（纯系统指令、无归属用户的 A2A 触达）没有对等回复对象 |
/// | Agent 自触发 | `self_trigger` | Discard | 触发器以 Owner Agent 名义发给自己的消息，回给自己会无限自唤醒循环 |
/// | 知会消息 | `agent_notify` | Discard | 发送方已声明无需回复；仅 Agent 来源生效，用户对话主干不受影响 |
pub fn judge_static_reply_route(input: RouteInput<'_>) -> Option<AutoReplyRoute> {
    let metrics = Metrics::new()
        .with(keys::FROM_ROLE, input.from_role.to_i32() as u64)
        .with(keys::FROM_ID, input.from_id)
        .with(keys::AGENT_ID, input.agent_id)
        .with(keys::MESSAGE_TYPE, input.message_type.to_i32() as u64)
        .with(keys::RAW_OUTPUT, input.raw_output);

    // Or 组：按声明顺序上浮首个命中（哨兵在前，领域规则按声明序兜底）
    static_ruleset()
        .evaluate(&metrics)
        .first()
        .map(|id| static_outcome(id))
}

/// Final 回发路由的协作压力判定段（防乒乓最后防线）
///
/// 压力观测由 consumer 沿 `reply_to_id` 查库后委托组件完成
/// （见 `MessageConsumer::collect_chain_pressure`）。仅硬红线档位升级归属用户；
/// 压力带档位仍 Peer，但唤醒前已注入配套施压提示词，让模型主动收敛。
/// 出现工具结果 / 新参与方 / 外部输入时组件回血并重置连续窗口，
/// 因此「有新鲜感的协作」可以合法超过硬窗口，不再被固定次数一刀切。
pub fn judge_chain_reply_route(pressure: &ChainPressure) -> AutoReplyRoute {
    match pressure.judge().action {
        // 硬墙：不再「停下不说话」，而是把本轮 Final 收口成知会发出。
        PressureAction::Escalate => AutoReplyRoute::CloseChainAsNotify {
            reason: "collaboration pressure exhausted",
        },
        PressureAction::Continue | PressureAction::Nudge => AutoReplyRoute::Peer,
    }
}

/// 小脑判「无新信息 / 无需回复」时的收口理由
///
/// 与静态硬墙的 `"collaboration pressure exhausted"` 分开写：排障时要一眼看出
/// 「这条链是被谁收的」——是跳数撞墙，还是内容被判空。两个理由串共用同一条
/// `CloseChainAsNotify` 收口路径（消息照常作为知会送达 + 通知归属用户）。
pub const CEREBELLUM_CLOSE_REASON: &str = "cerebellum: no new information in latest message";

/// 小脑结论 → 路由（`None` = 不覆盖，调用方继续走静态判定）
///
/// 单独成函数而不是内联：这是「收口路径仍可用」与「放行不覆盖」两条断言的
/// 可测试点（见 `message_route_policy_tests`）。小脑只允许产出「收口」这一种
/// 路由，**不得**产出 `Peer` / `Discard` —— 那会把静态规则与压力档位的既有
/// 结论改掉，超出「判内容」这一件事的边界。
pub fn cerebellum_close_route(close: bool) -> Option<AutoReplyRoute> {
    close.then_some(AutoReplyRoute::CloseChainAsNotify {
        reason: CEREBELLUM_CLOSE_REASON,
    })
}

#[cfg(test)]
#[path = "message_route_policy_tests.rs"]
mod tests;
