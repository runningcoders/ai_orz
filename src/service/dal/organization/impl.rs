//! Organization DAL 实现（组织 CRUD / 影子同步 / 联邦委派传输）

use super::OrganizationDal;
use crate::models::events::OrganizationChangedEvent;
use crate::models::events::federation::FEDERATION_CMD_SEND_TASK;
use crate::models::organization::OrganizationPo;
use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::dao::agent_runtime::a2a::{
    FederatedCallConfig, execute_federated_agent_call, extract_text_from_task_result,
};
use crate::service::dao::organization::{OrganizationDao, OrganizationQuery, PeerOrgUpsert};
use crate::service::dao::organization_link::ws;
use common::api::OrganizationConfig;
use common::api::a2a::{A2aMessage, A2aMessagePart, SendTaskParams};
use common::error::Result;
use std::sync::Arc;

/// 联邦 Agent 委派的全程预算（send + 轮询，秒）
const FEDERATED_CALL_DEADLINE_SECS: u64 = 120;
/// 联邦 Agent 委派的 tasks/get 轮询间隔（毫秒）
const FEDERATED_CALL_POLL_INTERVAL_MS: u64 = 1000;

/// Organization DAL 实现
pub(super) struct OrganizationDalImpl {
    pub(super) organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
    pub(super) link_dao:
        Arc<dyn crate::service::dao::organization_link::OrganizationLinkDao + Send + Sync>,
}

#[async_trait::async_trait]
impl OrganizationDal for OrganizationDalImpl {
    async fn is_initialized(&self, ctx: RequestContext) -> Result<bool> {
        let count = self.organization_dao.count_all(ctx).await?;
        Ok(count > 0)
    }

    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>> {
        self.organization_dao.find_by_id(ctx, org_id).await
    }

    async fn find_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>> {
        self.organization_dao
            .find_by_invite_code(ctx, invite_code)
            .await
    }

    async fn get_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<OrganizationConfig> {
        self.organization_dao.get_org_config(ctx, org_id).await
    }

    async fn update_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()> {
        self.organization_dao
            .set_org_config(ctx, org_id, config)
            .await
    }

