//! 飞书集成 handlers（finance domain：身份凭证资产 + 授权/绑定）
//!
//! 路由统一挂 `/api/v1/finance/identity/lark/`：
//! 凭证 CRUD + 用户 OAuth device flow + 绑定快照聚合 + config init --new 自动绑定。

pub mod auth_complete;
pub mod auth_logout;
pub mod auth_start;
pub mod auth_status;
pub mod bind_cancel;
pub mod bind_start;
pub mod bind_status;
pub mod create_credential;
pub mod delete_credential;
pub mod get_status;
pub mod set_default_credential;
pub mod update_credential;
