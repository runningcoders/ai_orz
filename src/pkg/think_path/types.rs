//! 思维链路图引擎 · 核心标识与信号
//!
//! 本文件只放**类型与枚举**，不放任何行为。
//!
//! 设计要点：
//! - `PhaseId` 是**标签**不是顺序：它只约束同类节点的契约模板与可视化分组，
//!   不构成执行顺序（顺序由仲裁器按总线状态运行时决定）。
//! - `Stimulus` 是 run 的构造参数（零 I/O），**不在** `ContextKey` 体系内。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 节点定义标识（蓝图内唯一）
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// 阶段标签：只做契约模板与可视化分组，**不构成执行顺序**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PhaseId {
    #[default]
    Stimulus,
    Reflex,
    Triage,
    Prepare,
    Deliberate,
    Consolidate,
    Review,
}

impl PhaseId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stimulus => "stimulus",
            Self::Reflex => "reflex",
            Self::Triage => "triage",
            Self::Prepare => "prepare",
            Self::Deliberate => "deliberate",
            Self::Consolidate => "consolidate",
            Self::Review => "review",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "stimulus" => Some(Self::Stimulus),
            "reflex" => Some(Self::Reflex),
            "triage" => Some(Self::Triage),
            "prepare" => Some(Self::Prepare),
            "deliberate" => Some(Self::Deliberate),
            "consolidate" => Some(Self::Consolidate),
            "review" => Some(Self::Review),
            _ => None,
        }
    }

    pub const ALL: [PhaseId; 7] = [
        Self::Stimulus,
        Self::Reflex,
        Self::Triage,
        Self::Prepare,
        Self::Deliberate,
        Self::Consolidate,
        Self::Review,
    ];
}

/// 数据总线分区键（八分区，防总线变成上帝对象）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContextKey {
    Memory,
    Tools,
    Skills,
    Runtime,
    Business,
    Prompt,
    Budget,
    Artifacts,
}

impl ContextKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Tools => "tools",
            Self::Skills => "skills",
            Self::Runtime => "runtime",
            Self::Business => "business",
            Self::Prompt => "prompt",
            Self::Budget => "budget",
            Self::Artifacts => "artifacts",
        }
    }

    pub const ALL: [ContextKey; 8] = [
        Self::Memory,
        Self::Tools,
        Self::Skills,
        Self::Runtime,
        Self::Business,
        Self::Prompt,
        Self::Budget,
        Self::Artifacts,
    ];

    /// 反序列化 / 策略算子键解析用；未知返回 `None`
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|key| key.as_str() == s)
    }
}

/// 加载深度：`Shallow < Full`
///
/// 总线记住每个 key 已达成的深度，**已达成 ≥ 请求档才跳过**；
/// 只有向更深档请求才真正重新加载。这让「反射需要一点上下文」与
/// 「反射必须廉价」不必二选一。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum LoadDepth {
    #[default]
    Shallow,
    Full,
}

/// 取数失败语义
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadFailureMode {
    /// 可降级：记 warn，填 `Value::Null` 继续
    #[default]
    Degrade,
    /// 硬失败：中断整个 run
    HardFail,
}

/// 刺激：run 的构造参数，由入站消息直接携带，零 I/O、零装配
///
/// 它**不是** `ContextKey`——反射弧的「廉价」来自不做 I/O 取数，
/// 不是「什么都不看」：它读的就是 `stimulus`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stimulus {
    /// 渠道来源（如 `sse` / `lark` / `ilink`）
    pub source: String,
    pub text: String,
    pub message_id: Option<String>,
}

/// 信号标签
pub type SignalTag = String;

pub const SIGNAL_DONE: &str = "done";
pub const SIGNAL_FAIL: &str = "fail";

/// 节点产出：只说明「我这边发生了什么」，**不知道下一跳是谁**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeSignal {
    Done,
    Yield(SignalTag),
    Fail(String),
}

impl NodeSignal {
    /// 统一为可匹配的字符串标签，`Fail` 归一为 `fail`
    pub fn tag(&self) -> &str {
        match self {
            Self::Done => SIGNAL_DONE,
            Self::Yield(t) => t.as_str(),
            Self::Fail(_) => SIGNAL_FAIL,
        }
    }
}
