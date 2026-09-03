//! `redact!` 宏的类型分派层（autoref specialization）
//!
//! 同一个 [`redact!`] 宏要同时支持字符串族与可 JSON 往返的 DTO / `Value`，
//! 两者的最优路径不同（字符串走文本扫描、结构体走 JSON 遍历），而 Rust 没有
//! 原生特化。这里用「自动引用特化」实现零歧义分派：
//!
//! - **第一优先级** `T: AsRef<str>`（`String` / `&str` / `Cow<str>` …）
//!   → 文本扫描，返回 `String`
//! - **第二优先级** `T: Serialize + DeserializeOwned + Clone`
//!   （DTO / `Value` / `Vec<T>` / `Option<T>` …）→ JSON 往返，返回同类型 `T`
//!
//! 原理：宏把表达式包一层引用 `(&expr).redact_dispatch(..)`，方法解析按候选
//! receiver 顺序探测：`&T`（Self=T 的 `&self` 方法）→ `&&T`（Self=&T 的
//! `&self` 方法）。字符串 impl 在第一级命中；其余类型第一级不适用（非
//! `AsRef<str>`），落到第二级。两级 impl 分属不同 trait，无重叠冲突。
//!
//! 失败兜底（fail-safe 降级链）：JSON 往返失败 → 全遮蔽重试 → 仍失败则
//! 原值返回并打 `log_error`（该路径在生产中几乎不可达，仅防御非对称
//! serde derive 的极端情况）。

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::engine;
use super::policy::RedactPolicy;
use super::rule::MASK_FULL;

/// 第一优先级分派：字符串族 → 文本级扫描
pub trait RedactStrDispatch {
    fn redact_dispatch(&self, policy: &RedactPolicy) -> String;
}

impl<T> RedactStrDispatch for T
where
    T: ?Sized + AsRef<str>,
{
    fn redact_dispatch(&self, policy: &RedactPolicy) -> String {
        engine::redact_text(self.as_ref(), *policy).into_owned()
    }
}

/// 第二优先级分派：可 JSON 往返类型 → 序列化脱敏反序列化
pub trait RedactSerdeDispatch<T> {
    fn redact_dispatch(&self, policy: &RedactPolicy) -> T;
}

impl<T> RedactSerdeDispatch<T> for &T
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn redact_dispatch(&self, policy: &RedactPolicy) -> T {
        let mut value = match serde_json::to_value(&**self) {
            Ok(v) => v,
            Err(err) => {
                // 无法获得 JSON 表示（非自描述序列化等，极罕见）：无从扫描，原样返回
                log_error!("redaction: serialize for redaction failed, returning original: {err}");
                return (*(*self)).clone();
            }
        };

        engine::redact_json(&mut value, *policy);

        match serde_json::from_value::<T>(value) {
            Ok(redacted) => redacted,
            Err(_) => {
                // 值被 Partial 遮蔽后可能触发 DTO 上的反序列化校验：降级为全遮蔽重试，
                // 保证出口处绝不带出原文
                let fully_masked = mask_all_strings(
                    serde_json::to_value(&**self)
                        .unwrap_or(serde_json::Value::String(MASK_FULL.to_string())),
                );
                match serde_json::from_value::<T>(fully_masked) {
                    Ok(fallback) => fallback,
                    Err(err) => {
                        log_error!(
                            "redaction: deserialize after redaction failed, returning original: {err}"
                        );
                        (*(*self)).clone()
                    }
                }
            }
        }
    }
}

/// 递归把所有字符串值替换为全遮蔽标记（末级兜底，不依赖键名规则）
fn mask_all_strings(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(_) => Value::String(MASK_FULL.to_string()),
        Value::Array(items) => Value::Array(items.into_iter().map(mask_all_strings).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, mask_all_strings(v)))
                .collect(),
        ),
        other => other,
    }
}

/// 对外接口输出脱敏：把字符串 / 结构体 / `serde_json::Value` 放进去，返回脱敏后的同类型值
///
/// - `redact!(text)` —— `String` / `&str` → `String`（文本级扫描）
/// - `redact!(dto)` —— 任意 `Serialize + DeserializeOwned` 类型（DTO / `Value` /
///   `Vec` / `Option` …）→ 同类型（JSON 往返脱敏）
/// - `redact!(value, LOG)` —— 第二参数指定策略（默认 [`EXPORT`](crate::pkg::redaction::EXPORT)）
///
/// 在需要脱敏的接口末尾对返回值调用即可：
/// `Ok(Json(redact!(response)))`
#[macro_export]
macro_rules! redact {
    ($value:expr $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::pkg::redaction::{RedactSerdeDispatch, RedactStrDispatch};
        (&$value).redact_dispatch(&$crate::pkg::redaction::EXPORT)
    }};
    ($value:expr, $policy:expr $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::pkg::redaction::{RedactSerdeDispatch, RedactStrDispatch};
        (&$value).redact_dispatch(&$policy)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct Dto {
        api_key: String,
        max_tokens: u32,
        note: String,
    }

    #[test]
    fn macro_redacts_string() {
        let s = String::from("api_key=sk-abcdef123456");
        assert_eq!(redact!(s), "api_key=sk-a***3456");

        let r = "password: hunter2hunter2";
        assert_eq!(redact!(r), "password: hunt***ter2");
    }

    #[test]
    fn macro_redacts_json_value_in_place_type() {
        let value = json!({"api_key": "sk-abcdef123456", "max_tokens": 4096});
        let out = redact!(value);
        assert!(out.is_object());
        assert_eq!(out["api_key"], "sk-a***3456");
        assert_eq!(out["max_tokens"], 4096);
    }

    #[test]
    fn macro_redacts_dto_round_trip() {
        let dto = Dto {
            api_key: "sk-abcdef123456".into(),
            max_tokens: 4096,
            note: "call with --token supersecretvalue99".into(),
        };
        let out: Dto = redact!(dto);
        assert_eq!(out.api_key, "sk-a***3456");
        assert_eq!(out.max_tokens, 4096);
        assert_eq!(out.note, "call with --token supe***ue99");
    }

    #[test]
    fn macro_supports_custom_policy() {
        let dirty = json!({"password": "hunter2hunter2"});
        let out = redact!(dirty, super::super::policy::LOG);
        assert_eq!(out["password"], MASK_FULL);
    }

    #[test]
    fn vec_and_option_work() {
        let items = vec![Dto {
            api_key: "sk-abcdef123456".into(),
            max_tokens: 1,
            note: String::new(),
        }];
        let out = redact!(items);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].api_key, "sk-a***3456");

        let opt = Some(json!({"api_key": "sk-abcdef123456"}));
        let out = redact!(opt);
        assert_eq!(out.unwrap()["api_key"], "sk-a***3456");
    }
}
