//! 工作台场景投影的数据入口
//!
//! 页面只与 [`SceneSource`] 对话：v1 是 HTTP 轮询，v2 换成 SSE 时新增一个实现即可。
//! 本模块只负责取快照，不做颜色 / 过渡 / 镜头等表现决策 —— 表现层由
//! `scene::canvas` 的 `VisualBook` 从前后快照差异自行推导，因此不需要事件流。

use common::api::{GetSceneProjectionRequest, SceneProjectionResponse};

use crate::api::scene::get_scene_projection;

/// 场景投影轮询间隔（毫秒）
pub const SCENE_POLL_MS: u32 = 5_000;

/// 缺省工位数（与后端契约一致，上限 8）
pub const DEFAULT_SEAT_LIMIT: u32 = 8;

/// 场景投影数据源
///
/// 页面只依赖本 trait；v2 换 SSE 时新增一个实现即可，调用方（页面壳）不用改。
pub trait SceneSource {
    /// 拉取一次最新投影快照
    async fn poll(&mut self) -> Result<SceneProjectionResponse, crate::api::ApiError>;
}

/// v1 实现：HTTP 轮询
pub struct PollingSource {
    seat_limit: u32,
}

impl PollingSource {
    /// 使用缺省工位数创建
    pub fn new() -> Self {
        Self {
            seat_limit: DEFAULT_SEAT_LIMIT,
        }
    }
}

impl Default for PollingSource {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneSource for PollingSource {
    async fn poll(&mut self) -> Result<SceneProjectionResponse, crate::api::ApiError> {
        get_scene_projection(GetSceneProjectionRequest {
            seat_limit: Some(self.seat_limit),
        })
        .await
    }
}
