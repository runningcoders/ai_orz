//! Graph 数据结构定义

/// 图节点数据
///
/// 表示图中的一个节点，由 ID、标签、可选分类组成。
/// 分类可用于渲染时着色等样式区分（如任务状态 done/doing/todo）。
#[derive(Debug, Clone)]
pub struct GraphNodeData {
    /// 节点唯一 ID（用于边的引用）
    pub id: String,
    /// 节点显示标签
    pub label: String,
    /// 节点分类（可选，用于样式区分）
    pub category: Option<String>,
}

impl GraphNodeData {
    /// 创建新节点
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category: None,
        }
    }

    /// 设置节点分类
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }
}

/// 图边
///
/// 表示两个节点之间的有向连接。
#[derive(Debug, Clone)]
pub struct GraphLine {
    /// 起点节点 ID
    pub from: String,
    /// 终点节点 ID
    pub to: String,
    /// 边标签（可选）
    pub label: Option<String>,
}

impl GraphLine {
    /// 创建新边
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: None,
        }
    }

    /// 设置边标签
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// 图数据结构
///
/// 由节点列表和边列表组成，是 Renderer 的输入。
#[derive(Debug, Clone, Default)]
pub struct Graph {
    /// 节点列表
    pub nodes: Vec<GraphNodeData>,
    /// 边列表
    pub lines: Vec<GraphLine>,
}

impl Graph {
    /// 创建空图
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加节点
    pub fn add_node(&mut self, node: GraphNodeData) -> &mut Self {
        self.nodes.push(node);
        self
    }

    /// 添加边
    pub fn add_line(&mut self, line: GraphLine) -> &mut Self {
        self.lines.push(line);
        self
    }

    /// 根据 ID 查找节点
    pub fn find_node(&self, id: &str) -> Option<&GraphNodeData> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// 节点数量
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 边数量
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// 渲染图
    pub fn render(&self, renderer: &dyn super::GraphRenderer) -> String {
        renderer.render(self)
    }
}

/// GraphNode trait
///
/// 实体实现此 trait 即可转换为图节点。
/// 这是可选的便捷接口，调用方也可以直接构造 GraphNodeData。
pub trait GraphNode {
    /// 转换为图节点数据
    fn to_graph_node(&self) -> GraphNodeData;
}
