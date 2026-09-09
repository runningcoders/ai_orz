//! 联邦异步回调任务令牌（S2，SSOT: docs/plan/联邦鉴权升级方案.md §2.4）
//!
//! 自签发短期任务令牌：委派方 A 在 `tasks/send` 时用**本端私钥**签发，随 payload
//! 下发；对端 B 回调时作为 `Authorization: Bearer` 带回；A 端用**自己的公钥**
//! 验证——零往返、零服务端状态、单任务作用域。
//!
//! 与 JWT（用户会话 HS256）无关；与同步链路的每请求签名互补（同步路径签名
//! 严格更强，见方案 §2.4，绝不替换）。
//!
//! 线格式：`base64url(claims_json).base64url(Ed25519 签名)`，无 padding。
//! 令牌不落库（签发方无状态；对端如需回传可自行暂存于回调渠道配置）。

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::error::{Error, Result, err};
use serde::{Deserialize, Serialize};

use super::did::{sign_request, verify_request};

/// 任务令牌 claims
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskTokenClaims {
    /// 签发方组织 DID（即回调接收方，验证方用**自己的**公钥验签）
    pub iss: String,
    /// 受众：被委派方组织 DID（令牌只对这次委派的对端有效）
    pub aud: String,
    /// 绑定的任务 ID（与回调路径参数一致）
    pub task_id: String,
    /// 过期时间（Unix 秒）；TTL 必须 ≥ 任务超时 + 余量（安全来自单任务作用域，
    /// 不来自短 TTL，见方案 §2.4「TTL 修正」）
    pub exp: i64,
}

/// 签发任务令牌（`b64(claims).b64(sig)`，私钥 = 签发方本端联邦私钥明文 base64）
pub fn issue_task_token(signing_key_b64: &str, claims: &TaskTokenClaims) -> Result<String> {
    let claims_json = serde_json::to_vec(claims)
        .map_err(|e| err!(Internal, "任务令牌 claims 序列化失败: {}", e))?;
    let signature = sign_request(signing_key_b64, &claims_json)?;
    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(claims_json),
        signature
    ))
}

/// 验证任务令牌结构与签名（**仅验签**，不校验 exp/aud/task_id——调用方按业务
/// 逐项比对后使用；任一不符即 401 依据）
pub fn verify_task_token_signature(
    verification_key_b64: &str,
    token: &str,
) -> Result<TaskTokenClaims> {
    let (payload_b64, sig_b64) = token
        .split_once('.')
        .ok_or_else(|| Error::unauthorized("任务令牌格式非法（缺少 payload.signature 分段）"))?;
    let claims_json = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| Error::unauthorized("任务令牌 payload 解码失败"))?;
    verify_request(verification_key_b64, &claims_json, sig_b64)?;
    serde_json::from_slice(&claims_json)
        .map_err(|_| Error::unauthorized("任务令牌 claims 解析失败"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::crypto::did::generate_keypair;

    fn claims() -> TaskTokenClaims {
        TaskTokenClaims {
            iss: "did:key:z6MkIssuer".to_string(),
            aud: "did:key:z6MkPeer".to_string(),
            task_id: "task-1".to_string(),
            exp: 1_800_000_000,
        }
    }

    #[test]
    fn issue_and_verify_roundtrip() {
        let kp = generate_keypair();
        let token = issue_task_token(&kp.signing_key, &claims()).unwrap();
        let decoded = verify_task_token_signature(&kp.verification_key, &token).unwrap();
        assert_eq!(decoded, claims());
    }

    #[test]
    fn verify_fails_with_wrong_key_or_tampered_token() {
        let kp = generate_keypair();
        let other = generate_keypair();
        let token = issue_task_token(&kp.signing_key, &claims()).unwrap();
        assert!(verify_task_token_signature(&other.verification_key, &token).is_err());
        let mut broken = token.clone();
        broken.push('x');
        assert!(verify_task_token_signature(&kp.verification_key, &broken).is_err());
        assert!(verify_task_token_signature(&kp.verification_key, "not-a-token").is_err());
    }
}
