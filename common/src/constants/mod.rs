//! 常量定义模块
//!
//! 分类存放不同用途的常量和类型：
//! - agent_roles: Agent 预设角色标签（开放 roles 列表的系统侧常量）
//! - http_header: HTTP 请求头常量
//! - message: 消息相关常量（默认会话哨兵 project id）
//! - sentinel: 通用「清除字段」哨兵（写侧部分更新协议）
//! - settle: 记忆沉淀队列护栏（运行时域与系统域共用的默认值）
//! - utils: 公共工具函数
//!
//! All enums have been moved to `common::enums::*` grouped by business domain

pub mod agent_roles;
pub mod http_header;
pub mod message;
pub mod sentinel;
pub mod settle;
pub mod utils;
