//! Workspace 工作台侧边栏数据加载 hook
//!
//! 只加载侧边栏所需的 projects + agents 列表（轻量）。
//! 中心图的 tasks 数据由 workspace.rs 按视图按需加载。
//!
//! 数据采用「挂载立即加载 + 周期轮询」模式：后台异步自动唤醒会让
//! Agent 运行态在 Idle/Resting/Busy 间变化，侧边栏与顶栏忙碌卡
//! 依赖本数据，需周期刷新与后端内存态保持一致。

use crate::api::hr::list_agents;
use crate::api::project::list_projects;
use crate::store::toast::{ToastState, use_toast};
use common::api::{AgentListItem, ListAgentsRequest, ListProjectsRequest, ProjectListItem};
use dioxus::prelude::*;

/// 侧边栏数据轮询间隔（毫秒），与 workspace 页 DuckDB 读数轮询节奏一致
const POLL_INTERVAL_MS: u32 = 30_000;

/// Workspace 侧边栏数据
#[derive(Debug, Clone, Default)]
pub struct WorkspaceData {
    pub projects: Vec<ProjectListItem>,
    pub agents: Vec<AgentListItem>,
}

/// 拉取一次侧边栏数据并写入 `data`。
///
/// 「暂无 Project 和 Agent 数据」提示只在首次加载（`data` 尚为 None）且
/// 结果为空时弹出一次；后续轮询与手动刷新不重复弹窗。
fn spawn_workspace_load(mut data: Signal<Option<WorkspaceData>>, toast: ToastState) {
    let is_first = data.read().is_none();
    spawn(async move {
        let projects = list_projects(ListProjectsRequest::default())
            .await
            .map(|r| r.items)
            .unwrap_or_default();
        let agents = list_agents(ListAgentsRequest::default())
            .await
            .map(|r| r.items)
            .unwrap_or_default();

        if is_first && projects.is_empty() && agents.is_empty() {
            toast.info("暂无 Project 和 Agent 数据");
        }

        data.set(Some(WorkspaceData { projects, agents }));
    });
}

/// Workspace 侧边栏数据加载 hook
///
/// 只加载侧边栏列表（projects + agents），中心图的 tasks 由调用方按需加载。
/// 返回 (data_signal, refresh_fn)。
pub fn use_workspace_data() -> (Signal<Option<WorkspaceData>>, impl FnMut()) {
    let data: Signal<Option<WorkspaceData>> = use_signal(|| None);
    let toast = use_toast();

    use_future(move || async move {
        loop {
            spawn_workspace_load(data, toast);
            gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_MS).await;
        }
    });

    (data, move || {
        spawn_workspace_load(data, toast);
    })
}
