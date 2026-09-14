//! 邮箱机器人集成 handlers（finance domain：身份凭证资产）
//!
//! 路由统一挂 `/api/v1/finance/identity/email/`：
//! 用户自建代理邮箱（platform = 邮箱提供商 qq / 163 / …）的凭证 CRUD
//! + 默认凭证（按 provider 隔离槽位）+ 集成状态聚合（按 provider 过滤）。

pub mod create_credential;
pub mod delete_credential;
pub mod get_status;
pub mod set_default_credential;
pub mod update_credential;
