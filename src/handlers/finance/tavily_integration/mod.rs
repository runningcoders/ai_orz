//! Tavily 集成 handlers（finance domain：身份凭证资产）
//!
//! 路由统一挂 `/api/v1/finance/identity/tavily/`：
//! 凭证 CRUD + 默认凭证 + 集成状态聚合（凭证快照 + 共享 key 配置状态）。

pub mod create_credential;
pub mod delete_credential;
pub mod get_status;
pub mod set_default_credential;
pub mod update_credential;
