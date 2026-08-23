//! Builtin doubao_search tool implementation
//!
//! 通过火山引擎豆包搜索 API 让 Agent 获取实时网络搜索结果，中文语料与时
//! 效性优于 Tavily，两者通过相同的 `[search, network]` tag 并存互补。
//!
//! # 授权（GenericToken + platform 模式）
//!
//! API key 取该用户凭证库中 `kind=GenericToken, platform=doubao_search`
//! 的个人令牌（加密存储）。与 TavilyKey 专用枚举不同，豆包复用通用令牌
//! 类型，以 platform 维度精确匹配——单字段 API Key 类凭据无需为每家供
//! 应商发明专用 CredentialKind。凭据需求由工厂静态声明，domain 编排层
//! （`resolve_tool_credentials`）据此取数，经 `CoreTool::check` 注入实
//! 例 `api_key` 字段；未注入 → 返回 `api_key_missing` 结构化引导。
//!
//! key 不在工具入参中传递，永不回显；结果返回结构化 JSON
//! （title/url/snippet/summary 列表），LLM 自行取舍。
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
use std::time::Duration;

/// 豆包搜索 API 端点
const DOUBAO_SEARCH_URL: &str = "https://open.feedcoopapi.com/search_api/web_search";

/// 默认返回条数
const DEFAULT_COUNT: u64 = 5;

/// 最大返回条数（web/web_summary 模式）
const MAX_COUNT: u64 = 50;

/// 单条 snippet 截断上限（字符）
const SNIPPET_MAX_CHARS: usize = 1500;

/// 请求超时缺省（毫秒；可经工具 PO config 的 `timeout_ms` 覆盖）
const DEFAULT_TIMEOUT_MS: u64 = 20_000;

/// platform 标识（GenericToken 二元匹配键第二维）
const PLATFORM: &str = "doubao_search";

// ==================== 凭据需求声明（工厂与实例共用单点，D17） ====================

/// 凭据需求静态声明：个人 GenericToken（platform=doubao_search）。
/// 单条 Internal 注入实例 `api_key` 字段；readiness 判定与 call_tool
/// 编排经工厂读取，check 注入经实例读取。
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

/// 授权缺失引导文案
const API_KEY_MISSING_ERROR: &str = "未找到可用的豆包搜索 API key（用户凭证未绑定）";
const API_KEY_MISSING_GUIDANCE: &str =
    "绑定豆包搜索 key（设置 → 身份凭证 → 通用令牌 → platform 填 doubao_search）";

// ==================== 工具参数 ====================

/// `doubao_search` 工具参数（LLM 入参，snake_case）
#[derive(Debug, Deserialize)]
pub struct DoubaoSearchParams {
    /// 搜索关键词（必填）
    pub query: String,
    /// 搜索类型：web（网页结果+逐条摘要）/ web_summary（网页结果+LLM 整体总结，流式）
    pub search_type: Option<String>,
    /// 返回条数（1-50，越界钳制，默认 5）
    pub count: Option<u64>,
    /// 是否返回逐条 LLM 摘要（默认 true；web 模式下每条结果含 500-1000 字摘要）
    pub need_summary: Option<bool>,
    /// 时间范围：day/week/month/year（默认不限）
    pub time_range: Option<String>,
    /// 仅返回权威站点内容（默认 false）
    pub auth_only: Option<bool>,
    /// 自动改写口语化 query 为搜索式（默认 true）
    pub query_rewrite: Option<bool>,
}

