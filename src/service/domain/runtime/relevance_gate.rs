//! 渠道入站相关性判断（小脑 Noul 通用场景）
//!
//! 供 producer 的入站过滤层调用：判断群聊消息是否值得 Agent 介入回复。
//! 域层只声明场景口径，实际小脑调用与降级由 DAL（`dal::brain::judge_noul`）完成，
//! 保持 Domain → DAL → DAO 的单向分层。
//!
//! 任何降级都不放行，但区分两种降级：`Unavailable`（未配置决策模型，配置态）
//! 与 `Failed`（有模型但判定失败，故障态），由调用方决定是否回执。

use crate::pkg::RequestContext;

pub use crate::service::dal::brain::NoulOutcome;

/// 固定问题 ID（同一场景固定，便于日志与调试对齐）
const QUESTION_ID: &str = "inbound_relevance.primary";

/// 入站判定可用的模型上下文容量（token）
///
/// 取默认小脑 provider 的 `min(推荐上下文, 最大上下文)`；`None` 表示未配置容量，
/// 调用方按项目既有口径「未配不裁」处理，绝不拍固定字符数。
pub async fn context_capacity(ctx: &RequestContext) -> Option<u64> {
    crate::service::dal::brain::cerebellum_context_capacity(ctx).await
}

/// 用默认小脑判断上下文中的当前消息是否值得 Agent 回复
///
/// 返回 [`NoulOutcome`]：`Probability` 有结论；`Unavailable` 没有可用决策模型；
/// `Failed` 有模型但本次判定失败。调用方按 fail-closed 处置两者，但回执不同。
pub async fn judge_relevance(
    ctx: &RequestContext,
    instructions: &str,
    context: &str,
    timeout_ms: u64,
) -> NoulOutcome {
    crate::service::dal::brain::judge_noul(ctx, QUESTION_ID, instructions, context, timeout_ms)
        .await
}
