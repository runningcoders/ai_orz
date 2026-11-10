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

use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::api::cerebellum::{CerebellumQuestion, ThinkFastResult};
use common::error::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

/// System One 默认端点（Jev 协议 base_url 缺省兜底；provider 配置 base_url 优先）
///
/// 与 OpenAI 兼容 provider 同口径：**版本前缀由 base_url 承载**（如 Qwen 的
/// `/compatible-mode/v1`），client 只追加 `/systemone`。
pub const DEFAULT_SYSTEM_ONE_BASE_URL: &str = "https://api.typesafe.ai/v1";
/// 默认请求超时（毫秒）：小脑快判断的全局缺省预算；config.timeout_ms 可逐模型覆盖
///
/// **终态 = 1000ms**（AMan 2026-11-10 09:08「把小脑判断的超时时间调大一点，1 秒吧」，
/// 落地 `cedf4022`）。沿革：`800 → 1000（cedf4022）→（误）800（ea06affb）→ 1000（本笔）`；
/// 中间那次 800 是按**一条已被撤回的裁定**落的码（前台 `01a1c374` 误判后自行撤回，
/// 责任不在执行侧），故本笔是「纠正一次误落」，不是回滚任何用户指令。
///
/// 同日 10:55 AMan 追加的「5 秒」是**给 `chain_reply` 的**（语境是链内多拉上下文），
/// 从未取代本条「1 秒」，落法是**场景级覆盖**（`chain_reply::SCENE_TIMEOUT_MS = 5000`）。
///
/// 为什么 5 秒不能做全局缺省：本常量是整批共享的**一份**预算（`BrainDal::decide` 的
/// for 循环整体包一个 `tokio::time::timeout`），作用于全部已接 scene。`entry_gate`
/// 是**每条入站消息**都过的门，端点挂掉时若缺省是 5000，每条消息白等 5 秒，
/// 影响面比「让 chain_reply 多读上下文」大一个量级。反过来把它压回 800 则是抗一条
/// 未被撤销的用户指令 —— 两个方向都由字面断言钉住。
///
/// 取值顺序（三级链，唯一梯子点是 `brain::effective_timeout_ms`）：
/// `DecisionRequest.timeout_ms`（场景声明）→ `provider.config.timeout_ms` → 本常量。
/// 现网 provider 的 `config` 无一条显式写 `timeout_ms`，故其余 scene 的实际生效值
/// 就是本常量。
pub const DEFAULT_CEREBELLUM_TIMEOUT_MS: u64 = 1_000;

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

/// 获取 Cerebellum DAO 单例（未初始化返回 `None`，不 panic）
///
/// ⚠️ **生产调用点（小脑快判断路由）必须用它，不能用 [`dao`]** —— 小脑是
/// **可选增强**（B3 快判断路由，命中与否都只是优化），不是主流程的依赖项。
/// 单例未初始化时正确行为是**降级跳过**，而不是 panic 掉整个思考循环：
/// 小脑一挂就把主模型的完整思考一起拖垮，是典型的「增强功能反向劫持主流程」。
pub fn try_dao() -> Option<Arc<dyn CerebellumDao>> {
    CEREBELLUM_DAO.get().cloned()
}

/// 初始化 Cerebellum DAO 单例
pub fn init() {
    let _ = CEREBELLUM_DAO.set(Arc::new(SystemOneCerebellumDao::new()));
}
