//! Agent 实体

use crate::models::brain::{Brain, Cortex, CortexTrait};
use crate::models::tool::Tool;
use crate::pkg::agent_runtime_state::AgentRuntimeInfo;
use crate::pkg::request_context::{EnrichContext, RequestContextBuilder};
use common::enums::AgentStatus;
use common::models::AgentStats;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;

/// Agent 运行时配置
///
/// 存储在 agents.runtime_config 字段（JSON 格式）
/// 方便后续扩展各类运行时参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRuntimeConfig {
    /// 最大思考深度（轮次），默认 10
    #[serde(default = "default_max_thinking_depth")]
    pub max_thinking_depth: i32,

    /// 思考间隔（毫秒），避免过快调用，默认 0（无间隔）
    #[serde(default)]
    pub thinking_interval_ms: i32,

    /// 单步最大工具调用次数，默认 5
    #[serde(default = "default_max_tool_calls_per_step")]
    pub max_tool_calls_per_step: i32,

    /// 是否启用反思模式
    #[serde(default)]
    pub enable_reflection: bool,

    /// 是否启用用户确认机制
    #[serde(default = "default_true")]
    pub require_user_confirm: bool,

    /// 已安装的工具包 tag 列表
    ///
    /// 记录 Agent 通过入职培训等方式安装的工具包。
    /// 唤醒时，这些 tag 对应的工具会自动注入到 Prompt 中（免绑定）。
    /// 典型值："project_management"、"data_analysis" 等
    #[serde(default)]
    pub installed_tags: Vec<String>,

    /// 已安装的技能包 tag 列表
    /// 记录 Agent 通过入职或手动安装的技能包。
    /// 安装时会将技能复制到 Agent 目录，卸载时仅移除 tag 关联（保留副本）。
    #[serde(default)]
    pub installed_skill_packs: Vec<String>,
}

impl Default for AgentRuntimeConfig {
    fn default() -> Self {
        Self {
            max_thinking_depth: default_max_thinking_depth(),
            thinking_interval_ms: 0,
            max_tool_calls_per_step: default_max_tool_calls_per_step(),
            enable_reflection: false,
            require_user_confirm: true,
            installed_tags: Vec::new(),
            installed_skill_packs: Vec::new(),
        }
    }
}

impl AgentRuntimeConfig {
    /// 从 JSON 字符串解析
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// 序列化为 JSON 字符串
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// 安装工具包 tag（幂等：已安装则跳过）
    pub fn install_tag(&mut self, tag: &str) {
        if !self.installed_tags.iter().any(|t| t == tag) {
            self.installed_tags.push(tag.to_string());
        }
    }

    /// 卸载工具包 tag
    pub fn uninstall_tag(&mut self, tag: &str) {
        self.installed_tags.retain(|t| t != tag);
    }

    /// 检查是否已安装某个 tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.installed_tags.iter().any(|t| t == tag)
    }

    /// 安装技能包 tag（幂等：已安装则跳过）
    pub fn install_skill_pack_tag(&mut self, tag: &str) {
        if !self.installed_skill_packs.iter().any(|t| t == tag) {
            self.installed_skill_packs.push(tag.to_string());
        }
    }

    /// 卸载技能包 tag
    pub fn uninstall_skill_pack_tag(&mut self, tag: &str) {
        self.installed_skill_packs.retain(|t| t != tag);
    }

    /// 检查是否已安装某个技能包 tag
    pub fn has_skill_pack_tag(&self, tag: &str) -> bool {
        self.installed_skill_packs.iter().any(|t| t == tag)
    }
}

// 辅助函数用于 serde default
fn default_max_thinking_depth() -> i32 {
    10
}

fn default_max_tool_calls_per_step() -> i32 {
    5
}

fn default_true() -> bool {
    true
}

/// Agent 业务对象（DAL 层）
///
/// 组合 AgentPo 和其他相关信息，作为业务层的核心对象
/// 后续可扩展：执行环境、权限、配置等字段
#[derive(Clone)]
pub struct Agent {
    /// 底层持久化对象
    pub po: AgentPo,
    /// 装配好的 Brain（推理执行实体）
    ///
    /// 如果为 None，表示还没有装配，需要调用 AgentDal::wake_brain 装配
    pub brain: Option<Brain>,
    /// 绑定的工具列表
    ///
    /// 每个工具包含元数据 + 可执行的 trait 对象
    pub tools: Vec<Tool>,
    /// 运行时状态信息（由 DAL 层从内存注入）
    ///
    /// None 表示未注入（如刚创建还未查询）
    pub runtime_info: Option<AgentRuntimeInfo>,
    /// 统计数据（由 DAL 层按需注入）
    ///
    /// None 表示未查询
    pub stats: Option<AgentStats>,
    // 后续扩展字段：
    // pub execution_env: ExecutionEnv,
    // pub permissions: Vec<Permission>,
    // pub config: AgentConfig,
}

