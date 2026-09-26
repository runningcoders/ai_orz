//! 通用 localStorage 组件层 —— 统一键名前缀、类型化读写、版本兼容与业务结构体集中定义
//!
//! 背景（AMan 拍板 + 技术负责人评估报告路径一）：此前各业务散点直调 localStorage，
//! 键名两套、编码三种并存，存在键名冲突与一致性问题。本层收敛为唯一出口：
//!
//! - 统一 [`KEY_PREFIX`] 前缀，Key 常量集中定义（`keys` 模块），禁止业务侧散点自造键名；
//! - 类型化接口 [`get_json`] / [`set_json`] / [`remove`]，JSON 编码统一；
//! - 版本兼容：值内嵌 `{"v":1,"data":...}` 包装（[`DATA_VERSION`]），未来结构演进按 `v` 分流；
//! - 错误分层：组件层返回 [`LocalStoreError`]，调用方决定兜底策略（静默 / toast / 默认值）；
//! - 存量迁移：[`get_json_with_legacy`] / [`get_string_with_legacy`] 在新键未命中时回退读
//!   旧键旧编码，命中即回写新键，完成一次性迁移（防登录态 / 配置丢失）；
//! - 相关结构体集中定义（如未读角标 [`UnreadBadges`]）。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 所有业务键的统一前缀；新增 Key 必须挂在 `keys` 模块下并以此开头。
/// 当前生产代码经 `keys` 模块常量间接遵循前缀约定，本常量作为外部约定入口与
/// 单测兜底校验基准保留（bin 目标暂无直接调用方，故豁免 dead_code）。
#[allow(dead_code)]
pub const KEY_PREFIX: &str = "ai_orz:";

/// 组件层数据格式版本（值内嵌包装 `{"v":1,"data":...}`）
pub const DATA_VERSION: u32 = 1;

/// 集中定义的全部业务 Key（新编码，均以 [`super::KEY_PREFIX`] 开头，模块内测试兜底校验）
pub mod keys {
    /// 登录标志位（旧键 `ai_orz_logged_in`）
    pub const AUTH_LOGGED_IN: &str = "ai_orz:auth_logged_in";
    /// 用户角色（旧键 `ai_orz_role`）
    pub const AUTH_ROLE: &str = "ai_orz:auth_role";
    /// 用户名（旧键 `ai_orz_username`）
    pub const AUTH_USERNAME: &str = "ai_orz:auth_username";
    /// 显示名（旧键 `ai_orz_display_name`）
    pub const AUTH_DISPLAY_NAME: &str = "ai_orz:auth_display_name";
    /// 前端配置（旧键 `ai_orz_config`）
    pub const CONFIG: &str = "ai_orz:config";
    /// 主题（旧键 `ai_orz_theme`）
    pub const THEME: &str = "ai_orz:theme";
    /// 对话页信息侧栏展开状态（旧键 `chat_project_panel_open`）
    pub const CHAT_PANEL_OPEN: &str = "ai_orz:chat_panel_open";
    /// 会话未读角标（问题二新增）
    pub const UNREAD_BADGES: &str = "ai_orz:unread_badges";
}

/// 存量旧键常量（仅迁移读取 / 清除用，禁止新代码写入）
pub mod legacy {
    pub const AUTH_LOGGED_IN: &str = "ai_orz_logged_in";
    pub const AUTH_ROLE: &str = "ai_orz_role";
    pub const AUTH_USERNAME: &str = "ai_orz_username";
    pub const AUTH_DISPLAY_NAME: &str = "ai_orz_display_name";
    pub const CONFIG: &str = "ai_orz_config";
    pub const THEME: &str = "ai_orz_theme";
    pub const CHAT_PANEL_OPEN: &str = "chat_project_panel_open";
}

