//! Workspace 工作台侧边栏数据加载 hook
//!
//! 只加载侧边栏所需的 projects + agents 列表（轻量）。
//! 中心图的 tasks 数据由 workspace.rs 按视图按需加载。

use crate::api::hr::list_agents;
use crate::api::project::list_projects;
use crate::store::toast::use_toast;
use common::api::{AgentListItem, ListAgentsRequest, ListProjectsRequest, ProjectListItem};
use dioxus::prelude::*;

/// Workspace 侧边栏数据
#[derive(Debug, Clone, Default)]
pub struct WorkspaceData {
    pub projects: Vec<ProjectListItem>,
    pub agents: Vec<AgentListItem>,
}

/// Workspace 侧边栏数据加载 hook
///
/// 只加载侧边栏列表（projects + agents），中心图的 tasks 由调用方按需加载。
/// 返回 (data_signal, refresh_fn)。
pub fn use_workspace_data() -> (Signal<Option<WorkspaceData>>, impl FnMut()) {
    let mut data: Signal<Option<WorkspaceData>> = use_signal(|| None);
    let toast = use_toast();

    let load = move || {
        spawn(async move {
            let projects = list_projects(ListProjectsRequest::default())
                .await
                .map(|r| r.items)
                .unwrap_or_default();
            let agents = list_agents(ListAgentsRequest::default())
                .await
                .map(|r| r.items)
                .unwrap_or_default();

            if projects.is_empty() && agents.is_empty() {
                toast.info("暂无 Project 和 Agent 数据");
            }

            data.set(Some(WorkspaceData { projects, agents }));
        });
    };

    use_effect(move || {
        load();
    });

    (data, move || {
        load();
    })
}