impl fmt::Debug for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Agent")
            .field("po", &self.po)
            .field("brain", &"[Brain]")
            .field("tools", &format_args!("[{} tools]", self.tools.len()))
            .field("runtime_info", &self.runtime_info)
            .field("stats", &self.stats)
            .finish()
    }
}

impl Agent {
    /// 从 Po 创建 Agent
    pub fn from_po(po: AgentPo) -> Self {
        Self {
            po,
            brain: None,
            tools: Vec::new(),
            runtime_info: None,
            stats: None,
        }
    }

    /// 从 Po 创建 Agent 并带上工具列表
    pub fn from_po_with_tools(po: AgentPo, tools: Vec<Tool>) -> Self {
        Self {
            po,
            brain: None,
            tools,
            runtime_info: None,
            stats: None,
        }
    }

    /// 转换为 Po
    pub fn into_po(self) -> AgentPo {
        self.po
    }

    /// 获取 Agent ID
    pub fn id(&self) -> &str {
        self.po.id.as_str()
    }

    /// 获取 Agent 名称
    pub fn name(&self) -> &str {
        self.po.name.as_str()
    }

    /// 获取模型提供商 ID
    pub fn model_provider_id(&self) -> &str {
        self.po.model_provider_id.as_str()
    }

    /// 设置装配好的 Brain
    pub fn set_brain(&mut self, brain: Brain) {
        self.brain = Some(brain);
    }

    /// 获取 Brain 引用
    pub fn brain(&self) -> Option<&Brain> {
        self.brain.as_ref()
    }

    /// 获取 Brain 内部的 Cortex 引用
    pub fn cortex(&self) -> Option<&Cortex> {
        self.brain.as_ref().map(|b| b.cortex())
    }

    /// 获取 Brain 内部的 CortexTrait 引用
    pub fn cortex_trait(&self) -> Option<&(dyn CortexTrait + Send + Sync)> {
        self.brain.as_ref().map(|b| b.cortex_trait())
    }

    /// 生成 Agent 的 System Prompt 头部
    ///
    /// 委托给 AgentPo::to_system_prompt()
    pub fn to_system_prompt(&self) -> String {
        self.po.to_system_prompt()
    }

    /// 获取绑定的工具列表
    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// 设置绑定的工具列表
    pub fn set_tools(&mut self, tools: Vec<Tool>) {
        self.tools = tools;
    }
}

/// AgentPo 持久化对象
///
/// 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AgentPo {
    pub id: String,
    pub name: String,
    /// Agent 角色标签数组（JSON string）
    pub role: String,
    pub description: String,
    /// 详细能力描述数组（JSON string）
    pub capabilities: String,
    /// 长文本：角色/性格/灵魂设定
    pub soul: String,
    /// 关联模型提供商 ID
    pub model_provider_id: String,
    /// 运行时配置（JSON string，AgentRuntimeConfig）
    pub runtime_config: String,
    /// 生命周期状态
    pub status: AgentStatus,
    /// 创建者
    pub created_by: String,
    /// 修改者
    pub modified_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}
impl AgentPo {
    /// 生成 Agent 的 System Prompt 头部
    ///
    /// 包含：Agent ID、Agent 名称、角色描述、灵魂设定
    /// 所有字段都使用统一的【】标识格式，便于大模型识别和提取
    pub fn to_system_prompt(&self) -> String {
        let mut prompt = format!("【Agent ID】{}\n【Agent 名称】{}\n", self.id, self.name);

        if !self.description.is_empty() {
            prompt.push_str(&format!("【角色描述】{}\n", self.description));
        }

        if !self.soul.is_empty() {
            prompt.push_str(&format!("\n【灵魂设定】\n{}\n", self.soul));
        }

        prompt
    }