/// 组件层统一错误类型；调用方按场景决定兜底策略
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalStoreError {
    /// localStorage 不可用（无 window / 隐私模式等）
    StorageUnavailable,
    /// Storage 读取 API 失败
    Read(String),
    /// 内容反序列化失败
    Decode(String),
    /// 写入失败（序列化 / Storage API 错误）
    Write(String),
    /// 删除失败
    Remove(String),
}

impl std::fmt::Display for LocalStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StorageUnavailable => write!(f, "localStorage not available"),
            Self::Read(msg) => write!(f, "localStorage read failed: {msg}"),
            Self::Decode(msg) => write!(f, "localStorage decode failed: {msg}"),
            Self::Write(msg) => write!(f, "localStorage write failed: {msg}"),
            Self::Remove(msg) => write!(f, "localStorage remove failed: {msg}"),
        }
    }
}

/// 版本包装：新键统一以 `{"v":<DATA_VERSION>,"data":<业务值>}` 落盘
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Versioned<T> {
    v: u32,
    data: T,
}

/// 未读角标持久化结构（问题二）：会话键 → 未读数。
/// 会话键约定：默认对话 = [`UNREAD_DEFAULT_KEY`]；项目会话 = project_id 原值。
pub type UnreadBadges = HashMap<String, u32>;

/// 未读角标中「默认对话」的哨兵会话键
pub const UNREAD_DEFAULT_KEY: &str = "default";

/// 读取类型化值（新键，版本包装编码）。键不存在返回 `Ok(None)`。
pub fn get_json<T: DeserializeOwned>(key: &str) -> Result<Option<T>, LocalStoreError> {
    let Some(storage) = crate::utils::local_storage() else {
        return Err(LocalStoreError::StorageUnavailable);
    };
    let raw_opt = storage
        .get(key)
        .map_err(|e| LocalStoreError::Read(format!("{e:?}")))?;
    let Some(raw) = raw_opt else {
        return Ok(None);
    };
    let versioned: Versioned<T> =
        serde_json::from_str(&raw).map_err(|e| LocalStoreError::Decode(e.to_string()))?;
    // 预留：未来结构演进在此按 versioned.v 分流读取
    Ok(Some(versioned.data))
}

/// 写入类型化值（新键，版本包装编码）
pub fn set_json<T: Serialize>(key: &str, value: &T) -> Result<(), LocalStoreError> {
    let Some(storage) = crate::utils::local_storage() else {
        return Err(LocalStoreError::StorageUnavailable);
    };
    let raw = serde_json::to_string(&Versioned {
        v: DATA_VERSION,
        data: value,
    })
    .map_err(|e| LocalStoreError::Write(e.to_string()))?;
    storage
        .set(key, &raw)
        .map_err(|e| LocalStoreError::Write(format!("{e:?}")))
}

/// 删除键（新旧编码通用）
pub fn remove(key: &str) -> Result<(), LocalStoreError> {
    let Some(storage) = crate::utils::local_storage() else {
        return Err(LocalStoreError::StorageUnavailable);
    };
    storage
        .remove_item(key)
        .map_err(|e| LocalStoreError::Remove(format!("{e:?}")))
}

/// 兼容读取（存量迁移）：新键（版本包装）未命中时回退读旧键裸 JSON；
/// 旧键命中即回写新键，完成一次性迁移。适用于旧编码本身是合法 JSON 的场景
/// （config 整体 JSON；auth 布尔 / 数字的明文——`"true"` / `"1"` 恰为合法 JSON 字面量）。
pub fn get_json_with_legacy<T: DeserializeOwned + Serialize>(
    key: &str,
    legacy_key: &str,
) -> Result<Option<T>, LocalStoreError> {
    if let Some(value) = get_json::<T>(key)? {
        return Ok(Some(value));
    }
    let Some(storage) = crate::utils::local_storage() else {
        return Err(LocalStoreError::StorageUnavailable);
    };
    let raw_opt = storage
        .get(legacy_key)
        .map_err(|e| LocalStoreError::Read(format!("{e:?}")))?;
    let Some(raw) = raw_opt else {
        return Ok(None);
    };
    let value: T =
        serde_json::from_str(&raw).map_err(|e| LocalStoreError::Decode(e.to_string()))?;
    // 一次性迁移：读旧成功即写新；写失败不阻断读取（下次进入再试）
    let _ = set_json(key, &value);
    Ok(Some(value))
}

