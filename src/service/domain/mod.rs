//! Domain 层（业务逻辑层）
//!
//! 分类存放不同业务领域：
//! - hr → 人力资源（智能体管理）
//! - finance → 财务管理（模型提供商管理）
//! - organization → 组织管理（组织和用户管理）
//! - message → 消息领域（消息投递和管理）
//! - tool → 工具领域（工具管理和执行）


pub mod hr;
pub mod finance;
pub mod organization;
pub mod message;
pub mod tool;

// Tests are located in subdirectories: finance/model_provider_test.rs and hr/agent_test.rs
// No need to declare them here because mod rs already declared in subdirectories


/// 初始化所有 Domain
pub fn init_all() {
    hr::init();
    finance::init();
    organization::init();
    message::init();
    tool::instance();
}
