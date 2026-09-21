//! 通用「清除字段」哨兵
//!
//! 部分更新场景下，`Option<T>` 的 `None` 已被占用为「不修改该字段」，
//! 无法表达「清除该字段」。空字符串不可靠：序列化链路上可能被规整为
//! null 或直接丢失，语义无法安全穿透。
//!
//! 哨兵值把「清除」从 `None` 里拆出来，成为协议层一个普通的非空字符串取值：
//!
//! | 传入值                       | 语义                       |
//! |------------------------------|----------------------------|
//! | `None`                       | 不修改，保持原值            |
//! | `Some(CLEAR_FIELD_SENTINEL)` | 清除该字段（落库为 `NULL`） |
//! | `Some(真实值)`               | 设置为该值                  |
//!
//! ## 关键约束：只存在于协议层，永不落库
//!
//! 接收方（handler）必须在解释后折叠回 `None`；创建等无原值的写路径
//! 应先过 [`fold_clear_sentinel`] 防御，保证哨兵字符串本身不进数据库。
//!
//! ## 为什么不会和真实值撞车
//!
//! 常见字符串业务 ID（UUIDv7、模板 ID、数字 ID 等）字符集不含下划线，
//! 本常量双下划线包裹，构造上不可能相等，无需额外校验。
//!
//! 先例：[`crate::constants::message::DEFAULT_CONVERSATION_PROJECT_ID`]（读侧
//! 哨兵）；本模块面向**写侧部分更新协议**。

/// 「清除字段」哨兵值：写入该值表示删除/清空目标字段，与 `None`（不修改）区分
pub const CLEAR_FIELD_SENTINEL: &str = "__clear__";

/// 折叠哨兵取值：`Some(哨兵)` 折叠为 `None`，其余原样返回
///
/// 用于创建等「无原值」的写路径防御：调用方收到哨兵时按未设置处理。
pub fn fold_clear_sentinel(value: Option<String>) -> Option<String> {
    match value {
        Some(v) if v == CLEAR_FIELD_SENTINEL => None,
        other => other,
    }
}

/// 可清除字符串字段的部分更新合并
///
/// - `incoming = None`：不修改，返回 `current`（原值）
/// - `incoming = Some(哨兵)`：清除，返回 `None`
/// - `incoming = Some(v)`：设置，返回 `Some(v)`
pub fn merge_clearable_string(current: Option<String>, incoming: Option<String>) -> Option<String> {
    match incoming {
        None => current,
        Some(v) if v == CLEAR_FIELD_SENTINEL => None,
        Some(v) => Some(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinel_can_never_collide_with_uuid() {
        // UUID / 模板 ID 字符集不含下划线 → 构造上不可能相等
        assert!(
            CLEAR_FIELD_SENTINEL
                .chars()
                .any(|c| !c.is_ascii_hexdigit() && c != '-')
        );
    }

    #[test]
    fn fold_sentinel_to_none() {
        assert_eq!(
            fold_clear_sentinel(Some(CLEAR_FIELD_SENTINEL.to_string())),
            None
        );
        assert_eq!(fold_clear_sentinel(None), None);
        assert_eq!(
            fold_clear_sentinel(Some("0192f3a1-7c4e-7000-8a2b-0d1e2f3a4b5c".to_string())),
            Some("0192f3a1-7c4e-7000-8a2b-0d1e2f3a4b5c".to_string())
        );
    }

    #[test]
    fn merge_none_keeps_current() {
        assert_eq!(
            merge_clearable_string(Some("orig".to_string()), None),
            Some("orig".to_string())
        );
        assert_eq!(merge_clearable_string(None, None), None);
    }

    #[test]
    fn merge_sentinel_clears() {
        assert_eq!(
            merge_clearable_string(
                Some("orig".to_string()),
                Some(CLEAR_FIELD_SENTINEL.to_string())
            ),
            None
        );
        assert_eq!(
            merge_clearable_string(None, Some(CLEAR_FIELD_SENTINEL.to_string())),
            None
        );
    }

    #[test]
    fn merge_value_overrides() {
        assert_eq!(
            merge_clearable_string(Some("orig".to_string()), Some("new".to_string())),
            Some("new".to_string())
        );
        assert_eq!(
            merge_clearable_string(None, Some("new".to_string())),
            Some("new".to_string())
        );
    }
}
