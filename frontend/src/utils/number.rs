//! 数值展示格式化 - 紧凑读数用
//!
//! 工作台顶栏这类横条读数区不适合直接铺原始十进制大数（Token 合计轻易上百万，
//! 会把同行其它指标挤出去），统一走这里的紧凑格式，保证全站口径一致。

/// 整数紧凑格式：1000 进位到 `k`，1_000_000 进位到 `M`。
///
/// 有效位数随量级收敛（`1234` → `1.23k`、`12345` → `12.3k`、`123456` → `123k`），
/// 小数部分末尾无意义的 0 会被去掉（`1200` → `1.2k`，不是 `1.20k`）。
pub fn format_compact_count(n: u64) -> String {
    const K: f64 = 1_000.0;
    const M: f64 = 1_000_000.0;
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format_unit(n as f64 / K, "k")
    } else {
        format_unit(n as f64 / M, "M")
    }
}

/// 定点小数格式，去掉无意义的尾随 0（`12.0` → `12`、`0.50` → `0.5`）。
pub fn format_decimal(v: f64, digits: usize) -> String {
    trim_decimal(format!("{:.*}", digits, v))
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
        assert_eq!(format_compact_count(1_000), "1k");
        assert_eq!(format_compact_count(1_200), "1.2k");
        assert_eq!(format_compact_count(1_234), "1.23k");
        assert_eq!(format_compact_count(12_345), "12.3k");
        assert_eq!(format_compact_count(123_456), "123k");
        assert_eq!(format_compact_count(1_234_567), "1.23M");
        assert_eq!(format_compact_count(12_000_000), "12M");
    }

    #[test]
    fn decimal_keeps_significant_digits_only() {
        assert_eq!(format_decimal(12.0, 1), "12");
        assert_eq!(format_decimal(0.5, 1), "0.5");
        assert_eq!(format_decimal(1234.567, 1), "1234.6");
        assert_eq!(format_decimal(1234.567, 0), "1235");
    }
}
