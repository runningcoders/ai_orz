//! 模型提供商实体

use crate::pkg::request_context::{EnrichContext, RequestContextBuilder};
use common::enums::{
    ChatApiFlavor, ModelAccessMode, ModelCapability, ModelProviderStatus, ProviderType,
    ThinkingIntensity,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;

/// 模型提供商配置（JSON 存储）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelProviderConfig {
    /// 上下文窗口长度（模型支持的最大 token 数）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_length: Option<i32>,
    /// 推荐上下文长度（建议的工作上下文上限，优先作为压缩触发阈值）
    ///
    /// 未设置时按 `max_context_length * 60%` 自动计算。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_context_length: Option<i32>,
    /// 最大输出 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    /// 默认温度值 (0.0 - 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_default: Option<f32>,
    /// 每分钟请求数限制
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm_limit: Option<i32>,
    /// 每分钟 token 数限制
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm_limit: Option<i32>,
    /// 每日配额（token 或 请求数）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily_quota: Option<i64>,
    /// 下行调用访问模式（方案②：stream=流式 SSE / non_stream=非流式一次性响应）
    ///
    /// None=未配置（生效时等价 Stream，保持存量行为）；存量 config JSON 缺字段
    /// 自动兼容，无 migration。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_mode: Option<ModelAccessMode>,
    /// 请求超时（毫秒）。目前仅 Jev/System One 小脑 client 消费：
    /// 缺省时走 client 内建默认超时兜底（一期 dao/cerebellum 常量）。
    /// None=未配置；存量 config JSON 缺字段自动兼容，无 migration。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// 是否支持图像输入（vision 能力位，批5：UI 开关收编）
    ///
    /// 语义：chat 推理端点能否接收图像（OpenAI vision content parts）。
    /// None=false（保守降级，保持批4 存量行为）；存量 config JSON 缺字段
    /// 自动兼容，零迁移。**与 provider_type 相互独立**（含 DoubaoVision——
    /// 批4 已裁定其语义=多模态 embedding 端点≠chat vision 能力位，UI 开关
    /// 不以其作判定源）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_vision: Option<bool>,
    /// Chat 协议口味（chat_completions=传统 /chat/completions；responses=Responses API）
    ///
    /// **与 provider_type 相互独立**（同 supports_vision 的裁定）：provider_type
    /// 回答「哪家厂商」，api_flavor 回答「哪种 wire 协议」。同一厂商可能两种协议
    /// 都提供（OpenAI 自家即如此），故协议不可编码进 ProviderType 枚举。
    ///
    /// None=未配置（生效时等价 ChatCompletions，保持存量行为）；存量 config JSON
    /// 缺字段自动兼容，无 migration。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_flavor: Option<ChatApiFlavor>,
    /// 默认思考强度（0–100 分，**兼任能力开关**）
    ///
    /// `None` = 未配置：请求体**完全不下发** reasoning_effort / reasoning 字段，
    /// 保持存量行为；此时 Agent 覆盖值与场景偏移均不生效（能力开关优先）。
    /// 非 None 时作为基准分数，再叠加 Agent 覆盖与场景偏移，请求下游前按
    /// 下游支持的形式转换（完整六档 minimal/low/medium/high/xhigh/max）。
    ///
    /// 只应对支持该字段的端点配置；不支持的网关保持 None，否则会被拒收。
    ///
    /// 存量 config JSON 缺字段自动兼容，无 migration。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_intensity: Option<ThinkingIntensity>,
}

impl ModelProviderConfig {
    /// 解析下行调用访问模式（方案 §2.3 口径）：字段缺省 → Stream（保持存量行为）
    pub fn access_mode_or_default(&self) -> ModelAccessMode {
        self.access_mode.unwrap_or(ModelAccessMode::Stream)
    }

    /// 解析 vision 能力位（批5 口径）：字段缺省 → false（保守降级，保持存量行为）
    pub fn supports_vision_or_default(&self) -> bool {
        self.supports_vision.unwrap_or(false)
    }

    /// 解析 chat 协议口味：字段缺省 → Responses（平台主线协议）
    ///
    /// 未提供 `/responses` 端点的自建/老旧网关由用户显式配 `"chat_completions"`
    /// 回退；**不做按 provider_type 的特判**（厂商 × 协议是正交两维）。
    pub fn api_flavor_or_default(&self) -> ChatApiFlavor {
        self.api_flavor.unwrap_or(ChatApiFlavor::Responses)
    }
}

/// 模型提供商持久化对象
///
/// 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModelProviderPo {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub model_name: String,
    pub capability: ModelCapability,
    pub api_key: String,
    pub base_url: Option<String>,
    pub description: Option<String>,
    pub config: String, // JSON 存储配置
    pub status: ModelProviderStatus,
    pub created_by: String,
    pub modified_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 模型提供商业务对象
