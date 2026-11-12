//! 单元测试（拆分自 cerebellum_context.rs）
//! 用 #[path] 挂载保住模块层级，use super::* 仍可见父模块私有项。

use super::*;

/// 占比之和必须 ≤ 硬顶，且当前表**恰好用满**（留空等于又拍了一个隐式预留）
#[test]
fn shares_sum_never_exceeds_the_hard_ceiling() {
    assert!(
        validate_share_sum(share::TOTAL_BP).is_ok(),
        "占比之和 {} bp 超过硬顶 {} bp",
        share::TOTAL_BP,
        share::BASIS
    );
    assert_eq!(share::TOTAL_BP, share::BASIS, "各块之和必须恰好 100%");
}

/// 反事实：把占比调成 >100% 必须被捕获，不允许静默超限
#[test]
fn over_allocated_shares_are_rejected_not_silently_accepted() {
    assert!(validate_share_sum(share::BASIS).is_ok(), "恰好 100% 合法");
    assert!(
        validate_share_sum(share::BASIS + 1).is_err(),
        "100.01% 必须被拒，否则超限会静默通过"
    );
    assert!(
        validate_share_sum(share::BASIS + share::FRAME_BP).is_err(),
        "任意超限组合同样必须被拒（不只 BASIS+1 这一个点）"
    );
}

/// 骨架预留随输入预算变化 —— 改造前是固定常量 500（两个输入恒同值）⇒ 本断言改前必红
#[test]
fn frame_budget_scales_with_input_not_a_fixed_constant() {
    assert_eq!(frame_tokens(2_000), 250, "小输入按比例：2000 × 12.5%");
    assert_eq!(
        frame_tokens(4_000),
        500,
        "标准输入：4000 × 12.5% = 实测定标上限"
    );
    assert_eq!(frame_tokens(8_000), 1_000, "大输入同样按比例：8000 × 12.5%");
    assert_eq!(
        frame_tokens(200_000),
        25_000,
        "不再有绝对封顶：骨架随容量线性增长（比例是唯一口径）"
    );
    assert_ne!(
        frame_tokens(2_000),
        frame_tokens(4_000),
        "同一位置换两个容量的 provider ⇒ 预算必须不同（固定常量的反事实）"
    );
}

/// 骨架 + 正文恒不超过输入（AMan 原话「只要最终不超过最大长度就可以」）
#[test]
fn frame_plus_body_never_exceeds_input() {
    for input in [0usize, 1, 37, 110, 500, 1_088, 4_000, 200_000, usize::MAX] {
        let frame = frame_tokens(input);
        let body = body_budget_tokens(input);
        assert!(
            frame.saturating_add(body) <= input,
            "input={input}: frame {frame} + body {body} 超过输入预算"
        );
    }
}

/// 待发消息是同一池里的份额，不是新增预算池
#[test]
fn pending_reply_is_a_share_of_the_same_pool() {
    let input = 4_000;
    let body = body_budget_tokens(input);
    let pending = pending_reply_tokens(input, body);
    assert_eq!(pending, 600, "4000 × 15%");
    assert!(pending < body, "待发预算必须小于正文池");
    assert_eq!(
        pending_reply_tokens(input, 100),
        25,
        "正文池极小时由 1/4 收敛"
    );
    assert_ne!(
        pending_reply_tokens(2_000, 1_750),
        pending_reply_tokens(4_000, 3_500),
        "待发预算必须随输入变化，不是固定 600"
    );
}

/// provider 未配容量 ⇒ 回退路径仍可用（不炸、不改语义）
#[test]
fn none_capacity_falls_back_to_full_render() {
    let bundle = ContextBundle::new()
        .with_identity("agent-1", "技术负责人")
        .with_scene("wake_gate")
        .with_user_message("用户消息");
    assert_eq!(bundle.render_within(None), bundle.render());
}
