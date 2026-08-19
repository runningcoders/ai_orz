//! Builtin tavily_search tool implementation
//!
//! 通过 Tavily Search API 让 Agent 获取实时网络搜索结果。
//!
//! # 双轨授权（设计见 docs/design/web_search_and_browser_tools_design.md）
//!
//! API key 解析顺序：
//! 1. **用户凭证优先**：按 `ctx.user_id` 经 `TavilyCredentialResolver` 查该用户
//!    凭证库中的 TavilyKey（个人 key，加密存储）
//! 2. **共享 config 兜底**：用户未绑定时取 `ai_orz.toml [tavily].api_key`
//! 3. 两者皆缺 → 返回 `api_key_missing` 结构化引导（双路径：绑个人 key / 配共享 key）
//!
//! key 不在工具入参中传递，永不回显；结果返回结构化 JSON
//! （title/url/snippet 列表），LLM 自行取舍。
//!
//! # 分层说明
//!
//! `TavilyCredentialResolver` trait 定义在 pkg 层（无上层依赖），具体实现由
//! user DAL 提供并在 `service::init` 注册，工具不直连 DAL/DAO。

use crate::config::get;
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::BuiltinToolFactory;
use crate::pkg::tool_registry::tool_readiness;
use anyhow::anyhow;
use async_trait::async_trait;
use common::enums::{ControlMode, ToolProtocol};
use common::error::Result;
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

// ==================== 凭证解析器 ====================

/// Tavily API key 凭证解析器（pkg 层抽象，由上层实现并注册）
#[async_trait]
pub trait TavilyCredentialResolver: Send + Sync {
    /// 解析当前上下文用户的 Tavily API key（已解密）；未绑定返回 None
    async fn resolve(&self, ctx: &RequestContext) -> Result<Option<String>>;
}

static RESOLVER: OnceLock<Box<dyn TavilyCredentialResolver>> = OnceLock::new();

/// 注册全局凭证解析器（service::init 阶段调用，仅首次生效）
pub fn set_credential_resolver(resolver: Box<dyn TavilyCredentialResolver>) {
    let _ = RESOLVER.set(resolver);
}

/// 获取已注册的全局凭证解析器
pub fn get_credential_resolver() -> Option<&'static dyn TavilyCredentialResolver> {
    RESOLVER.get().map(|r| r.as_ref())
}

/// 授权来源标记（调用方可见，便于区分个人/共享授权）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeySource {
    /// 用户凭证库个人 key
    UserCredential,
    /// 实例共享 config key
    SharedConfig,
}

impl ApiKeySource {
    fn as_str(self) -> &'static str {
        match self {
            Self::UserCredential => "user_credential",
            Self::SharedConfig => "shared_config",
        }
    }
}

/// 双轨授权解析：用户凭证优先 → 共享 config 兜底；皆缺返回 None
async fn resolve_api_key(ctx: &RequestContext) -> Result<Option<(String, ApiKeySource)>> {
    if let Some(resolver) = get_credential_resolver()
        && let Some(key) = resolver.resolve(ctx).await?
    {
        return Ok(Some((key, ApiKeySource::UserCredential)));
    }
    let shared = get().tavily.api_key.trim().to_string();
    if !shared.is_empty() {
        return Ok(Some((shared, ApiKeySource::SharedConfig)));
    }
    Ok(None)
}

/// 授权缺失引导文案（双路径：绑个人 key / 配共享 key）
const API_KEY_MISSING_ERROR: &str = "未找到可用的 Tavily API key（用户凭证与实例共享配置均未提供）";
const API_KEY_MISSING_GUIDANCE: &str = "绑定个人 Tavily key（设置 → 身份凭证 → Tavily 区块），或由管理员在服务端 ai_orz.toml 的 [tavily].api_key 配置共享 key";

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
            name: "tavily_search".to_string(),
            description: concat!(
                "Search the web for real-time information via the Tavily Search API. ",
                "Returns a structured list of results, each with title, url and a content snippet ",
                "(snippets may be truncated). Optionally returns an LLM-generated answer summary ",
                "when include_answer=true. Use for fresh facts, news, docs or anything beyond ",
                "training data. Read-only and safe to call automatically."
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
        Box::new(TavilySearchCoreTool { po })
    }
}

/// tavily_search 工具核心实现
#[derive(Debug, Clone)]
pub struct TavilySearchCoreTool {
    po: ToolPo,
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
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
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

        // 2. 双轨授权解析（皆缺 → 统一结构化引导）
        let Some((api_key, key_source)) = resolve_api_key(&ctx).await? else {
            return Ok(tool_readiness::api_key_missing_json(
                API_KEY_MISSING_ERROR,
                API_KEY_MISSING_GUIDANCE,
            ));
        };

        // 3. 调用 Tavily Search API
        let timeout = Duration::from_millis(get().tavily.timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| anyhow!("failed to build http client: {}", e))
            .map_err(common::error::Error::from)?;
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
                "API key 无效或配额不足：请检查已绑定的 Tavily key（个人凭证或共享配置）"
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
            "key_source": key_source.as_str(),
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
        let tool = TavilySearchCoreTool {
            po: TavilySearchToolFactory.create_po(),
        };
        let result = tool
            .call(test_ctx(), json!({ "query": "  " }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("query"));
    }

    #[tokio::test]
    async fn call_with_invalid_depth_returns_error_json() {
        let tool = TavilySearchCoreTool {
            po: TavilySearchToolFactory.create_po(),
        };
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
    async fn call_without_key_returns_api_key_missing_guidance() {
        // 单测环境：resolver 未注册且共享 config 为空（config 可能被其他测试污染，
        // 仅在确无 key 时断言引导结构）
        let _ = crate::config::init();
        let tool = TavilySearchCoreTool {
            po: TavilySearchToolFactory.create_po(),
        };
        let ctx = test_ctx();
        if get_credential_resolver().is_none() && get().tavily.api_key.trim().is_empty() {
            let result = tool.call(ctx, json!({ "query": "rust" })).await.unwrap();
            assert_eq!(result["success"], false);
            assert_eq!(result["error_code"], "api_key_missing");
            let guidance = result["guidance"].as_str().unwrap();
            assert!(
                guidance.contains("身份凭证"),
                "guidance should mention personal key path"
            );
            assert!(
                guidance.contains("[tavily].api_key"),
                "guidance should mention shared config path"
            );
        }
    }
}
