//! 数值展示格式化 - 紧凑读数用
//!
//! 工作台顶栏这类横条读数区不适合直接铺原始十进制大数（Token 合计轻易上百万，
//! 会把同行其它指标挤出去），统一走这里的紧凑格式，保证全站口径一致。

/// 整数紧凑格式（读数区统一入口）：`K` / `M` / `B` 三级进位。
///
/// 有效位数随量级收敛（`1234` → `1.23K`、`12345` → `12.3K`、`123456` → `123K`），
/// 小数部分末尾无意义的 0 会被去掉（`1200` → `1.2K`，不是 `1.20K`）。
///
/// 后缀的大小写与进位档位跟坐标轴刻度（[`format_compact_axis`]）**刻意保持同一套**：
/// 工作台顶栏与图表往往同屏出现，`1.2k` / `1.2K` 两种写法是最容易被一眼看出的不一致。
/// 卡片 / 徽标 / 表格单元格里的量级读数一律走这里，别再就地写 `{:.1}K` 这类局部实现
/// —— 它们普遍漏掉 `B`（十亿级会退化成 `1234.6M`，7 个字符）。
pub fn format_compact_count(n: u64) -> String {
    const K: f64 = 1_000.0;
    const M: f64 = 1_000_000.0;
    const B: f64 = 1_000_000_000.0;
    let v = n as f64;
    if v >= B {
        format_unit(v / B, "B")
    } else if v >= M {
        format_unit(v / M, "M")
    } else if v >= K {
        format_unit(v / K, "K")
    } else {
        n.to_string()
    }
}

/// 坐标轴刻度紧凑格式：`K` / `M` / `B` 三级进位，把字符数压在 5 个以内。
///
/// 与 [`format_compact_count`] 的差异只在**入参类型与小数值**，改动前先看清：
/// - 入参 `f64`：刻度由 `max * i / 4` 算出，天然是小数；QPS 这类小量纲还要能显示 `0.5`，
///   轴底恒为 0 也要能写成 `0` 而不是 `0.0`（白占一格）。
/// - 刻度贴在画布左缘，画布只给它约 34px（10px 字约 5 字符），溢出即顶破外框 ——
///   只进位到 `K` 的实现会让百万级刻度画出 `1234.6K`（7 字符）。
pub fn format_compact_axis(v: f64) -> String {
    const K: f64 = 1_000.0;
    const M: f64 = 1_000_000.0;
    const B: f64 = 1_000_000_000.0;
    if v >= B {
        format_unit(v / B, "B")
    } else if v >= M {
        format_unit(v / M, "M")
    } else if v >= K {
        format_unit(v / K, "K")
    } else if v >= 10.0 {
        format!("{:.0}", v)
    } else if v > 0.0 {
        // 小数值（如 Token QPS）保留一位小数，避免刻度清一色显示 0
        format!("{:.1}", v)
    } else {
        // 轴底恒为 0：写成 `0.0` 只会白占一格宽度
        "0".to_string()
    }
}

/// 定点小数格式，去掉无意义的尾随 0（`12.0` → `12`、`0.50` → `0.5`）。
pub fn format_decimal(v: f64, digits: usize) -> String {
    trim_decimal(format!("{:.*}", digits, v))
}

/// 匹配相关度百分比：`0.0~1.0` 的相关度 → `0% ~ 100%`。
///
/// ⚠️ 别就地写 `format!("{:.4}", score)`：那会吐出 `0.4723` 这种没有单位、
/// 也没有方向的裸数字，用户读成「匹配度只有 0.47，很低」，而字段语义其实是
/// **越大越相关**（后端 `MemoryResult::score` 已由向量距离换算成相关度）。
/// 图谱详情、记忆列表、记忆检索三处的相关度读数一律走这里。
pub fn format_relevance(score: f32) -> String {
    format!("{:.0}%", score.clamp(0.0, 1.0) * 100.0)
}

