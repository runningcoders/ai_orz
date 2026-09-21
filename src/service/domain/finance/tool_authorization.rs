//! 工具授权子域（阶段③批次一 S1b）
//!
//! 内存授权存储（InMemoryAuthorizationStore）+ 审批回路编排（AuthorizationService）。
//! 设计：方案 artifact 01a0c2bc-514a v5 §四/§六/§14.2/§15.2。
//! 存储内聚 domain（§14.2 定案）；重启失效为有意安全默认；过期惰性判定不建后台 job。
//! 基建（pkg::authorization）承载领域类型/证据静态校验/裁决纯函数；
//! 本子域负责：查消息表（证据链第①条）、证据单次消费防重放、审批状态机、审计留痕。
//!
//! 并发纪律：锁仅在同步临界区内持有，await 一律在锁外（find_by_id 先行、临界区再提交）。

use crate::pkg::authorization::{
    AuthorizationGrant, AuthorizationStatus, EvidenceCheck, PendingAuthorization, clamp_ttl_secs,
    default_max_uses, evaluate_evidence,
};
use async_trait::async_trait;

use crate::pkg::RequestContext;
use crate::service::dal::message::MessageDal;
use common::api::{AuthorizationDetailDto, AuthorizationStatusDto, EvidenceClassDto};
use common::enums::MessageRole;
use common::error::{Error, Result, bail_err};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

// ==================== 内存授权存储 ====================

/// 记录形态：授权单 + 已签发授权 + 审计信息
#[derive(Debug)]
struct AuthorizationRecord {
    pending: PendingAuthorization,
    grant: Option<AuthorizationGrant>,
    /// 当前状态（六态；由审批状态机维护）
    status: AuthorizationStatus,
    /// 拦截规则幂等性（决定签发默认次数上限）
    rule_idempotent: bool,
    /// 决策/撤销理由（审计）
    decision_reason: Option<String>,
}

/// 内存授权存储
#[derive(Debug, Default)]
struct InMemoryAuthorizationStore {
    /// 主索引：authorization_id → record
    records: HashMap<String, AuthorizationRecord>,
    /// (agent_id, tool_id) → 授权单 ID 集合（裁决查询加速）
    by_agent_tool: HashMap<(String, String), Vec<String>>,
    /// 已消费证据消息 ID（防重放；跨授权单全局唯一——一次用户确认只能支撑一次决策）
    used_evidence: HashSet<String>,
}

// ==================== 编排服务 ====================

/// 工具授权编排服务（由 FinanceDomainImpl 持有，实现 ToolAuthorizationManage）
pub struct AuthorizationService {
    store: RwLock<InMemoryAuthorizationStore>,
    /// 消息 DAL（证据链第①条「消息真实存在」查询；测试/未接线实例为 None → 聊天通道 fail-closed）
    pub message_dal: Option<Arc<dyn MessageDal>>,
}

impl std::fmt::Debug for AuthorizationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationService")
            .finish_non_exhaustive()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn status_to_dto(s: AuthorizationStatus) -> AuthorizationStatusDto {
    match s {
        AuthorizationStatus::Pending => AuthorizationStatusDto::Pending,
        AuthorizationStatus::Active => AuthorizationStatusDto::Active,
        AuthorizationStatus::Expired => AuthorizationStatusDto::Expired,
        AuthorizationStatus::Revoked => AuthorizationStatusDto::Revoked,
        AuthorizationStatus::Rejected => AuthorizationStatusDto::Rejected,
        AuthorizationStatus::Consumed => AuthorizationStatusDto::Consumed,
    }
}

impl AuthorizationService {
    /// 创建服务（空存储，无消息 DAL；生产 init 流程再接线）
    pub fn new() -> Self {
        Self {
            store: RwLock::new(InMemoryAuthorizationStore::default()),
            message_dal: None,
        }
    }

    /// 注入消息 DAL（生产 init 由 finance mod 接线；测试可直接构造）
    pub fn with_message_dal(mut self, dal: Arc<dyn MessageDal>) -> Self {
        self.message_dal = Some(dal);
        self
    }

