//! HR (Human Resources) Domain 模块
//!
//! 人力资源模块，管理：
//! - Agent - AI 智能体
//! - Employee - 人类员工

pub mod agent;

pub use self::agent::{domain, init, HrDomain};
