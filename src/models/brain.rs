//! Brain 实体 - 封装 rig Agent

use async_trait::async_trait;
use anyhow::Result;

/// 统一的 Rig Agent  trait
#[async_trait]
pub trait RigAgent: Send + Sync {
    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;
    
    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

/// Brain 封装了 rig Agent，负责与外部 LLM 提供商通信
///
/// 由 brain dao 工厂根据 ModelProvider 配置创建
pub struct Brain {
    pub(crate) agent: Box<dyn RigAgent + Send + Sync>,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(agent: Box<dyn RigAgent + Send + Sync>) -> Self {
        Self { agent }
    }

    /// 运行 prompt，获取回答
    pub async fn prompt(&self, prompt: &str) -> Result<String> {
        self.agent.prompt(prompt).await
    }
}