    fn detail_dto(rec: &AuthorizationRecord) -> AuthorizationDetailDto {
        AuthorizationDetailDto {
            authorization_id: rec.pending.authorization_id.clone(),
            agent_id: rec.pending.agent_id.clone(),
            tool_id: rec.pending.tool_id.clone(),
            user_id: rec.pending.user_id.clone(),
            command_signature: rec.pending.command_signature.clone(),
            blocking_rule: rec.pending.blocking_rule.clone(),
            requested_at_ms: rec.pending.requested_at_ms,
            status: status_to_dto(rec.status),
            grant_id: rec.grant.as_ref().map(|g| g.grant_id.clone()),
            expires_at_ms: rec.grant.as_ref().map(|g| g.expires_at_ms),
            remaining_uses: rec.grant.as_ref().and_then(|g| g.remaining_uses()),
        }
    }
}

impl Default for AuthorizationService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::ToolAuthorizationManage for AuthorizationService {
    async fn create_pending_authorization(
        &self,
        ctx: RequestContext,
        cmd: super::CreateAuthorizationCmd,
    ) -> Result<PendingAuthorization> {
        let authorization_id = uuid::Uuid::now_v7().to_string();
        let pending = PendingAuthorization {
            authorization_id: authorization_id.clone(),
            agent_id: cmd.agent_id,
            tool_id: cmd.tool_id,
            user_id: ctx.user_id.clone().unwrap_or_default(),
            command_signature: cmd.command_signature,
            blocking_rule: cmd.blocking_rule,
            requested_at_ms: now_ms(),
            status: AuthorizationStatus::Pending,
        };
        let key = (pending.agent_id.clone(), pending.tool_id.clone());
        let reason_log = cmd.reason.clone();
        let mut store = self
            .store
            .write()
            .map_err(|_| Error::internal("授权存储状态异常"))?;
        // 同签名 Pending 已存在 → 拒绝重复建单（防通知风暴）
        if let Some(ids) = store.by_agent_tool.get(&key) {
            for id in ids {
                let dup = store.records.get(id).is_some_and(|rec| {
                    rec.status == AuthorizationStatus::Pending
                        && rec.pending.command_signature == pending.command_signature
                });
                if dup {
                    bail_err!(Conflict, "同签名待审批授权单已存在 authorization_id={id}");
                }
            }
        }
        store
            .by_agent_tool
            .entry(key)
            .or_default()
            .push(authorization_id.clone());
        store.records.insert(
            authorization_id.clone(),
            AuthorizationRecord {
                status: AuthorizationStatus::Pending,
                pending: pending.clone(),
                grant: None,
                rule_idempotent: cmd.rule_idempotent,
                decision_reason: None,
            },
        );
        drop(store);
        log_info!(
            "tool_authorization 审计 [建单] authorization_id={} agent={} tool={} signature={} rule={} reason={:?}",
            pending.authorization_id,
            pending.agent_id,
            pending.tool_id,
            pending.command_signature,
            pending.blocking_rule,
            reason_log
        );
        Ok(pending)
    }

