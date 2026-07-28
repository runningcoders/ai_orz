//! Graph 渲染器实现

use super::types::{Graph, GraphLine, GraphNodeData};

/// 图渲染器 trait
///
/// 定义图的输出格式。任何实现此 trait 的渲染器都可以将 Graph 转换为字符串。
/// 例如 MermaidRenderer 输出 Mermaid 语法，未来可扩展 PlantUmlRenderer、DotRenderer 等。
pub trait GraphRenderer: Send + Sync {
    /// 渲染图为字符串
    fn render(&self, graph: &Graph) -> String;
}

/// Mermaid 方向
#[derive(Debug, Clone, Copy, Default)]
pub enum MermaidDirection {
    /// 左到右
    #[default]
    LR,
    /// 上到下
    TD,
}

impl MermaidDirection {
    fn as_str(&self) -> &'static str {
        match self {
            MermaidDirection::LR => "LR",
            MermaidDirection::TD => "TD",
        }
    }
}

/// Mermaid 渲染器
///
/// 输出 Mermaid flowchart 语法的字符串，可直接嵌入 Markdown 等文本中。
///
/// 节点样式：
/// - 有 category 的节点会生成 `class <id> <category>` 样式类
/// - 调用方可在 Markdown 中配合自定义 CSS 使用这些 class
///
/// 外部节点处理：
/// - 边引用了图中不存在的节点 ID 时，自动补出该节点（标签为 `(external) <id>`）
#[derive(Debug, Clone)]
pub struct MermaidRenderer {
    /// 图方向
    pub direction: MermaidDirection,
}

impl MermaidRenderer {
    /// 创建渲染器
    pub fn new(direction: MermaidDirection) -> Self {
        Self { direction }
    }

    /// 使用默认方向（LR）创建渲染器
    pub fn default_lr() -> Self {
        Self::new(MermaidDirection::LR)
    }

    /// 转义节点标签中的特殊字符
    fn escape_label(label: &str) -> String {
        label.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// 收集所有被边引用但不在 nodes 列表中的节点 ID（外部节点）
    fn collect_external_nodes(graph: &Graph) -> Vec<String> {
        let existing: std::collections::HashSet<&str> =
            graph.nodes.iter().map(|n| n.id.as_str()).collect();
        let mut external: Vec<String> = Vec::new();
        for line in &graph.lines {
            if !existing.contains(line.from.as_str()) {
                external.push(line.from.clone());
            }
            if !existing.contains(line.to.as_str()) {
                external.push(line.to.clone());
            }
        }
        external
    }
}

impl GraphRenderer for MermaidRenderer {
    fn render(&self, graph: &Graph) -> String {
        let mut out = String::new();
        out.push_str(&format!("flowchart {}\n", self.direction.as_str()));

        // 节点定义
        for node in &graph.nodes {
            let label = Self::escape_label(&node.label);
            out.push_str(&format!("    {}[\"{}\"]\n", node.id, label));
        }

        // 外部节点（边引用但未在 nodes 中的节点）
        let external_nodes = Self::collect_external_nodes(graph);
        for ext_id in &external_nodes {
            out.push_str(&format!(
                "    {}[\"(external) {}\"]\n",
                ext_id,
                Self::escape_label(ext_id)
            ));
        }

        // 边定义
        for line in &graph.lines {
            match &line.label {
                Some(label) => {
                    out.push_str(&format!("    {} -- {} --> {}\n", line.from, label, line.to));
                }
                None => {
                    out.push_str(&format!("    {} --> {}\n", line.from, line.to));
                }
            }
        }

        // 样式类（基于 category）
        let mut classes_seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for node in &graph.nodes {
            if let Some(cat) = &node.category {
                if classes_seen.insert(cat.as_str()) {
                    out.push_str(&format!("    class {} {}\n", node.id, cat));
                } else {
                    // 同一 category 的其它节点也要加上 class 声明
                    out.push_str(&format!("    class {} {}\n", node.id, cat));
                }
            }
        }

        out
    }
}
