//! 联邦身份密钥：Ed25519 + `did:key` 编解码 + 请求签名原语（S1，SSOT: docs/plan/联邦鉴权升级方案.md）
//!
//! 职责边界（方案 §四「crate 归属」）：本模块只放纯算法，不读配置、不碰 DB，
//! 密钥的生成时机与持久化在 domain 层组装。存储约定：
//! - `did`：`did:key:z<multibase>`，multicodec = ed25519-pub（0xed 0x01）+ 32B 公钥，base58btc
//! - `verification_key`：公钥 32B 的 base64（标准字母表），随目录同步公开
//! - `signing_key`：私钥种子 32B 的 base64，**落库前必须经 `encrypt_channel_secret` 加密**
//! - 签名头值：base64url 无 padding（§四）

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::error::{Error, Result, err};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// `did:key` 前缀
pub const DID_KEY_PREFIX: &str = "did:key:";

/// ed25519-pub multicodec 前缀（varint(0xed) = 0xed 0x01）
const MULTICODEC_ED25519_PUB: [u8; 2] = [0xed, 0x01];

/// 联邦身份密钥对（生成的原始形态；signing_key 为明文，调用方负责加密后落库）
pub struct FederationKeyPair {
    /// `did:key:z...`（公钥编码进标识符，换域名/IP/端口身份不变）
    pub did: String,
    /// 公钥 32B base64（随目录同步公开）
    pub verification_key: String,
    /// 私钥种子 32B base64（明文中间形态，禁止直接落库）
    pub signing_key: String,
}

/// 生成 Ed25519 密钥对并编码为 did:key 形态
pub fn generate_keypair() -> FederationKeyPair {
    let mut seed = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut seed);
    let signing = SigningKey::from_bytes(&seed);
    let verifying = signing.verifying_key();
    FederationKeyPair {
        did: encode_did_key(&verifying.to_bytes()),
        verification_key: B64.encode(verifying.as_bytes()),
        signing_key: B64.encode(signing.to_bytes()),
    }
}

/// 公钥 → `did:key:z...`
pub fn encode_did_key(verifying_key: &[u8; 32]) -> String {
    let mut raw = Vec::with_capacity(34);
    raw.extend_from_slice(&MULTICODEC_ED25519_PUB);
    raw.extend_from_slice(verifying_key);
    format!("{}z{}", DID_KEY_PREFIX, base58_encode(&raw))
}

/// `did:key:z...` → 公钥（非 ed25519-pub 编码、长度不符均报错）
pub fn decode_did_key(did: &str) -> Result<[u8; 32]> {
    let encoded = did
        .strip_prefix(DID_KEY_PREFIX)
        .ok_or_else(|| Error::bad_request("did 必须是 did:key 形式"))?;
    let body = encoded
        .strip_prefix('z')
        .ok_or_else(|| Error::bad_request("did:key multibase 前缀必须为 z"))?;
    let raw = base58_decode(body).ok_or_else(|| Error::bad_request("did:key base58 解码失败"))?;
    if raw.len() != 34 || raw[..2] != MULTICODEC_ED25519_PUB {
        return Err(Error::bad_request(
            "did:key 必须编码 ed25519-pub 32 字节公钥",
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&raw[2..]);
    Ok(key)
}

/// 请求签名（§四）：`X-Federation-Signature` = Ed25519(规范串)，base64url 无 padding
///
/// `signing_key_b64` 为**明文**私钥种子 base64（调用方先 `decrypt_channel_secret`）。
pub fn sign_request(signing_key_b64: &str, msg: &[u8]) -> Result<String> {
    let signing = signing_key_from_b64(signing_key_b64)?;
    Ok(URL_SAFE_NO_PAD.encode(signing.sign(msg).to_bytes()))
}

/// 验签（§四 验签流程第 4 步）：失败即 401 的依据，错误一律 `unauthorized`
pub fn verify_request(verification_key_b64: &str, msg: &[u8], signature_b64: &str) -> Result<()> {
    let key_bytes = verification_key_from_b64(verification_key_b64)?;
    let verifying =
        VerifyingKey::from_bytes(&key_bytes).map_err(|e| err!(Internal, "联邦公钥非法: {}", e))?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(signature_b64)
        .map_err(|_| Error::unauthorized("联邦签名 base64url 解码失败"))?;
    let sig_bytes: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| Error::unauthorized("联邦签名长度非法（应为 64 字节）"))?;
    verifying
        .verify(msg, &Signature::from_bytes(&sig_bytes))
        .map_err(|_| Error::unauthorized("联邦签名验证失败"))
}

/// 待签规范串（§四，`\n` 分隔，顺序固定）：
/// `{method 大写}\n{path 含 query}\n{timestamp 秒}\n{nonce}\n{sha256(body) 十六进制小写}`
///
/// 无 body 时 `body_hash_hex` 取空串的 SHA-256，保证字段数恒定。
pub fn canonical_request_string(
    method: &str,
    path_with_query: &str,
    timestamp_seconds: i64,
    nonce: &str,
    body_hash_hex: &str,
) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        path_with_query,
        timestamp_seconds,
        nonce,
        body_hash_hex
    )
}

