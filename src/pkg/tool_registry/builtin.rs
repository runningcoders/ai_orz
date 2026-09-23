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
mod tests {
    use super::*;

    /// 通用内置工具的 tag 分组定义（供工具包按 tag 安装）
    #[test]
    fn generic_builtin_tools_carry_expected_tags() {
        let expected: Vec<(&str, Vec<&str>)> = vec![
            ("http_fetch", vec!["http"]),
            ("fs_read", vec!["fs"]),
            ("fs_write", vec!["fs"]),
            ("shell_exec", vec!["shell"]),
            ("lark_cli", vec!["lark"]),
            ("gh_cli", vec!["github"]),
            ("tavily_search", vec!["search", "network"]),
            ("doubao_search", vec!["search", "network"]),
            ("browser", vec!["browser", "network"]),
            ("mark_artifact", vec!["artifact"]),
        ];
        for (id, tags) in expected {
            let factory = GENERIC_BUILTIN_TOOLS
                .iter()
                .find(|(fid, _)| fid == id)
                .map(|(_, f)| f)
                .unwrap_or_else(|| panic!("builtin tool {} not registered", id));
            assert_eq!(factory.create_po().get_tags(), tags, "tags of {}", id);
        }
    }

    /// 护栏：消息类工具必须对**每个 Agent** 可达（带 `neural`）。
    ///
    /// 背景：Agent 的工具面 = 绑定工具 ∪ tag(`neural` ∪ `runtime_config.installed_tags`)
    /// （装配点 `hr/agent.rs::get_agent(with_tools)`）。系统提示词（`builder/default.rs`）
    /// 与协作沟通技能（`TEMPLATE_COMMUNICATION`）都在按名字推荐这些工具，但它们原先
    /// 只挂 `collaboration` / `messaging` tag，而这两个 tag 从未进任何 Agent 的
    /// `installed_tags`（也不在 `BASE_AGENT_PACKS`）⇒ 它们其实**不在任何 Agent 的工具
    /// 列表里**（`send_message_to_agent` 线上实测 0 次调用）。模型只能退回 `send_message`
    /// 并把同伴 Agent 的 ID 塞进 `to_user_id` → 消息落成 `to_role=User + to_id=<Agent>`
    /// → 投递全失败 → 重试 8 次后静默丢弃（`search_messages` 同理：技能让模型"必须做一次
    /// 语义检索"，工具却不在面板里）。
    ///
    /// 这几个工具是同一条沟通链路的不同出口，可达性必须一致：任一被取消 `neural`
    /// 都会让提示词 / 技能的推荐重新变成「做不到的事」，且症状是**静默丢消息**而非报错。
    #[test]
    fn message_tools_are_neural_reachable() {
        let registry = crate::pkg::tool_registry::get_registry();
        for id in [
            "send_message",
            "send_message_to_agent",
            "send_task_assignment_message",
            "list_messages",
            "search_messages",
        ] {
            let factory = registry
                .get_builtin_factory(id)
                .unwrap_or_else(|| panic!("builtin tool {} not registered", id));
            let tags = factory.create_po().get_tags();
            assert!(
                tags.iter().any(|t| t == "neural"),
                "{id} 必须带 neural tag（否则不在任何 Agent 的工具面里，\
                 提示词 / 技能的推荐会变成静默丢消息）。实际 tags={tags:?}"
            );
        }
    }

    /// 护栏：所有内置工具的参数 schema 必须是「LLM 友好」形状（无 `$ref`/`anyOf`/`oneOf`）。
    ///
    /// 背景：模型在属性层看不到 `type`/`enum` 时，会把枚举值写成**未加引号的裸标识符**
    /// （`{"status":Pending}`），整段 arguments 变成非法 JSON → 降级为 null →
    /// 下游只看到 `empty tool arguments`。线上实测 25 次/天、25/25 命中 `anyOf+$ref` 形状。
    ///
    /// 收敛由 `common::llm_schema::schema_for_llm` 在 `#[tool(..)]` 宏里完成；
    /// 这条测试防止新工具绕过它（例如手写 `create_po` 或新增工厂）。
    ///
    /// 判定复用 `common::llm_schema::unflattened_positions`（与折叠规则同源）——
    /// 判别联合（`#[serde(tag="type")]`，如 `CredentialBinding`）属设计内保留，不算违规。
    #[test]
    fn builtin_tool_schemas_are_llm_flattened() {
        let registry = crate::pkg::tool_registry::get_registry();
        let ids = registry.list_builtin_ids();
        // 防空跑守卫：注册表因故为空时会「零工具全绿」，必须挡住
        assert!(ids.len() > 20, "内置工具数异常偏少: {}", ids.len());

        let mut offenders = Vec::new();
        for id in ids {
            let Some(factory) = registry.get_builtin_factory(&id) else {
                continue;
            };
            let Some(schema) = factory.create_po().parameters_schema else {
                continue;
            };
            for pos in common::llm_schema::unflattened_positions(&schema) {
                offenders.push(format!("  {id}: {pos}"));
            }
        }
        assert!(
            offenders.is_empty(),
            "以下内置工具的 schema 未收敛（模型会写坏枚举参数）:\n{}",
            offenders.join("\n")
        );
    }
}
