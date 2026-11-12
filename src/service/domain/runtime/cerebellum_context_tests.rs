//! 单元测试（拆分自 cerebellum_context.rs）
//! 用 #[path] 挂载保住模块层级，use super::* 仍可见父模块私有项。

use super::*;

/// 原三键信息搬进正文后必须逐字还在（形态变更最容易的失血点）
#[test]
fn render_keeps_identity_values_verbatim() {
    let bundle = ContextBundle::new().with_identity("agent-7f3a", "技术负责人");
    let rendered = bundle.render();
    assert!(
        rendered.contains("## 会话身份"),
        "身份块标题必须出现：{rendered}"
    );
    assert!(
        rendered.contains("agent_id: agent-7f3a"),
        "agent_id 实际取值必须逐字出现在正文：{rendered}"
    );
    assert!(
        rendered.contains("agent_name: 技术负责人"),
        "agent_name 实际取值必须逐字出现在正文：{rendered}"
    );
}

/// 取不到上下文就整块跳过，不塞「标题 + 空 body」
#[test]
fn empty_dialogue_block_is_skipped_entirely() {
    let bundle = ContextBundle::new()
        .with_scene("wake_gate")
        .with_recent_dialogue(&[], RECENT_DIALOGUE_LIMIT, None);
    let rendered = bundle.render();
    assert!(
        rendered.contains("## 当前场景"),
        "有内容的块必须出现：{rendered}"
    );
    assert!(
        !rendered.contains("## 最近对话"),
        "空块标题不得出现（不是标题 + 空 body）：{rendered}"
    );
    assert!(!bundle.render().is_empty());
}

/// 摘要预算 = 输入预算 × `share::SNIPPET_BP`（换容量就换预算，不是固定整数）
#[test]
fn snippet_budget_is_a_share_of_the_input_budget() {
    assert_eq!(bp_of(4_000, share::SNIPPET_BP), 160, "@4000 ⇒ 4%");
    assert_eq!(bp_of(128_000, share::SNIPPET_BP), 5_120, "@128K ⇒ 4%");
    assert_eq!(bp_of(200_000, share::SNIPPET_BP), 8_000, "@200K ⇒ 4%");
    // 块内派生：加进总和会把「同一块数据分几段写」重复计费
    assert_eq!(share::TOTAL_BP, share::BASIS);
}

/// 摘要截断按 token 计：CJK 1 token/字、拉丁 4 字符/token —— 同一预算下两者能装的
/// **字符数**差 4 倍（旧字符口径下会一模一样，这正是本轮要删的那套体系）
#[test]
fn snippet_truncation_counts_tokens_not_chars() {
    let cjk = "缓".repeat(400);
    let cut = truncate_to_tokens(&cjk, 160, "");
    assert!(estimate_tokens(&cut) <= 160, "CJK 截断后必须守住预算");
    assert_eq!(cut.chars().count(), 160, "CJK：1 字 ≈ 1 token");

    let latin = "a".repeat(2_000);
    let cut = truncate_to_tokens(&latin, 160, "");
    assert!(estimate_tokens(&cut) <= 160, "拉丁截断后必须守住预算");
    assert_eq!(cut.chars().count(), 640, "拉丁：4 字符 ≈ 1 token");
}

/// 注册序即渲染序，两次调用逐字节相等
#[test]
fn render_is_deterministic_and_follows_registration_order() {
    let build = || {
        ContextBundle::new()
            .with_identity("a-1", "n-1")
            .with_scene("intent_gate")
            .with_user_message("你好")
    };
    let first = build().render();
    let second = build().render();
    assert_eq!(first, second, "同一组输入必须逐字节同输出");
    let identity_at = first.find("## 会话身份").expect("身份块");
    let scene_at = first.find("## 当前场景").expect("场景块");
    let message_at = first.find("## 当前用户消息").expect("消息块");
    assert!(
        identity_at < scene_at && scene_at < message_at,
        "渲染序必须等于注册序：{first}"
    );
}

/// 用户消息原文进正文（哨兵串逐字保留，供 gate 层反事实断言复用同一口径）
#[test]
fn user_message_goes_into_body_verbatim() {
    let bundle = ContextBundle::new().with_user_message("__SENTINEL_MSG__");
    assert!(bundle.render().contains("__SENTINEL_MSG__"));
}