/// 兼容读取（存量迁移，明文变体）：新键未命中时回退读旧键明文字符串；
/// 旧键命中即回写新键。适用于旧编码为裸文本的场景（主题、用户名、侧栏开关 `"1"`/`"0"`）。
pub fn get_string_with_legacy(
    key: &str,
    legacy_key: &str,
) -> Result<Option<String>, LocalStoreError> {
    if let Some(value) = get_json::<String>(key)? {
        return Ok(Some(value));
    }
    let Some(storage) = crate::utils::local_storage() else {
        return Err(LocalStoreError::StorageUnavailable);
    };
    let raw_opt = storage
        .get(legacy_key)
        .map_err(|e| LocalStoreError::Read(format!("{e:?}")))?;
    let Some(raw) = raw_opt else {
        return Ok(None);
    };
    // 一次性迁移：读旧成功即写新；写失败不阻断读取（下次进入再试）
    let _ = set_json(key, &raw);
    Ok(Some(raw))
}

/// 读取未读角标全集（缺失时为空表）
pub fn load_unread_badges() -> UnreadBadges {
    get_json::<UnreadBadges>(keys::UNREAD_BADGES)
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_keys_use_unified_prefix() {
        for key in [
            keys::AUTH_LOGGED_IN,
            keys::AUTH_ROLE,
            keys::AUTH_USERNAME,
            keys::AUTH_DISPLAY_NAME,
            keys::CONFIG,
            keys::THEME,
            keys::CHAT_PANEL_OPEN,
            keys::UNREAD_BADGES,
        ] {
            assert!(
                key.starts_with(KEY_PREFIX),
                "key {key} must start with unified prefix {KEY_PREFIX}"
            );
        }
    }

    #[test]
    fn versioned_roundtrip_preserves_data() {
        let raw = serde_json::to_string(&Versioned {
            v: DATA_VERSION,
            data: 42i32,
        })
        .unwrap();
        let decoded: Versioned<i32> = serde_json::from_str(&raw).unwrap();
        assert_eq!(decoded.v, DATA_VERSION);
        assert_eq!(decoded.data, 42);
    }

    #[test]
    fn unread_badges_json_roundtrip() {
        let mut badges: UnreadBadges = HashMap::new();
        badges.insert(UNREAD_DEFAULT_KEY.to_string(), 3);
        badges.insert("proj-1".to_string(), 7);
        let raw = serde_json::to_string(&badges).unwrap();
        let decoded: UnreadBadges = serde_json::from_str(&raw).unwrap();
        assert_eq!(decoded.get(UNREAD_DEFAULT_KEY), Some(&3));
        assert_eq!(decoded.get("proj-1"), Some(&7));
    }

    #[test]
    fn error_display_messages() {
        // 主机测试不可触达 window（js-sys 在 non-wasm 目标 panic），
        // 本测只覆盖错误展示与解码分层；StorageUnavailable 路径（无 window /
        // 隐私模式）由浏览器实机验证承接。
        assert_eq!(
            LocalStoreError::StorageUnavailable.to_string(),
            "localStorage not available"
        );
        assert_eq!(
            LocalStoreError::Decode("bad json".to_string()).to_string(),
            "localStorage decode failed: bad json"
        );
    }

    #[test]
    fn versioned_decode_rejects_non_wrapped_payload() {
        // 版本兼容分层：未按 {"v":..,"data":..} 包装的载荷应解码失败而非静默取值
        let decoded: Result<Versioned<i32>, _> = serde_json::from_str("42");
        assert!(decoded.is_err());
    }

    #[test]
    fn unread_default_key_is_sentinel() {
        assert_eq!(UNREAD_DEFAULT_KEY, "default");
    }
}
