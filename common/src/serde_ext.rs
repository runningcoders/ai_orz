//! serde 扩展：跨 JSON 边界保真的三态字段。
//!
//! 场景：更新类 DTO 需要区分「字段没传（保持原值）」「显式传 null（清空）」
//! 「传了值（覆盖）」。Rust 侧用 `Option<Option<T>>` 表达恰好：
//! - `None` = 保持
//! - `Some(None)` = 清空
//! - `Some(Some(v))` = 覆盖
//!
//! 但 serde 对 `Option<Option<T>>` 的默认实现会把 JSON `null` 反序列化成**外层** `None`，
//! 与「字段缺失」合并，导致「清空」永远无法跨越 JSON 边界（前端 → HTTP → 后端、
//! 工具调用 → 后端都走 JSON）。本模块提供配套的 `deserialize_with` 修正：
//! 字段缺失走 `#[serde(default)]` 得 `None`；字段出现时由本函数接收，`null` → `Some(None)`。
//! 序列化方向 `Option<Option<T>>` 的默认行为已经正确（`Some(None)` → `null`），无需配套。

use serde::{Deserialize, Deserializer};

/// 三态字段的反序列化适配，与 `#[serde(default, deserialize_with = "...")]` 配套使用。
///
/// 必须在字段上同时写 `#[serde(default)]`：字段缺失时不会调用本函数，由 `default`
/// 给出 `None`（保持）；字段出现（含值为 `null`）时才调用，本函数一律包一层 `Some`，
/// 于是 `null` → `Some(None)`（清空）、有值 → `Some(Some(v))`（覆盖）。
pub fn deserialize_tri_state<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Fixture {
        #[serde(
            default,
            deserialize_with = "deserialize_tri_state",
            skip_serializing_if = "Option::is_none"
        )]
        value: Option<Option<i64>>,
    }

    /// 三态必须能跨 JSON 往返，否则「清空」在 HTTP / 工具调用路径上静默失效。
    #[test]
    fn tri_state_survives_json_roundtrip() {
        let absent: Fixture = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(absent.value, None);

        let cleared: Fixture = serde_json::from_str(r#"{"value":null}"#).unwrap();
        assert_eq!(cleared.value, Some(None));

        let set: Fixture = serde_json::from_str(r#"{"value":7}"#).unwrap();
        assert_eq!(set.value, Some(Some(7)));

        assert_eq!(
            serde_json::to_string(&Fixture { value: None }).unwrap(),
            "{}"
        );
        assert_eq!(
            serde_json::to_string(&Fixture { value: Some(None) }).unwrap(),
            r#"{"value":null}"#
        );
        assert_eq!(
            serde_json::to_string(&Fixture {
                value: Some(Some(7))
            })
            .unwrap(),
            r#"{"value":7}"#
        );
    }
}
