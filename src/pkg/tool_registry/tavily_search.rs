//! Builtin tavily_search tool implementation
//!
//! 通过 Tavily Search API 让 Agent 获取实时网络搜索结果。
//!
//! # 授权（单轨，D27）
//!
//! API key 仅取该用户凭证库中的 GenericToken+platform="tavily"（个人 key，加密存储）：凭据需求由
//! 工厂静态声明，domain 编排层（`resolve_tool_credentials`）据此取数，经
//! `CoreTool::check` 注入实例 `api_key` 字段（D17 工厂化）；未注入 → 返回
//! `api_key_missing` 结构化引导（绑定个人 key 单路径）。
//!
//! key 不在工具入参中传递，永不回显；结果返回结构化 JSON
//! （title/url/snippet 列表），LLM 自行取舍。
//!
//! # 分层说明
//!
//! 凭据取数在 domain 编排层（D17 v1.5）：pkg 只保留纯函数与静态需求声明
//! （工厂与实例共用单点），工具实例不直连 DAL/DAO。

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::BuiltinToolFactory;
use crate::pkg::tool_registry::tool_readiness;
use anyhow::anyhow;
use async_trait::async_trait;
use common::enums::{ControlMode, ToolProtocol};
use common::error::{Result, err};
use common::models::{CredentialBinding, CredentialKind, CredentialRequirement};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::time::Duration;

/// Tavily Search API 端点
const TAVILY_SEARCH_URL: &str = "https://api.tavily.com/search";

/// 默认返回条数上限
const DEFAULT_MAX_RESULTS: u64 = 5;

/// 单条 snippet 截断上限（字符）
const SNIPPET_MAX_CHARS: usize = 1000;

/// 请求超时缺省（毫秒；可经工具 PO config 的 `timeout_ms` 覆盖）
const DEFAULT_TIMEOUT_MS: u64 = 15_000;

// ==================== 凭据需求声明（工厂与实例共用单点，D17） ====================

/// 凭据需求静态声明：个人 GenericToken + platform=tavily（单轨 D27；单条 Internal 注入实例
/// `api_key` 字段；readiness 判定与 call_tool 编排经工厂读取，check 注入经实例读取）
const PLATFORM: &str = "tavily";
fn credential_requirements() -> Vec<CredentialRequirement> {
    vec![CredentialRequirement {
        kind: CredentialKind::GenericToken,
        platform: Some(PLATFORM.to_string()),
        field: None,
        enhancer: None,
        binding: CredentialBinding::Internal {
            field: "api_key".to_string(),
        },
    }]
}

/// 授权缺失引导文案（单路径：绑个人 key）
const API_KEY_MISSING_ERROR: &str = "未找到可用的 Tavily API key（用户凭证未绑定）";
const API_KEY_MISSING_GUIDANCE: &str = "绑定个人 Tavily key（设置 → 身份凭证 → Tavily 区块）";

// ==================== 工具定义 ====================

/// `tavily_search` 工具参数
#[derive(Debug, Deserialize)]
pub struct TavilySearchParams {
    /// 搜索关键词（必填）
    pub query: String,
    /// 搜索深度：basic（默认，快）/ advanced（更准，消耗更多配额）
    pub search_depth: Option<String>,
    /// 返回条数（1-10，越界钳制，默认 5）
    pub max_results: Option<u64>,
    /// 是否附带 LLM 摘要答案（默认 false）
    pub include_answer: Option<bool>,
}

/// tavily_search 内置工具工厂
#[derive(Debug, Clone, Default)]
pub struct TavilySearchToolFactory;

impl BuiltinToolFactory for TavilySearchToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "tavily_search".to_string(),
            name: "Search Web (Tavily)".to_string(),
            description: concat!(
                "Search the web for real-time information via the Tavily Search API. ",
                "Returns a structured list of results, each with title, url and a content snippet ",
                "(snippets may be truncated). Optionally returns an LLM-generated answer summary ",
                "when include_answer=true. Use for fresh facts, news, docs or anything beyond ",
                "training data. Best for English-language queries and Western sites; for Chinese-language queries prefer doubao_search. Read-only and safe to call automatically."
            )
            .to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query keywords."
                    },
                    "search_depth": {
                        "type": "string",
                        "enum": ["basic", "advanced"],
                        "description": "Optional: search depth. 'basic' (default) is fast; 'advanced' is more thorough but consumes more quota."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "description": "Optional: number of results (1-10, clamped). Default: 5."
                    },
                    "include_answer": {
                        "type": "boolean",
                        "description": "Optional: include an LLM-generated short answer for the query. Default: false."
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            })),
            config: Value::Null,
            tags: serde_json::to_string(&vec!["search".to_string(), "network".to_string()])
                .unwrap_or_default(),
            ..Default::default()
        };
        po.fill_defaults_for_builtin();
        po
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(TavilySearchCoreTool::new(po))
    }

    /// 凭据需求静态声明：个人 TavilyKey（单轨，D27；readiness 据此判定，D28）
    fn credential_requirements(&self) -> Vec<CredentialRequirement> {
        credential_requirements()
    }
}

