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
//! 失败语义（fail-closed）：JSON 往返失败 → 全遮蔽重试 → 仍失败则返回
//! `Err`（`serde_json::Error`），**绝不把原文带回给调用方**。打不打日志、
//! 降级还是报错，由上层使用者自行决定——本库内部不做任何日志输出。

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::engine;
use super::policy::RedactPolicy;
use super::rule::MASK_FULL;

/// 第一优先级分派：字符串族 → 文本级扫描
///
/// 文本扫描不可能失败，`Err` 分支恒不触发；返回 `Result` 是为了让
/// [`redact!`] 宏对字符串族与 DTO 统一产出 `Result`，调用方可以无差别
/// 地用 `?` 传播。
pub trait RedactStrDispatch {
    /// 文本级脱敏：扫描敏感 KV / CLI flag / Bearer 形态，返回脱敏后的字符串
    fn redact_dispatch(&self, policy: &RedactPolicy) -> Result<String, serde_json::Error>;
}

impl<T> RedactStrDispatch for T
where
    T: ?Sized + AsRef<str>,
{
    fn redact_dispatch(&self, policy: &RedactPolicy) -> Result<String, serde_json::Error> {
        Ok(engine::redact_text(self.as_ref(), *policy).into_owned())
    }
}

/// 第二优先级分派：可 JSON 往返类型 → 序列化脱敏反序列化
pub trait RedactSerdeDispatch<T> {
    /// JSON 往返脱敏：序列化 → 引擎脱敏 → 反序列化，返回同类型；
    /// 遮蔽值无法通过反序列化校验时返回 `Err`（fail-closed，不回退原文）
    fn redact_dispatch(&self, policy: &RedactPolicy) -> Result<T, serde_json::Error>;
}

impl<T> RedactSerdeDispatch<T> for &T
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn redact_dispatch(&self, policy: &RedactPolicy) -> Result<T, serde_json::Error> {
        let mut value = serde_json::to_value(&**self)?;

        engine::redact_json(&mut value, *policy);

        match serde_json::from_value::<T>(value) {
            Ok(redacted) => Ok(redacted),
            Err(_) => {
                // 值被 Partial 遮蔽后可能触发 DTO 上的反序列化校验：降级为全遮蔽重试，
                // 尽量挽救；连全遮蔽都无法反序列化则返回 Err，绝不回退原文（fail-closed）
                let fully_masked = mask_all_strings(serde_json::to_value(&**self)?);
                serde_json::from_value::<T>(fully_masked)
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
/// - `redact!(text)` —— `String` / `&str` → `Result<String, _>`（文本级扫描，恒 `Ok`）
/// - `redact!(dto)` —— 任意 `Serialize + DeserializeOwned` 类型（DTO / `Value` /
///   `Vec` / `Option` …）→ `Result<同类型, serde_json::Error>`（JSON 往返脱敏）
/// - `redact!(value, LOG)` —— 第二参数指定策略（默认 [`EXPORT`](crate::redaction::EXPORT)）
///
/// 返回 `Result` 是 fail-closed 语义：脱敏失败时不回退原文，由调用方决定
/// 报错还是降级。`common::error` 已提供 `From<serde_json::Error>`，在返回
/// `common::error::Result` 的接口里直接 `?` 即可：
///
/// ```ignore
/// Ok(Json(redact!(response)?))
/// ```
#[macro_export]
macro_rules! redact {
    ($value:expr $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::redaction::{RedactSerdeDispatch, RedactStrDispatch};
        (&$value).redact_dispatch(&$crate::redaction::EXPORT)
    }};
    ($value:expr, $policy:expr $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::redaction::{RedactSerdeDispatch, RedactStrDispatch};
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
        assert_eq!(redact!(s).unwrap(), "api_key=sk-a***3456");

        let r = "password: hunter2hunter2";
        assert_eq!(redact!(r).unwrap(), "password: hunt***ter2");
    }

    #[test]
    fn macro_redacts_json_value_in_place_type() {
        let value = json!({"api_key": "sk-abcdef123456", "max_tokens": 4096});
        let out = redact!(value).unwrap();
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
        let out: Dto = redact!(dto).unwrap();
        assert_eq!(out.api_key, "sk-a***3456");
        assert_eq!(out.max_tokens, 4096);
        assert_eq!(out.note, "call with --token supe***ue99");
    }

    #[test]
    fn macro_supports_custom_policy() {
        let dirty = json!({"password": "hunter2hunter2"});
        let out = redact!(dirty, crate::redaction::policy::LOG).unwrap();
        assert_eq!(out["password"], MASK_FULL);
    }

    #[test]
    fn vec_and_option_work() {
        let items = vec![Dto {
            api_key: "sk-abcdef123456".into(),
            max_tokens: 1,
            note: String::new(),
        }];
        let out = redact!(items).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].api_key, "sk-a***3456");

        let opt = Some(json!({"api_key": "sk-abcdef123456"}));
        let out = redact!(opt).unwrap();
        assert_eq!(out.unwrap()["api_key"], "sk-a***3456");
    }

    /// 反序列化校验拒绝遮蔽值的类型：两阶段（Partial → 全遮蔽）都过不去，
    /// 必须返回 Err 而不是原文（fail-closed 验证）
    #[derive(Debug, Clone, Serialize, PartialEq)]
    struct StrictDto {
        name: String,
    }

    impl<'de> serde::Deserialize<'de> for StrictDto {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let name = String::deserialize(deserializer)?;
            if name == "***" {
                return Err(serde::de::Error::custom("masked value rejected"));
            }
            Ok(StrictDto { name })
        }
    }

    #[test]
    fn macro_fails_closed_when_deserialization_rejects_masks() {
        // "password" 是敏感键名规则词，遮蔽必然发生；StrictDto 连全遮蔽值都拒绝，
        // 因此两阶段全部失败 → 必须 Err，绝不能带出原文
        let strict = StrictDto {
            name: "password123".into(),
        };
        let out = redact!(strict);
        assert!(out.is_err(), "fail-closed: must not return original");
    }
}
