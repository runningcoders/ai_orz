//! A2A (Agent-to-Agent) Protocol Runtime DAO
//!
//! 通过 HTTP JSON-RPC 2.0 调用支持 A2A 协议的远程 Agent。
//! 遵循 Google A2A 协议规范（https://github.com/google/A2A）。
//!
//! 核心方法：tasks/send - 发送任务给远程 Agent 并等待结果

use async_trait::async_trait;
use common::api::a2a::{
    A2aMessagePart, A2aTask, A2aTaskState, GetTaskParams, JsonRpcRequest, JsonRpcResponse,
    SendTaskParams,
};
use common::error::{Result, err};
use reqwest::Client;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

use super::AgentRuntimeDao;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;

/// JSON-RPC 请求 ID 生成器（单调递增）
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> Value {
    let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    Value::Number(id.into())
}

/// A2A 远程 Agent 执行配置
#[derive(Debug, Clone)]
pub struct A2aRuntimeConfig {
    /// 远程 Agent 的 A2A 端点 URL
    pub endpoint: String,
    /// 目标 Agent 名称（用于 agents/sendTask 的 agent_id 参数）
    pub agent_name: String,
    /// 认证 token（可选，通过 Authorization: Bearer <token> 传递）
    pub auth_token: Option<String>,
    /// 请求超时时间（秒）
    pub timeout_secs: u64,
}

/// 执行跨组织联邦 Agent 调用的配置（P4；S2 起鉴权为每请求 Ed25519 签名）
#[derive(Debug, Clone)]
pub struct FederatedCallConfig {
    /// 对端 A2A 端点（organization_links.endpoint）
    pub endpoint: String,
    /// 本端联邦私钥（明文 base64，出站每请求签名 + 任务令牌签发）
    pub signing_key: String,
    /// 对端组织 DID（任务令牌 aud；S2 建联后必有）
    pub peer_did: String,
    /// `X-Federation-Caller` 声明头（已序列化的 JSON 明文；None = 连接级匿名）
    pub caller_declaration: Option<String>,
    /// send + poll 全程总预算（秒），超时返回错误
    pub deadline_secs: u64,
    /// tasks/get 轮询间隔（毫秒）
    pub poll_interval_ms: u64,
}

/// A2A Runtime DAO
#[derive(Debug, Clone)]
pub struct A2aRuntimeDao {
    config: A2aRuntimeConfig,
    http: Client,
}

impl A2aRuntimeDao {
    pub fn new(config: A2aRuntimeConfig) -> Self {
        // 此前失败时回退 `Client::new()`（无超时）——构建失败一律快速失败，绝不降级为裸客户端
        let http = crate::pkg::http::presets::with_timeout(Some(std::time::Duration::from_secs(
            config.timeout_secs,
        )))
        .build()
        .expect("构建 A2A Runtime HTTP 客户端失败");
        Self { config, http }
    }

    /// 调用 tasks/get 获取远程任务状态
    pub async fn fetch_task(&self, remote_task_id: &str) -> Result<A2aTask> {
        fetch_a2a_task(
            &self.http,
            &self.config.endpoint,
            &self.config.auth_token,
            remote_task_id,
        )
        .await
    }
}

// ==================== Implementation ====================

#[async_trait]
impl AgentRuntimeDao for A2aRuntimeDao {
    async fn invoke(&self, _ctx: RequestContext, agent: &AgentPo, prompt: &str) -> Result<String> {
        execute_a2a_send(
            &self.http,
            &agent.id,
            &self.config.endpoint,
            &self.config.auth_token,
            prompt,
        )
        .await
    }
}

