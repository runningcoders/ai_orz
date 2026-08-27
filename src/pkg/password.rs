//! 用户密码哈希（bcrypt）
//!
//! 服务端唯一哈希点：所有密码写入路径必须经 `ensure_hashed` 归一化，
//! 验证路径统一走 `verify`。历史明文口令通过登录时透明升级完成迁移：
//! `is_bcrypt_hash` 为假 → 先按明文比对 → 命中即场重哈希回写。

use bcrypt::{hash, verify};
use common::error::{Result, err};

/// 生产默认 cost=10（约 60-100ms/次，OWASP 下限之上）
pub const PASSWORD_COST: u32 = 10;

/// 判断存储值是否已是 bcrypt 哈希（$2a$/$2b$/$2y$ 前缀）
pub fn is_bcrypt_hash(value: &str) -> bool {
    value.starts_with("$2")
}

/// 明文哈希为 bcrypt
pub fn hash_password(plain: &str) -> Result<String> {
    hash(plain, PASSWORD_COST).map_err(|e| err!(Internal, "密码哈希失败: {}", e))
}

/// 校验明文与 bcrypt 哈希是否匹配
pub fn verify_password(plain: &str, hashed: &str) -> Result<bool> {
    verify(plain, hashed).map_err(|e| err!(Internal, "密码校验失败: {}", e))
}

/// 写入口归一化：已是 bcrypt 则原样返回，否则当场哈希（幂等）
///
/// 用于 seed 的 INHERIT_CURRENT 透传、管理员建成员等"值可能已经是哈希"的场景；
/// 普通注册/改密路径已知是明文，可直接调 [`hash_password`]。
pub fn ensure_hashed(value: &str) -> Result<String> {
    if is_bcrypt_hash(value) {
        Ok(value.to_string())
    } else {
        hash_password(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_verify_roundtrip() {
        let h = hash_password("s3cret-密码").unwrap();
        assert!(is_bcrypt_hash(&h));
        assert!(verify_password("s3cret-密码", &h).unwrap());
        assert!(!verify_password("wrong", &h).unwrap());
    }

    #[test]
    fn test_salt_produces_unique_hashes() {
        assert_ne!(
            hash_password("same").unwrap(),
            hash_password("same").unwrap()
        );
    }

    #[test]
    fn test_is_bcrypt_hash_detection() {
        assert!(!is_bcrypt_hash("plaintext-password"));
        assert!(!is_bcrypt_hash(""));
        assert!(is_bcrypt_hash("$2b$12$abcdefghijklmnopqrstuv"));
        assert!(is_bcrypt_hash("$2y$10$x"));
        assert!(is_bcrypt_hash("$2a$08$y"));
    }

    #[test]
    fn test_ensure_hashed_idempotent() {
        let once = ensure_hashed("hunter2").unwrap();
        assert!(is_bcrypt_hash(&once));
        // 已是哈希的值原样透传，不二次哈希
        let twice = ensure_hashed(&once).unwrap();
        assert_eq!(once, twice);
    }
}
