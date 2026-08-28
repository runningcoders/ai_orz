//! Tool 持久化对象和完整实体

use crate::pkg::request_context::RequestContext;
use async_trait::async_trait;
use common::enums::tool::ControlMode;
use common::enums::{ToolProtocol, ToolStatus};
use common::error::{Result, err};
use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// Re-export
pub use common::models::ToolCallTraceRef;
#[async_trait]
pub trait CoreTool: Send + Sync + DynClone {
    /// 执行工具调用
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value>;

    /// 获取工具对应的数据库持久化对象
    fn po(&self) -> &ToolPo;

    /// 工具凭据需求声明（共享工具从实例 config 读；内置工具静态声明；默认空）
    ///
    /// 编排层（domain resolve_tool_credentials）据此取数加工注入值（D17）。
    fn credential_requirements(&self) -> Vec<common::models::CredentialRequirement> {
        Vec::new()
    }

    /// 凭据注入生命周期：编排层在 call 前调用--校验声明与注入匹配 + 存实例字段。
    /// 实例单次使用（create -> check -> call），凭据是对象状态（D22 红线）。
    fn check(&mut self, resolved: &[crate::pkg::credential::ResolvedRequirement]) -> Result<()> {
        // 默认无凭据工具：注入非空即为编排层错配（防御）
        if resolved.is_empty() {
            Ok(())
        } else {
            Err(common::error::Error::new(
                common::error::ErrorCode::InvalidRequest,
                "工具未声明凭据需求，但编排层传入了注入值",
            ))
        }
    }

    /// 获取原始的 inner 工具，如果已经被装饰过的话
    /// 默认实现返回自身，覆盖这个方法用于装饰器取出原始工具
    fn as_original(&self) -> &(dyn CoreTool + Send + Sync)
    where
        Self: Sized,
    {
        self
    }
}

dyn_clone::clone_trait_object!(CoreTool);

/// Runtime 工具执行成功结果.
///
/// 工具调用成功时必须同时返回业务结果与 tool-specific call trace 引用,
/// 避免上层把 request_id 误当成真实工具调用 call_id.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolExecutionResult {
    /// 工具业务返回值.
    pub result: Value,
    /// 指向 tool-specific call_trace 的轻量引用.
    pub trace_ref: ToolCallTraceRef,
}

impl ToolExecutionResult {
    pub fn new(result: Value, tool_id: String, call_id: String) -> Self {
        Self {
            result,
            trace_ref: ToolCallTraceRef { tool_id, call_id },
        }
    }
}

/// domain → DAL 统一工具执行传参（D26）。
///
/// `tool` 为 PO 载体：可执行实例由 DAL per-call 重组装（D22 单次实例，
/// 不复用调用方预装配实例）；`resolved` 为编排层（domain
/// `resolve_tool_credentials`）加工完成的凭据注入值，DAL 组装实例后经
/// `CoreTool::check` 注入实例字段。
///
/// 命名与 `ToolExecutionResult` 成对；内部调用结构非 HTTP DTO，不进 common。
pub struct ToolExecutionRequest {
    /// 工具 PO 载体（协议路由与重组装依据）
    pub tool: ToolPo,
    /// 调用参数
    pub args: Value,
    /// 凭据注入值（无凭据需求的工具为空集）
    pub resolved: Vec<crate::pkg::credential::ResolvedRequirement>,
}

/// Tool 持久化对象
///
/// 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default)]
pub struct ToolPo {
    /// 工具 ID
    pub id: String,
    /// 工具名称（唯一）
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 工具协议类型
    pub protocol: ToolProtocol,
    /// 控制模式：auto (rig原生) / manual (自建链路)
    pub control_mode: ControlMode,
    /// 协议配置（JSON）
    pub config: serde_json::Value,
    /// 参数 JSON Schema（动态工具必填，内置工具可选）
    pub parameters_schema: Option<serde_json::Value>,
    /// 标签列表（JSON string）：用于能力匹配和筛选
    pub tags: String,
    /// 工具状态
    pub status: ToolStatus,
    /// 创建时间
    pub created_at: i64,
    /// 更新时间
    pub updated_at: i64,
    /// 创建者
    pub created_by: Option<String>,
    /// 更新者
    pub updated_by: Option<String>,
}

