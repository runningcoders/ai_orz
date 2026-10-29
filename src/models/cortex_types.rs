//! Cortex 类型定义
//!
//! 本模块定义 cortex dao 与 brain dal 之间的数据契约：
//! - `ThinkResult`: 模型思考结果（最终回答或工具调用请求）
//! - `ToolDescriptor`: 工具描述符（从业务 Tool 直接派生）
//! - `ToolCallRequest`: 模型发起的工具调用请求
//!
//! 这些类型是「思考层」与「执行层」的边界：
//! - cortex dao 负责「想」→ 返回 ThinkResult
//! - 上层（awakening）负责「做」→ 执行 ToolCallRequest
//! - brain dal 透传 ThinkResult

use common::enums::ThinkingIntensity;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 一次模型调用的控制参数（调用方意图）
///
/// 只装「本次调用想怎么跑」的旋钮。`access_mode` 这类由 Provider 配置派生的
/// 下行协议差异仍由 cortex DAO 内部解析，避免 Domain 层重复读配置。
/// 后续 `temperature` / `max_output_tokens` / `tool_choice` 等统一加在这里，
/// 避免 `think` 签名继续膨胀。
#[derive(Debug, Clone, Default)]
pub struct ModelCallOptions {
    /// 思考强度（平台 0–100 分）；None = 请求体不下发该字段（能力开关关闭）
    pub thinking_intensity: Option<ThinkingIntensity>,
}

/// 模型思考结果
///
/// 模型返回有两种可能：
/// - `Final`: 最终回答（推理结束，可直接返回给用户）
/// - `ToolCall`: 需要调用工具（由上层执行后将结果拼回 prompt 继续推理）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThinkResult {
    /// 最终回答（纯文本，无 tool_calls）
    Final {
        /// 模型回答内容
        content: String,
        /// 本次调用的 token 使用统计
        usage: TokenUsage,
        /// 本轮产生的推理项，需由上层回传给下一轮（见 [`ReasoningItem`]）
        reasoning_items: Vec<ReasoningItem>,
    },
    /// 工具调用请求（模型要求调用工具）
    ToolCall {
        /// 模型给出的思考内容（可能为空，用于展示推理过程）
        content: Option<String>,
        /// 需要调用的工具列表
        tool_calls: Vec<ToolCallRequest>,
        /// 本次调用的 token 使用统计
        usage: TokenUsage,
        /// 本轮产生的推理项，需由上层回传给下一轮（见 [`ReasoningItem`]）
        reasoning_items: Vec<ReasoningItem>,
    },
}

impl ThinkResult {
    /// 是否为最终回答
    pub fn is_final(&self) -> bool {
        matches!(self, ThinkResult::Final { .. })
    }

    /// 是否为工具调用请求
    pub fn is_tool_call(&self) -> bool {
        matches!(self, ThinkResult::ToolCall { .. })
    }

    /// 获取 token 使用统计
    pub fn usage(&self) -> &TokenUsage {
        match self {
            ThinkResult::Final { usage, .. } => usage,
            ThinkResult::ToolCall { usage, .. } => usage,
        }
    }

    /// 获取本轮产生的推理项（Responses 协议需要回传；Chat Completions 恒为空）
    pub fn reasoning_items(&self) -> &[ReasoningItem] {
        match self {
            ThinkResult::Final {
                reasoning_items, ..
            } => reasoning_items,
            ThinkResult::ToolCall {
                reasoning_items, ..
            } => reasoning_items,
        }
    }
}

/// 推理项（Responses API 的 `type:"reasoning"` output item）
///
/// 部分推理模型要求下一轮 `input` **原样回传**上一轮的 reasoning item，否则报 400。
/// 承载它的 [`ChatMessage::Reasoning`] 作用域限定在单次 `run_think_loop` 内：
/// messages 是该函数的局部变量，全仓库零处序列化持久化，故纯内存即可，
/// 不需要存储载体、不需要 migration。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReasoningItem {
    /// item id（形如 `rs_xxx`），回传时原样带上
    pub id: Option<String>,
    /// 摘要文本（可展示；与 encrypted_content 互斥地提供上下文）
    pub summary: Vec<String>,
    /// provider 私有的加密推理内容，仅可在**同一 provider** 回传。
    ///
    /// 红线：绝不进 `to_summary_text` / trace / 日志——它既长又是密文，
    /// 对人与记忆均无价值，且经 RoundDigest → abort_summary 可能到达用户可见文本。
    pub encrypted_content: Option<String>,
}

