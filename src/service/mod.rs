pub mod dal;
pub mod dao;
pub mod domain;

// 初始化所有 service 层组件（由 main 调用）
pub fn init() {
    // 初始化 DAO 层
    dao::init_all();

    // 初始化 DAL 层（依赖 DAO）
    dal::init_all();

    // 初始化 Domain 层（依赖 DAL）
    domain::init_all();

    // browser 工具截图产物存储器：project Domain 实现
    // （截图拷贝入项目产物目录 + GeneratedContent 产物记录）
    crate::pkg::tool_registry::browser::set_screenshot_storer(Box::new(
        domain::project::ProjectScreenshotStorer,
    ));

    // mark_artifact 工具产物注册器：project Domain 实现
    // （工具运行日志复制晋升入项目产物目录，与 ① 层 TTL 清理解耦）
    crate::pkg::tool_registry::mark_artifact::set_artifact_registrar(Box::new(
        domain::project::ProjectToolOutputRegistrar,
    ));
}

/// 第二阶段：异步初始化各 Domain 的基础数据（幂等写入默认条目等）。
///
/// 与同步的 `init()` 分开：`init()` 只做内存里的单例/静态注册，
/// 本函数用于需要 DB IO 的幂等默认数据注入，失败仅记录日志不阻塞启动。
pub async fn init_base_data() {
    domain::init_all_base_data().await;
}