/// Tool - complete tool entity with PO and boxed trait object
///
/// Contains persistent metadata + actual executable tool object
pub struct Tool {
    /// Persistent metadata from DB
    pub po: ToolPo,
    /// Our core interface tool
    pub our_tool: Box<dyn CoreTool + Send + Sync>,
    /// ✅ 搜索匹配元信息（可选）
    /// - 普通查询返回：None
    /// - 搜索返回：Some(包含匹配类型、距离、命中等元信息)
    pub search_match: Option<crate::models::vector::SearchMatchInfo>,
    /// ✅ 统计数据（可选）
    /// - 普通查询返回：None
    /// - with_stats=true 时返回：Some(ToolStats)
    pub stats: Option<common::models::ToolStats>,
}

// Manual Debug implementation - skip the dyn fields
impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("po", &self.po)
            .field("our_tool", &format_args!("Box<dyn CoreTool + Send + Sync>"))
            .finish()
    }
}

// Manual Clone implementation - Agent derives Clone, but dyn Trait can't be cloned
// In practice, Agent is wrapped in Arc when shared, so this unreachable is safe
impl Clone for Tool {
    fn clone(&self) -> Self {
        unreachable!("Tool cannot be cloned due to dyn Trait object. Use Arc<Tool> for sharing.");
    }
}

/// 管理面占位工具。
///
/// HTTP/MCP 动态工具的运行时执行链路尚未接入时，管理面仍需要完整 Tool 实体
/// 承载元数据 CRUD/status 操作；真正执行时应由 ToolCallDao 组装可执行工具。
#[derive(Clone)]
struct ManagementOnlyTool {
    po: ToolPo,
}

#[async_trait]
impl CoreTool for ManagementOnlyTool {
    async fn call(&self, _ctx: RequestContext, _args: Value) -> Result<Value> {
        Err(err!(
            ToolExecutionFailed,
            "Tool {} is not executable in management context",
            self.po.id
        ))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

impl Tool {
    /// 从 Po 创建管理面 Tool 实体。
    ///
    /// 该构造仅用于配置管理，不代表工具运行时可执行。
    pub fn from_po_for_management(po: ToolPo) -> Self {
        Self {
            our_tool: Box::new(ManagementOnlyTool { po: po.clone() }),
            po,
            search_match: None,
            stats: None,
        }
    }

    /// 当前状态下允许通过状态更新 Action 切换到的目标状态。
    pub fn available_statuses(&self) -> Vec<ToolStatus> {
        match self.po.status {
            ToolStatus::Enabled => vec![ToolStatus::Enabled, ToolStatus::Disabled],
            ToolStatus::Disabled => vec![ToolStatus::Disabled, ToolStatus::Enabled],
            // Stale is sync-owned: only MCP sync can restore it when the remote tool reappears.
            // Management status updates must not manually move it back into normal business paths.
            ToolStatus::Stale => vec![ToolStatus::Stale],
        }
    }

    /// 判断是否允许通过状态更新 Action 切换到目标状态。
    pub fn can_transition_to(&self, target: ToolStatus) -> bool {
        self.available_statuses().contains(&target)
    }

    /// 切换工具状态。
    ///
    /// 只处理依赖自身字段即可判断的简单状态迁移；如果未来规则涉及权限、Agent
    /// 绑定、套餐或外部依赖，应上移到 Finance Domain 编排。
    pub fn transition_status(
        &mut self,
        target: ToolStatus,
        modified_by: impl Into<String>,
    ) -> Result<()> {
        if !self.can_transition_to(target) {
            return Err(err!(
                InvalidRequest,
                "Tool {} cannot transition from {:?} to {:?}",
                self.po.id,
                self.po.status,
                target
            ));
        }

        self.po.status = target;
        self.po.touch(Some(modified_by.into()));
        Ok(())
    }
}

impl ToolPo {
    /// 创建新 ToolPo（如果 id 为空自动生成 Uuid v7）
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        description: String,
        protocol: ToolProtocol,
        config: serde_json::Value,
        parameters_schema: Option<serde_json::Value>,
        tags: Vec<String>,
        creator: Option<String>,
    ) -> Self {
        let id = if id.is_empty() {
            Uuid::now_v7().to_string()
        } else {
            id
        };
        let now = common::constants::utils::current_timestamp_ms();
        let control_mode = match protocol {
            ToolProtocol::Http | ToolProtocol::Mcp => ControlMode::Manual,
            _ => ControlMode::Auto,
        };
        Self {
            id,
            name,
            description,
            protocol,
            control_mode,
            config,
            parameters_schema,
            tags: serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string()),
            status: ToolStatus::Enabled,
            created_at: now,
            updated_at: now,
            created_by: creator.clone(),
            updated_by: creator,
        }
    }

