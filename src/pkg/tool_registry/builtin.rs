//! Builtin tool factory - built-in tools are created from constant definitions

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::tool_registry::ToolRegistry;
use crate::pkg::tool_registry::browser::BrowserToolFactory;
use crate::pkg::tool_registry::doubao_search::DoubaoSearchToolFactory;
use crate::pkg::tool_registry::fs_read::FsReadToolFactory;
use crate::pkg::tool_registry::fs_write::FsWriteToolFactory;
use crate::pkg::tool_registry::gh_cli::GhCliToolFactory;
use crate::pkg::tool_registry::http_fetch::HttpFetchToolFactory;
use crate::pkg::tool_registry::lark_cli::LarkCliToolFactory;
use crate::pkg::tool_registry::mark_artifact::MarkArtifactToolFactory;
use crate::pkg::tool_registry::shell_exec::ShellExecToolFactory;
use crate::pkg::tool_registry::skillhub_cli::SkillhubCliToolFactory;
use crate::pkg::tool_registry::tavily_search::TavilySearchToolFactory;
use dyn_clone::DynClone;
use dyn_clone::clone_trait_object;
use once_cell::sync::Lazy;

/// Built-in tool factory - creates tool instance from given ToolPo
///
/// Each built-in tool registers a factory that knows how to construct itself given the ToolPo from DB.
/// Built-in tools cannot be modified or deleted by users - they can only be synced from code.
pub trait BuiltinToolFactory: DynClone + Send + Sync {
    /// Create default ToolPo for this built-in tool
    fn create_po(&self) -> ToolPo;
    /// Create a tool instance given the ToolPo from DB
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool>;
    /// 工具凭据需求静态声明（默认空；readiness 判定与 call_tool 编排共用，D17）
    ///
    /// 内置工具的需求由代码静态声明（tavily → [TavilyKey]；
    /// doubao → [GenericToken+platform=doubao_search]；
    /// gh/lark 的需求声明在 Step 4 随工厂化落地）。
    fn credential_requirements(&self) -> Vec<common::models::CredentialRequirement> {
        Vec::new()
    }

    /// 就绪探测（生命周期方法①）：给定持久化 PO，判定**当前配置是否健康**
    ///
    /// # 语义边界（写死，勿再漂移 —— 2026-10-22 方案 B）
    ///
    /// - **回答什么**：`po.config.command`（缺省时回落到工厂默认 PO 的命令）是否
    ///   **可寻址**。即「**配置健康度**」，不是「工具可用性」。
    /// - **不回答什么**：工具最终能不能用。默认安装布局命中、PATH / 补全目录兜底
    ///   都属于「怎么修」—— 归 [`Self::propose_self_heal`]（提议）与执行侧
    ///   `resolve_cli_path`（跑起来）负责。
    /// - **为什么必须这样切**：探针若自带兜底，「配置写错但机器别处能找到」会判
    ///   `Ready` → 平台修复器（`ToolProviderManage::reconcile_builtin_tool_readiness`）
    ///   的「未就绪才修」闸门直接跳过 → 配置**永远不会被纠正**；而运行时判定
    ///   （`tool_readiness::cli_binary_readiness`，无兜底）仍报 `NotReady` → 前端
    ///   长期显示「未就绪」。这正是 `skillhub_cli` 假故障的精确机制。
    ///   回归守卫见 `builtin_tests::unaddressable_config_is_never_masked_by_path_fallback`。
    /// - **实现要求**：CLI 型覆盖实现**必须**复用
    ///   [`crate::pkg::tool_registry::tool_readiness::cli_config_health_readiness`]
    ///   （= 运行时判定同一函数，同源不可能再分叉），不得在探针里自行做降级解析。
    ///
    /// **默认实现 = `Ready`**：非 CLI 型工具（`http_fetch`/`fs_*`/`shell_exec`/
    /// 搜索类/`mark_artifact`）没有外部二进制依赖，天然就绪。
    ///
    /// 与 [`Self::credential_requirements`] 同款「工厂态静态声明 + 默认零破坏」
    /// 形态：存量 11 个内置工厂无需任何改动即可编译通过。
    fn probe_readiness(&self, po: &ToolPo) -> common::api::RuntimeReady {
        let _ = po;
        common::api::RuntimeReady::Ready
    }

    /// 自愈配置提议（生命周期方法②）：返回**修复后的 config JSON**，
    /// `None` = 不可自愈（交人工）
    ///
    /// 与 [`Self::probe_readiness`] **互补而非同口径**：探针回答「配置是否健康」
    /// （不做兜底），本方法回答「能不能修」（**允许**默认布局 / PATH / 补全目录兜底）。
    /// 两者组合才回答「工具是否可用」：`NotReady` + `Some` = 写错可自愈；
    /// `NotReady` + `None` = 真的没装，交人工。
    ///
    /// **只提议、不落库**：写库权归平台侧唯一修复器
    /// （`ToolProviderManage::reconcile_builtin_tool_readiness`），由它决定
    /// 「允许修什么 / 何时修 / 怎么留痕」。返回的多余字段会被平台按
    /// allowlist 丢弃并 warn —— 自愈动作**严格限定单一字段 `config.command`**。
    fn propose_self_heal(&self, po: &ToolPo) -> Option<serde_json::Value> {
        let _ = po;
        None
    }
}

clone_trait_object!(BuiltinToolFactory);

/// List of all generic built-in tools (lazy initialized because default needs non-const call)
pub static GENERIC_BUILTIN_TOOLS: Lazy<Vec<(String, Box<dyn BuiltinToolFactory>)>> =
    Lazy::new(|| {
        vec![
            ("http_fetch".to_string(), Box::new(HttpFetchToolFactory)),
            ("fs_read".to_string(), Box::new(FsReadToolFactory)),
            ("fs_write".to_string(), Box::new(FsWriteToolFactory)),
            ("shell_exec".to_string(), Box::new(ShellExecToolFactory)),
            ("lark_cli".to_string(), Box::new(LarkCliToolFactory)),
            ("gh_cli".to_string(), Box::new(GhCliToolFactory)),
            ("skillhub_cli".to_string(), Box::new(SkillhubCliToolFactory)),
            (
                "tavily_search".to_string(),
                Box::new(TavilySearchToolFactory),
            ),
            (
                "doubao_search".to_string(),
                Box::new(DoubaoSearchToolFactory),
            ),
            ("browser".to_string(), Box::new(BrowserToolFactory)),
            (
                "mark_artifact".to_string(),
                Box::new(MarkArtifactToolFactory),
            ),
        ]
    });

/// Register all generic built-in tools to the global registry
pub fn register_all(registry: &ToolRegistry) {
    for (_id, factory) in GENERIC_BUILTIN_TOOLS.iter() {
        let _po = factory.create_po();
        registry.register_builtin_factory(factory.clone());
    }
}
#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;
