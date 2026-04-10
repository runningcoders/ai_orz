//! Brain 实体和 Cortex 实体
//!
//! 最终结构：
//! - Brain 直接持有 Cortex 实体 + Memory 记忆系统
//! - Cortex 实体持有 ModelProvider 和 CortexTrait（推理执行）
//! - ModelProvider 只保存配置信息
//! - Memory 持有核心记忆 + 工作记忆

use crate::models::memory::{self};
use crate::models::model_provider::ModelProvider;
use async_trait::async_trait;
use anyhow::Result;
use dyn_clone::DynClone;

/// 统一的 CortexTrait - 大脑皮层 trait，定义推理接口
#[async_trait]
pub trait CortexTrait: Send + Sync + DynClone {
    /// 运行 prompt，获取回答
    async fn prompt(&self, prompt: &str) -> Result<String>;

    /// 是否支持工具调用
    fn support_tools(&self) -> bool;
}

dyn_clone::clone_trait_object!(CortexTrait);

/// Cortex 实体 - 持有 ModelProvider 和具体的推理实现
///
/// Cortex = 模型配置 + 推理执行
#[derive(Clone)]

pub struct Cortex {
    /// 关联的模型提供商（业务对象，包含配置信息）
    pub model_provider: ModelProvider,
    /// 推理执行实例
    pub cortex: Box<dyn CortexTrait + Send + Sync>,
}

impl Cortex {
    /// 创建新 Cortex
    pub fn new(model_provider: ModelProvider, cortex: Box<dyn CortexTrait + Send + Sync>) -> Self {
        Self {
            model_provider,
            cortex,
        }
    }

    /// 获取 Cortex 引用
    pub fn cortex(&self) -> &(dyn CortexTrait + Send + Sync) {
        &*self.cortex
    }
}

/// Agent 核心记忆
///
/// 核心认知，来自 AgentPo，每次调用全部拼入 prompt
#[derive(Debug, Clone)]
pub struct CoreMemory {
    /// Agent 灵魂/性格/角色设定
    pub soul: String,
    /// Agent 能力列表 JSON
    pub capabilities: String,
}

impl CoreMemory {
    /// 从 AgentPo 创建 CoreMemory
    pub fn from_po(soul: String, capabilities: String) -> Self {
        Self {
            soul,
            capabilities,
        }
    }
}

/// Agent 记忆系统
///
/// 记忆分层：
/// - core: 核心认知（soul + capabilities）→ 每次全部拼入
/// - working: 当前会话工作记忆 → 每次全部拼入
#[derive(Clone)]

pub struct Memory {
    /// 核心认知
    pub core: CoreMemory,
    /// 当前会话工作记忆（原始对话）
    pub working: Vec<memory::MemoryTrace>,
}

impl Memory {
    /// 创建新的 Memory
    pub fn new(soul: String, capabilities: String) -> Self {
        Self {
            core: CoreMemory::from_po(soul, capabilities),
            working: Vec::new(),
        }
    }

    /// 添加新的记忆到工作记忆
    pub fn add_working(&mut self, trace: memory::MemoryTrace) {
        self.working.push(trace);
    }

    /// 清空工作记忆（会话结束）
    pub fn clear_working(&mut self) {
        self.working.clear();
    }
}

/// Brain 封装了完整的思考执行环境
///
/// Brain 直接持有 Cortex 实体 + Memory 记忆系统
#[derive(Clone)]
pub struct Brain {
    /// Cortex 实体（包含模型配置 + 推理执行）
    pub cortex: Cortex,
    /// Agent 记忆系统
    pub memory: Memory,
}

impl Brain {
    /// 创建新 Brain
    pub fn new(cortex: Cortex, memory: Memory) -> Self {
        Self {
            cortex,
            memory,
        }
    }

    /// 获取 Cortex 引用
    pub fn cortex(&self) -> &Cortex {
        &self.cortex
    }

    /// 获取 Cortex 内部的推理执行引用
    pub fn cortex_trait(&self) -> &(dyn CortexTrait + Send + Sync) {
        self.cortex.cortex()
    }
}
