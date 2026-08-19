//! User 持久化对象
//!
//! 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`

use common::constants::utils;
use common::enums::{UserRole, UserStatus};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// UserPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserPo {
    /// 用户 ID
    pub id: String,
    /// 所属组织 ID
    pub organization_id: String,
    /// 用户名（唯一登录名）
    pub username: String,
    /// 显示名称
    pub display_name: String,
    /// 邮箱
    pub email: String,
    /// 密码哈希（bcrypt）
    pub password_hash: String,
    /// 用户角色
    pub role: UserRole,
    /// 用户状态枚举
    pub status: UserStatus,
    /// 创建人
    pub created_by: String,
    /// 修改人
    pub modified_by: String,
    /// 创建时间戳（秒）
    pub created_at: i64,
    /// 更新时间戳（秒）
    pub updated_at: i64,
    /// 用户自述偏好（声明式画像，Markdown 自由文本，空字符串表示未设置）
    ///
    /// 只允许用户本人通过 update_current_user 修改，Agent 无写入路径；
    /// Agent 观察总结的偏好走知识图谱（user_preference tag），两者独立演进
    pub preferences: String,
}

impl UserPo {
    /// 生成用户基础信息的 Prompt 格式
    ///
    /// 包含：用户 ID、显示名称、用户名、邮箱、角色（偏好非空时附加）
    /// 所有字段使用统一的【】标识格式，便于大模型识别和提取
    /// 敏感信息（密码哈希等）不会暴露
    pub fn to_basic_info_prompt(&self) -> String {
        let role_name = match self.role {
            UserRole::SuperAdmin => "超级管理员",
            UserRole::Admin => "管理员",
            UserRole::Member => "成员",
        };

        let mut parts = vec![
            format!("【用户 ID】{}", self.id),
            format!("【显示名称】{}", self.display_name),
            format!("【用户名】{}", self.username),
            format!("【邮箱】{}", self.email),
            format!("【角色】{}", role_name),
            format!("【组织 ID】{}", self.organization_id),
        ];
        // 用户自述偏好：非空时拼入画像，空则不拼（保持 Prompt 干净）
        if !self.preferences.is_empty() {
            parts.push(format!("【用户偏好】{}", self.preferences));
        }

        parts.join("\n")
    }

    /// 创建新的 UserPo
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        organization_id: String,
        username: String,
        display_name: String,
        email: String,
        password_hash: String,
        role: UserRole,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp();
        Self {
            id,
            organization_id,
            username,
            display_name,
            email,
            password_hash,
            role,
            status: UserStatus::default(),
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
            preferences: String::new(),
        }
    }

    /// 获取用户角色（直接返回，不再需要转换）
    pub fn user_role(&self) -> UserRole {
        self.role
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user() -> UserPo {
        UserPo::new(
            "user-1".to_string(),
            "org-1".to_string(),
            "alice".to_string(),
            "Alice".to_string(),
            "alice@example.com".to_string(),
            "hash".to_string(),
            UserRole::Member,
            "system".to_string(),
        )
    }

    #[test]
    fn test_to_basic_info_prompt_without_preferences() {
        let user = test_user();
        let prompt = user.to_basic_info_prompt();
        assert!(prompt.contains("【用户 ID】user-1"));
        assert!(prompt.contains("【显示名称】Alice"));
        // 偏好为空时不拼入，保持 Prompt 干净
        assert!(!prompt.contains("【用户偏好】"));
    }

    #[test]
    fn test_to_basic_info_prompt_with_preferences() {
        let mut user = test_user();
        user.preferences = "- 回复请用中文\n- 汇报要简洁".to_string();
        let prompt = user.to_basic_info_prompt();
        assert!(prompt.contains("【用户偏好】- 回复请用中文\n- 汇报要简洁"));
        // 敏感信息不暴露
        assert!(!prompt.contains("hash"));
    }
}
