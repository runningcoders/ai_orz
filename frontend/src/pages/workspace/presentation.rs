//! 语义 → 表现的映射（纯函数）
//!
//! 后端只回答「是什么状态」，本层回答「看起来像什么」：颜色、半径、是否高亮。
//! 全部由投影快照直接推导，不持有计时器 —— 无新事件时画面保持静止，不做装忙。
//! 颜色过渡 / 亮度相位等逐帧插值留给后续表现状态机，本层先保证口径唯一。

use common::api::{SceneAttention, SceneCategory};

/// 运行时状态 → 场景类别（前端补齐「选中但不在投影里」的 Agent 时使用）
///
/// 与后端投影的判定口径保持同色系；这里只做近似映射，不参与 attention 判定。
pub fn category_for_runtime(runtime_state: i32) -> SceneCategory {
    match runtime_state {
        2 => SceneCategory::Working,
        1 => SceneCategory::Resting,
        0 => SceneCategory::Idle,
        _ => SceneCategory::Offline,
    }
}

/// 颜色：类别色（远景光点与近景圆点共用同一色系）
pub fn color_for(category: SceneCategory) -> &'static str {
    match category {
        SceneCategory::Idle => "#64748b",
        SceneCategory::Working => "#f59e0b",
        SceneCategory::Resting => "#3b82f6",
        SceneCategory::Waiting => "#ef4444",
        SceneCategory::Stalled => "#f97316",
        SceneCategory::Offline => "#334155",
    }
}

/// 圆点半径：越需要我越大（同色系内靠尺寸分层）
pub fn radius_for(attention: SceneAttention) -> f64 {
    match attention {
        SceneAttention::NeedsYou => 30.0,
        SceneAttention::Info => 24.0,
        SceneAttention::None => 20.0,
    }
}

/// 类别短文案：LOD 最高层在名称下方展示（无状态标签时的兜底）
pub fn category_label(category: SceneCategory) -> &'static str {
    match category {
        SceneCategory::Idle => "空闲",
        SceneCategory::Working => "工作中",
        SceneCategory::Resting => "休息",
        SceneCategory::Waiting => "等你回话",
        SceneCategory::Stalled => "疑似停滞",
        SceneCategory::Offline => "未运行",
    }
}
