//! Brain 实体
//!
//! - Brain 持有 ModelProvider 配置 + Memory 记忆系统
//! - Local agent: 持有 ModelProviderPo
//! - 外部 agent (Cli/Remote): model_provider 为 None，通过 runtime_config 执行

use crate::models::agent::AgentRuntimeConfig;
use crate::models::model_provider::ModelProviderPo;
use common::enums::AgentKind;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 单次唤醒（一个 Brain 生命周期）内的小脑调用配额
///
/// **取值 = 本 run 起点的 resolved 大脑轮次**（`max_thinking_rounds`：Agent
/// runtime_config 非 0 取它，否则取系统配置 `[agent].max_thinking_rounds`），
/// 由 `wake_agent_brain` 装配 Brain 时写入 ⇒ 随 run 生命周期固定，不跨 run 累积。
///
/// 用途只有一个：**防失控**。小脑单次成本低，图内又有
/// `overflow → compact → prepare` 与 `discover → retrieve` 等回环，同一处决策
/// 会被反复触发；配额把「整条唤醒链一共能问几次小脑」封在一个上界内，超限一律
/// **静默走原逻辑**（与超时 / 调用失败同一降级语义，不改变主流程）。
///
/// `limit == 0` 表示不限：测试与外部构造的 Brain 走这条路径，行为与未加配额前一致。
#[derive(Debug)]
pub struct CerebellumQuota {
    limit: usize,
    used: AtomicUsize,
}

impl CerebellumQuota {
    /// 按上限构造（0 = 不限）
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: AtomicUsize::new(0),
        }
    }

    /// 不限配额（测试 / 未按 run 装配的 Brain）
    pub fn unlimited() -> Self {
        Self::new(0)
    }

    /// 占用一次调用额度；`false` = 已超限，调用方**必须**走原逻辑
    pub fn try_acquire(&self) -> bool {
        if self.limit == 0 {
            // 不限：仍计数，便于观测真实用量
            self.used.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        self.used
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                (n < self.limit).then_some(n + 1)
            })
            .is_ok()
    }

    /// 本 run 的上限（0 = 不限）
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// 已占用的次数
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed)
    }
}

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
    /// 默认小脑模型配置（B2 二期注入；None=无启用小脑，行为与现状一致）
    ///
    /// 由 wake_brain Local 分支经 `get_default_cerebellum_provider` 装配后赋值
    /// （构造签名不动，零调用点破坏）；Cli/Remote 外部 Agent 维持 None。
    pub cerebellum: Option<ModelProviderPo>,
    /// 记忆列表
    pub memories: Vec<crate::models::memory::Memory>,
    /// 本 run 的小脑调用配额（见 [`CerebellumQuota`]）
    ///
    /// 默认不限；`wake_agent_brain` 在装配时按 resolved 大脑轮次写入。
    pub cerebellum_quota: Arc<CerebellumQuota>,
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
            cerebellum: None,
            cerebellum_quota: Arc::new(CerebellumQuota::unlimited()),
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
            cerebellum: None,
            cerebellum_quota: Arc::new(CerebellumQuota::unlimited()),
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

#[cfg(test)]
mod cerebellum_quota_tests {
    use super::CerebellumQuota;

    #[test]
    fn quota_blocks_after_limit() {
        let quota = CerebellumQuota::new(2);
        assert!(quota.try_acquire());
        assert!(quota.try_acquire());
        // 超限：调用方必须走原逻辑，且计数不再被顶高
        assert!(!quota.try_acquire());
        assert!(!quota.try_acquire());
        assert_eq!(quota.used(), 2);
        assert_eq!(quota.limit(), 2);
    }

    #[test]
    fn unlimited_quota_never_blocks() {
        let quota = CerebellumQuota::unlimited();
        for _ in 0..64 {
            assert!(quota.try_acquire());
        }
        assert_eq!(quota.used(), 64);
    }
}
