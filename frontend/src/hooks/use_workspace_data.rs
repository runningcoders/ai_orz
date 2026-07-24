//! Workspace 工作台数据加载 hook
//!
//! 封装 list_projects + list_agents + list_tasks 的调用和 Signal 管理。
//! 提供 refresh() 方法供定时刷新。

use dioxus::prelude::*;
use common::api::{AgentListItem, ProjectListItem, TaskListItem};
use crate::api::hr::list_agents;
use crate::api::project::{list_projects, list_tasks};
use crate::store::toast::use_toast;

/// Workspace 工作台数据
#[derive(Debug, Clone, Default)]
pub struct WorkspaceData {
    pub projects: Vec<ProjectListItem>,
    pub agents: Vec<AgentListItem>,
    pub tasks: Vec<TaskListItem>,
}

/// Workspace 数据加载 hook
///
/// 返回 (data_signal, refresh_fn)，调用方可在 use_effect 中触发首次加载，
/// 也可在交互时调用 refresh() 重新加载。
pub fn use_workspace_data() -> (Signal<Option<WorkspaceData>>, impl FnMut()) {
    let mut data: Signal<Option<WorkspaceData>> = use_signal(|| None);
    let toast = use_toast();

    let load = move || {
        spawn(async move {
            let projects = list_projects().await.map(|r| r.projects).unwrap_or_default();
            let agents = list_agents().await.map(|r| r.agents).unwrap_or_default();
            // 加载所有任务（不传过滤参数）
            let tasks = list_tasks(None, None, None, None).await.map(|r| r.tasks).unwrap_or_default();

            // 失败时 toast 提示但不阻断
            if projects.is_empty() && agents.is_empty() {
                toast.info("暂无 Project 和 Agent 数据");
            }

            data.set(Some(WorkspaceData { projects, agents, tasks }));
        });
    };

    // 首次加载
    use_effect(move || {
        load();
    });

    // 返回 refresh 函数
    (data, move || { load(); })
}
