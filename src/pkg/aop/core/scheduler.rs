use async_trait::async_trait;

#[async_trait]
pub trait Scheduler: Send + Sync {
    fn name(&self) -> &str;
    fn interval_secs(&self) -> u64;
    async fn run(&self) -> common::error::Result<()>;
}
