//! 飞书 DAO HTTP 实现（多应用模型）
//!
//! 包含：
//! - tenant_access_token 获取与缓存（per-app，带提前 5 分钟刷新）
//! - 消息发送（`/open-apis/im/v1/messages`）
//! - WebSocket 长连接生命周期管理（per-app 连接池，委托 `ws` 模块）
//!
//! 凭证不来自全局配置：出站（push/test_connection）与入站（start_event_listener）
//! 均由调用方（DAL 层）传入已解析的 `LarkAppCredentials`，DAO 不做凭证解析。

use super::error::{LarkResponse, from_reqwest, validate_config};
use super::token::{SharedTokenCache, shared as shared_token_cache};
use super::ws::{WsState, WsTokenSource};
use super::{LarkAppCredentials, LarkDao};
use crate::models::events::LarkMessageEvent;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use common::api::{LarkWsAppMetrics, LarkWsMetrics};
use common::error::{Result, err};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

// ==================== 飞书 OpenAPI 端点 ====================

const API_BASE: &str = "https://open.feishu.cn";
const PATH_TOKEN: &str = "/open-apis/auth/v3/tenant_access_token/internal";
const PATH_SEND_MESSAGE: &str = "/open-apis/im/v1/messages";

// ==================== 工厂方法 + 单例 ====================

static LARK_DAO: OnceLock<Arc<dyn LarkDao>> = OnceLock::new();

/// 创建一个全新的飞书 DAO 实例（无全局凭证，凭证按调用传入）
pub fn new() -> Arc<dyn LarkDao> {
    Arc::new(LarkDaoHttpImpl::new())
}

/// 获取 LarkDao 单例
pub fn dao() -> Arc<dyn LarkDao> {
    LARK_DAO
        .get()
        .cloned()
        .expect("LarkDao not initialized, call init() first")
}

/// 初始化单例（无全局凭证，凭证由 DAL 层解析后按调用传入）
pub fn init() {
    let _ = LARK_DAO.set(new());
}

// ==================== 实现 ====================

pub struct LarkDaoHttpImpl {
    http: reqwest::Client,
    /// per-app token 缓存（app_id 键控；Arc 包装供 WS token source 共享，避免循环引用）
    token_caches: Arc<RwLock<HashMap<String, SharedTokenCache>>>,
    /// per-app WebSocket 连接状态（app_id 键控）
    ws_conns: RwLock<HashMap<String, WsState>>,
}

impl LarkDaoHttpImpl {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
            token_caches: Arc::new(RwLock::new(HashMap::new())),
            ws_conns: RwLock::new(HashMap::new()),
        }
    }

    /// 获取（或创建）指定应用的 token 缓存句柄
    async fn token_cache_for(
        caches: &RwLock<HashMap<String, SharedTokenCache>>,
        app_id: &str,
    ) -> SharedTokenCache {
        {
            let caches = caches.read().await;
            if let Some(cache) = caches.get(app_id) {
                return cache.clone();
            }
        }
        let mut caches = caches.write().await;
        caches
            .entry(app_id.to_string())
            .or_insert_with(shared_token_cache)
            .clone()
    }

    /// 获取指定应用的 tenant_access_token（带缓存，提前 5 分钟刷新）
    ///
    /// 使用双重检查锁防止并发刷新。
    pub async fn get_tenant_access_token(&self, app_id: &str, app_secret: &str) -> Result<String> {
        fetch_token_with_caches(&self.http, &self.token_caches, app_id, app_secret).await
    }

    /// 发送文本消息到指定 open_id 用户
    ///
    /// 返回飞书 message_id
    pub async fn send_text_message(
        &self,
        token: &str,
        open_id: &str,
        text: &str,
    ) -> Result<String> {
        // 飞书文本消息 content：{"text":"消息内容"}
        let content = serde_json::json!({ "text": text }).to_string();

        #[derive(Serialize)]
        struct SendMessageReq<'a> {
            receive_id: &'a str,
            msg_type: &'static str,
            content: String,
        }

        #[derive(Default, Deserialize)]
        struct SendMessageData {
            #[serde(default)]
            message_id: String,
        }

        let url = format!("{}{}?receive_id_type=open_id", API_BASE, PATH_SEND_MESSAGE);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&SendMessageReq {
                receive_id: open_id,
                msg_type: "text",
                content,
            })
            .send()
            .await
            .map_err(|e| from_reqwest("send_message", e))?
            .json::<LarkResponse<SendMessageData>>()
            .await
            .map_err(|e| from_reqwest("send_message", e))?;

        let data = resp.check("send_message")?;
        Ok(data.message_id)
    }

    /// HTTP client 引用（供 ws 模块使用）
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http
    }
}

