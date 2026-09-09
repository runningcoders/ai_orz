//! 敏感数据加密工具
//!
//! AES-256-GCM 对称加密，用于渠道凭证等敏感字段落库加密：
//! - 密文格式：`enc:v1:<nonce_b64>:<ciphertext_b64>`
//! - 密钥：`security.secret_key` 经 SHA-256 派生 32 字节
//! - 兼容：无 `enc:v1:` 前缀视为明文，读取路径直接使用（测试阶段不做批量迁移）

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use common::error::{Result, err};

/// 密文前缀（版本标识）
const CIPHER_PREFIX: &str = "enc:v1:";

/// nonce 长度（AES-GCM 标准 12 字节）
const NONCE_LEN: usize = 12;

/// 判断值是否已是密文格式
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(CIPHER_PREFIX)
}

/// 从主密钥派生 32 字节 AES-256 密钥（SHA-256）
fn derive_key(secret_key: &str) -> [u8; 32] {
    let digest = sha256::digest(secret_key); // 64 位十六进制
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digest[i * 2..i * 2 + 2], 16).unwrap_or(0);
    }
    key
}

/// 读取当前进程的主加密密钥（security.secret_key）
fn master_key() -> Result<String> {
    crate::config::try_get()
        .map(|c| c.security.secret_key.clone())
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| {
            err!(
                Internal,
                "security.secret_key 未配置，无法执行敏感字段加解密"
            )
        })
}

/// 加密敏感字段（返回 `enc:v1:` 前缀密文）
pub fn encrypt_with_key(secret_key: &str, plaintext: &str) -> Result<String> {
    let key_bytes = derive_key(secret_key);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| err!(Internal, "敏感字段加密失败: {}", e))?;

    Ok(format!(
        "{}{}:{}",
        CIPHER_PREFIX,
        STANDARD.encode(nonce_bytes),
        STANDARD.encode(ciphertext)
    ))
}

/// 解密密文字段（仅接受 `enc:v1:` 前缀格式）
pub fn decrypt_with_key(secret_key: &str, ciphertext: &str) -> Result<String> {
    let body = ciphertext
        .strip_prefix(CIPHER_PREFIX)
        .ok_or_else(|| err!(Internal, "非法密文格式：缺少 enc:v1: 前缀"))?;
    let (nonce_b64, cipher_b64) = body
        .split_once(':')
        .ok_or_else(|| err!(Internal, "非法密文格式：缺少 nonce/ciphertext 分段"))?;

    let nonce_bytes = STANDARD
        .decode(nonce_b64)
        .map_err(|e| err!(Internal, "密文 nonce 解码失败: {}", e))?;
    let cipher_bytes = STANDARD
        .decode(cipher_b64)
        .map_err(|e| err!(Internal, "密文内容解码失败: {}", e))?;

    let key_bytes = derive_key(secret_key);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, cipher_bytes.as_ref())
        .map_err(|_| err!(Internal, "敏感字段解密失败（密钥不匹配或配置已损坏）"))?;

    String::from_utf8(plaintext).map_err(|e| err!(Internal, "解密内容非合法 UTF-8: {}", e))
}

/// 使用进程主密钥加密（渠道凭证写入路径）
pub fn encrypt_channel_secret(plaintext: &str) -> Result<String> {
    encrypt_with_key(&master_key()?, plaintext)
}

/// 使用进程主密钥解密，兼容明文（渠道凭证读取路径）
///
/// 无 `enc:v1:` 前缀视为明文直接返回（测试阶段不做批量迁移）。
pub fn decrypt_channel_secret(value: &str) -> Result<String> {
    if !is_encrypted(value) {
        return Ok(value.to_string());
    }
    decrypt_with_key(&master_key()?, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = "unit-test-secret-key";

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret = "my-lark-app-secret-123";
        let encrypted = encrypt_with_key(TEST_KEY, secret).unwrap();
        assert!(encrypted.starts_with("enc:v1:"));
        assert!(!encrypted.contains(secret));
        let decrypted = decrypt_with_key(TEST_KEY, &encrypted).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn test_wrong_key_fails() {
        let encrypted = encrypt_with_key(TEST_KEY, "secret").unwrap();
        assert!(decrypt_with_key("another-key", &encrypted).is_err());
    }

    #[test]
    fn test_nonce_randomness_produces_different_ciphertexts() {
        let a = encrypt_with_key(TEST_KEY, "same").unwrap();
        let b = encrypt_with_key(TEST_KEY, "same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn test_decrypt_plaintext_passthrough_via_channel_helper() {
        // 无 enc:v1: 前缀的值不经解密直接返回（明文兼容，不依赖 master_key）
        assert!(!is_encrypted("plaintext-secret"));
        assert_eq!(
            decrypt_channel_secret("plaintext-secret").unwrap(),
            "plaintext-secret"
        );
    }

    #[test]
    fn test_invalid_ciphertext_format() {
        assert!(decrypt_with_key(TEST_KEY, "enc:v1:garbage").is_err());
        assert!(decrypt_with_key(TEST_KEY, "enc:v1:aaa:bbb").is_err());
    }

    #[test]
    fn test_empty_plaintext_roundtrip() {
        let encrypted = encrypt_with_key(TEST_KEY, "").unwrap();
        assert_eq!(decrypt_with_key(TEST_KEY, &encrypted).unwrap(), "");
    }
}
