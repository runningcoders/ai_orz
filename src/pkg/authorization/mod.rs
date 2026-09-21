//! 授权域纯值加工模块（设计：方案 artifact 01a0c2bc-514a v5 §15.2/§15.6；蓝本：pkg/credential）
//!
//! 纯值加工模块：授权单领域模型 / 六态枚举 / 授权策略快照 / 命令规范化·签名·前缀匹配·
//! 快照裁决纯函数 / 规则幂等属性与 ttl 约定。
//! 零数据访问——授权单存储由 service 编排层（domain，内存授权存储）持有，
//! 数据经参数流入消费方；本模块不持有 ctx、不定义数据端口、无注入注册、
//! 零锁类型、零内部可变性、零 IO、对 crate::service 零引用（验收 grep 项）。
//!
//! 不隶属 tool_registry / policy（依赖方向单向）：授权裁决是授权域通用能力，
//! 未来非工具消费方可直接引用；pkg/policy 定位「通用判断引擎不感知业务语义」，
//! 授权单六态生命周期语义不进入 policy（方案 §15.2 定案）。

mod model;
mod signing;
mod verdict;

pub use model::*;
pub use signing::*;
pub use verdict::*;