/// 通用 A2A JSON-RPC 调用
///
/// `auth_token`：第三方 A2A 服务端的 Bearer 凭证（可选）；
/// `federation_signing_key`：本端联邦私钥（可选，提供则携带每请求 Ed25519 签名头，
/// 调用 ai_orz 节点时必传——S2 起入站验签 fail-closed，无回退路径）。
#[allow(clippy::too_many_arguments)]
async fn call_a2a_jsonrpc(
    http: &Client,
    endpoint: &str,
    auth_token: &Option<String>,
    federation_signing_key: Option<&str>,
    extra_header: Option<(&str, &str)>,
    method: &str,
    params: Value,
    context: &str,
) -> Result<Value> {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
        id: next_request_id(),
    };
    let body = serde_json::to_vec(&request).map_err(|e| {
        err!(
            Internal,
            "{}: failed to serialize JSON-RPC request: {}",
            context,
            e
        )
    })?;

    let mut req_builder = http
        .post(endpoint)
        .header("Content-Type", "application/json");

    if let Some(signing_key) = federation_signing_key {
        let path = crate::pkg::url_util::path_and_query(endpoint).ok_or_else(|| {
            err!(
                Internal,
                "{}: 无法解析 A2A 端点 path: {}",
                context,
                endpoint
            )
        })?;
        let signed =
            crate::pkg::crypto::did::sign_federation_request(signing_key, "POST", path, &body)?;
        req_builder = req_builder
            .header(
                common::constants::http_header::FEDERATION_KEY_ID,
                &signed.key_id,
            )
            .header(
                common::constants::http_header::FEDERATION_TIMESTAMP,
                signed.timestamp.to_string(),
            )
            .header(
                common::constants::http_header::FEDERATION_NONCE,
                &signed.nonce,
            )
            .header(
                common::constants::http_header::FEDERATION_SIGNATURE,
                &signed.signature,
            );
    }
    if let Some(token) = auth_token {
        req_builder = req_builder.bearer_auth(token);
    }
    if let Some((name, value)) = extra_header {
        req_builder = req_builder.header(name, value);
    }

    let response = req_builder
        .body(body)
        .send()
        .await
        .map_err(|e| err!(Internal, "{}: A2A HTTP request failed: {}", context, e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(err!(
            Internal,
            "{}: A2A HTTP error {}: {}",
            context,
            status,
            body
        ));
    }

    let rpc_response: JsonRpcResponse = response.json().await.map_err(|e| {
        err!(
            Internal,
            "{}: failed to parse A2A JSON-RPC response: {}",
            context,
            e
        )
    })?;

    if let Some(rpc_error) = rpc_response.error {
        return Err(err!(
            Internal,
            "{}: A2A JSON-RPC error {}: {}",
            context,
            rpc_error.code,
            rpc_error.message
        ));
    }

    Ok(rpc_response.result.unwrap_or_default())
}

/// 执行 A2A tasks/send 调用
pub async fn execute_a2a_send(
    http: &Client,
    agent_id: &str,
    endpoint: &str,
    auth_token: &Option<String>,
    prompt: &str,
) -> Result<String> {
    let task_id = uuid::Uuid::now_v7().to_string();

    let message = common::api::a2a::A2aMessage {
        role: "user".to_string(),
        parts: vec![A2aMessagePart::Text {
            text: prompt.to_string(),
        }],
        message_id: None,
        task_id: Some(task_id.clone()),
    };

    let params = SendTaskParams {
        id: task_id,
        message,
        session_id: None,
        metadata: None,
        notification_url: None,
    };

    let params_value = serde_json::to_value(&params).map_err(|e| {
        err!(
            Internal,
            "Agent {}: failed to serialize params: {}",
            agent_id,
            e
        )
    })?;

    let context = format!("Agent {}", agent_id);
    let result = call_a2a_jsonrpc(
        http,
        endpoint,
        auth_token,
        None,
        None,
        "tasks/send",
        params_value,
        &context,
    )
    .await?;

    extract_text_from_task_result(&result).ok_or_else(|| {
        err!(
            Internal,
            "Agent {}: A2A response has no text content: {}",
            agent_id,
            result
        )
    })
}

/// 执行 A2A tasks/get 调用，获取远程任务状态
pub async fn fetch_a2a_task(
    http: &Client,
    endpoint: &str,
    auth_token: &Option<String>,
    remote_task_id: &str,
) -> Result<A2aTask> {
    let params = GetTaskParams {
        id: remote_task_id.to_string(),
        history_length: None,
    };

    let params_value = serde_json::to_value(&params).map_err(|e| {
        err!(
            Internal,
            "Task {}: failed to serialize params: {}",
            remote_task_id,
            e
        )
    })?;

    let context = format!("Task {}", remote_task_id);
    let result = call_a2a_jsonrpc(
        http,
        endpoint,
        auth_token,
        None,
        None,
        "tasks/get",
        params_value,
        &context,
    )
    .await?;

    serde_json::from_value(result).map_err(|e| {
        err!(
            Internal,
            "Task {}: failed to parse A2aTask: {}",
            remote_task_id,
            e
        )
    })
}

