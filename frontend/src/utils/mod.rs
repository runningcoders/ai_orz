//! 通用工具函数 - 按功能分模块组织
//!
//! - `time`: 时间格式化
//! - `file`: 文件大小格式化
//! - `message`: 消息类型常量、角色映射、乐观消息辅助
//! - `status`: 任务/项目状态映射

pub mod time;
pub mod file;
pub mod message;
pub mod status;

// 重新导出所有公共 API，保持向后兼容（use crate::utils::xxx 不变）
pub use time::*;
pub use file::*;
pub use message::*;
pub use status::*;

use web_sys::window;

/// 获取 localStorage
pub fn local_storage() -> Option<web_sys::Storage> {
    window()?.local_storage().ok()?
}
