//! 思考强度（thinking intensity）
//!
//! 平台内部统一用 **0–100 分**表达「思考深度」，与下游模型的实际表达形式解耦：
//! - **Provider 基准值**（`ModelProviderConfig.thinking_intensity`，兼任能力开关：
//!   未配置则整条链路不下发该字段，保持存量行为）
//! - **Agent 覆盖值**（`AgentRuntimeConfig.thinking_intensity`，显式覆盖 provider 基准）
//! - **场景偏移**（`ThinkingScene::thinking_intensity_offset`，在基准值上做 ±N 分偏移，
//!   未配置的场景为 0）
//!
//! 最终值 = clamp(基准 + 偏移, 0, 100)。真正请求下游模型时，才按下游支持的形式做转换
//! （完整六档 `minimal`/`low`/`medium`/`high`/`xhigh`/`max`，见
//! [`ThinkingIntensity::to_reasoning_effort_wire`]）。`none` 不在强度档位内：
//! 平台的 `Option::None` 已经表达「不下发该字段」（走厂商默认），无需再发 `none`。
//! 未来厂商若支持分数，只需在此处扩展转换分支，上层配置与存储不变。

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// 思考强度（0–100 分，超出上限构造时饱和钳制到 100）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct ThinkingIntensity(u8);

impl ThinkingIntensity {
    /// 最低强度
    pub const MIN: Self = Self(0);
    /// 最高强度
    pub const MAX: Self = Self(100);
    /// 平台默认基准（中强度，落在六档下游的 medium 区间）
    pub const DEFAULT: Self = Self(50);

    /// 六档下游的区间上界（与 OpenAI `reasoning_effort` 规范对齐）：
    /// 0–16 minimal / 17–33 low / 34–50 medium / 51–66 high /
    /// 67–83 xhigh / 84–100 max
    const LEVELS_MINIMAL_MAX: u8 = 16;
    const LEVELS_LOW_MAX: u8 = 33;
    const LEVELS_MEDIUM_MAX: u8 = 50;
    const LEVELS_HIGH_MAX: u8 = 66;
    const LEVELS_XHIGH_MAX: u8 = 83;

    /// 由分数构造，超过 100 饱和钳制
    pub fn new(score: u8) -> Self {
        Self(score.min(Self::MAX.0))
    }

    /// 原始分数
    pub fn score(self) -> u8 {
        self.0
    }

    /// 在基准分数上做偏移（饱和钳制到 [0, 100]）
    ///
    /// 场景层只给偏移量（未配置 = 0），最终值 = clamp(基准 + 偏移)。
    pub fn offset(self, delta: i32) -> Self {
        Self((self.0 as i32 + delta).clamp(Self::MIN.0 as i32, Self::MAX.0 as i32) as u8)
    }

    /// 转换为下游完整六档支持（`reasoning_effort`）的 wire 字符串
    ///
    /// 与 OpenAI 规范逐字对齐：`minimal` / `low` / `medium` / `high` / `xhigh` / `max`。
    /// `none` 不在此列——平台用 `Option::None` 表达「不下发」。
    /// 未来下游支持分数时，在此增加分支（或按 provider 声明的能力选择转换），
    /// 上层 Agent/场景/存储无需改动。
    pub fn to_reasoning_effort_wire(self) -> &'static str {
        if self.0 <= Self::LEVELS_MINIMAL_MAX {
            "minimal"
        } else if self.0 <= Self::LEVELS_LOW_MAX {
            "low"
        } else if self.0 <= Self::LEVELS_MEDIUM_MAX {
            "medium"
        } else if self.0 <= Self::LEVELS_HIGH_MAX {
            "high"
        } else if self.0 <= Self::LEVELS_XHIGH_MAX {
            "xhigh"
        } else {
            "max"
        }
    }
}

impl<'de> Deserialize<'de> for ThinkingIntensity {
    /// 反序列化后统一钳制到 0–100，避免配置里写入越界分数
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(u8::deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::ThinkingIntensity;

    /// 分数按强度升序，默认基准落在 medium 区间。
    #[test]
    fn ordering_and_default_sit_in_expected_buckets() {
        assert!(ThinkingIntensity::MIN < ThinkingIntensity::DEFAULT);
        assert!(ThinkingIntensity::DEFAULT < ThinkingIntensity::MAX);
        assert_eq!(ThinkingIntensity::DEFAULT.score(), 50);
        assert_eq!(
            ThinkingIntensity::DEFAULT.to_reasoning_effort_wire(),
            "medium"
        );
    }

    /// 六档阈值边界：0–16 minimal / 17–33 low / 34–50 medium /
    /// 51–66 high / 67–83 xhigh / 84–100 max。
    #[test]
    fn levels6_thresholds_cover_all_buckets() {
        for (score, wire) in [
            (0u8, "minimal"),
            (16, "minimal"),
            (17, "low"),
            (33, "low"),
            (34, "medium"),
            (50, "medium"),
            (51, "high"),
            (66, "high"),
            (67, "xhigh"),
            (83, "xhigh"),
            (84, "max"),
            (100, "max"),
        ] {
            assert_eq!(
                ThinkingIntensity::new(score).to_reasoning_effort_wire(),
                wire,
                "score={score}"
            );
        }
    }

    /// 越界偏移饱和钳制到 [0, 100]，不得回绕。
    #[test]
    fn offset_saturates_at_both_ends() {
        assert_eq!(ThinkingIntensity::MIN.offset(-20), ThinkingIntensity::MIN);
        assert_eq!(
            ThinkingIntensity::MIN.offset(20),
            ThinkingIntensity::new(20)
        );
        assert_eq!(ThinkingIntensity::MAX.offset(20), ThinkingIntensity::MAX);
        assert_eq!(
            ThinkingIntensity::new(80).offset(-20),
            ThinkingIntensity::new(60)
        );
        assert_eq!(ThinkingIntensity::new(200), ThinkingIntensity::MAX);
    }

    /// 配置 JSON 直接存分数；反序列化越界值被钳制到 100。
    #[test]
    fn json_roundtrip_stores_score_and_clamps_out_of_range() {
        let value = ThinkingIntensity::new(42);
        assert_eq!(serde_json::to_string(&value).expect("serialize"), "42");
        let back: ThinkingIntensity = serde_json::from_str("42").expect("deserialize");
        assert_eq!(back, value);

        let clamped: ThinkingIntensity = serde_json::from_str("150").expect("deserialize");
        assert_eq!(clamped, ThinkingIntensity::MAX);
    }
}