/// tavily_search 工具核心实现
#[derive(Debug, Clone)]
pub struct TavilySearchCoreTool {
    po: ToolPo,
    /// check 注入的 Tavily API key（D22 create → check → call；None → api_key_missing 引导）
    api_key: Option<String>,
    /// 共享 HTTP 客户端：首次调用时惰性构建，避免每次调用新建连接池
    http: OnceLock<reqwest::Client>,
}

impl TavilySearchCoreTool {
    fn new(po: ToolPo) -> Self {
        Self {
            po,
            api_key: None,
            http: OnceLock::new(),
        }
    }

    /// 取（或惰性构建）共享 HTTP 客户端
    ///
    /// 超时取工具 PO config，缺省 DEFAULT_TIMEOUT_MS；非法值由 http 基建收敛。
    fn http_client(&self) -> Result<&reqwest::Client> {
        if let Some(client) = self.http.get() {
            return Ok(client);
        }
        let timeout_ms = self
            .po
            .config
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let client =
            crate::pkg::http::presets::with_timeout(Some(Duration::from_millis(timeout_ms)))
                .build()?;
        Ok(self.http.get_or_init(|| client))
    }
}

/// 单条结果 snippet 截断（超长尾部加省略标记，返回截断与否）
fn truncate_snippet(content: &str) -> (String, bool) {
    if content.chars().count() <= SNIPPET_MAX_CHARS {
        return (content.to_string(), false);
    }
    let truncated: String = content.chars().take(SNIPPET_MAX_CHARS).collect();
    (format!("{}...", truncated), true)
}

#[async_trait]
impl CoreTool for TavilySearchCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value> {
        // 1. 参数解析与校验
        let params: TavilySearchParams = serde_json::from_value(args)
            .map_err(|e| anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;
        let query = params.query.trim().to_string();
        if query.is_empty() {
            return Ok(json!({
                "success": false,
                "error": "query 不能为空"
            }));
        }
        let search_depth = match params.search_depth.as_deref() {
            None | Some("basic") => "basic",
            Some("advanced") => "advanced",
            Some(other) => {
                return Ok(json!({
                    "success": false,
                    "error": format!("search_depth 仅支持 basic/advanced，收到 '{}'", other)
                }));
            }
        };
        let max_results = params
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, 10);
        let include_answer = params.include_answer.unwrap_or(false);

        // 2. 取 check 注入的 API key（未注入 → 统一结构化引导；正常编排在
        //    domain 层 resolve 阶段已出引导，此处为直调/漏 check 的防御路径）
        let Some(api_key) = self.api_key.clone() else {
            return Ok(tool_readiness::api_key_missing_json(
                API_KEY_MISSING_ERROR,
                API_KEY_MISSING_GUIDANCE,
            ));
        };

