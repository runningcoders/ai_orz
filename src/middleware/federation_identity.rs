//! 联邦身份解析（跨传输层复用件，S2 签名鉴权）
//!
//! 从「每请求 Ed25519 签名（四个 `X-Federation-*` 头）+ 可选身份声明」解析出
//! 联邦调用方在本端的完整身份，**不绑定任何传输层**：HTTP 中间件
//! （[`crate::middleware::a2a_auth`]）、机器侧 handler（directory 等）与
//! 联邦 WS 握手共用同一份逻辑，避免多套鉴权实现漂移。
//!
//! 职责边界：
//! - 本模块只做「提取 + 解析 + 判定」，输入是纯数据（签名证明 + 方法/路径/
//!   body 哈希 + 已解析的声明），不碰请求体；
//! - 错误统一用 [`common::error::Error`] 表达，由调用方按 `code` 映射为
//!   自己的协议错误（HTTP 状态码 / WS 错误帧）；
//! - 解析结果 [`FederationIdentity`] 是纯数据，各传输层自行决定如何注入
//!   （HTTP 侧写 header 交给 request_context_middleware，长连接侧挂在会话上）。

use axum::http::{HeaderMap, HeaderValue};
use common::api::{CAPABILITY_A2A_TASK, FederationCallerDeclaration};
use common::constants::http_header;
use common::error::{Error, Result};

use crate::pkg::RequestContext;
use crate::service::domain::organization::{self, FederationRequestProof};

/// 联邦调用方在本端的身份（P6 接待模型落地结果）
#[derive(Debug, Clone)]
pub struct FederationIdentity {
    /// 目标组织（本端，数据作用域 = B）
    pub local_org_id: String,
    /// 发起组织（对端，审计/计量维度 = A）
    pub peer_org_id: String,
    /// 接待用户 ID：联邦访客的内部对接身份，此后 project/消息/权限与本地用户同构
    pub reception_user_id: String,
    /// 展示名：`federation:{peer_org_id}`
    pub username: String,
}

impl FederationIdentity {
    /// 注入到 HTTP header（供 `request_context_middleware` 读取）
    ///
    /// 联邦调用不获得任何本地角色权限，故不注入 `X-User-Role`。
    pub fn apply_to_headers(&self, headers: &mut HeaderMap) {
        insert_header(headers, http_header::USER_ID, &self.reception_user_id);
        insert_header(headers, http_header::USERNAME, &self.username);
        insert_header(headers, http_header::ORGANIZATION_ID, &self.local_org_id);
        insert_header(
            headers,
            http_header::CALLER_ORGANIZATION_ID,
            &self.peer_org_id,
        );
        headers.insert(http_header::CALLER_TYPE, HeaderValue::from_static("user"));
    }
}

/// 从 header 提取联邦签名证明（S2：四件套 `X-Federation-Key-Id / Timestamp /
/// Nonce / Signature`，缺一即 None → 调用方 401，无回退路径）
pub fn extract_federation_proof(headers: &HeaderMap) -> Option<FederationRequestProof> {
    let header_str = |name: &str| -> Option<String> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let key_id = header_str(http_header::FEDERATION_KEY_ID)?;
    let timestamp = header_str(http_header::FEDERATION_TIMESTAMP)?;
    let nonce = header_str(http_header::FEDERATION_NONCE)?;
    let signature = header_str(http_header::FEDERATION_SIGNATURE)?;
    Some(FederationRequestProof {
        key_id,
        timestamp: timestamp.parse().ok()?,
        nonce,
        signature,
    })
}

/// 解析联邦身份：签名验签 → 能力门禁 → 声明一致性 → 接待用户映射
///
/// # 参数
/// - `proof`：联邦签名四件套（[`extract_federation_proof`] 提取；缺失调用方直接 401）
/// - `method` / `path_with_query` / `body_hash`：签名规范串三要素
///   （规范串 = `method\npath\ntimestamp\nnonce\nsha256(body)`，见 `pkg/crypto/did`）
/// - `declaration`：对端的身份声明，缺失 = 连接级匿名调用
///
/// # 错误
/// - `401` 验签失败（连接不存在/已断联/时间窗/nonce 重放/签名不符/声明不一致）
/// - `403` 连接未开放 `a2a_task` 能力
/// - `500` 组织无可用接待用户（服务端配置问题，fail-closed）
pub async fn resolve_federation_identity(
    proof: &FederationRequestProof,
    method: &str,
    path_with_query: &str,
    body_hash: &str,
    declaration: Option<&FederationCallerDeclaration>,
) -> Result<FederationIdentity> {
    // 1) 入站验签（key_id 归属 Active 连接 → 时间窗 → nonce → Ed25519 验签；
    //    任一环节失败统一 401 防枚举，fail-closed 无回退路径）
    let link = organization::domain()
        .organization_manage()
        .authenticate_federation_request(
            RequestContext::new_system(),
            proof,
            method,
            path_with_query,
            body_hash,
        )
        .await
        .map_err(|e| {
            sys_debug!("federation signature rejected: {}", e);
            Error::unauthorized("联邦签名验证失败")
        })?;

    // 2) 连接级能力门禁（S3 合约授权）：能力集由 active 合约派生（唯一事实源），
    //    无 active 合约 = 空集（fail-closed）；未开放 a2a_task 的连接不允许跨组织委派
    let capabilities = organization::domain()
        .organization_manage()
        .contract_capabilities(
            RequestContext::new_system(),
            &link.local_org_id,
            &link.peer_org_id,
        )
        .await?;
    if !capabilities.iter().any(|c| c == CAPABILITY_A2A_TASK) {
        return Err(Error::forbidden("这条连接未开放 a2a_task 能力"));
    }

    // 3) 声明一致性：caller_org 与连接归属不符 → 401（防跨连接冒充发起组织）。
    //    身份锚点已是密码学强证明（签名绑定 key_id = 连接的 peer_did），声明仅作
    //    审计/计量，不参与内部身份构造——内部身份恒为本端接待用户。
    let declared_org = declaration.and_then(|d| d.caller_org.as_ref());
    if let Some(declared_org) = declared_org
        && declared_org != &link.peer_org_id
    {
        return Err(Error::unauthorized("声明组织与连接归属不一致"));
    }

    // 4) 接待用户映射（P6）：联邦访客的内部身份 = 本端接待用户。
    //    无可用接待用户（组织无管理员）= 服务端配置问题，fail-closed。
    let reception_user = organization::domain()
        .user_manage()
        .reception_user(RequestContext::new_system(), &link.local_org_id)
        .await
        .map_err(|e| {
            sys_debug!("reception user resolve failed: {}", e);
            Error::internal("组织无可用接待用户，无法受理联邦请求")
        })?;

    let username = format!("federation:{}", link.peer_org_id);
    Ok(FederationIdentity {
        local_org_id: link.local_org_id,
        peer_org_id: link.peer_org_id,
        username,
        reception_user_id: reception_user.id,
    })
}

/// 写入 header，非法 header 值静默跳过（身份字段均为服务端生成的 ID，理论上不会触发）
fn insert_header(headers: &mut HeaderMap, name: &'static str, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(name, v);
    }
}
