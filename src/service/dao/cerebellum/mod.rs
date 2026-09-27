//! Cerebellum DAO - 小脑快判断（System One 决策模型 client）
//!
//! 与 `dao/cortex/native` 同构：无状态单例 + 全部配置从 `&ModelProviderPo` 读取。
//! jev（TypeSafe AI System One）不兼容 OpenAI 协议，走独立 System One client；
//! 协议编解码依据官方公开口径（70~500ms / $0.042 per 1M，未经真实端点实测，
//! 连通性 Spike 顺延至凭据到位后二期启动前集中纠偏）。
//!
//! 一期边界：默认端点常量与超时兜底在此内建；「默认小脑」的获取由
//! `dao::model_provider::get_default_cerebellum_provider` 承担（status 单启用
//! 即默认标记，多小脑模型可配置、随时切换零额外操作）；Brain 注入 /
//! 运行时选路归二期（B2/B3），本模块一期无运行时消费点。

pub mod client;

use crate::models::cerebellum_types::{CerebellumQuestion, ThinkFastResult};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::error::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

/// System One 默认端点（Jev 协议 base_url 缺省兜底；provider 配置 base_url 优先）
pub const DEFAULT_SYSTEM_ONE_BASE_URL: &str = "https://api.typesafe.ai";
/// 默认请求超时（毫秒）：小脑快判断亚秒级预算；config.timeout_ms 可逐模型覆盖
pub const DEFAULT_CEREBELLUM_TIMEOUT_MS: u64 = 800;

/// Cerebellum DAO trait - 小脑快判断接口
#[async_trait]
pub trait CerebellumDao: Send + Sync {
    /// 快判断：单次请求并行评估多个类型化问题（noul/choice/score）
    ///
    /// 请求 model 取 `provider.model_name`（默认小脑模型标识不内建，
    /// 由 model_providers 记录本身承载）。
    async fn think_fast(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        state: Value,
        questions: BTreeMap<String, CerebellumQuestion>,
    ) -> Result<ThinkFastResult>;
}

/// System One client 单例
pub struct SystemOneCerebellumDao {
    client: reqwest::Client,
}

impl SystemOneCerebellumDao {
    fn new() -> Self {
        let client = crate::pkg::http::presets::llm()
            .build()
            .expect("Failed to build reqwest client for SystemOneCerebellumDao");
        Self { client }
    }
}

#[async_trait]
impl CerebellumDao for SystemOneCerebellumDao {
    async fn think_fast(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        state: Value,
        questions: BTreeMap<String, CerebellumQuestion>,
    ) -> Result<ThinkFastResult> {
        client::think_fast(&ctx, &self.client, provider, state, questions).await
    }
}

// ==================== 单例管理 ====================

static CEREBELLUM_DAO: OnceLock<Arc<dyn CerebellumDao>> = OnceLock::new();

/// 获取 Cerebellum DAO 单例
pub fn dao() -> Arc<dyn CerebellumDao> {
    CEREBELLUM_DAO
        .get()
        .cloned()
        .expect("CerebellumDao not initialized, call cerebellum::init() first")
}

/// 初始化 Cerebellum DAO 单例
pub fn init() {
    let _ = CEREBELLUM_DAO.set(Arc::new(SystemOneCerebellumDao::new()));
}
