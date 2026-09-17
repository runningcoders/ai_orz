//! 节点卡片几何与文案（纯函数 SSOT，零业务依赖）
//!
//! 图谱节点是本项目里唯一"要往画布上塞信息"的元素，此前是圆形：只能显示
//! 10 个字 + 一个 ID，信息量几乎为零。改矩形卡片后画布上直接展示
//! 「名称 + 摘要/描述 + 标签」，ID 退到 hover 详情。
//!
//! ⚠️ 卡片尺寸 / 折行 / 截断 / 文案全部收敛在此文件：
//! - `canvas_scene::DefaultRenderer`（Canvas 路径，三处图谱共用）
//! - `graph::Graph`（SVG 兜底路径）
//!
//! 两条渲染路径都只许**引用**这里的常量与函数。各画一套必然漂移——
//! 改了宽度忘改折行宽度，文字就会溢出卡片；改了一边另一边命中检测错位。
//!
//! 本模块刻意不依赖任何 Dioxus / 图谱 / Canvas 类型，便于单测与复用。

// ==================== 尺寸常量 ====================

/// 卡片固定宽度（高度按内容浮动，见 [`box_height`]）
pub const NODE_BOX_W: f64 = 168.0;
/// 卡片最小宽度：无正文、无标签的纯名称卡片可以窄一些，
/// 免得工作台/关系图那种"只有工具名"的图铺满一屏卡片
pub const NODE_BOX_MIN_W: f64 = 104.0;
/// 卡片圆角
pub const NODE_BOX_R: f64 = 6.0;
/// 卡片内边距
pub const NODE_BOX_PAD: f64 = 8.0;
/// 左侧类型色竖条宽度（取代圆形的整体填充，保留类型辨识度）
pub const NODE_ACCENT_W: f64 = 4.0;
/// 标题字号 / 行高
pub const NODE_TITLE_PX: f64 = 12.0;
pub const NODE_TITLE_H: f64 = 15.0;
/// 正文字号 / 行高 / 最大行数
pub const NODE_BODY_PX: f64 = 10.0;
pub const NODE_BODY_H: f64 = 13.0;
pub const NODE_BODY_MAX_LINES: usize = 2;
/// 标签胶囊高度
pub const NODE_TAG_H: f64 = 14.0;
/// 卡片内标签最多展示个数（超出聚合为 `+N`）
pub const NODE_TAG_MAX: usize = 3;
/// hover 详情卡正文折行宽度（与 canvas_scene 的 11px 卡片字号配套）
pub const HOVER_TEXT_W: f64 = 280.0;
pub const HOVER_FONT_PX: f64 = 11.0;

/// 卡片内容区可用宽度（扣掉左侧竖条与左右内边距）
pub fn content_w() -> f64 {
    NODE_BOX_W - NODE_ACCENT_W - NODE_BOX_PAD * 2.0
}

/// 卡片高度：标题行 + 正文行 + 可选标签行
///
/// `body_line_count` 由 [`body_lines`] 的实际行数决定——知识节点有摘要/描述
/// 时是高卡片，工作台/关系图里只有名称的工具节点是矮卡片，同一个渲染器
/// 能自适应两种信息密度。
pub fn box_height(body_line_count: usize, has_tags: bool) -> f64 {
    let lines = body_line_count.min(NODE_BODY_MAX_LINES) as f64;
    let mut h = NODE_BOX_PAD * 2.0 + NODE_TITLE_H + NODE_BODY_H * lines;
    if has_tags {
        h += NODE_TAG_H + 3.0;
    }
    h
}

/// 卡片宽度：按标题与正文的实际内容收窄，夹在 [`NODE_BOX_MIN_W`] 与 [`NODE_BOX_W`] 之间
pub fn box_width(label: &str, body_line_count: usize) -> f64 {
    // 只有名称的卡片按名称裁剪宽度，避免"一个工具名占一张 168px 宽的卡"
    let want = if body_line_count == 0 {
        text_width(label, NODE_TITLE_PX) + NODE_ACCENT_W + NODE_BOX_PAD * 2.0 + 6.0
    } else {
        NODE_BOX_W
    };
    want.clamp(NODE_BOX_MIN_W, NODE_BOX_W)
}

// ==================== 文本度量与折行 ====================

/// 文本渲染宽度估算：CJK≈字号，ASCII≈0.55×字号
///
/// canvas 侧原本可用 `measure_text` 取真实度量，但 SVG 只能估算；
/// 两边共用这一套估算，卡片的折行结果才一致（否则同名节点在两种渲染下
/// 换行位置不同，高度也会跟着不一致）。
pub fn text_width(s: &str, font_px: f64) -> f64 {
    s.chars().fold(0.0, |acc, c| {
        acc + if c.is_ascii() {
            font_px * 0.55
        } else {
            font_px
        }
    })
}

