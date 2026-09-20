//! Handler: GET /api/v1/finance/identity/lark/status - 绑定快照聚合
//!
//! 三源聚合（纯查询，请求级组合）：
//! 1. 凭证列表（user_credentials 表，secret 恒不回显）
//! 2. 反查引用各凭证的飞书渠道（经 message channel domain 查询，限当前用户归属）
//! 3. 现场执行 `auth status --json` 拿用户授权现状（经 Domain 包装）

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{
    LarkCredentialChannelRef, LarkCredentialSnapshot, LarkIntegrationStatusRequest,
    LarkIntegrationStatusResponse, LarkUserAuthSnapshot,
};
use common::enums::{ChannelStatus, ChannelType};
use common::error::{Result, bail_err};
use common::models::{CredentialDetail, CredentialKind};

#[generate_http_handler]
pub async fn get_status(
    ctx: RequestContext,
    _params: LarkIntegrationStatusRequest,
) -> Result<LarkIntegrationStatusResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 1. 凭证列表（user_credentials 表；用户不存在降级为空列表）
    let credentials = domain()
        .identity_credential_manage()
        .get_identity_credentials(ctx.clone(), &user_id)
        .await?
        .unwrap_or_default();

    // 2. 当前用户名下的飞书渠道（经 message channel domain 查询）
    // 查询失败**必须上抛**（F14）：吞成空列表会把「渠道引用丢失」伪装成「无引用」，
    // 且前端已能区分加载失败与未绑定（S4-5），上抛是安全路径
    let channels = crate::service::domain::finance::domain()
        .message_channel_manage()
        .query_channels(
            ctx.clone(),
            crate::service::dao::message_channel::MessageChannelQuery {
                user_id: Some(user_id.clone()),
                channel_type: Some(ChannelType::Lark),
                ..Default::default()
            },
        )
        .await?
        .items;

    // 3. 逐凭证内存分组引用渠道（secret 恒不回显）
    let mut snapshots = Vec::new();
    for credential in &credentials {
        if credential.kind() != CredentialKind::LarkApp {
            continue;
        }
        let CredentialDetail::LarkApp { app_id, .. } = credential.detail() else {
            continue;
        };
        let refs = channels
            .iter()
            // 已删除渠道不计入引用（软删除：status=Deleted）
            .filter(|c| c.po.status != ChannelStatus::Deleted)
            .filter(|c| c.config().lark_credential_id.as_deref() == Some(credential.id()))
            .map(|c| LarkCredentialChannelRef {
                channel_id: c.po.id.clone(),
                channel_name: c.po.channel_name.clone(),
                enabled: c.is_enabled(),
            })
            .collect();
        snapshots.push(LarkCredentialSnapshot {
            credential_id: credential.id().to_string(),
            name: credential.name().to_string(),
            app_id: app_id.clone(),
            is_default: credential.po.is_default,
            // 绑定 / 最后轮换时间（D9 对齐：凭据卡展示）
            created_at: credential.po.created_at,
            updated_at: credential.po.updated_at,
            channels: refs,
        });
    }

    // 4. 用户授权现状（现场执行 auth status；前置不满足时降级为未授权 + 引导提示）
    let user_auth = match domain()
        .identity_credential_manage()
        .lark_auth_status(ctx, &user_id)
        .await
    {
        Ok(s) => LarkUserAuthSnapshot {
            logged_in: s.logged_in,
            user_name: s.user_name,
            degraded: s.degraded,
            hint: s.hint,
        },
        Err(e) => LarkUserAuthSnapshot {
            hint: Some(e.to_string()),
            ..Default::default()
        },
    };

    Ok(LarkIntegrationStatusResponse {
        credentials: snapshots,
        user_auth,
    })
}
