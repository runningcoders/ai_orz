//! 外部消息适配层（consumer/adapter）
//!
//! 作为 consumer 层的子模块，编排外部消息 → 内部消息的投递流程：
//! 1. 从 `pkg/adapter` 注册中心获取各渠道 DAL 适配者
//! 2. 调用适配者的 adapt 方法，得到内部 `AdaptedMessage`
//! 3. 调用 `MessageDomain` 完成内部消息发送
//!
//! 本模块不承载转换逻辑（转换在各渠道 DAL 中），只做编排，
//! 符合"consumer 层调用 adapter 拿到适配者，运行它们来获取适配后的 message 结果，
//! 然后通过 message domain 完成发送"的架构决策。

pub mod lark;

use common::config::AppConfig;
use common::error::Result;

/// 初始化外部消息适配层
///
/// 由 `consumer::init` 调用，按配置启动各渠道的事件监听。
pub async fn init(config: &AppConfig) -> Result<()> {
    if config.lark.enabled {
        lark::init(config).await?;
    } else {
        sys_info!("lark adapter disabled by config, skip init");
    }
    Ok(())
}

/// 关闭外部消息适配层（停止所有事件监听）
pub async fn shutdown() -> Result<()> {
    lark::shutdown().await;
    Ok(())
}
