//! AOP 消费者仪表盘组件（基于通用 Gauge 组件）
//!
//! 在通用 Gauge 之上封装 AOP 专属：
//! - 颜色编码逻辑（基于 pending/in_progress 状态）
//! - AOP 专属字段映射（oldest_age_secs → footer、order_keys_count → footer）
//!
//! 点击 canvas 触发 on_click 回调（用于切换事件列表）

use dioxus::prelude::*;

use crate::components::gauge::Gauge;

/// AopGauge 组件 Props（保持向后兼容）
#[derive(Props, Clone)]
pub struct AopGaugeProps {
    /// 消费者名称
    pub consumer_name: String,
    /// 待处理事件数
    pub pending: usize,
    /// 处理中事件数
    pub in_progress: usize,
    /// 最老事件年龄（秒），None 表示无事件
    pub oldest_age_secs: Option<u64>,
    /// order_keys 数量
    pub order_keys_count: usize,
    /// 是否选中（加强发光边框）
    pub is_selected: bool,
    /// 点击回调
    pub on_click: Option<EventHandler<()>>,
}

// EventHandler 无法比较，手动实现 PartialEq 时忽略 on_click 字段
impl PartialEq for AopGaugeProps {
    fn eq(&self, other: &Self) -> bool {
        self.consumer_name == other.consumer_name
            && self.pending == other.pending
            && self.in_progress == other.in_progress
            && self.oldest_age_secs == other.oldest_age_secs
            && self.order_keys_count == other.order_keys_count
            && self.is_selected == other.is_selected
    }
}

/// 根据队列状态获取主色
fn status_color(pending: usize, in_progress: usize) -> &'static str {
    if pending >= 10 {
        "#ef4444" // 红色：堆积告警
    } else if pending > 0 {
        "#fa520f" // 橙色：正常负载
    } else if in_progress > 0 {
        "#f59e0b" // 黄色：处理中
    } else {
        "#10b981" // 绿色：idle 健康
    }
}

/// AopGauge 组件（薄 wrap Gauge）
#[component]
pub fn AopGauge(props: AopGaugeProps) -> Element {
    let color = status_color(props.pending, props.in_progress).to_string();
    let center_value = if props.pending == 0 && props.in_progress == 0 {
        "OK".to_string()
    } else {
        props.pending.to_string()
    };
    let badge = if props.in_progress > 0 {
        Some(format!("⚙ {}", props.in_progress))
    } else {
        None
    };
    let mut footer = format!("{} order_keys", props.order_keys_count);
    if let Some(age) = props.oldest_age_secs {
        footer.push_str(&format!(" · {}s ago", age));
    }

    rsx! {
        Gauge {
            title: props.consumer_name.clone(),
            center_value,
            center_label: "pending".to_string(),
            color,
            badge,
            footer: Some(footer),
            is_selected: props.is_selected,
            on_click: props.on_click,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_color_idle() {
        assert_eq!(status_color(0, 0), "#10b981");
    }

    #[test]
    fn test_status_color_processing() {
        assert_eq!(status_color(0, 5), "#f59e0b");
    }

    #[test]
    fn test_status_color_normal_load() {
        assert_eq!(status_color(5, 0), "#fa520f");
        assert_eq!(status_color(9, 3), "#fa520f");
    }

    #[test]
    fn test_status_color_overload() {
        assert_eq!(status_color(10, 0), "#ef4444");
        assert_eq!(status_color(100, 5), "#ef4444");
    }
}