/// Token 使用统计
///
/// 从 HTTP response body 中直接提取，无需 rig hook。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 输入 token 数
    pub input_tokens: u64,
    /// 输出 token 数
    pub output_tokens: u64,
    /// 总 token 数（某些 provider 直接返回，否则 input + output）
    pub total_tokens: Option<u64>,
    /// 提示缓存命中的输入 token 数（`prompt_tokens_details.cached_tokens` /
    /// Responses 的 `input_tokens_details.cached_tokens`）。
    /// provider 不下发时为 0——**不代表未命中**，仅表示未上报。
    pub cached_tokens: u64,
    /// 思考/推理消耗的输出 token 数（`completion_tokens_details.reasoning_tokens` /
    /// Responses 的 `output_tokens_details.reasoning_tokens`）。
    pub reasoning_tokens: u64,
}

impl TokenUsage {
    /// 计算总 token 数
    pub fn total(&self) -> u64 {
        self.total_tokens
            .unwrap_or(self.input_tokens + self.output_tokens)
    }
}

/// 工具描述符
///
/// cortex dao 调用模型时传入的工具定义，遵循 OpenAI function calling 协议。
/// 从业务 `Tool` 结构体直接派生（通过 `From<&Tool>`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// 工具名称（唯一）
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 参数 JSON Schema
    pub parameters: Value,
}

/// 从业务 Tool 直接派生 ToolDescriptor
///
/// cortex dao 只需要工具的描述信息（name/description/parameters），
/// 不需要 CoreTool 的执行逻辑（执行由上层 awakening 负责）。
impl From<&crate::models::tool::Tool> for ToolDescriptor {
    fn from(tool: &crate::models::tool::Tool) -> Self {
        ToolDescriptor {
            // function.name 必须落在跨 provider LCD（^[a-zA-Z0-9_-]{1,128}$）内：
            // DB 的 name 列是**人类显示名**（"List Tool Tags" 带空格），MCP 工具名
            // 形如 mcp.<uuid>.<tool>（带点号），原样直发会被火山方舟等严端点 400
            // （报错还指不出是哪个工具 ⇒ 该 Agent 每轮全挂）。从 id 现算 wire name；
            // 回站匹配（think_loop）用同一函数，进出对称。
            name: common::llm_schema::to_wire_tool_name(&tool.po.id),
            description: tool.po.description.clone(),
            // 内置工具的 schema 在注册时已收敛过，这里再跑一次是幂等的；
            // 真正需要它的是 HTTP 自建工具与外部 MCP 同步的 inputSchema（不可信输入）。
            parameters: tool
                .po
                .parameters_schema
                .clone()
                .map(common::llm_schema::to_llm_schema)
                .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
        }
    }
}

/// 模型发起的工具调用请求
///
/// 对应 OpenAI Chat Completions response 中的 `tool_calls[i]`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    /// 工具调用 ID（模型生成，回传 tool 结果时需要匹配）
    pub id: String,
    /// 工具名称（对应 ToolDescriptor.name）
    pub name: String,
    /// 参数 JSON（模型生成的参数，已通过 schema 验证）
    pub arguments: Value,
}

/// 图像 part（模型输入侧；base64 data URL 由传输转换层拼装）
///
/// 仅用于 [`ChatMessage::UserMultimodal`]（最新 1 条用户消息的多模态输入）；
/// base64 不进 `to_summary_text` 轮次快照 / RoundDigest，也不进 trace raw_input
/// （builder.build() 为纯文本），token 与内存双保险。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePart {
    /// 图像 MIME 类型（image/png | image/jpeg | image/gif | image/webp）
    pub mime_type: String,
    /// 裸 base64 数据（不含 data: 前缀）
    pub data_base64: String,
}