impl Default for LarkDaoHttpImpl {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== token 获取（自由函数，供 DAO 与 WS token source 共享） ====================

/// 获取指定应用的 tenant_access_token（带缓存，提前 5 分钟刷新，双重检查锁防并发刷新）
async fn fetch_token_with_caches(
    http: &reqwest::Client,
    caches: &RwLock<HashMap<String, SharedTokenCache>>,
    app_id: &str,
    app_secret: &str,
) -> Result<String> {
    let cache = LarkDaoHttpImpl::token_cache_for(caches, app_id).await;
    // 第一次读检查
    {
        let c = cache.read().await;
        if let Some(token) = c.get_valid_token() {
            return Ok(token);
        }
    }
    // 升级写锁
    let mut c = cache.write().await;
    // 第二次检查（防止等待期间其他任务已刷新）
    if let Some(token) = c.get_valid_token() {
        return Ok(token);
    }
    let (token, expire) = fetch_tenant_access_token(http, app_id, app_secret).await?;
    c.update(token.clone(), expire);
    Ok(token)
}

/// 调用飞书 API 获取 tenant_access_token
async fn fetch_tenant_access_token(
    http: &reqwest::Client,
    app_id: &str,
    app_secret: &str,
) -> Result<(String, u64)> {
    validate_config(app_id, app_secret)?;

    #[derive(Serialize)]
    struct TokenReq<'a> {
        app_id: &'a str,
        app_secret: &'a str,
    }

    #[derive(Deserialize)]
    struct TokenResp {
        code: i32,
        #[serde(default)]
        msg: String,
        #[serde(default)]
        tenant_access_token: String,
        #[serde(default)]
        expire: u64,
    }

    let url = format!("{}{}", API_BASE, PATH_TOKEN);
    let resp = http
        .post(&url)
        .json(&TokenReq { app_id, app_secret })
        .send()
        .await
        .map_err(|e| from_reqwest("fetch_token", e))?
        .json::<TokenResp>()
        .await
        .map_err(|e| from_reqwest("fetch_token", e))?;

    if resp.code != 0 {
        return Err(err!(
            ThirdPartyError,
            "lark fetch_token failed: code={} msg={}",
            resp.code,
            resp.msg
        ));
    }
    if resp.tenant_access_token.is_empty() {
        return Err(err!(
            ThirdPartyError,
            "lark fetch_token returned empty token"
        ));
    }
    Ok((resp.tenant_access_token, resp.expire))
}

/// WS 连接的 token 来源：重连时实时取 token（共享 per-app 缓存，避免持有 DAO 自引用循环）
struct LarkWsTokenSource {
    http: reqwest::Client,
    token_caches: Arc<RwLock<HashMap<String, SharedTokenCache>>>,
    app_id: String,
    app_secret: String,
}

#[async_trait::async_trait]
impl WsTokenSource for LarkWsTokenSource {
    async fn token(&self) -> Result<String> {
        fetch_token_with_caches(
            &self.http,
            &self.token_caches,
            &self.app_id,
            &self.app_secret,
        )
        .await
    }
}

// 出站凭证解析归 DAL 层（dal::message_channel）：内联字段已在二期重构中删除，
// 渠道仅存引用，DAL 解析后以 LarkAppCredentials 传入本 DAO。