/// 量级自适应有效位：>=100 取整、>=10 保留 1 位、其余保留 2 位，再并入单位后缀。
fn format_unit(v: f64, suffix: &str) -> String {
    let text = if v >= 100.0 {
        format!("{:.0}", v)
    } else if v >= 10.0 {
        format!("{:.1}", v)
    } else {
        format!("{:.2}", v)
    };
    format!("{}{}", trim_decimal(text), suffix)
}

/// 去掉小数部分的尾随 0。
///
/// 没有小数点时原样返回——否则 `120` 会被误剪成 `12`。
fn trim_decimal(text: String) -> String {
    match text.split_once('.') {
        Some((int_part, frac)) => {
            let frac = frac.trim_end_matches('0');
            if frac.is_empty() {
                int_part.to_string()
            } else {
                format!("{}.{}", int_part, frac)
            }
        }
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_count_below_thousand_is_plain() {
        assert_eq!(format_compact_count(0), "0");
        assert_eq!(format_compact_count(999), "999");
    }

    #[test]
    fn compact_count_scales_by_magnitude() {
        assert_eq!(format_compact_count(1_000), "1K");
        assert_eq!(format_compact_count(1_200), "1.2K");
        assert_eq!(format_compact_count(1_234), "1.23K");
        assert_eq!(format_compact_count(12_345), "12.3K");
        assert_eq!(format_compact_count(123_456), "123K");
        assert_eq!(format_compact_count(1_234_567), "1.23M");
        assert_eq!(format_compact_count(12_000_000), "12M");
    }

    #[test]
    fn compact_count_scales_to_billions() {
        // 回归：只到 M 的实现会让十亿级读数退化成 `1234.6M`（7 字符）
        assert_eq!(format_compact_count(1_234_567_890), "1.23B");
        assert_eq!(format_compact_count(12_000_000_000), "12B");
    }

    #[test]
    fn compact_count_and_axis_share_unit_spelling() {
        // 同屏一致性：顶栏读数与图表刻度必须用同一套后缀，不能一个 `k` 一个 `K`
        assert_eq!(format_compact_count(1_234), format_compact_axis(1_234.0));
        assert_eq!(
            format_compact_count(1_234_567),
            format_compact_axis(1_234_567.0)
        );
    }

    #[test]
    fn compact_axis_scales_to_billions() {
        // 回归：只进位到 K 时，百万级刻度画成 `1234.6K`（7 字符）顶破画布左缘
        assert_eq!(format_compact_axis(1_234_567.0), "1.23M");
        assert_eq!(format_compact_axis(12_000_000.0), "12M");
        assert_eq!(format_compact_axis(123_456_789.0), "123M");
        assert_eq!(format_compact_axis(1_234_567_890.0), "1.23B");
    }

    #[test]
    fn compact_axis_keeps_small_values_readable() {
        assert_eq!(format_compact_axis(0.0), "0");
        assert_eq!(format_compact_axis(0.5), "0.5");
        assert_eq!(format_compact_axis(9.94), "9.9");
        assert_eq!(format_compact_axis(999.0), "999");
        assert_eq!(format_compact_axis(1_500.0), "1.5K");
    }

    #[test]
    fn relevance_is_percentage_and_clamped() {
        assert_eq!(format_relevance(1.0), "100%");
        assert_eq!(format_relevance(0.0), "0%");
        assert_eq!(format_relevance(0.4723), "47%");
        // 越界值夹紧，不出现 120% / -5%
        assert_eq!(format_relevance(1.4), "100%");
        assert_eq!(format_relevance(-0.1), "0%");
    }

    #[test]
    fn decimal_keeps_significant_digits_only() {
        assert_eq!(format_decimal(12.0, 1), "12");
        assert_eq!(format_decimal(0.5, 1), "0.5");
        assert_eq!(format_decimal(1234.567, 1), "1234.6");
        assert_eq!(format_decimal(1234.567, 0), "1235");
    }
}
