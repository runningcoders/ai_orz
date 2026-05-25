//! Domain 层（业务逻辑层）
//!
//! 分类存放不同业务领域：
//! - hr → 人力资源（智能体管理）
//! - finance → 财务管理（模型提供商管理）
//! - organization → 组织管理（组织和用户管理）
//! - message → 消息领域（消息投递和管理）
//! - runtime → 运行时领域（工具执行等运行时逻辑）
//! - project → 项目领域（项目管理和执行）


pub mod hr;
pub mod finance;
pub mod organization;
pub mod message;
pub mod runtime;
pub mod project;

// Tests are located in subdirectories
// No need to declare them here because mod rs already declared in subdirectories


/// 初始化所有 Domain
pub fn init_all() {
    hr::init();
    finance::init();
    organization::init();
    message::init();
    runtime::init();
    project::init();
}