/// 请求体 SHA-256 十六进制小写（规范串最后一项；空 body 传 `""`）
pub fn body_sha256_hex(body: &[u8]) -> String {
    sha256::digest(body)
}

// ==================== S2 出站签名组装 + 建联交叉签名 ====================

/// 出站联邦请求签名四件套（对应四个 `X-Federation-*` 头）
#[derive(Debug, Clone)]
pub struct SignedRequestHeaders {
    /// 发起方组织 DID（`did:key:z...`，由私钥推导，与本端 organizations.did 一致）
    pub key_id: String,
    /// Unix 秒级时间戳
    pub timestamp: i64,
    /// 随机 16 字节 hex
    pub nonce: String,
    /// Ed25519 签名（base64url 无 padding）
    pub signature: String,
}

/// 组装出站联邦请求签名（§四）：规范串 = method\npath\ntimestamp\nnonce\nsha256(body)
///
/// `key_id` 由私钥推导（did:key 即公钥编码，天然自洽），调用方无需单独传 DID。
/// `body` 传请求体原始字节（无 body 传空切片）。
pub fn sign_federation_request(
    signing_key_b64: &str,
    method: &str,
    path_with_query: &str,
    body: &[u8],
) -> Result<SignedRequestHeaders> {
    let signing = signing_key_from_b64(signing_key_b64)?;
    let key_id = encode_did_key(&signing.verifying_key().to_bytes());
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();
    let nonce = crate::pkg::nonce::generate();
    let msg = canonical_request_string(
        method,
        path_with_query,
        timestamp,
        &nonce,
        &body_sha256_hex(body),
    );
    let signature = URL_SAFE_NO_PAD.encode(signing.sign(msg.as_bytes()).to_bytes());
    Ok(SignedRequestHeaders {
        key_id,
        timestamp,
        nonce,
        signature,
    })
}

/// 由私钥推导对应 DID（did:key 即公钥编码，二者一一对应）
pub fn did_from_signing_key(signing_key_b64: &str) -> Result<String> {
    let signing = signing_key_from_b64(signing_key_b64)?;
    Ok(encode_did_key(&signing.verifying_key().to_bytes()))
}

/// 建联交叉签名上下文（域分隔，防签名串被挪用到请求签名）
pub const PAIRING_HANDSHAKE_CONTEXT: &str = "ai-orz:federation:pairing:v1";

/// 建联交叉签名待签串：`{context}\n{对方 DID}\n{sha256(配对码)}`
///
/// 双向语义（SSOT 方案 §六 S2）：
/// - 发起方 A 在 verify 请求里对（A 自己的 DID + 配对码哈希）签名 —— 证明
///   「A 拥有它所声明 DID 的私钥」；
/// - B 在响应里对（A 的 DID + 配对码哈希）签名 —— 证明「B 拥有其声明 DID 的
///   私钥，且把这次建联钉在 A 的身份上」。
///
/// 配对码单用途，签名不可跨建联重放。
pub fn pairing_handshake_message(peer_did: &str, pairing_code: &str) -> String {
    format!(
        "{}\n{}\n{}",
        PAIRING_HANDSHAKE_CONTEXT,
        peer_did,
        sha256::digest(pairing_code.as_bytes())
    )
}