/// 聊天消息（多轮对话历史）
///
/// 对应 OpenAI Chat Completions API 的 messages 数组中的元素。
/// think() 接收 messages 数组而非扁平字符串，确保模型能看到完整的对话历史
///（包括自己的 tool_calls 和 tool 结果），从而正确终止循环。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatMessage {
    /// 系统消息（Agent 人设 / 技能方法论 / 核心规则，高优先级）
    ///
    /// 正确的 System role 分离能显著提升 function calling 决策质量：
    /// 模型能明确区分"必须遵守的规则"和"用户输入的内容"。
    System { content: String },
    /// 用户消息
    User { content: String },
    /// 助手消息（可能包含 tool_calls）
    Assistant {
        /// 文本内容（可能为空）
        content: Option<String>,
        /// 工具调用请求（模型要求调用工具时）
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCallRequest>>,
    },
    /// 工具结果消息（回传给模型）
    Tool {
        /// 对应的 tool_call_id（模型生成，用于匹配）
        tool_call_id: String,
        /// 工具执行结果
        content: String,
    },
    /// 用户多模态消息：文本 part + 图像 parts（仅最新 1 条用户消息使用）
    ///
    /// 与 `User{content}` 并存：既有纯文本构造点零波及；仅组装层（awakening）
    /// 在当前消息携带图像时升级为本变体。`role` tag 序列化为 "user_multimodal"。
    UserMultimodal {
        /// 文本 part（原完整 user 文本，各文本区块照旧）
        text: String,
        /// 图像 parts（内置参数：≤4 张/单条，单图原始字节 ≤10MB，mime 白名单 image/*）
        images: Vec<ImagePart>,
    },
    /// 推理项（Responses 协议专用：模型上一轮产出的 reasoning item，需原样回传）
    ///
    /// **仅对 Responses 协议生效**；Chat Completions 协议的 `messages_to_json`
    /// 会跳过本变体（该协议无此概念，输出必然 400）。
    ///
    /// 作用域限定单次 `run_think_loop`（messages 是局部变量、零持久化），
    /// 故跨轮次不会泄漏给其他 provider。
    Reasoning {
        /// 推理项内容
        item: ReasoningItem,
    },
}

impl ChatMessage {
    /// 创建 System 消息（人设 / 规则 / 方法论）
    pub fn system(content: impl Into<String>) -> Self {
        ChatMessage::System {
            content: content.into(),
        }
    }

    /// 创建 User 消息
    pub fn user(content: impl Into<String>) -> Self {
        ChatMessage::User {
            content: content.into(),
        }
    }

    /// 创建 Tool 结果消息
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        ChatMessage::Tool {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }

    /// 创建用户多模态消息（文本 + 图像 parts）
    pub fn user_multimodal(text: impl Into<String>, images: Vec<ImagePart>) -> Self {
        ChatMessage::UserMultimodal {
            text: text.into(),
            images,
        }
    }

    /// 创建推理项消息（Responses 协议回传用）
    pub fn reasoning(item: ReasoningItem) -> Self {
        ChatMessage::Reasoning { item }
    }

    /// 将消息序列化为摘要文本（用于 think_loop 的 `RoundDigest.transcript` 轮次快照与兜底摘要）
    ///
    /// 格式：
    /// - User: `[user] {content}`
    /// - Assistant: `[assistant] {content}` + tool_calls 摘要
    /// - Tool: `[tool:{name}] {content_truncated}`
    ///
    /// `max_content_len` 限制单条消息 content 的最大字符数，超出尾部截断。
    pub fn to_summary_text(&self, max_content_len: usize) -> String {
        match self {
            ChatMessage::System { content } => {
                format!("[system] {}", truncate_str(content, max_content_len))
            }
            ChatMessage::User { content } => {
                format!("[user] {}", truncate_str(content, max_content_len))
            }
            ChatMessage::Assistant {
                content,
                tool_calls,
            } => {
                let mut parts: Vec<String> = Vec::new();
                if let Some(c) = content {
                    parts.push(truncate_str(c, max_content_len).to_string());
                }
                if let Some(calls) = tool_calls {
                    for tc in calls {
                        let args_str = serde_json::to_string(&tc.arguments)
                            .unwrap_or_else(|_| "{}".to_string());
                        parts.push(format!(
                            "[called tool: {} with {}]",
                            tc.name,
                            truncate_str(&args_str, max_content_len)
                        ));
                    }
                }
                format!("[assistant] {}", parts.join(" | "))
            }
            ChatMessage::Tool {
                tool_call_id,
                content,
            } => {
                format!(
                    "[tool:{}] {}",
                    tool_call_id,
                    truncate_str(content, max_content_len)
                )
            }
            ChatMessage::UserMultimodal { text, images } => {
                // base64 不进轮次快照 / RoundDigest：只取 text，图像仅计数占位
                format!(
                    "[user] {} [+{} images]",
                    truncate_str(text, max_content_len),
                    images.len()
                )
            }
            ChatMessage::Reasoning { item } => {
                // 红线：`encrypted_content` 是 provider 私有密文，**绝不进轮次快照**。
                // 照抄 UserMultimodal 的 base64 排除约定：只留 id + 摘要计数。
                // 通道风险：RoundDigest.transcript 经 abort_summary 可到达用户可见
                // 文本与短期记忆，密文一旦进来就不可逆。
                format!(
                    "[reasoning] id={} ({} summary items, encrypted omitted)",
                    item.id.as_deref().unwrap_or("-"),
                    item.summary.len()
                )
            }
        }
    }
}

fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // 按字符边界截断，避免切到 UTF-8 中间
        match s.char_indices().nth(max_len) {
            Some((idx, _)) => &s[..idx],
            None => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_multimodal_serializes_with_role_tag() {
        let msg = ChatMessage::user_multimodal(
            "看图",
            vec![ImagePart {
                mime_type: "image/png".to_string(),
                data_base64: "AAAA".to_string(),
            }],
        );
        let v = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(v["role"], "user_multimodal");
        assert_eq!(v["text"], "看图");
        assert_eq!(v["images"][0]["mime_type"], "image/png");
        assert_eq!(v["images"][0]["data_base64"], "AAAA");
    }

    #[test]
    fn to_summary_text_multimodal_counts_images_without_base64() {
        let msg = ChatMessage::user_multimodal(
            "看两张图",
            vec![
                ImagePart {
                    mime_type: "image/png".to_string(),
                    data_base64: "QQ==".to_string(),
                },
                ImagePart {
                    mime_type: "image/jpeg".to_string(),
                    data_base64: "Aw==".to_string(),
                },
            ],
        );
        let summary = msg.to_summary_text(200);
        assert_eq!(summary, "[user] 看两张图 [+2 images]");
        assert!(!summary.contains("QQ=="), "base64 不得进入轮次快照");
    }

    #[test]
    fn to_summary_text_plain_user_unchanged() {
        let summary = ChatMessage::user("纯文本").to_summary_text(200);
        assert_eq!(summary, "[user] 纯文本");
    }

    #[test]
    fn reasoning_serializes_with_role_tag() {
        let msg = ChatMessage::reasoning(ReasoningItem {
            id: Some("rs_abc".to_string()),
            summary: vec!["先算总量".to_string()],
            encrypted_content: Some("gAAAAABpaDWNMxMeLw".to_string()),
        });
        let v = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(v["role"], "reasoning");
        assert_eq!(v["item"]["id"], "rs_abc");
        assert_eq!(v["item"]["summary"][0], "先算总量");
        assert_eq!(v["item"]["encrypted_content"], "gAAAAABpaDWNMxMeLw");
    }

    /// 红线守卫：密文不得进入轮次快照 / RoundDigest（照抄 base64 排除约定）。
    ///
    /// 泄漏通道：messages → RoundDigest.transcript → abort_summary → 用户可见文本
    /// 与短期记忆。密文本身既长又不可读，进去了既浪费 token 又不可逆。
    #[test]
    fn to_summary_text_reasoning_omits_encrypted_content() {
        let msg = ChatMessage::reasoning(ReasoningItem {
            id: Some("rs_abc".to_string()),
            summary: vec!["第一步".to_string(), "第二步".to_string()],
            encrypted_content: Some("gAAAAABpaDWNMxMeLw_SUPER_SECRET".to_string()),
        });
        let summary = msg.to_summary_text(200);
        assert!(
            !summary.contains("gAAAAABpaDWNMxMeLw_SUPER_SECRET"),
            "加密推理内容不得进入轮次快照"
        );
        assert!(!summary.contains("SUPER_SECRET"), "密文片段不得泄漏");
        assert_eq!(
            summary,
            "[reasoning] id=rs_abc (2 summary items, encrypted omitted)"
        );
    }
}
