//! Domain 层（业务逻辑层）
//!
//! 分类存放不同业务领域：
//! - hr → 人力资源（智能体管理）
//! - finance → 财务管理（模型提供商管理）
//! - organization → 组织管理（组织和用户管理）
//! - message → 消息领域（消息投递和管理）
//! - runtime → 运行时领域（工具执行等运行时逻辑）
//! - project → 项目领域（项目管理和执行）
//! - system → 系统领域（定时触发器等系统功能）

pub mod finance;
pub mod hr;
pub mod message;
pub mod organization;
pub mod project;
pub mod runtime;
pub mod system;

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
    system::init();
}

/// 第二阶段：异步初始化各 Domain 的基础数据（幂等写入 DB 的默认条目等）。
///
/// 与 `init_all()` 分离开的原因：基础数据通常需要 DB IO（必须 async），
/// 而 `init_all()` 里有大量静态/单例注册逻辑，测试里常用同步 `Once::call_once` 调用它。
/// 目前只有 system domain 需要补基础数据（2 条系统级 cron triggers），
/// 其它 domain 如后续有默认条目的幂等注入需求，可在这里追加 await。
pub async fn init_all_base_data() {
    system::init_base_data().await;
}
