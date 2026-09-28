//! Cerebellum DAL 模块
//!
//! 职责：把小脑快判断（System One decision 模型）的 DAO 消费点从 domain 层
//! 归位到 DAL 层（分层红线：Domain 禁直接调用 DAO）。与 BrainDal 持 CortexDao
//! 同构：DAL 组合 DAO 单例并暴露组合方法，domain 层（cerebellum_router）只依赖
//! DAL trait，DAO 的生产/测试注入点平移到 DAL 层。

use crate::models::cerebellum_types::{CerebellumQuestion, ThinkFastResult};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dao::cerebellum::CerebellumDao;
use async_trait::async_trait;
use common::error::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static CEREBELLUM_DAL: OnceLock<Arc<dyn CerebellumDal>> = OnceLock::new();

/// 获取 Cerebellum DAL 单例
pub fn cerebellum_dal() -> Arc<dyn CerebellumDal> {
    CEREBELLUM_DAL
        .get()
        .cloned()
        .expect("CerebellumDal not initialized, call dal::cerebellum::init() first")
}

/// 初始化 Cerebellum DAL
///
/// 内部取 CerebellumDao 单例；service::init 顺序 dao::init_all() → dal::init_all()，时序安全
pub fn init() {
    let _ = CEREBELLUM_DAL.set(new(crate::service::dao::cerebellum::dao()));
}

/// 创建 Cerebellum DAL（返回 trait 对象；测试可注入 mock CerebellumDao）
pub fn new(cerebellum_dao: Arc<dyn CerebellumDao + Send + Sync>) -> Arc<dyn CerebellumDal> {
    Arc::new(CerebellumDalImpl { cerebellum_dao })
}

// ==================== DAL 接口 ====================

/// Cerebellum DAL 接口 - 小脑快判断
#[async_trait]
pub trait CerebellumDal: Send + Sync {
    /// 快判断：单次请求并行评估多个类型化问题（noul/choice/score）
    ///
    /// 与 `CerebellumDao::think_fast` 同签名透传；provider/state/questions 语义
    /// 见 DAO 层文档，本层只做消费点归位，不附加业务逻辑。
    async fn think_fast(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        state: Value,
        questions: BTreeMap<String, CerebellumQuestion>,
    ) -> Result<ThinkFastResult>;
}

// ==================== 实现 ====================

struct CerebellumDalImpl {
    cerebellum_dao: Arc<dyn CerebellumDao>,
}

#[async_trait]
impl CerebellumDal for CerebellumDalImpl {
    async fn think_fast(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        state: Value,
        questions: BTreeMap<String, CerebellumQuestion>,
    ) -> Result<ThinkFastResult> {
        self.cerebellum_dao
            .think_fast(ctx, provider, state, questions)
            .await
    }
}
