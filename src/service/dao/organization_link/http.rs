//! 联邦出站 HTTP 客户端 DAO
//!
//! 组织组网的对外 API 出站调用（AGENTS.md 分层职责：DAO 层负责外部 API 出站）。
//! 当前仅承载建联握手：调对端机器端点 `POST /links/pairing/verify` 完成配对码
//! 验证与双向凭证交换；后续目录同步（S5）复用同一客户端。
//!
//! 无状态（每次调用新建带超时的 reqwest Client），不依赖 DB pool，
//! 故不走 `dao::init_all()` 注册，首次 `client()` 惰性构建单例。

use std::sync::Arc;

use async_trait::async_trait;
use common::api::{
    ApiResponse, DirectoryResponse, DirectorySyncRequest, DirectorySyncResponse,
    PeerOrgDirectoryEntry, VerifyPairingCodeRequest, VerifyPairingCodeResponse,
};
use common::error::{Error, Result};

/// 对端 verify 端点路径（与 `router.rs` root 层直挂路径对齐，评审稿 D7）
const VERIFY_PATH: &str = "/api/v1/organization/links/pairing/verify";

/// 对端目录端点路径（机器侧，契约凭证鉴权）
const DIRECTORY_PATH: &str = "/api/v1/organization/links/directory";

/// 对端目录推送端点路径（机器侧，契约凭证鉴权）
const DIRECTORY_SYNC_PATH: &str = "/api/v1/organization/links/directory/sync";

/// 出站调用超时（秒）：建联是低频管理操作，10s 足够覆盖公网 RTT
const OUTBOUND_TIMEOUT_SECS: u64 = 10;

/// 联邦出站 HTTP 客户端接口
#[async_trait]
pub trait FederationHttpClient: Send + Sync {
    /// 调对端机器端点 verify：验证配对码 + 交换凭证
    ///
    /// - 对端返回非 0 业务码（配对码无效/过期/已用）→ 统一转 unauthorized
    ///   （不区分具体原因，与防枚举策略一致）
    /// - 网络/超时/非 JSON 响应 → internal 错误（建联失败可重试：重新签发配对码）
    async fn verify_pairing_code(
        &self,
        peer_endpoint: &str,
        req: &VerifyPairingCodeRequest,
    ) -> Result<VerifyPairingCodeResponse>;

    /// 拉取对端组织目录（契约凭证鉴权，`Authorization: Bearer <access_token>`）
    async fn fetch_directory(
        &self,
        peer_endpoint: &str,
        access_token: &str,
    ) -> Result<Vec<PeerOrgDirectoryEntry>>;

    /// 推送本地目录给对端（契约凭证鉴权）
    async fn push_directory(
        &self,
        peer_endpoint: &str,
        access_token: &str,
        orgs: Vec<PeerOrgDirectoryEntry>,
    ) -> Result<()>;
}

/// 构造出站客户端（统一超时；契约凭证由 per-request `bearer_auth` 携带）
fn outbound_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(OUTBOUND_TIMEOUT_SECS))
        .build()
        .map_err(|e| Error::internal(format!("构建 HTTP 客户端失败: {}", e)))
}

/// 解析 ApiResponse 包裹的目录响应
async fn parse_directory(resp: reqwest::Response, url: &str) -> Result<Vec<PeerOrgDirectoryEntry>> {
    let api: ApiResponse<DirectoryResponse> = resp.json().await.map_err(|e| {
        Error::internal(format!(
            "对端 directory 响应解析失败（{}）：对端可能不是 ai_orz 节点: {}",
            url, e
        ))
    })?;
    if !api.is_success() {
        return Err(Error::unauthorized(format!(
            "对端拒绝目录访问：{}",
            api.message
        )));
    }
    Ok(api.data.map(|d| d.orgs).unwrap_or_default())
}

/// reqwest 实现
pub struct ReqwestFederationClient;

#[async_trait]
impl FederationHttpClient for ReqwestFederationClient {
    async fn verify_pairing_code(
        &self,
        peer_endpoint: &str,
        req: &VerifyPairingCodeRequest,
    ) -> Result<VerifyPairingCodeResponse> {
        let base = peer_endpoint.trim().trim_end_matches('/');
        if base.is_empty() {
            return Err(Error::bad_request("对端地址不能为空"));
        }
        let url = format!("{}{}", base, VERIFY_PATH);

        let client = outbound_client()?;

        let resp = client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| Error::internal(format!("调用对端 verify 失败 ({}): {}", url, e)))?;

        let api: ApiResponse<VerifyPairingCodeResponse> = resp.json().await.map_err(|e| {
            Error::internal(format!(
                "对端 verify 响应解析失败（{}）：对端可能不是 ai_orz 节点: {}",
                url, e
            ))
        })?;

        if !api.is_success() {
            // 统一 unauthorized，不区分无效/过期/已用（防枚举，评审稿 §6.3）
            return Err(Error::unauthorized(format!(
                "对端拒绝建联：{}",
                api.message
            )));
        }

        api.data
            .ok_or_else(|| Error::internal("对端 verify 响应缺少 data 字段"))
    }

    async fn fetch_directory(
        &self,
        peer_endpoint: &str,
        access_token: &str,
    ) -> Result<Vec<PeerOrgDirectoryEntry>> {
        let base = peer_endpoint.trim().trim_end_matches('/');
        if base.is_empty() {
            return Err(Error::bad_request("对端地址不能为空"));
        }
        let url = format!("{}{}", base, DIRECTORY_PATH);

        let client = outbound_client()?;
        let resp = client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| Error::internal(format!("拉取对端目录失败 ({}): {}", url, e)))?;

        parse_directory(resp, &url).await
    }

    async fn push_directory(
        &self,
        peer_endpoint: &str,
        access_token: &str,
        orgs: Vec<PeerOrgDirectoryEntry>,
    ) -> Result<()> {
        let base = peer_endpoint.trim().trim_end_matches('/');
        if base.is_empty() {
            return Err(Error::bad_request("对端地址不能为空"));
        }
        let url = format!("{}{}", base, DIRECTORY_SYNC_PATH);

        let client = outbound_client()?;
        let resp = client
            .post(&url)
            .bearer_auth(access_token)
            .json(&DirectorySyncRequest { orgs })
            .send()
            .await
            .map_err(|e| Error::internal(format!("推送本地目录失败 ({}): {}", url, e)))?;

        let api: ApiResponse<DirectorySyncResponse> = resp.json().await.map_err(|e| {
            Error::internal(format!(
                "对端 directory/sync 响应解析失败（{}）：对端可能不是 ai_orz 节点: {}",
                url, e
            ))
        })?;
        if !api.is_success() {
            return Err(Error::unauthorized(format!(
                "对端拒绝目录推送：{}",
                api.message
            )));
        }
        Ok(())
    }
}

static CLIENT: std::sync::OnceLock<Arc<dyn FederationHttpClient>> = std::sync::OnceLock::new();

/// 获取联邦 HTTP 客户端单例（无状态，惰性构建）
pub fn client() -> Arc<dyn FederationHttpClient> {
    CLIENT
        .get_or_init(|| Arc::new(ReqwestFederationClient) as Arc<dyn FederationHttpClient>)
        .clone()
}