/// 五块夹具：身份 / 场景 / 用户消息 / 附加块一 / 附加块二（注册序固定）
fn five_block_bundle() -> ContextBundle {
    let mut bundle = ContextBundle::new()
        .with_identity("agent-5b", "五块夹具")
        .with_scene("wake_gate")
        .with_user_message("__SENTINEL_FIVE__");
    bundle.push("附加块一", "alpha-block-body-one".to_string());
    bundle.push(
        "附加块二",
        "第五块正文用于让整块砍可判定，长度必须大于省略标注本身".to_string(),
    );
    bundle
}

/// 按 `\n\n` 切块后以 `## ` 开头的段落数（标注紧贴末块正文，不单独成段）
fn count_blocks(rendered: &str) -> usize {
    rendered
        .split("\n\n")
        .filter(|part| part.starts_with("## "))
        .count()
}

/// 每个 `## ` 块的正文都必须非空（不得出现无 body 的裸标题）
fn assert_every_block_has_body(rendered: &str) {
    for part in rendered.split("\n\n").filter(|p| p.starts_with("## ")) {
        let body = part.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
        assert!(!body.trim().is_empty(), "不得出现无 body 的裸标题：{part}");
    }
}

/// 容量为 `None`（未配小脑 provider / 未配上下限）⇒ 逐字节等于 `render()`
#[test]
fn render_within_none_matches_render_byte_for_byte() {
    let bundle = five_block_bundle();
    assert_eq!(bundle.render_within(None), bundle.render());
}

/// 容量充裕（远超全文）⇒ 同样逐字节等于 `render()`，且不带省略标注
#[test]
fn render_within_ample_capacity_matches_render_byte_for_byte() {
    let bundle = five_block_bundle();
    // 充裕容量 = 全文所需的两倍（按比例反算：body = capacity × 87.5% ⇒
    // capacity > 全文 / 0.875 即够，取两倍留足余量；不再依赖任何固定上限）
    let ample = (estimate_tokens(&bundle.render()) as u64 + 1) * 2;
    let rendered = bundle.render_within(Some(ample));
    assert_eq!(rendered, bundle.render());
    assert!(
        !rendered.contains(OMISSION_NOTE_PREFIX),
        "未截断时不得出现省略标注：{rendered}"
    );
}

/// 预算紧张时仍不得出现裸 `## 标题`（唯一结构判据）
#[test]
fn render_within_tight_budget_never_leaves_bare_title() {
    let bundle = five_block_bundle();
    // 恰好卡在「保留 2 块」的预算（更紧的预算会落到末块截尾分支）
    let keep_two = format!(
        "{}\n\n{}\n{}",
        "## 会话身份\nagent_id: agent-5b\nagent_name: 五块夹具",
        "## 当前场景\nwake_gate",
        "…（上下文已按模型容量截断，省略 3 块）"
    );
    let rendered = bundle.render_within_tokens(estimate_tokens(&keep_two));
    assert_every_block_has_body(&rendered);
    assert_eq!(count_blocks(&rendered), 2);
}

/// N 只认并列值（注册 5 / 保留 2 ⇒ N=3；反向 保留 4 ⇒ N=1），不接受只看下界
#[test]
fn omission_note_count_is_measured_not_hardcoded() {
    let bundle = five_block_bundle();
    let block0 = "## 会话身份\nagent_id: agent-5b\nagent_name: 五块夹具";
    let block1 = "## 当前场景\nwake_gate";
    let block2 = "## 当前用户消息\n__SENTINEL_FIVE__";
    let block3 = "## 附加块一\nalpha-block-body-one";

    let keep_two = format!("{block0}\n\n{block1}\n…（上下文已按模型容量截断，省略 3 块）");
    let rendered_two = bundle.render_within_tokens(estimate_tokens(&keep_two));
    assert_eq!(rendered_two, keep_two);
    assert_eq!(count_blocks(&rendered_two), 2, "注册 5 块、保留 2 块");

    let keep_four = format!(
        "{block0}\n\n{block1}\n\n{block2}\n\n{block3}\n…（上下文已按模型容量截断，省略 1 块）"
    );
    let rendered_four = bundle.render_within_tokens(estimate_tokens(&keep_four));
    assert_eq!(rendered_four, keep_four);
    assert_eq!(count_blocks(&rendered_four), 4, "注册 5 块、保留 4 块");
}