    /// 创建 built-in 工具的默认 ToolPo
    /// id == name for built-in tools since they are constants
    pub fn new_builtin(id: String, name: String, description: String) -> Self {
        Self::new(
            id,
            name,
            description,
            ToolProtocol::Builtin,
            serde_json::Value::Null, // No extra config for built-in tools
            None,                    // Parameters can be extracted from trait at runtime if needed
            Vec::new(),              // Empty tags by default
            None,                    // System built-in, no specific creator
        )
    }

    /// 获取标签列表
    pub fn get_tags(&self) -> Vec<String> {
        if self.tags.is_empty() {
            return Vec::new();
        }
        serde_json::from_str(&self.tags).unwrap_or_default()
    }

    /// 更新时间戳和修改者
    pub fn touch(&mut self, modifier: Option<String>) {
        self.updated_at = common::constants::utils::current_timestamp_ms();
        self.updated_by = modifier;
    }

    /// CLI 型工具判定 + 命令读取单点（D28「CLI 型 = po.config.command」不变式）
    ///
    /// 仅 CLI 包装类内置工具（browser/gh_cli/lark_cli）的 config 含 `command`；
    /// 存量 DB PO config 无该字段 → `None`（调用方按需以工厂默认兜底，零迁移）。
    pub fn cli_command(&self) -> Option<String> {
        self.config
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    /// CLI 安装引导文案（po.config.install_hint；缺省 `None`，由调用方兜底）
    pub fn cli_install_hint(&self) -> Option<String> {
        self.config
            .get("install_hint")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    /// 行为参数：单次调用超时（毫秒），po.config.timeout_ms 缺省 `default` 兜底
    pub fn config_timeout_ms(&self, default: u64) -> u64 {
        self.config
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(default)
    }

    /// 行为参数：输出截断上限（字节），po.config.max_output_bytes 缺省 `default` 兜底
    pub fn config_max_output_bytes(&self, default: u64) -> u64 {
        self.config
            .get("max_output_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(default)
    }

    /// 行为参数：无进展检测上限——单次思考循环内本工具累计调用达到该次数即判定死循环
    ///
    /// po.config.no_progress_max_calls 缺省 `None`（不限制）。
    /// 仅检索类等易死循环工具需要配置；代码执行类高频工具不应配置。
    pub fn config_no_progress_max_calls(&self) -> Option<usize> {
        self.config
            .get("no_progress_max_calls")
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0)
            .map(|v| v as usize)
    }

    /// 为内置工具填充缺省值（sync 时调用）
    /// 确保 protocol 一定是 Builtin，control_mode 有合理默认值
    pub fn fill_defaults_for_builtin(&mut self) {
        self.protocol = ToolProtocol::Builtin;
        // 如果 control_mode 未设置，默认使用 Auto
        // 用户自定义的 control_mode 会被保留
        if self.control_mode == ControlMode::Auto {
            self.control_mode = ControlMode::Auto;
        }
    }
}

// ==================== 实现 Vectorizable trait ====================

use crate::models::vector::Vectorizable;

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tool_tests;

impl Vectorizable for ToolPo {
    fn vectorize_text(&self) -> String {
        // ToolPo 向量化：名称 + 描述 + 标签
        let tags = self.get_tags().join(" ");
        format!("{} {} {}", self.name, self.description, tags)
    }

    fn vector_collection() -> &'static str {
        "tools"
    }
}

impl Vectorizable for Tool {
    fn vectorize_text(&self) -> String {
        self.po.vectorize_text()
    }

    fn vector_collection() -> &'static str {
        "tools"
    }
}
