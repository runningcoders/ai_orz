//! Agent 实体

use crate::models::brain::Brain;
use crate::models::skill::Skill;
use crate::models::tool::Tool;
use crate::models::vector::{SearchMatchInfo, Vectorizable};
use crate::pkg::agent_runtime_state::AgentRuntimeInfo;
use crate::pkg::request_context::{EnrichContext, RequestContextBuilder};
use common::enums::AgentStatus;
use common::models::{AgentStats, ModelCallStats};
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

    /// 外部 Agent 执行配置（仅 Cli/Remote kind 时使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_config: Option<ExternalAgentConfig>,
}

/// 外部 Agent 执行配置
///
/// 不同类型的外部 Agent（CLI 子进程、A2A 远程等）各自的执行参数。
/// 通过 `#[serde(tag = "executor")]` 做内部标签，便于 JSON 序列化区分。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "executor", rename_all = "snake_case")]
pub enum ExternalAgentConfig {
    /// CLI 子进程执行器配置（如 Codex / Claude Code / Aider）
    Cli {
        /// 启动命令（如 "codex"、"claude"、"aider"）
        command: String,
        /// 命令参数
        args: Vec<String>,
        /// 工作目录（绝对路径）
        work_dir: String,
        /// 环境变量（key, value 列表）
        env: Vec<(String, String)>,
        /// 超时时间（秒）
        timeout_secs: u64,
        /// 自定义 prompt 模板（None 用默认模板）
        /// 使用 {prompt} 占位符标记 prompt 位置
        prompt_template: Option<String>,
    },
    /// A2A 远程执行器配置
    Remote {
        /// A2A Server 的 base URL
        endpoint: String,
        /// 目标 Agent 名称（agents/sendTask 的 agent_id 参数）
        agent_name: String,
        /// 认证 token（Bearer）
        auth_token: Option<String>,
        /// 超时时间（秒）
        timeout_secs: u64,
    },
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
            external_config: None,
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
    /// Agent 已安装的技能副本列表（业务实体，由 hr_domain 加载，供 wake/awaken 使用）
    pub skills: Vec<Skill>,
    /// 运行时状态信息（由 DAL 层从内存注入）
    ///
    /// None 表示未注入（如刚创建还未查询）
    pub runtime_info: Option<AgentRuntimeInfo>,
    /// 统计数据（由 DAL 层按需注入）
    ///
    /// None 表示未查询
    pub stats: Option<AgentStats>,
    /// 模型调用统计数据（由 DAL 层按需注入）
    ///
    /// None 表示未查询
    pub model_call_stats: Option<ModelCallStats>,
    /// 搜索匹配元信息（搜索场景下由 DAL 层填充）
    pub search_match: Option<SearchMatchInfo>,
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
            .field("skills", &format_args!("[{} skills]", self.skills.len()))
            .field("runtime_info", &self.runtime_info)
            .field("stats", &self.stats)
            .field("model_call_stats", &self.model_call_stats)
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
            skills: Vec::new(),
            runtime_info: None,
            stats: None,
            model_call_stats: None,
            search_match: None,
        }
    }

    /// 从 Po 创建 Agent 并带上工具列表
    pub fn from_po_with_tools(po: AgentPo, tools: Vec<Tool>) -> Self {
        Self {
            po,
            brain: None,
            tools,
            skills: Vec::new(),
            runtime_info: None,
            stats: None,
            model_call_stats: None,
            search_match: None,
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

    /// 获取 Brain 内部的 ModelProvider 配置引用（仅 Local agent 有值）
    pub fn model_provider(&self) -> Option<&crate::models::model_provider::ModelProviderPo> {
        self.brain.as_ref().and_then(|b| b.model_provider())
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

    /// 获取已安装的技能副本列表
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// 设置已安装的技能副本列表
    pub fn set_skills(&mut self, skills: Vec<Skill>) {
        self.skills = skills;
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
    /// Agent 类型（Local/Cli/Remote）
    pub kind: common::enums::AgentKind,
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
            model_provider_id,
            runtime_config: runtime_config.to_json(),
            status: AgentStatus::Interviewing,
            kind: common::enums::AgentKind::Local,
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

    /// 是否为本地 Agent
    pub fn is_local(&self) -> bool {
        self.kind.is_local()
    }

    /// 是否为 CLI Agent
    pub fn is_cli(&self) -> bool {
        self.kind.is_cli()
    }

    /// 是否为远程 Agent
    pub fn is_remote(&self) -> bool {
        self.kind.is_remote()
    }

    /// 是否为外部 Agent（CLI 或远程）
    pub fn is_external(&self) -> bool {
        self.kind.is_external()
    }

    /// 获取外部 Agent 配置（如果是外部 Agent 且配置存在）
    pub fn get_external_config(&self) -> Option<ExternalAgentConfig> {
        if !self.kind.is_external() {
            return None;
        }
        self.get_runtime_config().external_config
    }

    /// 设置外部 Agent 配置
    pub fn set_external_config(&mut self, config: ExternalAgentConfig) {
        let mut runtime_config = self.get_runtime_config();
        runtime_config.external_config = Some(config);
        self.set_runtime_config(&runtime_config);
    }

    /// 获取 CLI 配置（仅当 Agent 是 CLI 类型且配置正确时返回拥有所有权的值）
    pub fn get_cli_config(&self) -> Option<CliAgentConfig> {
        let config = self.get_runtime_config();
        match config.external_config? {
            ExternalAgentConfig::Cli {
                command,
                args,
                work_dir,
                env,
                timeout_secs,
                prompt_template,
            } => {
                if !self.kind.is_cli() {
                    return None;
                }
                Some(CliAgentConfig {
                    command,
                    args,
                    work_dir,
                    env,
                    timeout_secs,
                    prompt_template,
                })
            }
            _ => None,
        }
    }

    /// 获取 A2A 配置（仅当 Agent 是 Remote 类型且配置正确时返回拥有所有权的值）
    pub fn get_remote_config(&self) -> Option<RemoteAgentConfig> {
        let config = self.get_runtime_config();
        match config.external_config? {
            ExternalAgentConfig::Remote {
                endpoint,
                agent_name,
                auth_token,
                timeout_secs,
            } => {
                if !self.kind.is_remote() {
                    return None;
                }
                Some(RemoteAgentConfig {
                    endpoint,
                    agent_name,
                    auth_token,
                    timeout_secs,
                })
            }
            _ => None,
        }
    }
}

/// CLI Agent 配置（拥有所有权，从 AgentPo 提取）
#[derive(Debug, Clone)]
pub struct CliAgentConfig {
    pub command: String,
    pub args: Vec<String>,
    pub work_dir: String,
    pub env: Vec<(String, String)>,
    pub timeout_secs: u64,
    pub prompt_template: Option<String>,
}

/// 远程 A2A Agent 配置（拥有所有权，从 AgentPo 提取）
#[derive(Debug, Clone)]
pub struct RemoteAgentConfig {
    pub endpoint: String,
    pub agent_name: String,
    pub auth_token: Option<String>,
    pub timeout_secs: u64,
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

// ==================== Vectorizable 实现 ====================

impl Vectorizable for AgentPo {
    fn vectorize_text(&self) -> String {
        // AgentPo 向量化文本：name + role + description + capabilities
        // 注意：role 和 capabilities 是 JSON 字符串数组（如 ["worker"]），直接拼接原始字符串即可
        // trigram 分词器会自动处理子串匹配
        // soul 字段不参与向量化（灵魂设定不适合搜索）
        let parts: Vec<&str> = vec![
            &self.name,
            &self.role,
            &self.description,
            &self.capabilities,
        ];
        // 过滤掉空字符串，用换行符拼接
        parts
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn vector_collection() -> &'static str {
        "agents"
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
