//! 飞书 DAO HTTP 实现
//!
//! 包含：
//! - tenant_access_token 获取与缓存（带提前 5 分钟刷新）
//! - 消息发送（`/open-apis/im/v1/messages`）
//! - WebSocket 长连接生命周期管理（委托 `ws` 模块）

use super::error::{LarkResponse, from_reqwest, validate_config};
use super::event::LarkMessageEvent;
use super::token::{SharedTokenCache, TokenCache};
use super::ws::WsState;
use super::{LarkDao, LarkEventHandler};
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use common::config::LarkConfig;
use common::error::{Result, err};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

// ==================== 飞书 OpenAPI 端点 ====================

const API_BASE: &str = "https://open.feishu.cn";
const PATH_TOKEN: &str = "/open-apis/auth/v3/tenant_access_token/internal";
const PATH_SEND_MESSAGE: &str = "/open-apis/im/v1/messages";

// ==================== 工厂方法 + 单例 ====================

static LARK_DAO: OnceLock<Arc<dyn LarkDao>> = OnceLock::new();

/// 创建一个全新的飞书 DAO 实例（默认空配置，用于测试或未启用场景）
pub fn new() -> Arc<dyn LarkDao> {
    Arc::new(LarkDaoHttpImpl::new(LarkConfig::default()))
}

/// 创建一个全新的飞书 DAO 实例（带配置，用于测试）
pub fn new_with_config(config: LarkConfig) -> Arc<dyn LarkDao> {
    Arc::new(LarkDaoHttpImpl::new(config))
}

/// 获取 LarkDao 单例
pub fn dao() -> Arc<dyn LarkDao> {
    LARK_DAO
        .get()
        .cloned()
        .expect("LarkDao not initialized, call init() first")
}

/// 初始化单例（使用全局 AppConfig 中的 [lark] 配置）
///
/// 在测试环境或未调用 `config::init()` 的场景下，
/// 自动 fallback 到默认配置（app_id/app_secret 为空，飞书功能不可用但不会 panic）。
pub fn init() {
    let config = crate::config::try_get()
        .map(|c| c.lark.clone())
        .unwrap_or_default();
    let _ = LARK_DAO.set(Arc::new(LarkDaoHttpImpl::new(config)) as Arc<dyn LarkDao>);
}

// ==================== 实现 ====================

pub struct LarkDaoHttpImpl {
    config: LarkConfig,
    http: reqwest::Client,
    token_cache: SharedTokenCache,
    ws_state: Arc<RwLock<Option<WsState>>>,
}

impl LarkDaoHttpImpl {
    pub fn new(config: LarkConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            token_cache: Arc::new(RwLock::new(TokenCache::new())),
            ws_state: Arc::new(RwLock::new(None)),
        }
    }

    /// 获取 tenant_access_token（带缓存，提前 5 分钟刷新）
    ///
    /// 使用双重检查锁防止并发刷新。
    pub async fn get_tenant_access_token(&self) -> Result<String> {
        // 第一次读检查
        {
            let cache = self.token_cache.read().await;
            if let Some(token) = cache.get_valid_token() {
                return Ok(token);
            }
        }
        // 升级写锁
        let mut cache = self.token_cache.write().await;
        // 第二次检查（防止等待期间其他线程已刷新）
        if let Some(token) = cache.get_valid_token() {
            return Ok(token);
        }
        let (token, expire) = self.fetch_tenant_access_token().await?;
        cache.update(token.clone(), expire);
        Ok(token)
    }

    /// 调用飞书 API 获取 tenant_access_token
    async fn fetch_tenant_access_token(&self) -> Result<(String, u64)> {
        validate_config(&self.config.app_id, &self.config.app_secret)?;

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
        let resp = self
            .http
            .post(&url)
            .json(&TokenReq {
                app_id: &self.config.app_id,
                app_secret: &self.config.app_secret,
            })
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

    /// 发送文本消息到指定 open_id 用户
    ///
    /// 返回飞书 message_id
    pub async fn send_text_message(
        &self,
        token: &str,
        open_id: &str,
        text: &str,
    ) -> Result<String> {
        validate_config(&self.config.app_id, &self.config.app_secret)?;

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

    /// 配置引用（供 ws 模块使用）
    pub fn config(&self) -> &LarkConfig {
        &self.config
    }

    /// HTTP client 引用（供 ws 模块使用）
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http
    }

    /// token cache 引用（供 ws 模块使用）
    pub fn token_cache(&self) -> SharedTokenCache {
        self.token_cache.clone()
    }
}

#[async_trait::async_trait]
impl LarkDao for LarkDaoHttpImpl {
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
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

        let token = self.get_tenant_access_token().await?;
        let message_id = self.send_text_message(&token, open_id, content).await?;
        log_info!(
            &ctx,
            "lark_push",
            "推送消息到飞书 channel_id={} open_id={} lark_message_id={}",
            channel.po.id,
            open_id,
            message_id
        );
        Ok(())
    }

    async fn test_connection(&self, ctx: RequestContext, _channel: &MessageChannel) -> Result<()> {
        self.get_tenant_access_token().await?;
        log_info!(&ctx, "lark_test_connection", "飞书连接测试成功");
        Ok(())
    }

    async fn start_event_listener(&self, handler: Arc<dyn LarkEventHandler>) -> Result<()> {
        validate_config(&self.config.app_id, &self.config.app_secret)?;

        let mut ws_state = self.ws_state.write().await;
        if ws_state.is_some() {
            return Err(err!(Conflict, "飞书事件监听已启动"));
        }

        let state = super::ws::start_event_loop(
            self.http.clone(),
            self.config.clone(),
            self.token_cache.clone(),
            handler,
        )
        .await?;

        *ws_state = Some(state);
        log_info!("lark event listener started");
        Ok(())
    }

    async fn stop_event_listener(&self) -> Result<()> {
        let mut ws_state = self.ws_state.write().await;
        if let Some(state) = ws_state.take() {
            super::ws::stop_event_loop(state).await;
            log_info!("lark event listener stopped");
        }
        Ok(())
    }
}

// 防止未使用警告（event 模块在 push 链路中被 trait 间接使用）
#[allow(dead_code)]
fn _ensure_event_linked(_: &LarkMessageEvent) {}
