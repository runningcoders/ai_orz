//! Organization DAL 模块
//!
//! 职责：Organization 领域的数据访问层，封装 OrganizationDao 提供统一的查询接口
//! 注意：User 相关操作已移至 User DAL，跨领域编排在 Domain 层完成

use crate::models::events::OrganizationChangedEvent;
use crate::models::events::federation::FEDERATION_CMD_SEND_TASK;
use crate::models::organization::OrganizationPo;
use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::dao::agent_runtime::a2a::{
    FederatedCallConfig, execute_federated_agent_call, extract_text_from_task_result,
};
use crate::service::dao::organization;
use crate::service::dao::organization::{OrganizationDao, OrganizationQuery, PeerOrgUpsert};
use crate::service::dao::organization_link::ws;
use common::api::OrganizationConfig;
use common::api::a2a::{A2aMessage, A2aMessagePart, SendTaskParams};
use common::error::Result;
use std::sync::{Arc, OnceLock};

/// 联邦 Agent 委派的全程预算（send + 轮询，秒）
const FEDERATED_CALL_DEADLINE_SECS: u64 = 120;
/// 联邦 Agent 委派的 tasks/get 轮询间隔（毫秒）
const FEDERATED_CALL_POLL_INTERVAL_MS: u64 = 1000;

// ==================== 单例管理 ====================

static ORGANIZATION_DAL: OnceLock<Arc<dyn OrganizationDal + Send + Sync>> = OnceLock::new();

/// 获取 Organization DAL 单例
pub fn dal() -> Arc<dyn OrganizationDal + Send + Sync> {
    ORGANIZATION_DAL.get().cloned().unwrap()
}

/// 初始化 Organization DAL
pub fn init() {
    let _ = ORGANIZATION_DAL.set(new(
        organization::dao(),
        crate::service::dao::organization_link::dao(),
    ));
}

/// 创建 Organization DAL（返回 trait 对象）
pub fn new(
    organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
    link_dao: Arc<dyn crate::service::dao::organization_link::OrganizationLinkDao + Send + Sync>,
) -> Arc<dyn OrganizationDal + Send + Sync> {
    Arc::new(OrganizationDalImpl {
        organization_dao,
        link_dao,
    })
}

// ==================== DAL 接口 ====================

/// Organization DAL 接口
#[async_trait::async_trait]
pub trait OrganizationDal: Send + Sync {
    /// 检查系统是否已经初始化
    ///
    /// 通过检查 organizations 表是否有记录判断
    async fn is_initialized(&self, ctx: RequestContext) -> Result<bool>;

    /// 根据 ID 获取组织
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>>;

    /// 根据邀请码获取组织（仅返回未删除的有效组织）
    async fn find_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>>;

    /// 读取组织级配置（透传 DAO，带缓存）
    async fn get_org_config(&self, ctx: RequestContext, org_id: &str)
    -> Result<OrganizationConfig>;

    /// 写入组织级配置（透传 DAO，写穿缓存）
    async fn update_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()>;

    /// 创建组织
    async fn create(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>>;

    /// 获取所有组织
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>>;

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<()>;

    /// 统计组织总数
    async fn count_organizations(&self, ctx: RequestContext) -> Result<u64>;

    /// 统计符合查询条件的组织数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<u64>;

    // ==================== 联邦影子（静默写入：不发布 organization.changed）==========
    //
    // organizations 表承载两类数据：业务组织（Local/Linked，上方方法管，写后发事件）
    // 与远端影子（Remote，复制同步所得，下方方法管，静默写入）。
    // 影子同步是**复制，不是业务变更**——不发布事件是刻意的：若发布，
    // 对端推送 → 写影子 → 发事件 → consumer 再推送 → 对端又推回，形成事件风暴。
    // 递归防护是结构性的：影子写入路径不经过事件发布点，不依赖任何运行时开关。
    // 新增副作用（缓存/审计）时：业务方法加在 create/update/delete；影子方法另行评估。

    /// 目录同步所得 Remote 影子 upsert（新者胜 / 不动 scope / 护 Local，评审稿 §5.2）
    ///
    /// 返回是否发生写入（false = 跳过），供上层审计。
    async fn upsert_remote_shadow(&self, ctx: RequestContext, peer: &PeerOrgUpsert)
    -> Result<bool>;

    /// 直接建联的对端影子 upsert（强制 Linked，评审稿 R5 护 Local）
    ///
    /// 返回是否发生写入，供上层审计。
    async fn upsert_linked_shadow(&self, ctx: RequestContext, peer: &PeerOrgUpsert)
    -> Result<bool>;

    /// 断联组合操作：连接置 Revoked + 对端影子 Linked → Remote
    ///
    /// 组合 link DAO（links 表）与 organization DAO（organizations 表）：
    /// 先断链后降影，非同一事务（跨 DAO 不开分布式事务，YAGNI）。两步各自幂等，
    /// 第二步失败时重试本方法即可修复（重放断链无害）。不发布事件（断联是
    /// 链接层状态变更，不改变本地组织元信息，无需触发目录变更推送）。
    async fn revoke_link(
        &self,
        ctx: RequestContext,
        link_id: &str,
        peer_org_id: &str,
    ) -> Result<()>;

    /// 批量读取组织自报联邦地址全集（P7，透传 DAO）
    async fn list_addresses(&self, ctx: RequestContext) -> Result<Vec<(String, String)>>;

    /// 解析对端当前首选可达地址（P7 内外网可达性）
    ///
    /// 组合 organization DAO（对端自报地址候选池）与可达性解析器（内网优先
    /// first-match 探测 + TTL 缓存）。永返回值：对端无自报地址或全不通时
    /// 维持 `link.endpoint` 主地址，出站失败语义与 P7 之前一致。
    async fn resolve_peer_endpoint(&self, ctx: RequestContext, link: &OrganizationLinkPo)
    -> String;

    // ==================== 跨组织联邦调用（P4）==========

    /// 联邦 Agent 委派传输层：经指定连接调对端 A2A 出站（send → 轮询 tasks/get 到终态）
    ///
    /// endpoint / auth_token 取自 link（access_token = 对端所发出站凭证），
    /// 携带可选 `X-Federation-Caller` 声明头（已序列化 JSON）。连接的合法性
    /// （Active / 能力白名单）与路由决策由 domain 层完成，本方法只管传输。
    async fn send_federated_agent_task(
        &self,
        ctx: RequestContext,
        link: &OrganizationLinkPo,
        peer_agent_id: &str,
        prompt: &str,
        caller_declaration: Option<String>,
    ) -> Result<String>;
}

// ==================== DAL 实现 ====================

/// Organization DAL 实现
struct OrganizationDalImpl {
    organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
    link_dao: Arc<dyn crate::service::dao::organization_link::OrganizationLinkDao + Send + Sync>,
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

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                FEDERATED_CALL_DEADLINE_SECS + 5,
            ))
            .build()
            .unwrap_or_default();
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