    async fn decide_authorization(
        &self,
        ctx: RequestContext,
        cmd: super::AuthorizationDecisionCmd,
    ) -> Result<super::AuthorizationDecisionOutcome> {
        // 红线④：decide 强制 user ctx（Agent 不得自我审批，结构性拒绝）
        if ctx.agent_id.is_some() {
            bail_err!(InvalidRequest, "Agent 不得自我审批（decide 强制 user ctx）");
        }
        let decided_by = ctx
            .user_id
            .clone()
            .ok_or_else(|| Error::bad_request("审批需要用户身份（user ctx）"))?;
        let auth_id = cmd.authorization_id.clone();
        let class = cmd.evidence_class.unwrap_or(EvidenceClassDto::Ui);
        let mediator = cmd.mediator_agent_id.clone();

        // 阶段一（锁外）：取单 + 证据链静态校验（消息查询 await 不持锁）
        let pending = {
            let store = self
                .store
                .read()
                .map_err(|_| Error::internal("授权存储状态异常"))?;
            let rec = store
                .records
                .get(&auth_id)
                .ok_or_else(|| Error::not_found(format!("授权单不存在 {auth_id}")))?;
            if rec.status != AuthorizationStatus::Pending {
                bail_err!(Conflict, "授权单非待审批态 status={:?}", rec.status);
            }
            rec.pending.clone()
        };
        let needs_evidence = matches!(
            class,
            EvidenceClassDto::ChatDirective | EvidenceClassDto::ChatMediated
        );
        let mut evidence_id: Option<String> = None;
        if needs_evidence {
            // 第⑤条：申请人不得自代呈（先于其他要素快速失败）
            if mediator.as_deref().is_some_and(|m| m == pending.agent_id) {
                bail_err!(InvalidRequest, "申请人不得自代呈（代呈 Agent 即申请人）");
            }
            let ev_id = cmd
                .evidence_message_id
                .clone()
                .ok_or_else(|| Error::bad_request("聊天通道审批必须携带证据消息 ID"))?;
            let dal = self
                .message_dal
                .as_ref()
                .ok_or_else(|| Error::internal("消息数据访问未接线（聊天通道证据链校验不可用）"))?;
            let msg = dal
                .find_by_id(ctx.clone(), &ev_id)
                .await?
                .ok_or_else(|| Error::not_found(format!("证据消息不存在 {ev_id}")))?;
            let check = EvidenceCheck {
                from_id: msg.po.from_id.clone(),
                from_role_is_user: msg.po.from_role == MessageRole::User,
                created_at_ms: msg.po.created_at,
            };
            evaluate_evidence(Some(&check), &pending, mediator.as_deref())
                .map_err(|r| Error::bad_request(r.describe()))?;
            evidence_id = Some(ev_id);
        }

        // 阶段二（临界区提交）：先不可变预检（状态 + 证据防重放），后可变提交
        let mut store = self
            .store
            .write()
            .map_err(|_| Error::internal("授权存储状态异常"))?;
        let rule_idempotent;
        {
            let rec = store
                .records
                .get(&auth_id)
                .ok_or_else(|| Error::not_found(format!("授权单不存在 {auth_id}")))?;
            if rec.status != AuthorizationStatus::Pending {
                bail_err!(Conflict, "授权单非待审批态 status={:?}", rec.status);
            }
            rule_idempotent = rec.rule_idempotent;
            // 证据单次消费防重放（跨授权单全局唯一；同一临界区内检查+登记防竞态）
            let replay = cmd.approve
                && evidence_id
                    .as_deref()
                    .is_some_and(|ev| store.used_evidence.contains(ev));
            if replay {
                bail_err!(
                    Conflict,
                    "证据消息已被消费（防重放） {}",
                    evidence_id.clone().unwrap()
                );
            }
        }
        let rec = store
            .records
            .get_mut(&auth_id)
            .ok_or_else(|| Error::not_found(format!("授权单不存在 {auth_id}")))?;
        if !cmd.approve {
            rec.status = AuthorizationStatus::Rejected;
            rec.decision_reason = Some("rejected".to_string());
            drop(store);
            log_info!(
                "tool_authorization 审计 [拒绝] authorization_id={} decided_by={}",
                auth_id,
                decided_by
            );
            return Ok(super::AuthorizationDecisionOutcome {
                authorization_id: auth_id,
                status: AuthorizationStatus::Rejected,
                grant_id: None,
            });
        }
        let ttl = clamp_ttl_secs(
            cmd.ttl_secs
                .unwrap_or(crate::pkg::authorization::DEFAULT_TTL_SECS),
        );
        let grant = AuthorizationGrant {
            grant_id: uuid::Uuid::now_v7().to_string(),
            authorization_id: pending.authorization_id.clone(),
            agent_id: pending.agent_id.clone(),
            tool_id: pending.tool_id.clone(),
            command_signature: cmd
                .scope_command_signature
                .unwrap_or_else(|| pending.command_signature.clone()),
            prefix_match: cmd.prefix_match,
            expires_at_ms: now_ms() + ttl * 1000,
            max_uses: cmd.max_uses.or_else(|| default_max_uses(rule_idempotent)),
            uses: 0,
        };
        let grant_id = grant.grant_id.clone();
        rec.grant = Some(grant);
        rec.status = AuthorizationStatus::Active;
        if let Some(ev_id) = &evidence_id {
            store.used_evidence.insert(ev_id.clone());
        }
        drop(store);
        log_info!(
            "tool_authorization 审计 [批准] authorization_id={} grant_id={} decided_by={} evidence={:?} mediator={:?}",
            auth_id,
            grant_id,
            decided_by,
            cmd.evidence_message_id,
            cmd.mediator_agent_id
        );
        Ok(super::AuthorizationDecisionOutcome {
            authorization_id: auth_id,
            status: AuthorizationStatus::Active,
            grant_id: Some(grant_id),
        })
    }