/// 按像素宽度折行，最多 `max_lines` 行；超出时末行截断加省略号
pub fn wrap_text(s: &str, max_width: f64, font_px: f64, max_lines: usize) -> Vec<String> {
    // 正文常含换行/Markdown 换行，先摊平成单行再折，避免卡片里出现半截空行
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() || max_lines == 0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in flat.chars() {
        let probe: String = cur.chars().chain(std::iter::once(ch)).collect();
        if !cur.is_empty() && text_width(&probe, font_px) > max_width {
            lines.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            let mut t = last.clone();
            while !t.is_empty() && text_width(&format!("{t}…"), font_px) > max_width {
                t.pop();
            }
            t.push('…');
            *last = t;
        }
    }
    lines
}

/// 截断到 `max` 个字符（超出加省略号）
pub fn truncate_chars(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max - 1).collect();
    t.push('…');
    t
}

// ==================== 卡片文案 ====================

/// 卡片标题（展示名单行，超出省略）
pub fn title(label: &str) -> String {
    wrap_text(label, content_w(), NODE_TITLE_PX, 1)
        .pop()
        .unwrap_or_default()
}

/// 卡片正文行：摘要优先，回退正文描述；两者都空则返回空（渲染成矮卡片）
pub fn body_lines(summary: Option<&str>, description: &str) -> Vec<String> {
    let summary = summary.map(str::trim).unwrap_or("");
    let text = if summary.is_empty() {
        description.trim()
    } else {
        summary
    };
    if text.is_empty() {
        return Vec::new();
    }
    wrap_text(text, content_w(), NODE_BODY_PX, NODE_BODY_MAX_LINES)
}

/// 标签胶囊（最多 [`NODE_TAG_MAX`] 个，超出聚合为 `+N`）
///
/// 返回 (文字, 颜色)：颜色由 [`chip_color`] 按 tag 名稳定派生，
/// 同一个 tag 在画布、SVG、hover 详情卡三处必然同色。
pub fn tag_chips(tags: &[String]) -> Vec<(String, &'static str)> {
    if tags.is_empty() {
        return Vec::new();
    }
    let mut chips: Vec<(String, &'static str)> = tags
        .iter()
        .take(NODE_TAG_MAX)
        .map(|t| (t.clone(), chip_color(t)))
        .collect();
    let rest = tags.len().saturating_sub(NODE_TAG_MAX);
    if rest > 0 {
        chips.push((format!("+{rest}"), "#4b5563"));
    }
    chips
}

/// hover 详情行：名称 / 类型 / 标签 / 摘要 / 描述 / ID
///
/// ID 只在这里出现 —— 画布卡片上不再直出 ID。此前默认渲染器把完整 ID
/// 画在圆形节点下方，一屏全是 ulid，用户反馈「只显示了知识节点和它的 id」。
pub fn hover_lines(
    id: &str,
    label: &str,
    node_type: &str,
    tags: &[String],
    summary: Option<&str>,
    description: &str,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "名称: {}",
            if label.trim().is_empty() {
                "(未命名)"
            } else {
                label.trim()
            }
        ),
        format!("类型: {}", type_label(node_type)),
    ];
    if !tags.is_empty() {
        lines.push(format!("标签: {}", tags.join("、")));
    }
    let summary = summary.map(str::trim).unwrap_or("");
    if !summary.is_empty() {
        lines.extend(field_lines("摘要", summary));
    }
    let desc = description.trim();
    if !desc.is_empty() {
        lines.extend(field_lines("描述", desc));
    }
    lines.push(format!("ID: {id}"));
    lines
}

/// 长文本字段折成「字段名 + 折行正文」多行（供 hover 详情卡展示）
fn field_lines(name: &str, text: &str) -> Vec<String> {
    let mut lines = vec![format!("{name}:")];
    lines.extend(wrap_text(text, HOVER_TEXT_W, HOVER_FONT_PX, 3));
    lines
}

/// 节点类型 → 中文标签
///
/// 三处图谱（工作台拓扑 / Agent 关系图 / 知识图谱）的类型词表在此合并，
/// 此前 canvas_scene 与 graph.rs 各有一份、互不覆盖，同一个 kind 在两处
/// 显示成不同文案。未知类型原样返回（生命周期跟随入参，不造临时 String）。
pub fn type_label(t: &str) -> &str {
    match t {
        // 知识图谱
        "knowledge_node" => "知识节点",
        "short_term" => "短期记忆",
        "trace" => "调用记录",
        "relation" => "关系",
        // 工作台拓扑 / Agent 关系图
        "agent" => "Agent",
        "neural_tool" => "神经工具",
        "bound_tool" => "绑定工具",
        "pack_tool" => "工具包工具",
        "skill" | "neural_skill" => "技能",
        "project" => "项目",
        "task" => "任务",
        "" => "未知",
        other => other,
    }
}

// ==================== 标签胶囊配色 ====================

/// 标签胶囊预设色板（鲜艳且可区分）
pub const TAG_COLORS: &[&str] = &[
    "#ef4444", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#10b981", "#06b6d4", "#3b82f6",
    "#8b5cf6", "#ec4899",
];

