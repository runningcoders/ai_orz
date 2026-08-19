//! UserCredential 持久化对象
//!
//! 对应 SQL 建表语句：`migrations/20260819000000_user_credentials.sql`
//!
//! 用户身份凭证独立表（一凭证一行，取代 users.identity_credentials JSON 列）：
//! - kind / visibility 为 TEXT 字符串枚举（分类型枚举用 TEXT 先例）
//! - is_default 作用域由 visibility 派生（private=个人默认 / public=组织默认）
//! - detail 为「secret 已加密」的 JSON（加密发生在 Domain 编排层）
//! - 凭据表零外部使用方引用（Agent/工具/渠道绑定归使用方实体，D12）

use common::constants::utils;
use common::models::{CredentialDetail, CredentialKind, CredentialVisibility};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;

/// UserCredentialPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserCredentialPo {
    /// 凭证 ID（UUID v7，使用方引用键）
    pub id: String,
    /// 组织 ID（多租户隔离）
    pub org_id: String,
    /// 凭证归属用户 ID（资产所有者）
    pub user_id: String,
    /// 凭证类型（TEXT 字符串枚举）
    pub kind: CredentialKind,
    /// 用户自定义名称（仅展示，不参与解析）
    pub name: String,
    /// 凭证详情 JSON（secret 类字段落库前已加密）
    pub detail: Json<CredentialDetail>,
    /// 可见性：private=仅所有者 / public=同 org 可显式引用
    pub visibility: CredentialVisibility,
    /// 默认标记：作用域由 visibility 派生（private=个人默认 / public=组织默认）
    pub is_default: bool,
    /// 软删除：1=Active, 0=Deleted
    pub status: i32,
    /// 创建人 ID
    pub created_by: String,
    /// 最后修改人 ID
    pub modified_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

impl UserCredentialPo {
    /// 创建新的 UserCredentialPo（Active + private 起步，默认标记由 set_default 显式设立）
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        org_id: String,
        user_id: String,
        kind: CredentialKind,
        name: String,
        detail: CredentialDetail,
        visibility: CredentialVisibility,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id,
            org_id,
            user_id,
            kind,
            name,
            detail: Json(detail),
            visibility,
            is_default: false,
            status: 1,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// 是否活跃（未软删除）
    pub fn is_active(&self) -> bool {
        self.status != 0
    }
}

/// UserCredential 业务实体
///
/// PO 不越层（AGENTS §3.5）：对外经 UserDal 输出本实体；
/// detail 密封边界不变——DAO 层不感知 detail 字段结构，加解密发生在 Domain 编排层。
#[derive(Debug, Clone)]
pub struct UserCredential {
    /// 底层持久化对象
    pub po: UserCredentialPo,
}

impl UserCredential {
    /// 从 Po 创建实体
    pub fn from_po(po: UserCredentialPo) -> Self {
        Self { po }
    }

    /// 转换为 Po
    pub fn into_po(self) -> UserCredentialPo {
        self.po
    }

    /// 凭证 ID
    pub fn id(&self) -> &str {
        self.po.id.as_str()
    }

    /// 凭证归属用户 ID
    pub fn user_id(&self) -> &str {
        self.po.user_id.as_str()
    }

    /// 凭证类型
    pub fn kind(&self) -> CredentialKind {
        self.po.kind
    }

    /// 凭证名称（仅展示）
    pub fn name(&self) -> &str {
        self.po.name.as_str()
    }

    /// 凭证详情（secret 已加密形态）
    pub fn detail(&self) -> &CredentialDetail {
        &self.po.detail.0
    }

    /// 可见性
    pub fn visibility(&self) -> CredentialVisibility {
        self.po.visibility
    }

    /// 是否活跃（未软删除）
    pub fn is_active(&self) -> bool {
        self.po.is_active()
    }
}