    async fn create(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()> {
        self.organization_dao.insert(ctx.clone(), org).await?;
        aop::publish(&ctx, OrganizationChangedEvent::new(&org.id, "created")).await;
        Ok(())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>> {
        self.organization_dao.query(ctx, query).await
    }

    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>> {
        self.query(ctx, OrganizationQuery::default()).await
    }

    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()> {
        self.organization_dao.update(ctx.clone(), org).await?;
        aop::publish(&ctx, OrganizationChangedEvent::new(&org.id, "updated")).await;
        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<()> {
        self.organization_dao.delete(ctx.clone(), org_id).await?;
        aop::publish(&ctx, OrganizationChangedEvent::new(org_id, "deleted")).await;
        Ok(())
    }

    async fn count_organizations(&self, ctx: RequestContext) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(ctx, OrganizationQuery::default()).await
    }

    async fn count(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<u64> {
        self.organization_dao.count(ctx, query).await
    }

    async fn upsert_remote_shadow(
        &self,
        ctx: RequestContext,
        peer: &PeerOrgUpsert,
    ) -> Result<bool> {
        self.organization_dao.upsert_remote_shadow(ctx, peer).await
    }

    async fn upsert_linked_shadow(
        &self,
        ctx: RequestContext,
        peer: &PeerOrgUpsert,
    ) -> Result<bool> {
        self.organization_dao.upsert_linked_shadow(ctx, peer).await
    }

    async fn revoke_link(
        &self,
        ctx: RequestContext,
        link_id: &str,
        peer_org_id: &str,
    ) -> Result<()> {
        // 1) 连接置 Revoked（links 表，幂等）
        self.link_dao.revoke(ctx.clone(), link_id).await?;
        // 2) 对端影子 Linked → Remote（organizations 表，幂等）；失败向上传播，
        //    调用方重试即可修复（重放断链无害）
        self.organization_dao
            .degrade_shadow_to_remote(ctx, peer_org_id)
            .await?;
        Ok(())
    }

    async fn list_addresses(&self, ctx: RequestContext) -> Result<Vec<(String, String)>> {
        self.organization_dao.list_addresses(ctx).await
    }

    async fn resolve_peer_endpoint(
        &self,
        ctx: RequestContext,
        link: &OrganizationLinkPo,
    ) -> String {
        use common::api::organization_link::FederationAddress;

        // 对端自报地址候选池（影子 addresses 列，裸 SQL 不进 PO）；读取失败
        // 视为无候选（快速路径维持主地址），不阻断出站
        let addresses: Vec<FederationAddress> = self
            .organization_dao
            .list_addresses(ctx)
            .await
            .ok()
            .and_then(|rows| {
                rows.into_iter()
                    .find(|(id, _)| id == &link.peer_org_id)
                    .map(|(_, json)| json)
            })
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();

        crate::service::dao::organization_link::resolver::resolver()
            .resolve(&link.endpoint, &addresses)
            .await
    }

    async fn send_federated_agent_task(
        &self,
        ctx: RequestContext,
        link: &OrganizationLinkPo,
        peer_agent_id: &str,
        prompt: &str,
        caller_declaration: Option<String>,
    ) -> Result<String> {
        // P7：出站前解析对端首选可达地址（内网优先探测 + TTL 缓存，
        // 无自报地址时原样返回 link.endpoint）
        let endpoint = self.resolve_peer_endpoint(ctx.clone(), link).await;

        // P8 call_peer facade：WS 优先——有活连接走长连接请求-响应（零额外
        // 握手开销）；无活连接时后台 best-effort 拨号（本次仍走 HTTP，连接
        // 建立后后续调用自动升级 WS）。业务侧对通道零感知。
        if let Some(reply) = self
            .try_send_over_ws(
                ctx.clone(),
                link,
                peer_agent_id,
                prompt,
                caller_declaration.clone(),
                &endpoint,
            )
            .await
        {
            return reply;
        }

        // 此前失败时回退 `Client::default()`（无超时）；构建失败直接上抛，绝不降级为裸客户端
        let http = crate::pkg::http::presets::with_timeout(Some(std::time::Duration::from_secs(
            FEDERATED_CALL_DEADLINE_SECS + 5,
        )))
        .build()?;
        let config = FederatedCallConfig {
            endpoint,
            auth_token: link.access_token.clone(),
            caller_declaration,
            deadline_secs: FEDERATED_CALL_DEADLINE_SECS,
            poll_interval_ms: FEDERATED_CALL_POLL_INTERVAL_MS,
        };
        let reply = execute_federated_agent_call(&http, peer_agent_id, &config, prompt).await?;
        log_info!(
            &ctx,
            "federated_agent_task",
            peer_org = link.peer_org_id,
            peer_agent = peer_agent_id,
            channel = "http",
            "联邦委派完成",
        );
        Ok(reply)
    }
}

impl OrganizationDalImpl {
    /// P8 `call_peer` 通道选择：WS 活连接则经长连接发 send_task 并等待响应
    ///
    /// 返回 `None` = 无活连接（或 WS 失败），调用方回退 HTTP 路径；
    /// 返回 `Some(result)` = 已完成（成功或带错误），不再回退。
    async fn try_send_over_ws(
        &self,
        ctx: RequestContext,
        link: &OrganizationLinkPo,
        peer_agent_id: &str,
        prompt: &str,
        caller_declaration: Option<String>,
        resolved_endpoint: &str,
    ) -> Option<Result<String>> {
        let peer_org = link.peer_org_id.clone();
        if !ws::registry().connected(&peer_org) {
            self.spawn_background_dial(ctx, link, caller_declaration, resolved_endpoint);
            return None;
        }

        // 与 HTTP 路径完全相同的参数形状（对端 consumer 反序列化同一 DTO）
        let task_id = uuid::Uuid::now_v7().to_string();
        let params = SendTaskParams {
            id: task_id.clone(),
            message: A2aMessage {
                role: "user".to_string(),
                parts: vec![A2aMessagePart::Text {
                    text: prompt.to_string(),
                }],
                message_id: None,
                task_id: Some(task_id),
            },
            session_id: None,
            metadata: None,
            notification_url: None,
        };
        let payload = match serde_json::to_value(&params) {
            Ok(v) => v,
            Err(e) => {
                return Some(Err(common::error::Error::internal(format!(
                    "federation ws params serialize failed: {}",
                    e
                ))));
            }
        };
        let correlation_id = uuid::Uuid::now_v7().to_string();

        match ws::request_over_ws(
            &peer_org,
            FEDERATION_CMD_SEND_TASK,
            correlation_id.clone(),
            payload,
        )
        .await
        {
            Ok(reply) => {
                log_info!(
                    &ctx,
                    "federated_agent_task",
                    peer_org = peer_org,
                    peer_agent = peer_agent_id,
                    channel = "ws",
                    "联邦委派完成",
                );
                Some(Self::parse_ws_send_response(reply))
            }
            Err(e) => {
                // WS 通道失败（超时/连接断开）：告警并回退 HTTP
                log_warn!(
                    "federation ws send_task failed (fall back to http): peer={} correlation_id={} err={}",
                    peer_org,
                    correlation_id,
                    e
                );
                None
            }
        }
    }

    /// 解析对端 send_task 响应负载 `{"ok":bool,"task":...,"error":...}`
    fn parse_ws_send_response(reply: serde_json::Value) -> Result<String> {
        let ok = reply.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok {
            let msg = reply
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("peer returned unknown error");
            return Err(common::err!(
                ThirdPartyError,
                "federation peer rejected send_task: {}",
                msg
            ));
        }
        let task = reply
            .get("task")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        Ok(extract_text_from_task_result(&task).unwrap_or_else(|| task.to_string()))
    }

    /// 后台 best-effort 拨号（防重入；失败仅告警，不影响本次 HTTP 委派）
    fn spawn_background_dial(
        &self,
        ctx: RequestContext,
        link: &OrganizationLinkPo,
        caller_declaration: Option<String>,
        resolved_endpoint: &str,
    ) {
        let peer_org = link.peer_org_id.clone();
        if !ws::registry().try_mark_dialing(&peer_org) {
            return; // 已有拨号在途
        }
        let url = ws::ws_url_from_base(resolved_endpoint);
        let token = link.access_token.clone();
        let link_local_org = link.local_org_id.clone();
        log_info!(
            &ctx,
            "federation_ws_dial",
            peer_org = peer_org,
            "后台拨号联邦长连接",
        );
        tokio::spawn(async move {
            if let Err(e) =
                ws::dial_peer(&link_local_org, &peer_org, url, token, caller_declaration).await
            {
                log_warn!(
                    "federation ws background dial failed: peer={} err={}",
                    peer_org,
                    e
                );
            }
            ws::registry().clear_dialing(&peer_org);
        });
    }
}