/// 根据 tag 字符串 hash 稳定取色（同一 tag 全项目同色）
pub fn chip_color(tag: &str) -> &'static str {
    let hash: u32 = tag
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    TAG_COLORS[(hash as usize) % TAG_COLORS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_breaks_on_width() {
        // 内容区约 148px，12px 中文每字 12px → 一行约 12 字
        let lines = wrap_text(
            "一二三四五六七八九十甲乙丙丁",
            content_w(),
            NODE_TITLE_PX,
            2,
        );
        assert!(lines.len() >= 2, "超长文本应折行: {lines:?}");
        for l in &lines {
            assert!(
                text_width(l, NODE_TITLE_PX) <= content_w() + 0.01,
                "每行不得超出内容宽度: {l}"
            );
        }
    }

    #[test]
    fn wrap_text_ellipsis_on_overflow() {
        let lines = wrap_text(
            "一二三四五六七八九十甲乙丙丁戊己",
            content_w(),
            NODE_TITLE_PX,
            1,
        );
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].ends_with('…'),
            "超出末行应加省略号: {:?}",
            lines[0]
        );
    }

    #[test]
    fn wrap_text_flattens_newlines_to_space() {
        // 软换行摊平成空格（Markdown 语义）：直接删掉换行会让英文单词粘连
        let lines = wrap_text("第一行\n第二行", 1000.0, 10.0, 2);
        assert_eq!(lines, vec!["第一行 第二行".to_string()]);
    }

    #[test]
    fn body_lines_prefers_summary() {
        let lines = body_lines(Some("摘要内容"), "描述内容");
        assert_eq!(lines, vec!["摘要内容".to_string()]);
    }

    #[test]
    fn body_lines_falls_back_to_description() {
        let lines = body_lines(None, "描述内容");
        assert_eq!(lines, vec!["描述内容".to_string()]);
    }

    #[test]
    fn body_lines_empty_when_both_blank() {
        assert!(body_lines(None, "   ").is_empty());
        assert!(body_lines(Some("  "), "").is_empty());
    }

    #[test]
    fn box_height_grows_with_body_and_tags() {
        let bare = box_height(0, false);
        let with_body = box_height(2, false);
        let with_tags = box_height(2, true);
        assert!(bare < with_body, "有正文的卡片应更高");
        assert!(with_body < with_tags, "有标签的卡片应更高");
    }

    #[test]
    fn box_height_caps_body_lines() {
        // 超过最大行数不应无限增高（否则卡片会撑破画布）
        assert_eq!(box_height(9, false), box_height(NODE_BODY_MAX_LINES, false));
    }

    #[test]
    fn box_width_narrows_for_label_only() {
        let narrow = box_width("工具", 0);
        let wide = box_width("工具", 1);
        assert_eq!(wide, NODE_BOX_W, "有正文时用满宽");
        assert!(
            narrow < wide && narrow >= NODE_BOX_MIN_W,
            "纯名称卡片应收窄"
        );
    }

    #[test]
    fn tag_chips_aggregates_overflow() {
        let tags: Vec<String> = (0..5).map(|i| format!("t{i}")).collect();
        let chips = tag_chips(&tags);
        assert_eq!(chips.len(), NODE_TAG_MAX + 1);
        assert_eq!(chips.last().unwrap().0, "+2");
        assert_eq!(chips[0].1, chip_color("t0"), "胶囊颜色按 tag 稳定派生");
    }

    #[test]
    fn hover_lines_puts_id_last() {
        let lines = hover_lines(
            "01J8ZKQ7X4M2N5P6R8T0VWXYZ",
            "订单状态机",
            "knowledge_node",
            &["架构".to_string()],
            Some("摘要"),
            "描述",
        );
        assert_eq!(lines[0], "名称: 订单状态机");
        assert_eq!(lines[1], "类型: 知识节点");
        assert_eq!(lines[2], "标签: 架构");
        assert!(
            lines.last().unwrap().starts_with("ID: "),
            "ID 只出现在 hover 末行"
        );
    }

    #[test]
    fn hover_lines_marks_unnamed() {
        let lines = hover_lines("x", "  ", "unknown_kind", &[], None, "");
        assert_eq!(lines[0], "名称: (未命名)");
        assert_eq!(lines[1], "类型: unknown_kind");
    }

    #[test]
    fn type_label_covers_all_three_graphs() {
        assert_eq!(type_label("knowledge_node"), "知识节点");
        assert_eq!(type_label("agent"), "Agent");
        assert_eq!(type_label("project"), "项目");
        assert_eq!(type_label(""), "未知");
        assert_eq!(type_label("custom_thing"), "custom_thing");
    }

    #[test]
    fn chip_color_is_stable() {
        assert_eq!(chip_color("架构"), chip_color("架构"));
        assert!(TAG_COLORS.contains(&chip_color("架构")));
    }

    #[test]
    fn truncate_chars_adds_ellipsis() {
        assert_eq!(truncate_chars("abcdef", 3), "ab…");
        assert_eq!(truncate_chars("ab", 5), "ab");
    }
}
