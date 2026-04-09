//! JWT 工具模块
//!
//! 用于用户登录认证，签发和验证 JWT token

use jsonwebtoken::{
    decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};
use std::sync::OnceLock;
use serde::{Deserialize, Serialize};
use chrono::{Duration, Utc};

/// JWT Claims (包含用户信息)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 组织 ID
    pub organization_id: String,
    /// 过期时间 (Unix timestamp)
    pub exp: i64,
    /// 签发时间
    pub iat: i64,
}

impl Claims {
    /// 创建新的 Claims
    pub fn new(
        user_id: String,
        username: String,
        organization_id: String,
        expires_in: Duration,
    ) -> Self {
        let now = Utc::now();
        let exp = (now + expires_in).timestamp();
        let iat = now.timestamp();

        Self {
            user_id,
            username,
            organization_id,
            exp,
            iat,
        }
    }
}

/// JWT 配置
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// 签名密钥
    secret: Vec<u8>,
    /// 默认过期时间
    default_expiry: Duration,
}

impl JwtConfig {
    /// 创建新的 JWT 配置
    pub fn new(secret: &str, default_expiry_hours: i64) -> Self {
        Self {
            secret: secret.as_bytes().to_vec(),
            default_expiry: Duration::hours(default_expiry_hours),
        }
    }

    /// 签发 JWT token
    pub fn encode(
        &self,
        user_id: &str,
        username: &str,
        organization_id: &str,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let claims = Claims::new(
            user_id.to_string(),
            username.to_string(),
            organization_id.to_string(),
            self.default_expiry,
        );

        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(&self.secret),
        )
    }

    /// 验证并解码 JWT token
    pub fn decode(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let validation = Validation::new(Algorithm::HS256);

        decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.secret),
            &validation,
        ).map(|data| data.claims)
    }

    /// 获取默认过期时间（秒）
    pub fn default_expiry_seconds(&self) -> i64 {
        self.default_expiry.num_seconds()
    }
}

/// 全局 JWT 配置单例
static JWT_CONFIG: OnceLock<JwtConfig> = OnceLock::new();

/// 初始化全局 JWT 配置
pub fn init_jwt(secret: &str, default_expiry_hours: i64) {
    JWT_CONFIG.set(JwtConfig::new(secret, default_expiry_hours))
        .expect("JWT config already initialized");
}

/// 获取全局 JWT 配置
pub fn jwt_config() -> &'static JwtConfig {
    JWT_CONFIG.get().expect("JWT config not initialized")
}

/// 签发 JWT token 使用全局配置
pub fn encode_jwt(
    user_id: &str,
    username: &str,
    organization_id: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    jwt_config().encode(user_id, username, organization_id)
}

/// 验证并解码 JWT token 使用全局配置
pub fn decode_jwt(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    jwt_config().decode(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_encode_decode() {
        let config = JwtConfig::new("test-secret-key-very-long-for-security", 24);
        let token = config.encode(
            "user-123",
            "testuser",
            "org-456",
        ).expect("encode should succeed");

        println!("Generated token: {}", token);

        let claims = config.decode(&token).expect("decode should succeed");
        assert_eq!(claims.user_id, "user-123");
        assert_eq!(claims.username, "testuser");
        assert_eq!(claims.organization_id, "org-456");
        assert!(claims.exp > claims.iat);
    }
}
