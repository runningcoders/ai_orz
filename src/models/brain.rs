//! Brain 实体
//!
//! - Brain 持有 ModelProvider 配置 + Memory 记忆系统
//! - Local agent: 持有 ModelProviderPo
//! - 外部 agent (Cli/Remote): model_provider 为 None，通过 runtime_config 执行

use crate::models::agent::AgentRuntimeConfig;
use crate::models::model_provider::ModelProviderPo;
use common::enums::AgentKind;

/// Brain 封装了完整的思考执行环境
///
/// - Local agent: 持有 ModelProviderPo + 记忆列表
/// - 外部 agent (Cli/Remote): model_provider 为 None，通过 runtime_config 执行
#[derive(Clone)]
pub struct Brain {
    /// Agent 类型（分发依据）
    pub kind: AgentKind,
    /// Agent ID（日志/审计用）
    pub agent_id: String,
    /// Agent 名称（日志/审计用）
    pub agent_name: String,
    /// 运行时配置
    /// - Local agent: 预留，用于 max_thinking_depth 等参数
    /// - 外部 agent: 包含 external_config，执行时读取
    pub runtime_config: AgentRuntimeConfig,
    /// 模型提供商配置（仅 Local kind 有值，外部 agent 为 None）
    pub model_provider: Option<ModelProviderPo>,
    /// 记忆列表
    pub memories: Vec<crate::models::memory::Memory>,
}

impl Brain {
    /// Local agent 构造方法
    pub fn new_local(
        agent_id: String,
        agent_name: String,
        runtime_config: AgentRuntimeConfig,
        model_provider: ModelProviderPo,
        memories: Vec<crate::models::memory::Memory>,
    ) -> Self {
        Self {
            kind: AgentKind::Local,
            agent_id,
            agent_name,
            runtime_config,
            model_provider: Some(model_provider),
            memories,
        }
    }

    /// 外部 agent 构造方法（无 model_provider）
    pub fn new_external(
        kind: AgentKind,
        agent_id: String,
        agent_name: String,
        runtime_config: AgentRuntimeConfig,
        memories: Vec<crate::models::memory::Memory>,
    ) -> Self {
        debug_assert!(kind.is_external(), "new_external 只能用于 Cli/Remote kind");
        Self {
            kind,
            agent_id,
            agent_name,
            runtime_config,
            model_provider: None,
            memories,
        }
    }

    /// 是否为外部 agent
    pub fn is_external(&self) -> bool {
        self.kind.is_external()
    }

    /// 是否为 Local agent
    pub fn is_local(&self) -> bool {
        self.kind.is_local()
    }

    /// 获取 ModelProvider 配置引用（仅 Local 有值）
    pub fn model_provider(&self) -> Option<&ModelProviderPo> {
        self.model_provider.as_ref()
    }
}
