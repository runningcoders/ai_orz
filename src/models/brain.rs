//! Brain 实体 - 封装 rig Agent
//!
//! 只保存 Agent 实例，实际调用通过 BrainDao 完成

use async_trait::async_trait;
use anyhow::Result;

/// 统一的 Rig Agent trait
#[async_trait]
pub trait RigAgent: Send + Sync {
    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;
    
    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

/// Brain 封装了 rig Agent
///
/// 由 BrainDao 根据 ModelProvider 配置创建
/// 实际推理调用通过 BrainDao 完成
pub struct Brain {
    pub(crate) agent: Box<dyn RigAgent + Send + Sync>,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(agent: Box<dyn RigAgent + Send + Sync>) -> Self {
        Self { agent }
    }
}
