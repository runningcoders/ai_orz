//! 组织邀请码管理 HTTP 接口（管理员）
//!
//! 邀请码是组织级注册凭证：管理员在此签发/轮换，普通用户在登录页凭码自助注册。
//! 仅 `generate_http_handler`（不注册 Agent 工具），路由层以
//! `require_role_middleware(Admin)` 做门控。

pub mod get_invite_code;
pub mod regenerate_invite_code;

pub use get_invite_code::get_invite_code_handler;
pub use regenerate_invite_code::regenerate_invite_code_handler;
