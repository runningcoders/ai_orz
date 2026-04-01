//! Brain 实体 - 封装 cortex 思考模块
//!
//! 只保存 cortex 实例，实际调用通过 BrainDao 完成

use async_trait::async_trait;
use anyhow::Result;

/// 统一的 Cortex trait - 大脑皮层，负责思考推理
#[async_trait]
pub trait Cortex: Send + Sync {
    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;
    
    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

/// Brain 封装了 cortex 思考模块
///
/// 由 BrainDao 根据 ModelProvider 配置创建
/// 实际推理调用通过 BrainDao 完成
pub struct Brain {
    pub(crate) cortex: Box<dyn Cortex + Send + Sync>,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(cortex: Box<dyn Cortex + Send + Sync>) -> Self {
        Self { cortex }
    }
}
