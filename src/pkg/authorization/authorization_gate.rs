//! 授权门：拦截侧与授权子域的边界契约（方案 §14.1/§15.2 过渡态形态）
//!
//! trait 定义于 pkg（过渡态），domain 实现并经全局槽位装配注入（OnceLock set-once，
//! init 流程=唯一边界点，依赖箭头单向不越层）；两段化后置重构时定义迁移 domain、
//! 槽位消解（§14.4/§18 定案）。未装配（测试/未接线）时 gate() 返回 None，
//! 拦截层行为与现状逐字节一致（兼容性承诺）。

use crate::pkg::RequestContext;
use crate::pkg::authorization::{AuthorizationGrant, PendingAuthorization};
use async_trait::async_trait;
use common::error::Result;
use std::sync::{Arc, OnceLock};

/// 创建授权命令（拦截建单 / 主动申请共同入参；授权门契约类型）
#[derive(Debug, Clone)]
pub struct CreateAuthorizationCmd {
    /// 申请人 Agent ID
    pub agent_id: String,
    /// 目标工具 ID
    pub tool_id: String,
    /// 受限命令规范化签名
    pub command_signature: String,
    /// 命中拦截规则 id
    pub blocking_rule: String,
    /// 拦截规则幂等性（决定签发默认次数上限）
    pub rule_idempotent: bool,
    /// 申请理由（审计留痕）
    pub reason: Option<String>,
    /// 触发本次申请的工具调用 ID（拦截建单时取 `ctx.tool_call_id()`；
    /// 主动建单为 None）——用于把授权单与工具调用记录精确对上
    pub call_id: Option<String>,
}

/// 授权门（拦截侧消费的最小接口）
#[async_trait]
pub trait AuthorizationGate: Send + Sync {
    /// 查取对 (agent_id, tool_id) 的有效授权（由实现按签名匹配过滤）
    async fn check_grant(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
        command_signature: &str,
    ) -> Result<Vec<AuthorizationGrant>>;

    /// 创建待审批授权单（同签名 Pending 防重复建单由实现保证）
    async fn request_authorization(
        &self,
        ctx: RequestContext,
        cmd: CreateAuthorizationCmd,
    ) -> Result<PendingAuthorization>;

    /// 放行计数消耗（失效/次数用尽返回 None）
    async fn consume(
        &self,
        ctx: RequestContext,
        authorization_id: &str,
    ) -> Result<Option<AuthorizationGrant>>;
}

static GATE: OnceLock<Option<Arc<dyn AuthorizationGate>>> = OnceLock::new();

/// 装配注入（init 流程调用；set-once，重复安装返回 false）
pub fn install_gate(gate: Arc<dyn AuthorizationGate>) -> bool {
    GATE.set(Some(gate)).is_ok()
}

/// 获取已装配的授权门（未装配 = None → 拦截层现状行为）
pub fn gate() -> Option<&'static Arc<dyn AuthorizationGate>> {
    GATE.get().and_then(|g| g.as_ref())
}
