//! 向量索引维护相关枚举

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 向量索引重建模式
///
/// 三档差异只在两个动作上：**是否清空集合**、**是否逐条比对 `content_hash`**。
///
/// | 模式 | 清空集合 | 逐条比对 | 典型入口 |
/// |------|----------|----------|----------|
/// | `Auto` | 口径变化才清 | 否 | 换 embedding provider、启动补齐 |
/// | `Missing` | 口径变化才清 | 是 | 技能页「查漏补缺」 |
/// | `Force` | 总是清 | 否 | 技能页「强制重建」 |
///
/// `Auto` 与 `Missing` 的区别是最容易混淆的一点：`Auto` 在「provider 与向量文本
/// 口径一致」时**整体跳过**（一条不看），而集合里仍可能缺条目（写入时无 provider、
/// `upsert` 失败被 warn 吞掉、历史脏数据）。`Missing` 不看口径是否一致，一律逐条
/// 比对，所以它才是「查漏补缺」；把它实现成 `Auto` 会让按钮在常见情形下变成空操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VectorRebuildMode {
    /// 幂等：provider 与向量文本口径都一致就整体跳过（自动触发用）
    #[default]
    Auto,
    /// 查漏补缺：不清空集合，只重算「集合里没有」或「content_hash 与当前文本不符」的条目
    Missing,
    /// 强制全量：清空集合后逐条重算
    Force,
}

impl VectorRebuildMode {
    /// 线格式字符串（与 serde 的 `snake_case` 保持一致，用于日志与任务结果回显）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Missing => "missing",
            Self::Force => "force",
        }
    }

    /// 是否跳过「provider 与文本口径一致就整体跳过」的幂等短路
    ///
    /// `Missing` 也要跳过短路 —— 短路是「整体不看」，而查漏补缺必须逐条看。
    pub fn ignores_idempotent_skip(&self) -> bool {
        !matches!(self, Self::Auto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_auto() {
        // 缺省必须是 Auto：老客户端不传 `mode` 时行为不能变（只有口径变化才重算）
        assert_eq!(VectorRebuildMode::default(), VectorRebuildMode::Auto);
    }

    #[test]
    fn wire_format_is_snake_case() {
        for (mode, wire) in [
            (VectorRebuildMode::Auto, "\"auto\""),
            (VectorRebuildMode::Missing, "\"missing\""),
            (VectorRebuildMode::Force, "\"force\""),
        ] {
            assert_eq!(serde_json::to_string(&mode).unwrap(), wire);
            assert_eq!(mode.as_str(), wire.trim_matches('"'));
            assert_eq!(
                serde_json::from_str::<VectorRebuildMode>(wire).unwrap(),
                mode
            );
        }
    }

    #[test]
    fn missing_ignores_idempotent_skip_but_auto_does_not() {
        // 反例：把 Missing 也走幂等短路 ⇒ 口径一致时技能页「查漏补缺」一条不处理，
        // 用户看到「完成但 0 条」，真正缺失的向量永远不会被补上
        assert!(!VectorRebuildMode::Auto.ignores_idempotent_skip());
        assert!(VectorRebuildMode::Missing.ignores_idempotent_skip());
        assert!(VectorRebuildMode::Force.ignores_idempotent_skip());
    }
}
