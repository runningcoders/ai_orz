//! 后台任务注册中心
//!
//! 全局 HashMap 存 `Arc<dyn BackgroundTask>`，提供注册/查询/列表/清理。
//! 任意层可通过 `registry()` 访问。

use crate::pkg::background_task::BackgroundTask;
use common::api::{TaskProgressSnapshot, TaskStatus, TaskType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 后台任务注册中心
pub struct BackgroundTaskRegistry {
    tasks: RwLock<HashMap<String, Arc<dyn BackgroundTask>>>,
}

impl BackgroundTaskRegistry {
    /// 创建新的注册中心
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// 注册任务，spawn 执行，返回 task_id
    ///
    /// 任务对象以 Arc 形式存储，spawn 的任务体持有 Arc 引用，
    /// 外部可通过 registry 查询进度。
    pub async fn register(&self, task: Arc<dyn BackgroundTask>) -> String {
        let task_id = task.task_id().to_string();
        {
            let mut guard = self.tasks.write().await;
            guard.insert(task_id.clone(), task.clone());
        }
        let task_clone = task.clone();
        tokio::spawn(async move {
            let result = task_clone.run().await;
            // run 内部应已设置 result 和 status
            // 这里做兜底：如果 run 返回 Err 但任务未标记 Failed，记录日志
            if let Err(e) = result {
                tracing::error!("后台任务 {} 执行失败: {}", task_clone.task_id(), e);
            }
        });
        task_id
    }

    /// 获取任务对象引用
    pub async fn get(&self, task_id: &str) -> Option<Arc<dyn BackgroundTask>> {
        let guard = self.tasks.read().await;
        guard.get(task_id).cloned()
    }

    /// 查询进度快照
    pub async fn get_progress(&self, task_id: &str) -> Option<TaskProgressSnapshot> {
        let guard = self.tasks.read().await;
        guard.get(task_id).map(|t| t.progress())
    }

    /// 列出指定类型的所有任务
    pub async fn list_by_type(&self, task_type: TaskType) -> Vec<Arc<dyn BackgroundTask>> {
        let guard = self.tasks.read().await;
        guard
            .values()
            .filter(|t| t.task_type() == task_type)
            .cloned()
            .collect()
    }

    /// 列出所有任务
    pub async fn list_all(&self) -> Vec<Arc<dyn BackgroundTask>> {
        let guard = self.tasks.read().await;
        guard.values().cloned().collect()
    }

    /// 列出指定类型的进度快照
    pub async fn list_progress_by_type(&self, task_type: TaskType) -> Vec<TaskProgressSnapshot> {
        let guard = self.tasks.read().await;
        guard
            .values()
            .filter(|t| t.task_type() == task_type)
            .map(|t| t.progress())
            .collect()
    }

    /// 列出所有任务的进度快照
    ///
    /// 遍历注册中心中所有任务，调用 `progress()` 获取快照。
    /// 任务数量不大时性能可接受。
    pub async fn list_all_progress(&self) -> Vec<TaskProgressSnapshot> {
        let guard = self.tasks.read().await;
        guard.values().map(|t| t.progress()).collect()
    }

    /// 清理已完成的旧任务，保留每个类型最近 max_count 个已完成任务
    ///
    /// 按 finished_at 降序排序，超出 max_count 的已完成/失败任务被移除。
    /// 运行中或等待中的任务不受影响。
    pub async fn cleanup_finished(&self, max_count: usize) {
        let mut guard = self.tasks.write().await;
        // 按 task_type 分组，每组保留最近 max_count 个已完成任务
        let mut by_type: HashMap<String, Vec<(String, i64)>> = HashMap::new();
        for (id, task) in guard.iter() {
            let p = task.progress();
            if p.status == TaskStatus::Completed || p.status == TaskStatus::Failed {
                let finished = p.finished_at.unwrap_or(0);
                by_type
                    .entry(p.task_type)
                    .or_default()
                    .push((id.clone(), finished));
            }
        }
        let mut to_remove = Vec::new();
        for (_type, mut list) in by_type {
            // 按完成时间降序排序
            list.sort_by(|a, b| b.1.cmp(&a.1));
            for (id, _) in list.into_iter().skip(max_count) {
                to_remove.push(id);
            }
        }
        for id in to_remove {
            guard.remove(&id);
        }
    }
}

impl Default for BackgroundTaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}