/// 跨组织联邦 Agent 调用（P4）：tasks/send → 轮询 tasks/get 直到终态
///
/// 与 [`execute_a2a_send`] 的区别：
/// - 每请求携带本端 Ed25519 签名头（S2 联邦鉴权，对端 fail-closed 验签）
/// - 携带 `X-Federation-Caller` 声明头（R3 计量：对端日志带 org 维度）
/// - `tasks/send` 时用本端私钥**自签发短期任务令牌**随 payload 下发
///   （metadata.federation_callback_token；对端回调本端时作为 Bearer 带回，
///   本端用自己公钥验证——零状态、单任务作用域，见方案 §2.4）
/// - ai_orz 节点的 `tasks/send` 是异步提交（返回 working、无 assistant 文本），
///   需轮询 `tasks/get` 直到 Completed/Failed/Canceled；若 send 响应已带文本
///   且终态（同步型对端），首次检查即返回，不发多余的 get
pub async fn execute_federated_agent_call(
    http: &Client,
    agent_id: &str,
    config: &FederatedCallConfig,
    prompt: &str,
) -> Result<String> {
    use crate::pkg::crypto::task_token::{TaskTokenClaims, issue_task_token};

    let task_id = uuid::Uuid::now_v7().to_string();

    // 本端 DID（key_id 由签名推导；任务令牌 iss 与之同源）
    let local_did = crate::pkg::crypto::did::did_from_signing_key(&config.signing_key)?;

    // 自签发任务令牌：exp 对齐任务超时 + 余量（安全来自单任务作用域而非短 TTL）
    let task_token = issue_task_token(
        &config.signing_key,
        &TaskTokenClaims {
            iss: local_did,
            aud: config.peer_did.clone(),
            task_id: task_id.clone(),
            exp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default()
                + config.deadline_secs as i64
                + 600,
        },
    )?;

    let message = common::api::a2a::A2aMessage {
        role: "user".to_string(),
        parts: vec![A2aMessagePart::Text {
            text: prompt.to_string(),
        }],
        message_id: None,
        task_id: Some(task_id.clone()),
    };

    let params = SendTaskParams {
        id: task_id,
        message,
        session_id: None,
        metadata: Some(serde_json::json!({ "federation_callback_token": task_token })),
        notification_url: None,
    };

    let params_value = serde_json::to_value(&params).map_err(|e| {
        err!(
            Internal,
            "Agent {}: failed to serialize federated params: {}",
            agent_id,
            e
        )
    })?;

    let context = format!("Federated agent {}", agent_id);
    let extra_header = config
        .caller_declaration
        .as_deref()
        .map(|decl| (common::constants::http_header::FEDERATION_CALLER, decl));

    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_secs(config.deadline_secs);

    let mut task: A2aTask = {
        let result = call_a2a_jsonrpc(
            http,
            &config.endpoint,
            &None,
            Some(&config.signing_key),
            extra_header,
            "tasks/send",
            params_value,
            &context,
        )
        .await?;
        serde_json::from_value(result)
            .map_err(|e| err!(Internal, "{}: failed to parse A2aTask: {}", context, e))?
    };

    loop {
        match task.status.state {
            A2aTaskState::Completed => {
                return extract_text_from_task_result(
                    &serde_json::to_value(&task).unwrap_or_default(),
                )
                .ok_or_else(|| {
                    err!(
                        Internal,
                        "Agent {}: federated task completed but has no text content",
                        agent_id
                    )
                });
            }
            A2aTaskState::Failed | A2aTaskState::Canceled => {
                return Err(err!(
                    Internal,
                    "Agent {}: federated task ended with state {:?}",
                    agent_id,
                    task.status.state
                ));
            }
            A2aTaskState::InputRequired => {
                return Err(err!(
                    Internal,
                    "Agent {}: federated task requires input, interactive flow not supported",
                    agent_id
                ));
            }
            A2aTaskState::Submitted | A2aTaskState::Working => {}
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(err!(
                Internal,
                "Agent {}: federated task polling timed out after {}s",
                agent_id,
                config.deadline_secs
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(config.poll_interval_ms)).await;

        let get_params = GetTaskParams {
            id: task.id.clone(),
            history_length: None,
        };
        let result = call_a2a_jsonrpc(
            http,
            &config.endpoint,
            &None,
            Some(&config.signing_key),
            extra_header,
            "tasks/get",
            serde_json::to_value(&get_params).unwrap_or_default(),
            &context,
        )
        .await?;
        task = serde_json::from_value(result)
            .map_err(|e| err!(Internal, "{}: failed to parse A2aTask: {}", context, e))?;
    }
}

/// 从 A2A tasks/send 结果中提取文本内容
///
/// A2A 协议的 tasks/send 返回 Task 对象，包含 messages 数组，
/// 每个 message 有 parts 数组，每个 part 可能是 text 类型。
/// 我们提取所有 assistant role 的 text part 内容并拼接。
pub(crate) fn extract_text_from_task_result(result: &Value) -> Option<String> {
    let task: common::api::a2a::A2aTask = serde_json::from_value(result.clone()).ok()?;

    let mut texts = Vec::new();

    for msg in &task.messages {
        if msg.role != "assistant" && msg.role != "agent" {
            continue;
        }
        for part in &msg.parts {
            if let A2aMessagePart::Text { text } = part {
                texts.push(text.clone());
            }
        }
    }

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}
#[cfg(test)]
#[path = "a2a_tests.rs"]
mod tests;
