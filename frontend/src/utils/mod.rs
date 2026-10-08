//! 通用工具函数 - 按功能分模块组织
//!
//! - `time`: 时间格式化
//! - `file`: 文件大小格式化
//! - `message`: 消息类型常量、角色映射、乐观消息辅助
//! - `status`: 任务/项目状态映射
//! - `doc_link`: Markdown 渲染期站内链接后处理（data-repo-href 预拼）
//! - `mention`: 消息 @ 提及文本协议（`[@名](agent:id)`）解析与 chip 渲染
//! - `number`: 数值紧凑格式化（大数进位 k/M、定点小数去尾零）
//! - `local_store`: 通用 localStorage 组件层（统一前缀 / 类型化读写 / 版本兼容 / 结构体集中）

pub mod avatar;
pub mod doc_link;
pub mod file;
pub mod local_store;
pub mod mention;
pub mod message;
pub mod number;
pub mod status;
pub mod time;

// 重新导出所有公共 API，保持向后兼容（use crate::utils::xxx 不变）
pub use avatar::*;
pub use file::*;
pub use message::*;
pub use status::*;
pub use time::*;

use web_sys::window;

/// 获取 localStorage
pub fn local_storage() -> Option<web_sys::Storage> {
    window()?.local_storage().ok()?
}

/// 当前页面路径（不含 origin / query / hash）；取不到时返回空串
pub fn current_path() -> String {
    window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default()
}

/// 是否正停在接待页（`/login`）
///
/// 401 兜底跳转的目标就是这一页。鉴权失效的兜底逻辑若在**该页自己**发起，
/// 再写入一次 `location` 会把页面整个刷新一遍，于是「刷新 → 自检 → 401 → 刷新」
/// 闭成死循环（接待页自检 `/user/me`、App 根组件的登录态探活都跑在这一页）。
/// 需要整页跳转的兜底点一律先过这个门闩。
pub fn is_reception_page() -> bool {
    let path = current_path();
    path == "/login" || path == "/login/"
}