        // 3. 调用 Tavily Search API（共享客户端；超时在 http_client 内按 PO config 解析）
        let client = self.http_client()?;
        let response = client
            .post(TAVILY_SEARCH_URL)
            .bearer_auth(&api_key)
            .json(&json!({
                "query": query,
                "search_depth": search_depth,
                "max_results": max_results,
                "include_answer": include_answer,
            }))
            .send()
            .await
            .map_err(|e| anyhow!("tavily search request failed: {}", e))
            .map_err(common::error::Error::from)?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| anyhow!("tavily search response parse failed: {}", e))
            .map_err(common::error::Error::from)?;
        if !status.is_success() {
            // 401/432 类 key 问题给出可操作指引；其余透传状态码与 Tavily 报错
            let detail = body
                .get("detail")
                .and_then(|d| d.as_str())
                .unwrap_or("no detail");
            let hint = if status.as_u16() == 401 || status.as_u16() == 432 {
                "API key 无效或配额不足：请检查已绑定的个人 Tavily key"
            } else {
                "Tavily 服务返回错误，请稍后重试"
            };
            return Ok(json!({
                "success": false,
                "status": status.as_u16(),
                "error": format!("tavily api error ({}): {}", status.as_u16(), detail),
                "hint": hint
            }));
        }

        // 4. 结果映射（结构化 title/url/snippet + 截断标记）
        let results_src = body
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        let mut truncated_any = false;
        let results: Vec<Value> = results_src
            .iter()
            .map(|item| {
                let content = item.get("content").and_then(|c| c.as_str()).unwrap_or("");
                let (snippet, truncated) = truncate_snippet(content);
                truncated_any = truncated_any || truncated;
                json!({
                    "title": item.get("title").and_then(|t| t.as_str()).unwrap_or(""),
                    "url": item.get("url").and_then(|u| u.as_str()).unwrap_or(""),
                    "snippet": snippet,
                    "truncated": truncated
                })
            })
            .collect();
        let answer = if include_answer {
            body.get("answer")
                .and_then(|a| a.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        let mut payload = json!({
            "success": true,
            "query": query,
            "results": results,
            "truncated": truncated_any
        });
        if let Some(answer) = answer {
            payload["answer"] = json!(answer);
        }
        Ok(payload)
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }

    fn credential_requirements(&self) -> Vec<CredentialRequirement> {
        credential_requirements()
    }

    fn check(&mut self, resolved: &[crate::pkg::credential::ResolvedRequirement]) -> Result<()> {
        for item in resolved {
            match &item.requirement.binding {
                // 内置工具唯一合法注入点（静态声明已限定，此处防御兜底）
                CredentialBinding::Internal { field } if field == "api_key" => {
                    self.api_key = Some(item.value.clone());
                }
                _ => {
                    return Err(err!(
                        InvalidRequest,
                        "tavily_search 仅支持 api_key 内部凭据注入点"
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::request_context_test_support::new_test_ctx;

    /// 测试用 RequestContext（懒连接内存 SQLite，不产生真实 IO）
    fn test_ctx() -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        new_test_ctx("test-user", pool)
    }

    #[test]
    fn factory_po_metadata() {
        let po = TavilySearchToolFactory.create_po();
        assert_eq!(po.id, "tavily_search");
        assert_eq!(po.control_mode, ControlMode::Auto);
        assert_eq!(po.protocol, ToolProtocol::Builtin);
        assert_eq!(po.get_tags(), vec!["search", "network"]);
    }

    #[test]
    fn snippet_truncation() {
        let short = "hello world";
        let (out, truncated) = truncate_snippet(short);
        assert_eq!(out, short);
        assert!(!truncated);

        let long = "x".repeat(SNIPPET_MAX_CHARS + 10);
        let (out, truncated) = truncate_snippet(&long);
        assert!(truncated);
        assert!(out.ends_with("..."));
        assert!(out.chars().count() <= SNIPPET_MAX_CHARS + 3);
    }

    #[tokio::test]
    async fn call_with_empty_query_returns_error_json() {
        let tool = TavilySearchCoreTool::new(TavilySearchToolFactory.create_po());
        let result = tool
            .call(test_ctx(), json!({ "query": "  " }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("query"));
    }

    #[tokio::test]
    async fn call_with_invalid_depth_returns_error_json() {
        let tool = TavilySearchCoreTool::new(TavilySearchToolFactory.create_po());
        let result = tool
            .call(
                test_ctx(),
                json!({ "query": "rust", "search_depth": "deep" }),
            )
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("search_depth"));
    }

    #[tokio::test]
    async fn call_without_check_returns_api_key_missing_guidance() {
        // 未 check 注入（api_key 字段 None）→ api_key_missing 引导
        //（正常编排在 domain 层出引导，此处为直调/漏 check 的防御路径）
        let tool = TavilySearchCoreTool::new(TavilySearchToolFactory.create_po());
        let result = tool
            .call(test_ctx(), json!({ "query": "rust" }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert_eq!(result["error_code"], "api_key_missing");
        let guidance = result["guidance"].as_str().unwrap();
        assert!(
            guidance.contains("身份凭证"),
            "guidance should mention personal key path"
        );
        assert!(
            !guidance.contains("[tavily].api_key"),
            "guidance should not mention removed shared config path"
        );
    }

    #[test]
    fn check_injects_api_key_from_resolved_requirement() {
        let mut tool = TavilySearchCoreTool::new(TavilySearchToolFactory.create_po());
        assert_eq!(tool.api_key, None);
        let resolved = vec![crate::pkg::credential::ResolvedRequirement {
            requirement: credential_requirements().pop().unwrap(),
            value: "tvly-test-key".to_string(),
        }];
        tool.check(&resolved).unwrap();
        assert_eq!(tool.api_key.as_deref(), Some("tvly-test-key"));
    }

    #[test]
    fn factory_and_instance_requirements_are_consistent() {
        // 工厂声明（readiness/编排预判）与实例声明（DAL check 流程）同源，防漂移
        let tool = TavilySearchCoreTool::new(TavilySearchToolFactory.create_po());
        assert_eq!(
            TavilySearchToolFactory.credential_requirements(),
            tool.credential_requirements()
        );
        assert_eq!(
            tool.credential_requirements()[0].kind,
            CredentialKind::GenericToken
        );
        assert_eq!(
            tool.credential_requirements()[0].platform.as_deref(),
            Some("tavily")
        );
    }
}
