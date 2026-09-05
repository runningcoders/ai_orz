//! 联邦身份解析（跨传输层复用件，P7/P8 共用地基）
//!
//! 从「连接级凭证 + 可选身份声明」解析出联邦调用方在本端的完整身份，
//! **不绑定任何传输层**：HTTP 中间件（[`crate::middleware::a2a_auth`]）与
//! 未来的长连接握手（方向性组网 P8）共用同一份逻辑，避免两套鉴权实现漂移。
//!
//! 职责边界：
//! - 本模块只做「解析 + 判定」，输入是纯数据（凭证字符串 + 已解析的声明），
//!   不碰 `axum::Request`，也不感知消息来自 HTTP 头还是 WS 帧；
//! - 错误统一用 [`common::error::Error`] 表达，由调用方按 `code` 映射为
//!   自己的协议错误（HTTP 状态码 / WS 错误帧）；
//! - 解析结果 [`FederationIdentity`] 是纯数据，各传输层自行决定如何注入
//!   （HTTP 侧写 header 交给 request_context_middleware，长连接侧挂在会话上）。

use axum::http::{HeaderMap, HeaderValue};
use common::api::{CAPABILITY_A2A_TASK, FederationCallerDeclaration};
use common::constants::http_header;
use common::error::{Error, Result};

use crate::pkg::RequestContext;
use crate::service::domain::organization;

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

/// 解析联邦身份：凭证鉴权 → 能力门禁 → 声明一致性 → 接待用户映射
///
/// # 参数
/// - `credential`：连接级凭证（`Authorization: Bearer` 的原始 token，
///   或长连接握手帧里携带的等价字段）
/// - `declaration`：对端的身份声明，缺失 = 连接级匿名调用，仅凭凭证认定发起方
///
/// # 错误
/// - `401` 凭证无效 / 声明组织与连接归属不一致
/// - `403` 连接未开放 `a2a_task` 能力
/// - `500` 组织无可用接待用户（服务端配置问题，fail-closed）
pub async fn resolve_federation_identity(
    credential: &str,
    declaration: Option<&FederationCallerDeclaration>,
) -> Result<FederationIdentity> {
    // 1) 连接凭证鉴权（哈希匹配 Active 连接；无效/吊销统一 401 防枚举）
    let link = organization::domain()
        .organization_manage()
        .authenticate_link_call(RequestContext::new_system(), credential)
        .await
        .map_err(|e| {
            sys_debug!("federation credential rejected: {}", e);
            Error::unauthorized("联邦契约凭证无效")
        })?;

    // 2) 连接级能力白名单（P3）：未开放 a2a_task 的连接不允许跨组织委派
    if !link.has_capability(CAPABILITY_A2A_TASK) {
        return Err(Error::forbidden("这条连接未开放 a2a_task 能力"));
    }

    // 3) 声明一致性：caller_org 与连接归属不符 → 401（防跨连接冒充发起组织）。
    //    声明仅作审计/计量，不参与内部身份构造——内部身份恒为本端接待用户。
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
