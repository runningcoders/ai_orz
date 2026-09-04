//! Organization 持久化对象
//!
//! 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`

use common::constants::utils;
use common::enums::{OrganizationScope, OrganizationStatus};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// OrganizationPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrganizationPo {
    /// 组织 ID
    pub id: String,
    /// 组织名称
    pub name: String,
    /// 组织描述
    pub description: String,
    /// 组织外网访问基础 URL
    ///
    /// 例如：`https://ai-orz.example.com/org/acme`
    /// 用于前端生成访问链接
    pub base_url: String,
    /// 组织集团名（纯展示标签）
    ///
    /// 不参与任何逻辑判断（不用于鉴权/通信判定/信任边界），仅用于
    /// 「关联组织」界面的归组展示。建联时若本端为空且对端非空则抄录；
    /// 可重名、允许不一致（不一致只影响显示）。NULL 表示未设置集团名。
    pub group_name: Option<String>,
    /// 状态枚举
    pub status: OrganizationStatus,
    /// 组织范围枚举（区分本地/远程，用于多节点网络扩展）
    pub scope: OrganizationScope,
    /// 邀请码（唯一，NULL 表示未启用邀请注册）
    pub invite_code: Option<String>,
    /// 创建人
    pub created_by: String,
    /// 修改人
    pub modified_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

impl OrganizationPo {
    /// 创建新的 OrganizationPo
    pub fn new(
        id: String,
        name: String,
        description: String,
        base_url: Option<String>,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id,
            name,
            description,
            base_url: base_url.unwrap_or_default(),
            group_name: None,
            status: OrganizationStatus::default(),
            scope: OrganizationScope::default(),
            invite_code: None,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// 生成并设置一个新的邀请码（24 字符，去掉易混淆的 0/O/1/I）
    pub fn regenerate_invite_code(&mut self) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let mut rng = rand::thread_rng();
        let code: String = (0..24)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        self.invite_code = Some(code.clone());
        code
    }
}
