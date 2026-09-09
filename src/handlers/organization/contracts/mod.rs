//! 联邦合约（federation / contracts）HTTP 接口
//!
//! 用户侧管理员接口（S3 合约授权），统一前缀 `/api/v1/organization/contracts/*`。
//! 双宏标注规范：仅 `generate_http_handler`，**不注册 `register_handler_tool`**
//! （非 Agent 工具，防 Agent 误触组网，评审稿 §4.2）。

pub mod list_contracts;
pub mod terminate_contract;
pub mod update_contract_capabilities;

pub use list_contracts::list_contracts_handler;
pub use terminate_contract::terminate_contract_handler;
pub use update_contract_capabilities::update_contract_capabilities_handler;
