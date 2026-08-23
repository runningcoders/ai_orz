//! 通用 API Token 集成 handlers（finance domain：身份凭证资产）
//!
//! 路由统一挂 `/api/v1/finance/identity/generic-token/`：
//! 单字段 API Key 类平台（tavily / doubao_search / 未来任意平台）的凭证 CRUD
//! + 默认凭证（按 platform 隔离槽位）+ 集成状态聚合（按 platform 过滤）。

pub mod create_credential;
pub mod delete_credential;
pub mod get_status;
pub mod set_default_credential;
pub mod update_credential;