/// 预算 0 ⇒ 走末块截尾分支，标题后至少 1 个非空白字符（先自证该分支可达）
#[test]
fn zero_budget_keeps_one_visible_char_after_title() {
    let bundle = five_block_bundle();
    // R50 延伸：先自证「朴素截尾在 0 预算下确实为空」，否则本条走不到边界分支
    let naive = truncate_to_tokens(&bundle.sections[0].body, 0, "");
    assert!(
        naive.trim().is_empty(),
        "0 预算下朴素截尾必须为空，否则截尾分支不可达、本条判据无效"
    );

    let rendered = bundle.render_within_tokens(0);
    assert!(
        rendered.starts_with("## 会话身份\n"),
        "必须保标题：{rendered}"
    );
    let first = rendered
        .strip_prefix("## 会话身份\n")
        .and_then(|rest| rest.chars().next())
        .expect("标题后必须还有正文字符");
    assert!(
        !first.is_whitespace(),
        "截尾后 body 至少保留 1 个非空白字符：{rendered:?}"
    );
    assert_every_block_has_body(&rendered);
    assert!(
        rendered.contains(TAIL_TRUNCATION_NOTE) || rendered.contains(OMISSION_NOTE_PREFIX),
        "截尾必须留标注：{rendered}"
    );
}

/// 全部块都空 ⇒ 渲染为空串（调用方可据此判「没东西可带」）
#[test]
fn all_empty_blocks_render_empty() {
    let bundle = ContextBundle::new()
        .with_identity("", "")
        .with_scene("")
        .with_user_message("   ");
    assert_eq!(bundle.render(), "", "全空块必须渲染成空串");
}

/// 缺口 1（前台 01a1c645 裁定）：**截尾路径**也要跑一遍完整结构判据。
///
/// 砍块路径（`render_kept`）与截尾路径（保标题 + body 截尾）的拼装不一样，
/// #3 只覆盖了前者、#5 只断了首字符非空白 ⇒ 这里补「每段首行 `## ` 且非空行数 ≥ 2」。
#[test]
fn single_block_tail_truncation_passes_full_structure_criteria() {
    let bundle = ContextBundle::new().with_identity("agent-tail", "截尾夹具");
    assert_eq!(bundle.sections.len(), 1, "夹具必须恰一块才能走到 TAIL 分支");
    let rendered = bundle.render_within_tokens(0);
    for part in rendered.split("\n\n") {
        assert!(!part.trim().is_empty(), "不得出现空段落：{rendered:?}");
        let first = part.lines().next().unwrap_or("");
        assert!(first.starts_with("## "), "每块首行必须是二级标题：{part:?}");
        let non_empty = part.lines().filter(|line| !line.trim().is_empty()).count();
        assert!(non_empty >= 2, "每块至少标题 + 正文两行：{part:?}");
    }
    assert_every_block_has_body(&rendered);
}

/// 缺口 1（多块侧）：`total > 1` 的截尾走「省略 N 块」标注，同样要过结构判据。
#[test]
fn multi_block_tail_truncation_passes_full_structure_criteria() {
    let mut bundle = ContextBundle::new()
        .with_identity("agent-long", "长块夹具")
        .with_scene("wake_gate");
    // 首块正文撑到远超「第二块 + 标注」的成本 ⇒ 只有截尾分支能落地
    bundle.sections[0].body = "首块正文".repeat(200);
    let rendered = bundle.render_within_tokens(50);
    assert!(
        rendered.contains(&omission_note(1)),
        "被砍 1 块必须写成「省略 1 块」：{rendered:?}"
    );
    for part in rendered.split("\n\n") {
        assert!(!part.trim().is_empty(), "不得出现空段落：{rendered:?}");
        let first = part.lines().next().unwrap_or("");
        assert!(first.starts_with("## "), "每块首行必须是二级标题：{part:?}");
        let non_empty = part.lines().filter(|line| !line.trim().is_empty()).count();
        assert!(non_empty >= 2, "每块至少标题 + 正文两行：{part:?}");
    }
    assert_every_block_has_body(&rendered);
}

/// 缺口 2（前台裁定）：`TAIL_TRUNCATION_NOTE` 分支的语义断言 ——
/// 一块都没被砍，全文必须出现截尾标注、且**不得**残留「省略 N 块」。
#[test]
fn tail_truncation_note_never_claims_blocks_were_omitted() {
    let bundle = ContextBundle::new().with_identity("agent-tail", "截尾夹具");
    assert_eq!(bundle.sections.len(), 1, "夹具必须恰一块");
    let rendered = bundle.render_within_tokens(0);
    assert!(
        rendered.contains(TAIL_TRUNCATION_NOTE),
        "单块截尾必须用 TAIL_TRUNCATION_NOTE：{rendered:?}"
    );
    assert!(
        !rendered.contains("省略"),
        "没有任何块被砍，不得误标「省略」：{rendered:?}"
    );
}
