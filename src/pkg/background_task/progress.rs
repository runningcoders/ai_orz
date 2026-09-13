//! 后台任务的细粒度进度计数（background_task 基建的一部分）
//!
//! 定位：**任务对象 ↔ 执行方（如 DAL 重建逻辑）之间的进度通道**。
//!
//! `BackgroundTask` 的进度快照是步骤级的（current_step/total_steps），
//! 当任务体内部委托 service 层分批执行时（如全量向量重建逐页处理），
//! 执行方需要一个不依赖任务对象类型的途径把「已处理多少条」喂回来。
//! 计数由任务对象的 `progress()` 拼进 `TaskProgressSnapshot` 展示。
//!
//! 为什么不直接把计数放在任务对象上：执行方在 `service` 层，任务在 `handlers` 层，
//! 执行方不能依赖 handler 类型。句柄放在 `background_task`（后台任务基建层），
//! 两侧都能引用，且内部为 `Arc` + 原子计数，跨 `await` 传递零拷贝、无锁竞争。
//!
//! 计数语义：**跨批次累计**，与进度条的 `current_step / total_steps`（步骤级）
//! 正交 —— 进度条表达「走到第几步」，计数表达「这一步/整体处理了多少条」。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 后台任务细粒度进度上报句柄（可克隆，共享同一份计数）
#[derive(Debug, Clone, Default)]
pub struct TaskProgressCounter {
    processed: Arc<AtomicUsize>,
}

impl TaskProgressCounter {
    /// 创建新的进度句柄（计数从 0 开始）
    pub fn new() -> Self {
        Self::default()
    }

    /// 已完成处理的条数（跨批次累计）
    pub fn processed(&self) -> usize {
        self.processed.load(Ordering::Relaxed)
    }

    /// 累加已处理条数（执行方每处理完一批后调用）
    ///
    /// `Relaxed` 足够：该计数只用于展示，不参与任何同步决策。
    pub fn advance(&self, count: usize) {
        if count > 0 {
            self.processed.fetch_add(count, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_handle_starts_at_zero() {
        assert_eq!(TaskProgressCounter::new().processed(), 0);
    }

    #[test]
    fn advance_accumulates_across_handles() {
        let progress = TaskProgressCounter::new();
        let cloned = progress.clone();

        progress.advance(3);
        cloned.advance(4);

        // 克隆共享同一份计数（同一任务的多个调用点看到一致进度）
        assert_eq!(progress.processed(), 7);
        assert_eq!(cloned.processed(), 7);
    }

    #[test]
    fn advance_ignores_zero() {
        let progress = TaskProgressCounter::new();
        progress.advance(0);
        assert_eq!(progress.processed(), 0);
    }
}
