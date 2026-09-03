//! 统一脱敏引擎
//!
//! 项目边界决策（2026-09-03，用户拍板）
//! --------------------------
//! **系统内部不脱敏，仅在对外接口输出时按需脱敏。** 内部存储（JSONL trace、
//! SQLite、日志）保持原文，风险由访问控制承担；需要脱敏的出口接口在返回前
//! 用 [`redact!`] 宏包一层即可，不做全局响应改写。
//!
//! 用法
//! ----
//! ```ignore
//! use crate::redact;
//! // 字符串 → String；DTO / Value / Vec → 同类型
//! Ok(Json(redact!(response)))
//! ```
//!
//! 分层
//! ----
//! ```text
//! rule.rs     规则注册表   声明「匹配什么」（新增凭证 = 加一行，不改逻辑）
//! mask.rs     脱敏样式     决定命中后值呈现形态（Full / Partial 保留首尾）
//! policy.rs   场景策略     组合样式 + 扫描开关 + 深度/体积上限
//! engine.rs   引擎          JSON 遍历器 + AC 预检文本扫描器
//! redact.rs   分派 + 宏    redact! 宏的类型分派（autoref specialization）
//! ```
//!
//! 扩展指南
//! --------
//! - **新增一种凭证**：在 [`rule::KEY_RULES`] 追加一条 `KeyRule`，其余零改动。
//! - **新增一种值形态识别**（如裸 `sk-` / `ghp_` 前缀凭证）：在 `engine.rs` 的
//!   文本扫描循环里加一个分支，或在 `rule.rs` 增加 `ValueRule` 表。
//! - **新增一种场景**：在 [`policy`] 加一个 `RedactPolicy` 常量。

pub mod engine;
pub mod mask;
pub mod policy;
pub mod redact;
pub mod rule;

use std::borrow::Cow;

use serde_json::Value;

pub use engine::{redact_json as redact_json_with, redact_text as redact_text_with, warmup};
pub use mask::MaskStyle;
pub use policy::{EXPORT, LOG, PERSIST, RedactPolicy};
pub use redact::{RedactSerdeDispatch, RedactStrDispatch};
pub use rule::{KeyRule, MASK_FULL, ValueClass, match_key};

/// 对外接口输出脱敏：就地递归脱敏 JSON（使用 [`EXPORT`] 策略）
///
/// 用于 HTTP 响应体序列化前的最后一道处理。保留 JSON 结构与值类型，
/// 字符串值默认按 Partial 样式保留首尾。
///
/// 返回是否发生了修改：`false` 表示响应体干净，调用方可跳过重新序列化。
pub fn redact_json(value: &mut Value) -> bool {
    engine::redact_json(value, EXPORT)
}

/// 对外接口输出脱敏：文本级扫描（使用 [`EXPORT`] 策略）
///
/// 无敏感词时返回 [`Cow::Borrowed`]，调用方可据此跳过写回。
pub fn redact_text(text: &str) -> Cow<'_, str> {
    engine::redact_text(text, EXPORT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn facade_redacts_json_with_export_policy() {
        let mut value = json!({"api_key": "sk-abcdef123456", "max_tokens": 4096});
        redact_json(&mut value);
        assert_eq!(value["api_key"], "sk-a***3456");
        assert_eq!(value["max_tokens"], 4096);
    }

    #[test]
    fn facade_redacts_text_with_export_policy() {
        assert_eq!(
            redact_text("api_key=sk-abcdef123456"),
            "api_key=sk-a***3456"
        );
    }

    #[test]
    fn reports_whether_changed() {
        // 中间件据此判断是否值得重新序列化
        let mut clean = json!({"user": "alice", "max_tokens": 4096});
        assert!(!redact_json(&mut clean));

        let mut dirty = json!({"password": "hunter2hunter2"});
        assert!(redact_json(&mut dirty));
    }
}