    pub fn new(
        name: String,
        roles: Vec<String>,
        description: String,
        capabilities: Vec<String>,
        soul: String,
        model_provider_id: String,
        creator: String,
    ) -> Self {
        let runtime_config = AgentRuntimeConfig::default();
        Self {
            id: generate_id(),
            name,
            role: serde_json::to_string(&roles).unwrap_or_else(|_| "[]".to_string()),
            description,
            capabilities: serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string()),
            soul,
            model_provider_id: model_provider_id,
            runtime_config: runtime_config.to_json(),
            status: AgentStatus::Interviewing,
            created_by: creator.clone(),
            modified_by: creator,
            created_at: current_timestamp(),
            updated_at: current_timestamp(),
        }
    }

    /// 获取角色标签列表
    pub fn get_roles(&self) -> Vec<String> {
        if self.role.is_empty() {
            return Vec::new();
        }
        serde_json::from_str(&self.role).unwrap_or_default()
    }

    pub fn get_capabilities(&self) -> Vec<String> {
        serde_json::from_str(&self.capabilities).unwrap_or_default()
    }

    /// 获取运行时配置
    pub fn get_runtime_config(&self) -> AgentRuntimeConfig {
        AgentRuntimeConfig::from_json(&self.runtime_config)
    }

    /// 设置运行时配置
    pub fn set_runtime_config(&mut self, config: &AgentRuntimeConfig) {
        self.runtime_config = config.to_json();
        self.updated_at = current_timestamp();
    }

    /// 获取已安装的工具包 tags
    pub fn get_installed_tags(&self) -> Vec<String> {
        self.get_runtime_config().installed_tags
    }

    /// 安装工具包 tag 并更新 runtime_config
    pub fn install_tag(&mut self, tag: &str) {
        let mut config = self.get_runtime_config();
        config.install_tag(tag);
        self.set_runtime_config(&config);
    }

    /// 卸载工具包 tag 并更新 runtime_config
    pub fn uninstall_tag(&mut self, tag: &str) {
        let mut config = self.get_runtime_config();
        config.uninstall_tag(tag);
        self.set_runtime_config(&config);
    }

    /// 获取已安装的技能包 tags
    pub fn get_installed_skill_packs(&self) -> Vec<String> {
        self.get_runtime_config().installed_skill_packs
    }

    /// 安装技能包 tag 并更新 runtime_config
    pub fn install_skill_pack_tag(&mut self, tag: &str) {
        let mut config = self.get_runtime_config();
        config.install_skill_pack_tag(tag);
        self.set_runtime_config(&config);
    }

    /// 卸载技能包 tag 并更新 runtime_config
    pub fn uninstall_skill_pack_tag(&mut self, tag: &str) {
        let mut config = self.get_runtime_config();
        config.uninstall_skill_pack_tag(tag);
        self.set_runtime_config(&config);
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

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

impl crate::pkg::request_context::EnrichContext for AgentPo {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        builder
            .agent_id(self.id.clone())
            .model_provider_id(self.model_provider_id.clone())
    }
}

impl EnrichContext for Agent {
    fn enrich(&self, builder: RequestContextBuilder) -> RequestContextBuilder {
        self.po.enrich(builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_skill_pack_tag() {
        let mut config = AgentRuntimeConfig::default();
        assert!(config.installed_skill_packs.is_empty());

        config.install_skill_pack_tag("coding");
        assert_eq!(config.installed_skill_packs, vec!["coding".to_string()]);
        assert!(config.has_skill_pack_tag("coding"));
    }

    #[test]
    fn test_install_skill_pack_tag_idempotent() {
        let mut config = AgentRuntimeConfig::default();

        config.install_skill_pack_tag("coding");
        config.install_skill_pack_tag("coding");
        config.install_skill_pack_tag("coding");

        assert_eq!(config.installed_skill_packs.len(), 1);
        assert_eq!(config.installed_skill_packs, vec!["coding".to_string()]);
    }

    #[test]
    fn test_uninstall_skill_pack_tag() {
        let mut config = AgentRuntimeConfig::default();
        config.install_skill_pack_tag("coding");
        config.install_skill_pack_tag("writing");
        assert_eq!(config.installed_skill_packs.len(), 2);

        config.uninstall_skill_pack_tag("coding");
        assert_eq!(config.installed_skill_packs, vec!["writing".to_string()]);
        assert!(!config.has_skill_pack_tag("coding"));
        assert!(config.has_skill_pack_tag("writing"));

        // 卸载不存在的 tag 不报错
        config.uninstall_skill_pack_tag("not_exists");
        assert_eq!(config.installed_skill_packs, vec!["writing".to_string()]);
    }

    #[test]
    fn test_has_skill_pack_tag() {
        let mut config = AgentRuntimeConfig::default();

        assert!(!config.has_skill_pack_tag("coding"));

        config.install_skill_pack_tag("coding");
        assert!(config.has_skill_pack_tag("coding"));
        assert!(!config.has_skill_pack_tag("writing"));

        config.uninstall_skill_pack_tag("coding");
        assert!(!config.has_skill_pack_tag("coding"));
    }
}
