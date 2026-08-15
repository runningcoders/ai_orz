//! GitHub 集成 handlers（finance domain：身份凭证资产）
//!
//! 路由统一挂 `/api/v1/finance/identity/github/`：
//! 凭证 CRUD + 默认凭证 + 集成状态聚合（凭证快照 + gh 登录态实测）。

pub mod create_credential;
pub mod delete_credential;
pub mod get_status;
pub mod set_default_credential;
pub mod update_credential;
