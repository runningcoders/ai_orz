//! 组织组网（federation / links）HTTP 接口
//!
//! 共 7 个端点（用户侧 4 + 机器侧 3），统一前缀 `/api/v1/organization/links/*`。
//! 双宏标注规范：仅 `generate_http_handler`，**不注册 `register_handler_tool`**
//! （非 Agent 工具，防 Agent 误触组网，评审稿 §4.2）。

pub mod issue_pairing_code;
pub mod verify_pairing_code;

pub use issue_pairing_code::issue_pairing_code_handler;
pub use verify_pairing_code::verify_pairing_code_handler;