// ==================== API 请求体（PascalCase，豆包协议） ====================

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_info_level: Option<u8>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoTimeRange {
    range: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoQueryControl {
    #[serde(skip_serializing_if = "Option::is_none")]
    query_rewrite: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoSearchRequest {
    query: String,
    search_type: String,
    count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<DoubaoFilter>,
    need_summary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_range: Option<DoubaoTimeRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query_control: Option<DoubaoQueryControl>,
}

// ==================== API 响应体（PascalCase → snake_case 反序列化） ====================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoWebResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    site_name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    publish_time: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoSearchUsage {
    #[serde(default)]
    token_usage: u64,
    #[serde(default)]
    count: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoSearchResult {
    #[serde(default)]
    web_results: Vec<DoubaoWebResult>,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    query_rewrite: String,
    #[serde(default)]
    search_usage: Option<DoubaoSearchUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoResponse {
    #[serde(default)]
    result: DoubaoSearchResult,
    /// 错误响应字段
    #[serde(default)]
    response_metadata: Option<DoubaoResponseMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoResponseMetadata {
    #[serde(default)]
    error: Option<DoubaoError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DoubaoError {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

// ==================== 工具工厂 ====================

/// doubao_search 内置工具工厂
#[derive(Debug, Clone, Default)]
pub struct DoubaoSearchToolFactory;

impl BuiltinToolFactory for DoubaoSearchToolFactory {
    fn create_po(&self) -> ToolPo {
        let mut po = ToolPo {
            id: "doubao_search".to_string(),
            name: "doubao_search".to_string(),
            description: concat!(
                "Search the Chinese web for real-time information via Volcengine Doubao Search API. ",
                "Returns a structured list of web results with title, url, site name, publish time ",
                "and a content snippet. In web_summary mode, also returns an LLM-generated overall ",
                "summary. Strong for Chinese-language queries, domestic Chinese sites (Zhihu, WeChat, ",
                "Baidu), and time-sensitive results. Supports time-range filtering and authoritative-site ",
                "filtering. Read-only and safe to call automatically."
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
                    "search_type": {
                        "type": "string",
                        "enum": ["web", "web_summary"],
                        "description": "Optional: 'web' (default) returns per-result summaries; 'web_summary' additionally returns an LLM-generated overall answer."
                    },
                    "count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Optional: number of results (1-50, clamped). Default: 5."
                    },
                    "need_summary": {
                        "type": "boolean",
                        "description": "Optional: include 500-1000 char LLM summary per result. Default: true."
                    },
                    "time_range": {
                        "type": "string",
                        "enum": ["day", "week", "month", "year"],
                        "description": "Optional: restrict results to recent time range."
                    },
                    "auth_only": {
                        "type": "boolean",
                        "description": "Optional: only return results from highly authoritative sites. Default: false."
                    },
                    "query_rewrite": {
                        "type": "boolean",
                        "description": "Optional: auto-rewrite colloquial queries into search-friendly form. Default: true."
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
        Box::new(DoubaoSearchCoreTool::new(po))
    }

    fn credential_requirements(&self) -> Vec<CredentialRequirement> {
        credential_requirements()
    }
}

// ==================== 工具核心实现 ====================

/// doubao_search 工具核心实现
#[derive(Debug, Clone)]
pub struct DoubaoSearchCoreTool {
    po: ToolPo,
    /// check 注入的豆包 API key（None → api_key_missing 引导）
    api_key: Option<String>,
}

impl DoubaoSearchCoreTool {
    fn new(po: ToolPo) -> Self {
        Self { po, api_key: None }
    }
}

/// 单条 snippet 截断（超长尾部加省略标记，返回截断与否）
fn truncate_snippet(content: &str) -> (String, bool) {
    if content.chars().count() <= SNIPPET_MAX_CHARS {
        return (content.to_string(), false);
    }
    let truncated: String = content.chars().take(SNIPPET_MAX_CHARS).collect();
    (format!("{}...", truncated), true)
}

/// LLM 友好的时间范围值 → 豆包 API Range 枚举
fn map_time_range(value: &str) -> Option<DoubaoTimeRange> {
    let range = match value {
        "day" => "OneDay",
        "week" => "OneWeek",
        "month" => "OneMonth",
        "year" => "OneYear",
        _ => return None,
    };
    Some(DoubaoTimeRange {
        range: range.to_string(),
    })
}

/// 解析 web_summary 模式的 SSE 流式响应，合并为完整 DoubaoSearchResult。
///
/// 豆包 web_summary 通过 `text/event-stream` 返回多条 `data: {json}` 事件：
/// 首批事件携带 WebResults + 片段 Summary，后续事件追加 Summary 片段，
/// 末尾事件携带 SearchUsage。本函数收集所有事件并合并字段。
fn parse_sse_response(text: &str) -> DoubaoSearchResult {
    let mut merged = DoubaoSearchResult::default();
    for line in text.lines() {
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        let Ok(event) = serde_json::from_str::<DoubaoResponse>(payload) else {
            continue;
        };
        let r = event.result;
        if !r.web_results.is_empty() && merged.web_results.is_empty() {
            merged.web_results = r.web_results;
        }
        if !r.summary.is_empty() {
            merged.summary.push_str(&r.summary);
        }
        if !r.query_rewrite.is_empty() && merged.query_rewrite.is_empty() {
            merged.query_rewrite = r.query_rewrite;
        }
        if r.search_usage.is_some() {
            merged.search_usage = r.search_usage;
        }
    }
    merged
}

/// 从豆包 API 响应文本解析为统一结果（自动识别 JSON / SSE）
fn parse_response(text: &str, content_type: &str) -> DoubaoSearchResult {
    if content_type.contains("text/event-stream") {
        return parse_sse_response(text);
    }
    serde_json::from_str::<DoubaoResponse>(text)
        .map(|r| r.result)
        .unwrap_or_default()
}

#[async_trait]
impl CoreTool for DoubaoSearchCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value> {
        // 1. 参数解析与校验
        let params: DoubaoSearchParams = serde_json::from_value(args)
            .map_err(|e| anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;
        let query = params.query.trim().to_string();
        if query.is_empty() {
            return Ok(json!({
                "success": false,
                "error": "query 不能为空"
            }));
        }
        let search_type = match params.search_type.as_deref() {
            None | Some("web") => "web",
            Some("web_summary") => "web_summary",
            Some(other) => {
                return Ok(json!({
                    "success": false,
                    "error": format!("search_type 仅支持 web/web_summary，收到 '{}'", other)
                }));
            }
        };
        let count = params.count.unwrap_or(DEFAULT_COUNT).clamp(1, MAX_COUNT);
        let need_summary = params.need_summary.unwrap_or(true);
        let query_rewrite = params.query_rewrite;

        let time_range = match params.time_range.as_deref() {
            Some(v) => match map_time_range(v) {
                Some(tr) => Some(tr),
                None => {
                    return Ok(json!({
                        "success": false,
                        "error": format!("time_range 仅支持 day/week/month/year，收到 '{}'", v)
                    }));
                }
            },
            None => None,
        };
        let filter = if params.auth_only.unwrap_or(false) {
            Some(DoubaoFilter {
                auth_info_level: Some(1),
            })
        } else {
            None
        };
        let query_control = if query_rewrite.is_some() {
            Some(DoubaoQueryControl { query_rewrite })
        } else {
            None
        };

        // 2. 取 check 注入的 API key
        let Some(api_key) = self.api_key.clone() else {
            return Ok(tool_readiness::api_key_missing_json(
                API_KEY_MISSING_ERROR,
                API_KEY_MISSING_GUIDANCE,
            ));
        };

        // 3. 构造请求体并调用豆包搜索 API
        let req_body = DoubaoSearchRequest {
            query: query.clone(),
            search_type: search_type.to_string(),
            count,
            filter,
            need_summary,
            time_range,
            query_control,
        };

        let timeout_ms = self
            .po
            .config
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let timeout = Duration::from_millis(timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| anyhow!("failed to build http client: {}", e))
            .map_err(common::error::Error::from)?;
        let response = client
            .post(DOUBAO_SEARCH_URL)
            .bearer_auth(&api_key)
            .json(&req_body)
            .send()
            .await
            .map_err(|e| anyhow!("doubao search request failed: {}", e))
            .map_err(common::error::Error::from)?;

        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json")
            .to_string();
        let body_text = response
            .text()
            .await
            .map_err(|e| anyhow!("doubao search response read failed: {}", e))
            .map_err(common::error::Error::from)?;

        if !status.is_success() {
            // 尝试解析错误详情
            let error_detail = serde_json::from_str::<DoubaoResponse>(&body_text)
                .ok()
                .and_then(|r| r.response_metadata)
                .and_then(|m| m.error);
            let detail = error_detail
                .as_ref()
                .and_then(|e| e.message.as_deref())
                .unwrap_or("no detail");
            let code = error_detail
                .as_ref()
                .and_then(|e| e.code.as_deref())
                .unwrap_or("unknown");
            let hint = if status.as_u16() == 401 {
                "API key 无效或已过期：请检查已绑定的豆包搜索 key"
            } else if status.as_u16() == 429 {
                "请求超限或配额不足：豆包搜索默认 5 QPS / 每月 500 次免费额度"
            } else {
                "豆包搜索服务返回错误，请稍后重试"
            };
            return Ok(json!({
                "success": false,
                "status": status.as_u16(),
                "error": format!("doubao api error ({}: {}): {}", status.as_u16(), code, detail),
                "hint": hint
            }));
        }

        // 4. 解析响应（JSON 或 SSE）并映射为统一结构
        let result = parse_response(&body_text, &content_type);

        let mut truncated_any = false;
        let results: Vec<Value> = result
            .web_results
            .iter()
            .map(|item| {
                // 优先用 Summary（500-1000 字 LLM 摘要），兜底 Content
                let raw_snippet = if !item.summary.is_empty() {
                    &item.summary
                } else {
                    &item.content
                };
                let (snippet, truncated) = truncate_snippet(raw_snippet);
                truncated_any = truncated_any || truncated;
                json!({
                    "title": item.title,
                    "url": item.url,
                    "site_name": item.site_name,
                    "publish_time": item.publish_time,
                    "snippet": snippet,
                    "truncated": truncated
                })
            })
            .collect();

        let mut payload = json!({
            "success": true,
            "query": query,
            "search_type": search_type,
            "results": results,
            "truncated": truncated_any
        });

        // web_summary 模式的 LLM 整体总结
        if search_type == "web_summary" && !result.summary.is_empty() {
            payload["answer"] = json!(result.summary);
        }
        // query 改写结果
        if !result.query_rewrite.is_empty() {
            payload["rewritten_query"] = json!(result.query_rewrite);
        }
        // 用量统计
        if let Some(usage) = &result.search_usage {
            payload["usage"] = json!({
                "count": usage.count,
                "tokens": usage.token_usage
            });
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
                CredentialBinding::Internal { field } if field == "api_key" => {
                    self.api_key = Some(item.value.clone());
                }
                _ => {
                    return Err(err!(
                        InvalidRequest,
                        "doubao_search 仅支持 api_key 内部凭据注入点"
                    ));
                }
            }
        }
        Ok(())
    }
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::request_context_test_support::new_test_ctx;

    fn test_ctx() -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        new_test_ctx("test-user", pool)
    }

    #[test]
    fn factory_po_metadata() {
        let po = DoubaoSearchToolFactory.create_po();
        assert_eq!(po.id, "doubao_search");
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

    #[test]
    fn time_range_mapping() {
        assert_eq!(map_time_range("day").unwrap().range, "OneDay");
        assert_eq!(map_time_range("week").unwrap().range, "OneWeek");
        assert_eq!(map_time_range("month").unwrap().range, "OneMonth");
        assert_eq!(map_time_range("year").unwrap().range, "OneYear");
        assert!(map_time_range("invalid").is_none());
    }

    #[test]
    fn credential_requirements_use_generic_token_with_platform() {
        let reqs = credential_requirements();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].kind, CredentialKind::GenericToken);
        assert_eq!(reqs[0].platform.as_deref(), Some(PLATFORM));
        assert!(reqs[0].field.is_none());
        assert!(reqs[0].enhancer.is_none());
        match &reqs[0].binding {
            CredentialBinding::Internal { field } => assert_eq!(field, "api_key"),
            other => panic!("expected Internal binding, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn call_with_empty_query_returns_error_json() {
        let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
        let result = tool
            .call(test_ctx(), json!({ "query": "  " }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("query"));
    }

    #[tokio::test]
    async fn call_with_invalid_search_type_returns_error_json() {
        let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
        let result = tool
            .call(
                test_ctx(),
                json!({ "query": "rust", "search_type": "image" }),
            )
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("search_type"));
    }

    #[tokio::test]
    async fn call_with_invalid_time_range_returns_error_json() {
        let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
        let result = tool
            .call(test_ctx(), json!({ "query": "rust", "time_range": "hour" }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("time_range"));
    }

    #[tokio::test]
    async fn call_without_check_returns_api_key_missing_guidance() {
        let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
        let result = tool
            .call(test_ctx(), json!({ "query": "rust" }))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert_eq!(result["error_code"], "api_key_missing");
        let guidance = result["guidance"].as_str().unwrap();
        assert!(
            guidance.contains("doubao_search"),
            "guidance should mention platform"
        );
    }

    #[test]
    fn check_injects_api_key_from_resolved_requirement() {
        let mut tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
        assert_eq!(tool.api_key, None);
        let resolved = vec![crate::pkg::credential::ResolvedRequirement {
            requirement: credential_requirements().pop().unwrap(),
            value: "doubao-test-key".to_string(),
        }];
        tool.check(&resolved).unwrap();
        assert_eq!(tool.api_key.as_deref(), Some("doubao-test-key"));
    }

    #[test]
    fn factory_and_instance_requirements_are_consistent() {
        let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
        assert_eq!(
            DoubaoSearchToolFactory.credential_requirements(),
            tool.credential_requirements()
        );
        assert_eq!(
            tool.credential_requirements()[0].kind,
            CredentialKind::GenericToken
        );
        assert_eq!(
            tool.credential_requirements()[0].platform.as_deref(),
            Some(PLATFORM)
        );
    }

    #[test]
    fn parse_sse_response_accumulates_summary() {
        let sse = concat!(
            "data: {\"Result\":{\"WebResults\":[{\"Title\":\"Rust\",\"URL\":\"https://rust-lang.org\",\"Summary\":\"Rust lang\"}],\"Summary\":\"Rust is\"}}\n",
            "data: {\"Result\":{\"Summary\":\" a systems language.\"}}\n",
            "data: {\"Result\":{\"SearchUsage\":{\"TokenUsage\":42,\"Count\":1}}}\n",
            "data: [DONE]\n",
        );
        let result = parse_sse_response(sse);
        assert_eq!(result.web_results.len(), 1);
        assert_eq!(result.web_results[0].title, "Rust");
        assert_eq!(result.summary, "Rust is a systems language.");
        assert_eq!(result.search_usage.unwrap().token_usage, 42);
    }

    #[test]
    fn parse_json_response_extracts_results() {
        let json = r#"{
            "Result": {
                "WebResults": [
                    {"Title": "Test", "URL": "https://example.com", "SiteName": "Example", "Summary": "A test result", "PublishTime": "2026-08-24"}
                ],
                "SearchUsage": {"Count": 1, "TokenUsage": 10}
            }
        }"#;
        let result = parse_response(json, "application/json");
        assert_eq!(result.web_results.len(), 1);
        assert_eq!(result.web_results[0].title, "Test");
        assert_eq!(result.web_results[0].site_name, "Example");
        assert_eq!(result.search_usage.unwrap().count, 1);
    }
}