    async fn revoke_authorization(
        &self,
        ctx: RequestContext,
        authorization_id: &str,
        reason: Option<String>,
    ) -> Result<super::AuthorizationDecisionOutcome> {
        let _ = &ctx;
        let mut store = self
            .store
            .write()
            .map_err(|_| Error::internal("授权存储状态异常"))?;
        let rec = store
            .records
            .get_mut(authorization_id)
            .ok_or_else(|| Error::not_found(format!("授权单不存在 {authorization_id}")))?;
        if rec.status.terminal() {
            bail_err!(Conflict, "授权单已终态不可撤销 status={:?}", rec.status);
        }
        rec.status = AuthorizationStatus::Revoked;
        rec.decision_reason = reason;
        drop(store);
        log_info!(
            "tool_authorization 审计 [撤销] authorization_id={}",
            authorization_id
        );
        Ok(super::AuthorizationDecisionOutcome {
            authorization_id: authorization_id.to_string(),
            status: AuthorizationStatus::Revoked,
            grant_id: None,
        })
    }

    async fn list_authorizations(
        &self,
        ctx: RequestContext,
        query: common::api::AuthorizationQueryRequest,
    ) -> Result<Vec<common::api::AuthorizationDetailDto>> {
        let _ = &ctx;
        let store = self
            .store
            .read()
            .map_err(|_| Error::internal("授权存储状态异常"))?;
        let want_status = query.status.map(|s| match s {
            AuthorizationStatusDto::Pending => AuthorizationStatus::Pending,
            AuthorizationStatusDto::Active => AuthorizationStatus::Active,
            AuthorizationStatusDto::Expired => AuthorizationStatus::Expired,
            AuthorizationStatusDto::Revoked => AuthorizationStatus::Revoked,
            AuthorizationStatusDto::Rejected => AuthorizationStatus::Rejected,
            AuthorizationStatusDto::Consumed => AuthorizationStatus::Consumed,
        });
        let mut items = Vec::new();
        for rec in store.records.values() {
            if want_status.is_some_and(|ws| rec.status != ws) {
                continue;
            }
            if query
                .agent_id
                .as_ref()
                .is_some_and(|a| rec.pending.agent_id != *a)
            {
                continue;
            }
            if query
                .tool_id
                .as_ref()
                .is_some_and(|t| rec.pending.tool_id != *t)
            {
                continue;
            }
            if query
                .user_id
                .as_ref()
                .is_some_and(|u| rec.pending.user_id != *u)
            {
                continue;
            }
            items.push(Self::detail_dto(rec));
        }
        items.sort_by_key(|d| std::cmp::Reverse(d.requested_at_ms));
        Ok(items)
    }

    async fn consume_grant(
        &self,
        ctx: RequestContext,
        authorization_id: &str,
    ) -> Result<Option<AuthorizationGrant>> {
        let _ = &ctx;
        let now = now_ms();
        let mut store = self
            .store
            .write()
            .map_err(|_| Error::internal("授权存储状态异常"))?;
        let Some(rec) = store.records.get_mut(authorization_id) else {
            return Ok(None);
        };
        if rec.status != AuthorizationStatus::Active {
            return Ok(None);
        }
        let Some(grant) = rec.grant.as_mut() else {
            return Ok(None);
        };
        // 惰性过期判定
        if now >= grant.expires_at_ms {
            rec.status = AuthorizationStatus::Expired;
            return Ok(None);
        }
        if grant.max_uses.is_some_and(|max| grant.uses >= max) {
            rec.status = AuthorizationStatus::Consumed;
            return Ok(None);
        }
        grant.uses += 1;
        if grant.remaining_uses() == Some(0) {
            rec.status = AuthorizationStatus::Consumed;
        }
        Ok(Some(grant.clone()))
    }

    async fn active_grants_for(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<Vec<AuthorizationGrant>> {
        let _ = &ctx;
        let now = now_ms();
        let store = self
            .store
            .read()
            .map_err(|_| Error::internal("授权存储状态异常"))?;
        let mut out = Vec::new();
        if let Some(ids) = store
            .by_agent_tool
            .get(&(agent_id.to_string(), tool_id.to_string()))
        {
            for id in ids {
                let Some(rec) = store.records.get(id) else {
                    continue;
                };
                if rec.status != AuthorizationStatus::Active {
                    continue;
                }
                let Some(g) = &rec.grant else {
                    continue;
                };
                if g.expires_at_ms > now {
                    out.push(g.clone());
                }
            }
        }
        Ok(out)
    }
}