///
/// 只包含持久化配置，不包含 Cortex，Cortex 由 Brain 持有
#[derive(Clone)]
pub struct ModelProvider {
    pub po: ModelProviderPo,
    /// ✅ 模型调用统计（可选）
    /// - 普通查询返回：None
    /// - with_model_call_stats=true 时返回：Some(ModelCallStats)
    pub stats: Option<common::models::ModelCallStats>,
}

impl fmt::Debug for ModelProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelProvider")
            .field("po", &self.po)
            .field("stats", &self.stats)
            .finish()
    }
}

impl ModelProvider {
    /// 创建新的 ModelProvider
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        provider_type: ProviderType,
        capability: ModelCapability,
        model_name: String,
        api_key: String,
        base_url: Option<String>,
        description: Option<String>,
        creator: String,
    ) -> Self {
        Self {
            po: ModelProviderPo::new(
                name,
                provider_type,
                capability,
                model_name,
                api_key,
                base_url,
                description,
                creator,
            ),
            stats: None,
        }
    }

    /// 从 PO 创建业务对象
    pub fn from_po(po: ModelProviderPo) -> Self {
        Self { po, stats: None }
    }

    /// 获取 ID
    pub fn id(&self) -> &str {
        self.po.id.as_str()
    }

    /// 获取名称
    pub fn name(&self) -> &str {
        self.po.name.as_str()
    }

    /// 获取模型名称
    pub fn model_name(&self) -> &str {
        self.po.model_name.as_str()
    }

    /// 获取模型能力类型
    pub fn capability(&self) -> ModelCapability {
        self.po.capability
    }

    /// 获取 API Key
    pub fn api_key(&self) -> &str {
        self.po.api_key.as_str()
    }

    /// 更新时间戳
    pub fn touch(&mut self, modifier: &str) {
        self.po.modified_by = modifier.to_string();
        self.po.updated_at = common::constants::utils::current_timestamp_ms();
    }
}

impl ModelProviderPo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        provider_type: ProviderType,
        capability: ModelCapability,
        model_name: String,
        api_key: String,
        base_url: Option<String>,
        description: Option<String>,
        creator: String,
    ) -> Self {
        Self {
            id: generate_id(),
            name,
            provider_type,
            model_name,
            capability,
            api_key,
            base_url: base_url
                .map(|s| if s.is_empty() { None } else { Some(s) })
                .unwrap_or_default(),
            description: description
                .map(|s| if s.is_empty() { None } else { Some(s) })
                .unwrap_or_default(),
            config: "{}".to_string(),
            status: ModelProviderStatus::Normal,
            created_by: creator.clone(),
            modified_by: creator,
            created_at: common::constants::utils::current_timestamp_ms(),
            updated_at: common::constants::utils::current_timestamp_ms(),
        }
    }

    /// 解析配置为 ModelProviderConfig 对象
    pub fn config(&self) -> ModelProviderConfig {
        serde_json::from_str(&self.config).unwrap_or_default()
    }

    /// 更新配置（完整替换）
    pub fn set_config(&mut self, config: &ModelProviderConfig) {
        self.config = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
    }

    /// 更新部分配置字段
    pub fn update_config<F>(&mut self, f: F)
    where
        F: FnOnce(&mut ModelProviderConfig),
    {
        let mut config = self.config();
        f(&mut config);
        self.set_config(&config);
    }
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let random = rand_u32();
    format!("{:016x}{:08x}", timestamp, random)
}

fn rand_u32() -> u32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    SystemTime::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let time2 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u32;
    time2.wrapping_add(hasher.finish() as u32)
}

impl crate::pkg::request_context::EnrichContext for ModelProviderPo {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        builder
            .model_provider_id(self.id.clone())
            .model_name(self.model_name.clone())
    }
}

impl EnrichContext for ModelProvider {
    fn enrich(&self, builder: RequestContextBuilder) -> RequestContextBuilder {
        self.po.enrich(builder)
    }
}

#[cfg(test)]
mod access_mode_config_tests {
    use super::*;

