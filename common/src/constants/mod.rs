//! 常量定义模块
//!
//! 分类存放不同用途的常量和类型：
//! - agent_roles: Agent 预设角色标签（开放 roles 列表的系统侧常量）
//! - http_header: HTTP 请求头常量
//! - utils: 公共工具函数
//!
//! All enums have been moved to `common::enums::*` grouped by business domain

pub mod agent_roles;
pub mod http_header;
pub mod utils;
