//! 关系强度 → 边的视觉映射（中立 SSOT）
//!
//! 关系边带一个可选的「强度」（0.0~1.0，由产出关系的一方声明），图谱按它调
//! **粗细与浓淡**；两条渲染路径（Canvas 的 `canvas_scene` / SVG 的 `graph`）
//! 都只许引用这里的映射，不许各自写一套系数 —— 两处各写一份的结果是同一个
//! 强度在两个视图里粗细不同，用户会以为数据变了。
//!
//! # 为什么是「粗细 + 浓淡」而不是「颜色」
//! 边的颜色已经被关系类型/状态占用（`tag_color` 按 tag 哈希取色、关系图的
//! ready/not_ready 语义色）。再拿颜色表达强度就会和语义色打架，同一个色相
//! 到底代表「类型」还是「强度」说不清。所以强度走粗细与不透明度这两个还没
//! 被占用的通道。
//!
//! # 未标注（`None`）必须与「强度 0」区分
//! `None` 渲染**基准粗细**（`EDGE_BASE_WIDTH`），不是最细；`Some(0.0)` 才是
//! 最细最淡。存量关系没有强度，若把 `None` 当成 0，整张图会统一塌到最细，
//! 看起来像「所有关系都很弱」——那是伪造出来的语义。

/// 未标注强度时的线宽（也是所有边的视觉基准）
pub const EDGE_BASE_WIDTH: f64 = 1.5;

/// 已标注强度时的线宽区间（下界比基准细一点，上界明显更粗）
const WEIGHTED_MIN_WIDTH: f64 = 1.1;
const WEIGHTED_MAX_WIDTH: f64 = 3.8;

/// 已标注强度时的不透明度系数区间（乘在原本的边色透明度上）
const WEIGHTED_MIN_ALPHA_SCALE: f64 = 0.45;

/// 强度 → `(线宽, 不透明度系数)`
///
/// - `None`（未标注）→ `(EDGE_BASE_WIDTH, 1.0)`：基准粗细、原色浓淡
/// - `Some(w)` → 越强越粗越实，越弱越细越淡；越界值夹紧到 0.0~1.0
pub fn weight_style(weight: Option<f32>) -> (f64, f64) {
    match normalize(weight) {
        None => (EDGE_BASE_WIDTH, 1.0),
        Some(w) => (
            WEIGHTED_MIN_WIDTH + (WEIGHTED_MAX_WIDTH - WEIGHTED_MIN_WIDTH) * w,
            WEIGHTED_MIN_ALPHA_SCALE + (1.0 - WEIGHTED_MIN_ALPHA_SCALE) * w,
        ),
    }
}

/// 强度在 hover 提示里的读数文案；未标注返回 `None`（整行不渲染）
///
/// ⚠️ 未标注**不能**渲染成「强度 0%」—— 那是「明确很弱」，与「没人标过」
/// 不是一回事，用户会据此误判数据质量。缺省就是缺省，不显示。
pub fn weight_label(weight: Option<f32>) -> Option<String> {
    let w = normalize(weight)?;
    let pct = (w * 100.0).round() as i32;
    Some(format!("强度: {pct}%"))
}

/// 归一化：非有限值（NaN/Inf）视为**未标注**，越界值夹紧
///
/// 与后端 `KnowledgeRelationParam::normalized_weight` 同一套规则 ——
/// 脏数据（NaN）渲染成基准粗细，而不是塌成「最弱」或让 `round()` 吐出 0%。
fn normalize(weight: Option<f32>) -> Option<f64> {
    weight
        .filter(|w| w.is_finite())
        .map(|w| (w as f64).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_weight_renders_baseline_not_thinnest() {
        let (w, a) = weight_style(None);
        assert_eq!(w, EDGE_BASE_WIDTH);
        assert_eq!(a, 1.0);
        // 基准必须比「明确最弱」粗：否则存量关系会被读成「所有关联都很弱」
        let (weakest, _) = weight_style(Some(0.0));
        assert!(w > weakest);
        assert_eq!(weight_label(None), None, "未标注不显示读数");
    }

    #[test]
    fn stronger_weight_is_thicker_and_more_opaque() {
        let (w_low, a_low) = weight_style(Some(0.2));
        let (w_mid, a_mid) = weight_style(Some(0.5));
        let (w_high, a_high) = weight_style(Some(0.9));
        assert!(w_low < w_mid && w_mid < w_high, "粗细必须单调");
        assert!(a_low < a_mid && a_mid < a_high, "浓淡必须单调");
    }

    #[test]
    fn out_of_range_weight_is_clamped() {
        assert_eq!(weight_style(Some(-3.0)), weight_style(Some(0.0)));
        assert_eq!(weight_style(Some(9.0)), weight_style(Some(1.0)));
        assert_eq!(weight_label(Some(2.0)).as_deref(), Some("强度: 100%"));
    }

    #[test]
    fn non_finite_weight_counts_as_unset() {
        // 脏数据不能塌成「最弱」（那会把 NaN 说成一个强度）
        assert_eq!(weight_style(Some(f32::NAN)), weight_style(None));
        assert_eq!(weight_style(Some(f32::INFINITY)), weight_style(None));
        assert_eq!(weight_label(Some(f32::NAN)), None);
    }

    #[test]
    fn weight_label_reads_as_percentage() {
        assert_eq!(weight_label(Some(0.8)).as_deref(), Some("强度: 80%"));
        assert_eq!(weight_label(Some(0.0)).as_deref(), Some("强度: 0%"));
        assert_eq!(weight_label(Some(1.0)).as_deref(), Some("强度: 100%"));
    }
}
