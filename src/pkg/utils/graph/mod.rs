//! 通用图形渲染组件
//!
//! 设计理念：
//! - 定义 Graph/Node/Line 抽象，任何实体只要能转换为 GraphNodeData 即可渲染成图
//! - Renderer trait 支持多种输出格式，Mermaid 只是其中一种实现
//! - 不绑定任何业务实体，纯通用组件

mod types;
mod renderer;

pub use types::{Graph, GraphNodeData, GraphLine, GraphNode};
pub use renderer::GraphRenderer;
