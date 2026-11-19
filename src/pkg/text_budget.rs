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
//! - 本模块：一颗份额表管两件事——①**同池总和**：abort 摘要的四段落在同一份
//!   detail 文档里叠加，份额和由编译期断言钉住（见文件末尾 `const _`）；②**跨路径
//!   不加总**：中止摘要 / inbox 注入 / 原文记账 / 词表 / 意图区块分属互斥路径，
//!   不会同时落进同一个池 ⇒ 对它们造「全局总额」只是假约束。prompt 内区块
//!   （词表 + 意图条目 + 意图摘要）在上层 builder 侧另有一条编译期断言。
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

    /// 本体词表注入预算（1.5%）
    ///
    /// @200K ⇒ 3_000 token，对应改造前 `LEXICON_PROMPT_BUDGET_CHARS = 3_000`。
    /// 预置词表全量约 2k 字符 ⇒ 正常全量注入；管理员扩表超预算时按
    /// 「关系词 > 实体类 > 同义样例」优先级裁剪（design §3）。
    pub(crate) const LEXICON_PROMPT_BP: u32 = 150;

    /// 意图分析区块里**单条**条目（关键词 / 消歧 / 检索摘要 / 澄清问题）上限（0.08%）
    ///
    /// @200K ⇒ 160 token。改造前是 `MAX_ITEM_CHARS = 150`（字符口径）——
    /// 两者在 200K 上**不逐位相等**（150 → 160，+10 的位移），口径是「量级对齐」，
    /// 不是「逐位复现」：字符口径下同一行中文算 150 token、英文只算 37 token，
    /// 本来就没有唯一等价值。
    pub(crate) const INTENT_ITEM_BP: u32 = 8;

    /// 意图分析区块的「一句话理解总结」上限（0.4%）
    ///
    /// @200K ⇒ 800 token，对应改造前 `MAX_SUMMARY_CHARS = 800`。
    pub(crate) const INTENT_SUMMARY_BP: u32 = 40;

    /// discover 轨迹**单行**的内容预算（0.1%）
    ///
    /// @200K ⇒ 200 token，对应改造前 `TRAIL_SUMMARY_CHARS = 200`。
    pub(crate) const TRAIL_SUMMARY_BP: u32 = 10;

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

/// 同池总和自校验（编译期，零运行时成本）：abort 摘要的四段
/// **同处一份 detail 文档**、互不重复扣减，份额和超 100% 说明份额表被改坏。
///
/// 只钉「同一份文档内」的和；`INBOX_BATCH_BP` / `QUICK_TURN_HARD_CAP_BP` /
/// `LEXICON_PROMPT_BP` / intent 两块属互斥路径，不与本池相加。
const _: () = assert!(
    share::ABORT_SUMMARY_BP
        + share::ABORT_USER_MESSAGE_BP
        + share::ABORT_ERROR_MESSAGE_BP
        + share::ABORT_LAST_OUTPUT_BP
        <= share::BASIS
);

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

    /// prompt 侧三项 + discover 轨迹行 @200K 的取值（口径对齐改造前字符值）
    ///
    /// `INTENT_ITEM_BP` 是**量级对齐不是逐位复现**（150 字符 → 160 token），
    /// 单独分组，不许混进上面那组「逐位复现」断言里。
    #[test]
    fn prompt_side_shares_are_reproduced_at_200k() {
        let cap = Some(200_000);
        assert_eq!(tokens_of(cap, share::LEXICON_PROMPT_BP), Some(3_000));
        assert_eq!(tokens_of(cap, share::INTENT_SUMMARY_BP), Some(800));
        assert_eq!(tokens_of(cap, share::TRAIL_SUMMARY_BP), Some(200));
        // 量级对齐（+10 位移），不是逐位复现
        assert_eq!(tokens_of(cap, share::INTENT_ITEM_BP), Some(160));
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
            share::LEXICON_PROMPT_BP,
            share::INTENT_ITEM_BP,
            share::INTENT_SUMMARY_BP,
            share::TRAIL_SUMMARY_BP,
        ] {
            assert!(bp <= share::BASIS, "share {bp} bp exceeds 100%");
        }
    }
}