#[async_trait::async_trait]
impl LarkDao for LarkDaoHttpImpl {
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
        credentials: &LarkAppCredentials,
    ) -> Result<()> {
        let config = channel.config();
        let open_id = config.lark_open_id.as_ref().ok_or_else(|| {
            err!(
                InvalidRequest,
                "飞书渠道缺少 lark_open_id 配置 channel_id={}",
                channel.po.id
            )
        })?;

        let content = &message.po.content;
        if content.is_empty() {
            return Ok(());
        }

        let token = self
            .get_tenant_access_token(&credentials.app_id, &credentials.app_secret)
            .await?;
        let message_id = self.send_text_message(&token, open_id, content).await?;
        log_info!(
            &ctx,
            "lark_push",
            "推送消息到飞书 channel_id={} app_id={} open_id={} lark_message_id={}",
            channel.po.id,
            credentials.app_id,
            open_id,
            message_id
        );
        Ok(())
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        credentials: &LarkAppCredentials,
    ) -> Result<()> {
        self.get_tenant_access_token(&credentials.app_id, &credentials.app_secret)
            .await?;
        log_info!(
            &ctx,
            "lark_test_connection",
            "飞书连接测试成功 app_id={}",
            credentials.app_id
        );
        Ok(())
    }

    async fn start_event_listener(&self, credentials: LarkAppCredentials) -> Result<()> {
        validate_config(&credentials.app_id, &credentials.app_secret)?;

        // 幂等：已连接直接返回
        {
            let conns = self.ws_conns.read().await;
            if conns.contains_key(&credentials.app_id) {
                log_debug!(
                    "lark event listener already running for app_id={}",
                    credentials.app_id
                );
                return Ok(());
            }
        }

        // 预热 token 缓存 + 构造 WS token source（重连时实时刷新 token）
        self.get_tenant_access_token(&credentials.app_id, &credentials.app_secret)
            .await?;
        let token_source = Arc::new(LarkWsTokenSource {
            http: self.http.clone(),
            token_caches: self.token_caches.clone(),
            app_id: credentials.app_id.clone(),
            app_secret: credentials.app_secret.clone(),
        });

        let mut conns = self.ws_conns.write().await;
        // 双重检查（防止等待期间其他任务已建连）
        if conns.contains_key(&credentials.app_id) {
            return Ok(());
        }

        let app_id = credentials.app_id.clone();
        let state =
            super::ws::start_event_loop(self.http.clone(), app_id.clone(), token_source).await?;

        conns.insert(app_id.clone(), state);
        log_info!("lark event listener started for app_id={}", app_id);
        Ok(())
    }

    async fn stop_event_listener(&self, app_id: &str) -> Result<()> {
        let state = self.ws_conns.write().await.remove(app_id);
        if let Some(state) = state {
            super::ws::stop_event_loop(state).await;
            log_info!("lark event listener stopped for app_id={}", app_id);
        }
        Ok(())
    }

    async fn stop_all_event_listeners(&self) -> Result<()> {
        let states: Vec<(String, WsState)> = self.ws_conns.write().await.drain().collect();
        for (app_id, state) in states {
            super::ws::stop_event_loop(state).await;
            log_info!("lark event listener stopped for app_id={}", app_id);
        }
        Ok(())
    }

    async fn is_listening(&self, app_id: &str) -> bool {
        self.ws_conns.read().await.contains_key(app_id)
    }

    async fn listener_stats(&self) -> LarkWsMetrics {
        let conns = self.ws_conns.read().await;
        let mut apps = Vec::with_capacity(conns.len());
        for (app_id, state) in conns.iter() {
            let snap = state.conn_state_snapshot().await;
            apps.push(LarkWsAppMetrics {
                app_id: app_id.clone(),
                state: snap.phase.as_str().to_string(),
                reconnect_count: snap.reconnect_count,
            });
        }
        LarkWsMetrics {
            active_connections: apps.len() as u64,
            apps,
        }
    }
}

// 防止未使用警告（event 模块在 push 链路中被 trait 间接使用）
#[allow(dead_code)]
fn _ensure_event_linked(_: &LarkMessageEvent) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::dao::lark::LarkDao;

    /// per-app token 缓存隔离：不同 app_id 各自独立条目
    #[tokio::test]
    async fn token_cache_is_isolated_per_app() {
        let dao = LarkDaoHttpImpl::new();
        let _ = LarkDaoHttpImpl::token_cache_for(&dao.token_caches, "cli_app_a").await;
        let _ = LarkDaoHttpImpl::token_cache_for(&dao.token_caches, "cli_app_b").await;
        // 重复访问同一 app 不新增条目
        let _ = LarkDaoHttpImpl::token_cache_for(&dao.token_caches, "cli_app_a").await;
        let caches = dao.token_caches.read().await;
        assert_eq!(caches.len(), 2);
        assert!(caches.contains_key("cli_app_a"));
        assert!(caches.contains_key("cli_app_b"));
    }

    /// WS 连接池初始为空，stop 对不存在的 app 幂等不报错；无监听时 stats 为空快照
    #[tokio::test]
    async fn listener_state_starts_empty_and_stop_is_idempotent() {
        let dao = LarkDaoHttpImpl::new();
        assert!(!dao.is_listening("cli_app_x").await);
        dao.stop_event_listener("cli_app_x").await.unwrap();
        dao.stop_all_event_listeners().await.unwrap();
        assert!(!dao.is_listening("cli_app_x").await);
        let stats = dao.listener_stats().await;
        assert_eq!(stats.active_connections, 0);
        assert!(stats.apps.is_empty());
    }
}
