//! 联邦签名测试基建（S2）：取组织联邦身份 + 组装 `X-Federation-*` 签名头。
//!
//! S2 起机器侧鉴权 = 每请求 Ed25519 签名（Bearer link 凭证已删除）。
//! 集成测试统一从这里取签名身份、组装签名头，禁止再手写 Bearer。
//!
//! 本模块被所有集成测试 binary 通过 tests/common/mod.rs 引入，
//! 未用到联邦的 binary 会报 dead-code，整体 allow。
#![allow(dead_code)]

use axum::http::{HeaderMap, HeaderValue};
use common::constants::http_header;

/// 组织的联邦签名身份（signing_key 为解密后的明文 base64）
pub struct OrgFederationIdentity {
    pub did: String,
    pub signing_key: String,
    pub verification_key: String,
}

/// 读取组织的联邦签名身份（S1 密钥底座：Local 组织创建即生成）
pub async fn org_federation_identity(org_id: &str) -> OrgFederationIdentity {
    let ctx = ai_orz::pkg::RequestContext::from_storage(
        "fed-test-sign",
        ai_orz::pkg::storage::get().clone(),
    );
    let org = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .get_by_id(ctx, org_id)
        .await
        .expect("query org for federation identity failed")
        .expect("org should exist");
    let encrypted = org
        .signing_key
        .clone()
        .expect("org must have federation signing key (S1 base)");
    OrgFederationIdentity {
        did: org.did.expect("org must have did"),
        verification_key: org
            .verification_key
            .expect("org must have verification key"),
        signing_key: ai_orz::pkg::crypto::decrypt_channel_secret(&encrypted)
            .expect("decrypt signing key failed"),
    }
}

/// 组装联邦签名请求头（四件套 `X-Federation-Key-Id/Timestamp/Nonce/Signature`）
///
/// `path_with_query` 必须与实际请求的 `uri.path_and_query()` 完全一致（签名绑定
/// path），body 为请求体原始字节。
pub fn signature_headers(
    identity: &OrgFederationIdentity,
    method: &str,
    path_with_query: &str,
    body: &[u8],
) -> HeaderMap {
    let signed = ai_orz::pkg::crypto::did::sign_federation_request(
        &identity.signing_key,
        method,
        path_with_query,
        body,
    )
    .expect("sign federation request failed");
    let mut headers = HeaderMap::new();
    headers.insert(
        http_header::FEDERATION_KEY_ID,
        HeaderValue::from_str(&signed.key_id).expect("valid did header"),
    );
    headers.insert(
        http_header::FEDERATION_TIMESTAMP,
        HeaderValue::from_str(&signed.timestamp.to_string()).expect("valid ts header"),
    );
    headers.insert(
        http_header::FEDERATION_NONCE,
        HeaderValue::from_str(&signed.nonce).expect("valid nonce header"),
    );
    headers.insert(
        http_header::FEDERATION_SIGNATURE,
        HeaderValue::from_str(&signed.signature).expect("valid signature header"),
    );
    headers
}

/// 用**随机密钥对**签名（其 DID 不归属任何连接）——用于 401 场景
pub fn unknown_keypair_headers(method: &str, path_with_query: &str, body: &[u8]) -> HeaderMap {
    let kp = ai_orz::pkg::crypto::did::generate_keypair();
    let identity = OrgFederationIdentity {
        did: kp.did,
        signing_key: kp.signing_key,
        verification_key: kp.verification_key,
    };
    signature_headers(&identity, method, path_with_query, body)
}
