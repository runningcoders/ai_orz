//! Graph 渲染器接口定义
//!
//! Task 1 仅定义 trait 接口；MermaidRenderer 实现在 Task 2 补充。

use super::types::Graph;

/// 图渲染器 trait
///
/// 定义图的输出格式。任何实现此 trait 的渲染器都可以将 Graph 转换为字符串。
/// 例如 MermaidRenderer 输出 Mermaid 语法，未来可扩展 PlantUmlRenderer、DotRenderer 等。
pub trait GraphRenderer: Send + Sync {
    /// 渲染图为字符串
    fn render(&self, graph: &Graph) -> String;
}