    /// 存量 config JSON（无 access_mode 字段）必须零变化兼容（方案②：无 migration）
    #[test]
    fn legacy_config_without_access_mode_deserializes_to_default_stream() {
        let cfg: ModelProviderConfig =
            serde_json::from_str(r#"{"max_context_length":128000}"#).expect("legacy config");
        assert_eq!(cfg.max_context_length, Some(128_000));
        assert_eq!(cfg.access_mode, None);
        assert_eq!(cfg.access_mode_or_default(), ModelAccessMode::Stream);
    }

    /// access_mode 仅在已设置时序列化；取值为 serde snake_case 契约值
    #[test]
    fn access_mode_roundtrips_with_snake_case_value() {
        let empty = ModelProviderConfig::default();
        assert!(
            !serde_json::to_string(&empty)
                .unwrap()
                .contains("access_mode"),
            "未设置时不得序列化 access_mode 字段"
        );

        let cfg = ModelProviderConfig {
            access_mode: Some(ModelAccessMode::NonStream),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("non_stream"), "json: {json}");
        let back: ModelProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_mode, Some(ModelAccessMode::NonStream));
    }

    /// 批5：存量 config JSON（无 supports_vision 字段）反序列化 → None，语义等价 false
    #[test]
    fn legacy_config_without_supports_vision_defaults_to_false() {
        let cfg: ModelProviderConfig =
            serde_json::from_str(r#"{"max_context_length":128000}"#).expect("legacy config");
        assert_eq!(cfg.supports_vision, None);
        assert!(
            !cfg.supports_vision_or_default(),
            "缺省 None 语义必须等价 false（保守降级）"
        );
    }

    /// 批5：supports_vision 仅在已设置时序列化；true/false roundtrip 保形
    #[test]
    fn supports_vision_roundtrips_and_none_is_not_serialized() {
        let empty = ModelProviderConfig::default();
        assert!(
            !serde_json::to_string(&empty)
                .unwrap()
                .contains("supports_vision"),
            "未设置时不得序列化 supports_vision 字段"
        );

        for value in [true, false] {
            let cfg = ModelProviderConfig {
                supports_vision: Some(value),
                ..Default::default()
            };
            let json = serde_json::to_string(&cfg).unwrap();
            let back: ModelProviderConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(back.supports_vision, Some(value));
            assert_eq!(back.supports_vision_or_default(), value);
        }
    }

    /// 存量 config JSON（无 api_flavor 字段）→ None，语义等价 Responses（主线协议）。
    ///
    /// 与上面 `supports_vision` 的「缺省=保守降级 false」相反：协议缺省取**能力更强**
    /// 的一侧，因为 Responses 是主线（MCP / 知识库 / 联网搜索 / 缓存仅此协议提供），
    /// 且主流实现均已覆盖。存量记录无需 migration，缺省即切换。
    #[test]
    fn legacy_config_without_api_flavor_defaults_to_responses() {
        let cfg: ModelProviderConfig =
            serde_json::from_str(r#"{"max_context_length":128000}"#).expect("legacy config");
        assert_eq!(cfg.api_flavor, None);
        assert_eq!(cfg.api_flavor_or_default(), ChatApiFlavor::Responses);
    }

    /// 显式 `chat_completions` 必须能覆盖缺省（自建/老旧网关的回退路径可用），
    /// 且未设置时不得序列化（存量 JSON 零污染）
    #[test]
    fn explicit_chat_completions_overrides_default() {
        let empty = ModelProviderConfig::default();
        assert!(
            !serde_json::to_string(&empty)
                .unwrap()
                .contains("api_flavor"),
            "未设置时不得序列化 api_flavor 字段"
        );

        let cfg = ModelProviderConfig {
            api_flavor: Some(ChatApiFlavor::ChatCompletions),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("chat_completions"), "json: {json}");
        let back: ModelProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.api_flavor, Some(ChatApiFlavor::ChatCompletions));
        assert_eq!(back.api_flavor_or_default(), ChatApiFlavor::ChatCompletions);
    }

    /// 存量 config JSON（无 thinking_intensity 字段）→ None = 能力开关关闭（保持现状不下发）
    #[test]
    fn legacy_config_without_thinking_intensity_stays_disabled() {
        let cfg: ModelProviderConfig =
            serde_json::from_str(r#"{"max_context_length":128000}"#).expect("legacy config");
        assert_eq!(cfg.thinking_intensity, None);
    }

    /// thinking_intensity 仅在已设置时序列化，且 roundtrip 按分数保形
    #[test]
    fn thinking_intensity_roundtrips_and_none_is_not_serialized() {
        let empty = ModelProviderConfig::default();
        assert!(
            !serde_json::to_string(&empty)
                .unwrap()
                .contains("thinking_intensity"),
            "未设置时不得序列化 thinking_intensity 字段"
        );

        let cfg = ModelProviderConfig {
            thinking_intensity: Some(ThinkingIntensity::MAX),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"thinking_intensity\":100"), "json: {json}");
        let back: ModelProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.thinking_intensity, Some(ThinkingIntensity::MAX));
    }
}