/// 校验 DID 与公钥一一对应（did:key 公钥编码进标识符，二者必然自洽；
/// 不自洽 = 声明被篡改/伪造，直接拒绝）
pub fn did_matches_verification_key(did: &str, verification_key_b64: &str) -> bool {
    match decode_did_key(did) {
        Ok(pub_bytes) => B64.encode(pub_bytes) == verification_key_b64,
        Err(_) => false,
    }
}

fn signing_key_from_b64(signing_key_b64: &str) -> Result<SigningKey> {
    let seed = B64
        .decode(signing_key_b64)
        .map_err(|e| err!(Internal, "联邦私钥 base64 解码失败: {}", e))?;
    let seed: [u8; 32] = seed
        .try_into()
        .map_err(|_| err!(Internal, "联邦私钥长度非法（应为 32 字节种子）"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn verification_key_from_b64(verification_key_b64: &str) -> Result<[u8; 32]> {
    let bytes = B64
        .decode(verification_key_b64)
        .map_err(|e| err!(Internal, "联邦公钥 base64 解码失败: {}", e))?;
    bytes
        .try_into()
        .map_err(|_| err!(Internal, "联邦公钥长度非法（应为 32 字节）"))
}

// ==================== base58btc（Bitcoin 字母表，did:key multibase 用） ====================

const B58_ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

fn base58_encode(data: &[u8]) -> String {
    let zeros = data.iter().take_while(|&&b| b == 0).count();
    let mut digits: Vec<u8> = Vec::with_capacity(data.len() * 138 / 100 + 1);
    for &byte in &data[zeros..] {
        let mut carry = byte as usize;
        for d in digits.iter_mut() {
            carry += (*d as usize) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut out = String::with_capacity(zeros + digits.len());
    for _ in 0..zeros {
        out.push('1');
    }
    for d in digits.iter().rev() {
        out.push(B58_ALPHABET[*d as usize] as char);
    }
    out
}

fn base58_decode(s: &str) -> Option<Vec<u8>> {
    let zeros = s.bytes().take_while(|&c| c == b'1').count();
    let mut bytes: Vec<u8> = Vec::new();
    for c in s.bytes().skip(zeros) {
        let val = B58_ALPHABET.iter().position(|&a| a == c)?;
        let mut carry = val;
        for b in bytes.iter_mut() {
            carry += (*b as usize) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let mut out = vec![0u8; zeros];
    out.extend(bytes.into_iter().rev());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_key_roundtrip() {
        let kp = generate_keypair();
        assert!(kp.did.starts_with("did:key:z6Mk"));
        let decoded = decode_did_key(&kp.did).unwrap();
        assert_eq!(B64.encode(decoded), kp.verification_key);
    }

    #[test]
    fn did_key_known_vector_ed25519_prefix() {
        // Ed25519 did:key 的 base58btc 输出恒以 z6Mk 开头（0xed 0x01 前缀 + 高位公钥字节分布）
        for _ in 0..8 {
            let kp = generate_keypair();
            assert!(kp.did.starts_with("did:key:z6Mk"), "did={}", kp.did);
        }
    }

    #[test]
    fn did_key_decode_rejects_malformed() {
        assert!(decode_did_key("did:web:example.com").is_err());
        assert!(decode_did_key("did:key:u6Mk...").is_err());
        assert!(decode_did_key("did:key:z!!").is_err());
        // 非 34 字节（缺 multicodec 前缀）
        assert!(
            decode_did_key(&format!("{}z{}", DID_KEY_PREFIX, base58_encode(&[1u8; 32]))).is_err()
        );
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let kp = generate_keypair();
        let msg = b"POST\n/a2a\n1757400000\nnonce-1\nabc";
        let sig = sign_request(&kp.signing_key, msg).unwrap();
        verify_request(&kp.verification_key, msg, &sig).unwrap();
    }

    #[test]
    fn verify_fails_on_tampered_message() {
        let kp = generate_keypair();
        let sig = sign_request(&kp.signing_key, b"original").unwrap();
        assert!(verify_request(&kp.verification_key, b"tampered", &sig).is_err());
    }

    #[test]
    fn verify_fails_on_tampered_signature_and_wrong_key() {
        let kp = generate_keypair();
        let other = generate_keypair();
        let sig = sign_request(&kp.signing_key, b"msg").unwrap();
        let mut broken = sig.clone();
        broken.replace_range(0..1, if broken.starts_with('A') { "B" } else { "A" });
        assert!(verify_request(&kp.verification_key, b"msg", &broken).is_err());
        assert!(verify_request(&other.verification_key, b"msg", &sig).is_err());
    }

    #[test]
    fn canonical_string_is_order_fixed_and_uppercases_method() {
        let s = canonical_request_string("post", "/a2a?x=1", 123, "n", "h");
        assert_eq!(s, "POST\n/a2a?x=1\n123\nn\nh");
    }

    #[test]
    fn body_hash_is_lowercase_hex_and_empty_stable() {
        assert_eq!(body_sha256_hex(b""), sha256::digest(b""));
        assert!(
            body_sha256_hex(b"x")
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn sign_federation_request_produces_verifiable_headers() {
        let kp = generate_keypair();
        let body = br#"{"jsonrpc":"2.0"}"#;
        let headers = sign_federation_request(&kp.signing_key, "POST", "/a2a?x=1", body).unwrap();
        assert_eq!(headers.key_id, kp.did);
        let msg = canonical_request_string(
            "POST",
            "/a2a?x=1",
            headers.timestamp,
            &headers.nonce,
            &body_sha256_hex(body),
        );
        verify_request(&kp.verification_key, msg.as_bytes(), &headers.signature).unwrap();
    }

    #[test]
    fn sign_federation_request_empty_body_and_nonce_uniqueness() {
        let kp = generate_keypair();
        let a = sign_federation_request(&kp.signing_key, "GET", "/x", b"").unwrap();
        let b = sign_federation_request(&kp.signing_key, "GET", "/x", b"").unwrap();
        assert_ne!(a.nonce, b.nonce, "每次签名应产生新 nonce");
        // 空 body 的规范串最后一项 = 空串哈希
        let msg =
            canonical_request_string("GET", "/x", a.timestamp, &a.nonce, &body_sha256_hex(b""));
        verify_request(&kp.verification_key, msg.as_bytes(), &a.signature).unwrap();
    }

    #[test]
    fn did_matches_verification_key_and_pairing_message() {
        let kp = generate_keypair();
        assert!(did_matches_verification_key(&kp.did, &kp.verification_key));
        let other = generate_keypair();
        assert!(!did_matches_verification_key(
            &kp.did,
            &other.verification_key
        ));
        assert!(!did_matches_verification_key(
            "did:web:x",
            &kp.verification_key
        ));

        let m = pairing_handshake_message(&kp.did, "CODE");
        assert!(m.starts_with(PAIRING_HANDSHAKE_CONTEXT));
        assert_eq!(
            pairing_handshake_message(&kp.did, "CODE"),
            m,
            "同输入确定性"
        );
        assert_ne!(
            pairing_handshake_message(&kp.did, "CODE"),
            pairing_handshake_message(&other.did, "CODE")
        );
    }

    #[test]
    fn base58_known_vectors() {
        // RFC-ish 常用向量
        assert_eq!(base58_encode(b"hello world"), "StV1DL6CwTryKyV");
        assert_eq!(base58_decode("StV1DL6CwTryKyV").unwrap(), b"hello world");
        // 前导零 → '1'
        assert_eq!(base58_encode(&[0, 0, 1]), "112");
        assert_eq!(base58_decode("112").unwrap(), vec![0, 0, 1]);
        // 空输入
        assert_eq!(base58_encode(&[]), "");
        assert_eq!(base58_decode("").unwrap(), Vec::<u8>::new());
    }
}
