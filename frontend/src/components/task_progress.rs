//! 通用后台任务进度条组件
//!
//! 接收 `TaskProgressSnapshot` 快照，统一渲染 Pending/Running/Completed/Failed 四种状态。
//! 复用 index.html 中已定义的 `.init-progress-*` 样式（spinner / failed 图标 / 文案 / 进度条）。
//!
//! 用法：
//! ```rust,ignore
//! use common::api::{TaskProgressSnapshot, TaskStatus};
//! use crate::components::task_progress::TaskProgress;
//!
//! let mut current_task = use_signal(|| Option::<TaskProgressSnapshot>::None);
//!
//! rsx! {
//!     if let Some(p) = &current_task() {
//!         TaskProgress {
//!             progress: p.clone(),
//!             on_cancel: Some(move |_| current_task.set(None)),
//!         }
//!     }
//! }
//! ```

use common::api::{TaskProgressSnapshot, TaskStatus};
use dioxus::prelude::*;

/// 任务进度条组件 Props
#[derive(Props, Clone, PartialEq)]
pub struct TaskProgressProps {
    /// 任务进度快照
    pub progress: TaskProgressSnapshot,
    /// 取消/返回回调（失败时显示返回按钮）
    #[props(default = None)]
    pub on_cancel: Option<EventHandler<()>>,
}

/// 通用任务进度条组件
///
/// - Pending/Running：spinner + "正在执行..."
/// - Completed：✓ + "任务完成"
/// - Failed：✗ + "任务失败" + 错误信息 + 返回按钮（如提供 on_cancel）
#[component]
pub fn TaskProgress(props: TaskProgressProps) -> Element {
    let p = &props.progress;
    let pct = if p.total_steps > 0 {
        (p.current_step as f64 / p.total_steps as f64 * 100.0) as usize
    } else {
        0
    };
    let is_failed = p.status == TaskStatus::Failed;
    let is_running = matches!(p.status, TaskStatus::Pending | TaskStatus::Running);
    let is_completed = p.status == TaskStatus::Completed;
    let has_cancel = props.on_cancel.is_some();

    rsx! {
        div { class: "init-progress-container",
            // 顶部图标
            if is_failed {
                div { class: "init-progress-icon failed", "✗" }
            } else if is_running {
                div { class: "init-progress-spinner" }
            } else if is_completed {
                div {
                    class: "init-progress-icon",
                    style: "background: oklch(0.7 0.2 145); width: 40px; height: 40px; border-radius: 50%; color: #fff; font-size: 1.5rem; font-weight: 700; display: flex; align-items: center; justify-content: center; margin-bottom: 0.5rem;",
                    "✓"
                }
            }

            // 标题
            h3 { class: "init-progress-title",
                if is_failed { "任务失败" }
                else if is_completed { "任务完成" }
                else { "正在执行..." }
            }

            // 步骤描述
            p { class: "init-progress-step", "{p.step_message}" }

            // 步骤计数
            p { class: "init-progress-count",
                "步骤 {p.current_step} / {p.total_steps}"
            }

            // DaisyUI 进度条
            progress {
                class: "progress progress-primary w-full",
                value: "{pct}",
                max: "100",
            }

            // 失败：错误信息 + 返回按钮
            if is_failed {
                if let Some(err) = &p.error {
                    p { class: "init-progress-error", "{err}" }
                }
                if has_cancel {
                    button {
                        class: "btn btn-outline btn-sm mt-4",
                        onclick: move |_| {
                            if let Some(on_cancel) = &props.on_cancel {
                                on_cancel.call(());
                            }
                        },
                        "返回"
                    }
                }
            }
        }
    }
}
