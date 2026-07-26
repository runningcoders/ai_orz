use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

use super::Registry;

#[async_trait]
pub trait Producer: Send + Sync {
    fn name(&self) -> &str;

    async fn register(&self, registry: Arc<Registry>) -> Result<()>;

    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    /// 轮询间隔（秒）
    ///
    /// - 返回 0：非轮询模式，由 start() 自行管理生命周期（如外部渠道监听）
    /// - 返回 >0：轮询模式，事件中心每隔此间隔调用 poll()
    fn poll_interval_secs(&self) -> u64 {
        0
    }

    /// 执行一次生产（轮询模式下由事件中心调用）
    ///
    /// 非轮询模式下无需实现
    async fn poll(&self) -> Result<()> {
        Ok(())
    }
}
