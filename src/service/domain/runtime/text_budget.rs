//! 大脑侧输入预算的份额表（**唯一事实源**）
//!
//! AMan 2026-11-11 追加指令：「我看之前还有几个用字符计数的限制，我们也改成
//! token 了吧」——本模块承接那批「按字符计」的输入预算，把它们换成**按模型上下文
//! 的占比**，与 `cerebellum_context::share`（小脑侧）同形态、不同池。
//!
//! # 与 `cerebellum_context::share` 的分工
//!
//! - `cerebellum_context::share`：小脑一次判断的输入如何**切开分账**（骨架 /
//!   待发消息 / 正文 / 摘要片段），各份额相加 ≤ 100%，有硬顶断言。
//! - 本模块：各消费点（中止摘要 / inbox 注入 / 原文记账）都是**独立字段**的上限，
//!   不会同时出现在同一个 prompt 的同一个池里被加总 ⇒ **不设总和硬顶**——把它们
//!   加总没有语义，硬套一个 100% 断言只会逼出假约束。改为断言「单份额 ≤ 100%」。
//!
//! # 基准
//!
//! 唯一基准是 provider 的**推荐上下文长度**（缺则 `max_context_length × 60%`；
//! 两者都未配置 ⇒ `None`）。取值链只在
//! `ModelProviderConfig::recommended_context_length_tokens` 一处实现，本模块的
//! `capacity` 入参由调用方从**同一个入口**取（`think_loop::context_overflow_threshold`），
//! 这里不再算一遍比例。
//!
//! 容量未配 ⇒ [`tokens_of`] 返回 `None` ⇒ 调用方**不裁**（不猜、不拍兜底值），
//! 与 `faf9f52a` 已定的「未配不猜不裁」同一口径。

/// 各消费点占上下文容量的份额（basis points；1 bp = 0.01%）
pub(crate) mod share {
    /// 基准：10_000 bp = 100%
    pub(crate) const BASIS: u32 = 10_000;

    /// 中断兜底摘要的轮次正文预算（6%）
    ///
    /// @200K ⇒ 12_000 token，与改造前 `SUMMARY_BUDGET_CHARS = 12_000`
    /// （CJK 最坏情况 1 字符 = 1 token）逐位一致。
    pub(crate) const ABORT_SUMMARY_BP: u32 = 600;

    /// 中断摘要里「用户原始消息」的截断上限（2%）
    ///
    /// @200K ⇒ 4_000 token，对应改造前 `USER_MESSAGE_MAX_CHARS = 4_000`。
    pub(crate) const ABORT_USER_MESSAGE_BP: u32 = 200;

    /// 中断摘要里「错误详情」的截断上限（0.5%）
    ///
    /// @200K ⇒ 1_000 token，对应改造前 `ERROR_MESSAGE_MAX_CHARS = 1_000`。
    pub(crate) const ABORT_ERROR_MESSAGE_BP: u32 = 50;

    /// 中断摘要里「模型最后输出」的截断上限（1%）
    ///
    /// @200K ⇒ 2_000 token，对应改造前那句裸写 `truncate_chars(last, 2_000)`。
    pub(crate) const ABORT_LAST_OUTPUT_BP: u32 = 100;

    /// 单批 inbox 注入预算（1%）
    ///
    /// @200K ⇒ 2_000 token，对应改造前 `MAX_INJECTED_CHARS = 2_000`。
    pub(crate) const INBOX_BATCH_BP: u32 = 100;

    /// 轻量落档（`quick_turn`）的原文记账上限（0.6%）
    ///
    /// @200K ⇒ 1_200 token，对应改造前 `QUICK_TURN_HARD_CAP_CHARS = 1_200`。
    /// 同一份额**同时**是链路复杂度判据里「工具结果大到原文记账装不下」的阈值：
    /// 判据与截断同源，不允许各算一遍（否则判据会说谎）。
    pub(crate) const QUICK_TURN_HARD_CAP_BP: u32 = 60;
}

/// 按容量占比换算成 token 预算（容量未配 ⇒ `None` ＝ 不裁）
///
/// `.max(1)` 是**结构性下限**而非拍出来的整数：极小容量下占比会向下取整成 0，
/// 而「0 token 的预算」等于把内容整段删掉，与「未超限就不裁」自相矛盾。
pub(crate) fn tokens_of(capacity: Option<u64>, bp: u32) -> Option<usize> {
    let capacity = capacity?;
    let tokens = ((capacity as u128) * (bp as u128) / (share::BASIS as u128)) as usize;
    Some(tokens.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 改造前的字符值 @200K 容量逐位复现（CJK 最坏情况 1 字符 = 1 token）
    #[test]
    fn legacy_char_values_are_reproduced_at_200k() {
        let cap = Some(200_000);
        assert_eq!(
            tokens_of(cap, share::ABORT_SUMMARY_BP),
            Some(12_000),
            "SUMMARY_BUDGET_CHARS 12_000"
        );
        assert_eq!(
            tokens_of(cap, share::ABORT_USER_MESSAGE_BP),
            Some(4_000),
            "USER_MESSAGE_MAX_CHARS 4_000"
        );
        assert_eq!(
            tokens_of(cap, share::ABORT_ERROR_MESSAGE_BP),
            Some(1_000),
            "ERROR_MESSAGE_MAX_CHARS 1_000"
        );
        assert_eq!(
            tokens_of(cap, share::ABORT_LAST_OUTPUT_BP),
            Some(2_000),
            "中断前最后输出的裸 2_000"
        );
        assert_eq!(
            tokens_of(cap, share::INBOX_BATCH_BP),
            Some(2_000),
            "MAX_INJECTED_CHARS 2_000"
        );
        assert_eq!(
            tokens_of(cap, share::QUICK_TURN_HARD_CAP_BP),
            Some(1_200),
            "QUICK_TURN_HARD_CAP_CHARS 1_200"
        );
    }

    /// 预算随容量变（同一份额换两个容量的 provider ⇒ 预算不同）
    #[test]
    fn budget_follows_capacity() {
        let small = tokens_of(Some(64_000), share::ABORT_SUMMARY_BP);
        let large = tokens_of(Some(200_000), share::ABORT_SUMMARY_BP);
        assert_eq!(small, Some(3_840));
        assert_eq!(large, Some(12_000));
        assert_ne!(small, large);
    }

    /// 容量未配置 ⇒ `None` ⇒ 调用方不裁（不猜兜底值）
    #[test]
    fn missing_capacity_means_no_trim() {
        assert_eq!(tokens_of(None, share::ABORT_SUMMARY_BP), None);
        assert_eq!(tokens_of(None, share::INBOX_BATCH_BP), None);
    }

    /// 结构性下限：极小容量不返回 0（0 预算等于整段删掉）
    #[test]
    fn tiny_capacity_floors_at_one_token() {
        assert_eq!(tokens_of(Some(1), share::ABORT_ERROR_MESSAGE_BP), Some(1));
    }

    /// 单份额不越 100%（本模块各组是独立字段、不共池，故无总和硬顶；
    /// 但「单份额超过整个容量」一定是算错了）
    #[test]
    fn each_share_stays_within_basis() {
        for bp in [
            share::ABORT_SUMMARY_BP,
            share::ABORT_USER_MESSAGE_BP,
            share::ABORT_ERROR_MESSAGE_BP,
            share::ABORT_LAST_OUTPUT_BP,
            share::INBOX_BATCH_BP,
            share::QUICK_TURN_HARD_CAP_BP,
        ] {
            assert!(bp <= share::BASIS, "share {bp} bp exceeds 100%");
        }
    }
}
