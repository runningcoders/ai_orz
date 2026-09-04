//! OrganizationPairingCode 持久化对象
//!
//! 对应 SQL 建表语句：`migrations/20260904000003_create_organization_pairing_codes.sql`
//!
//! 配对码是组网引导的短时效、单用途凭证（评审稿 §4.1）：签发后 10 分钟有效、用后即焚。
//! 仅存哈希（防泄漏）+ 签发组织 + 过期时间 + 消费时间（单用途判定）。

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// OrganizationPairingCodePo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrganizationPairingCodePo {
    /// 记录 ID
    pub id: String,
    /// 签发组织 ID（对端验证时据此定位签发方身份）
    pub org_id: String,
    /// 配对码 SHA-256 哈希（明文不出库）
    pub code_hash: String,
    /// 过期绝对时间（毫秒时间戳）
    pub expires_at: i64,
    /// 消费时间（毫秒）；NULL = 未使用
    pub consumed_at: Option<i64>,
    /// 创建人
    pub created_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
}
