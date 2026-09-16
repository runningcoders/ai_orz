//! 入站渠道生产者共享的「失败后要不要重投」决策
//!
//! 设计稿（`docs/design/aop_producer_consumer_contract_design.md` §4.4）把重试决策
//! **下沉给生产者**：框架不设 `max_retry`，由唯一知道业务语义的一方回答。
//! email / wechat / lark 三个入站渠道的判定依据相同，集中在这里以免三份复制：
//!
//! 1. **错误内容**：`finish_consumption` 透传的 `err` 是 `Error` 的 `Display`
//!    （形如 `[invalid_request] xxx`，见 `common/src/error/types.rs`），
//!    按其中的**错误码**判定是否永久 —— 报文非法 / 资源不存在这类，
//!    重投一万次也不会变好。
//! 2. **尝试次数**（`attempt`，§4.4「决策依据二」）：累计到 [`MAX_ATTEMPTS`] 仍未成功
//!    → 判永久。没有这条兜底，一条"永远适配失败"的消息会把队列与游标双双卡死，
//!    并以 `error_retry_sleep_ms` 的节奏无限刷日志（正是上一轮「日志刷屏」的形状）。
//!
//! ⚠️ 判 `Discard` 是**不可逆**的（无死信存储）：框架会打 error 日志 +
//! 记 `on_consume_discarded` 独立埋点，且**仍回调 `on_consumed`**
//! —— 游标型渠道靠这一步跨过坏消息，否则下轮会重新拉到同一条，形成死循环。

use crate::pkg::aop::RetryDecision;

/// 累计尝试上限：达到即判永久失败（`Discard`）
///
/// 取值考虑：瞬时故障（DB 抖动 / 网络超时）通常几次内恢复；给足 8 次仍失败，
/// 就不再指望重投——转为留下审计痕迹并越过该条。
pub const MAX_ATTEMPTS: u32 = 8;

/// 永久性错误码：命中即 `Discard`（重投不会变好）
///
/// 取值与 `common/src/error/code.rs` 的 `code:` 字段逐字一致。
/// `Err` 侧的字符串由 `Error` 的 `Display` 生成，格式为 `[<code>] <msg>`。
const PERMANENT_CODES: &[&str] = &[
    // 报文 / 参数非法（邮件格式不合法、协议不匹配）
    "invalid_request",
    // 渠道 / 凭证 / 关联资源不存在
    "not_found",
    "resource_not_found",
    // 语义上不支持（如渠道类型与事件类型不匹配）
    "unsupported_operation",
    // 超出体积上限：重投同样超限
    "payload_too_large",
    // 配置缺失 / 非法：没人改配置就永远缺
    "config_missing",
    "config_invalid",
];

/// 从 `[<code>] <msg>` 形态里取出错误码
fn code_of(err: &str) -> Option<&str> {
    err.strip_prefix('[')?.split(']').next()
}

/// 判定一次失败是否已不可恢复
///
/// - 错误码命中 [`PERMANENT_CODES`] → 不可恢复
/// - 累计尝试达到 [`MAX_ATTEMPTS`] → 不可恢复（兜底）
/// - 其余（含无法解析错误码的形态）→ 可恢复，继续重投
pub fn is_permanent(err: &str, attempt: u32) -> bool {
    if attempt >= MAX_ATTEMPTS {
        return true;
    }
    match code_of(err) {
        Some(code) => PERMANENT_CODES.contains(&code),
        None => false,
    }
}

/// 由失败现场给出重试决策：永久 → `Discard`，否则 `Retry`
pub fn decide(err: &str, attempt: u32) -> RetryDecision {
    if is_permanent(err, attempt) {
        RetryDecision::Discard
    } else {
        RetryDecision::Retry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permanent_code_is_discarded_on_first_attempt() {
        // `Error` 的 Display 形态：`[invalid_request] 邮件格式非法`
        assert!(is_permanent("[invalid_request] 邮件格式非法", 1));
        assert!(is_permanent("[resource_not_found] 渠道不存在", 1));
        assert_eq!(
            decide("[config_missing] 凭证未配置", 1),
            RetryDecision::Discard
        );
    }

    #[test]
    fn transient_code_keeps_retrying_until_cap() {
        // DB / 网络类：前几次一律重投
        for attempt in 1..MAX_ATTEMPTS {
            assert!(
                !is_permanent("[db_query_failed] 查询失败", attempt),
                "attempt={} 应立即重投",
                attempt
            );
            assert_eq!(
                decide("[network_error] 超时", attempt),
                RetryDecision::Retry
            );
        }
        // 到上限转永久（避免一条坏消息把队列刷死）
        assert!(is_permanent("[db_query_failed] 查询失败", MAX_ATTEMPTS));
        assert_eq!(
            decide("[db_query_failed] 查询失败", MAX_ATTEMPTS),
            RetryDecision::Discard
        );
    }

    #[test]
    fn unparsable_err_is_treated_as_transient() {
        // 不带 `[code]` 前缀的形态（第三方字符串、手工构造）→ 安全方向：继续重投
        assert!(!is_permanent("something went wrong", 1));
        assert_eq!(decide("", 1), RetryDecision::Retry);
        // 前缀残缺也走安全方向
        assert!(!is_permanent("[", 1));
        assert!(!is_permanent("invalid_request", 1));
    }
}
