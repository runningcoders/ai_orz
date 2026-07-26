//! DAG 分层布局算法（Kahn 拓扑排序 + 同层水平均分）
//!
//! 算法步骤：
//! 1. 计算每个节点的入度（在 node_ids 中的前驱数量）
//! 2. 入度为 0 的节点入队，layer = 0
//! 3. BFS 取出节点，找其后继（依赖该节点 的节点），入度 -1，为 0 时入队 layer+1
//! 4. 环检测：若处理后数量 < 总数，剩余节点（环上）强制放最底层
//! 5. 同层节点水平均分

use std::collections::{HashMap, VecDeque};

/// 分层布局配置
#[derive(Debug, Clone, Copy)]
pub struct LayeredLayoutConfig {
    /// 画布宽度
    pub width: f64,
    /// 画布高度
    pub height: f64,
    /// 顶部留白
    pub top_margin: f64,
    /// 层间距（y 方向）
    pub layer_height: f64,
    /// 两侧留白
    pub side_margin: f64,
}

impl Default for LayeredLayoutConfig {
    fn default() -> Self {
        Self {
            width: 700.0,
            height: 500.0,
            top_margin: 80.0,
            layer_height: 80.0,
            side_margin: 60.0,
        }
    }
}

/// 计算分层布局
///
/// - `node_ids`: 参与布局的节点 ID 列表
/// - `dependencies`: node_id → 该节点的前驱节点 ID 列表（即该节点依赖哪些节点）
/// - `config`: 布局参数
///
/// 返回: node_id → (layer, x, y)
pub fn compute_layered_layout(
    node_ids: &[String],
    dependencies: &HashMap<String, Vec<String>>,
    config: &LayeredLayoutConfig,
) -> HashMap<String, (i32, f64, f64)> {
    let mut result: HashMap<String, (i32, f64, f64)> = HashMap::new();
    if node_ids.is_empty() {
        return result;
    }

    // 仅保留在 node_ids 中的前驱（跨项目依赖忽略）
    let id_set: std::collections::HashSet<&str> = node_ids.iter().map(|s| s.as_str()).collect();

    // 1. 计算入度（在 id_set 内的前驱数量）
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for id in node_ids {
        in_degree.insert(id.as_str(), 0);
    }
    for id in node_ids {
        if let Some(preds) = dependencies.get(id) {
            for pred in preds {
                if id_set.contains(pred.as_str()) {
                    *in_degree.get_mut(id.as_str()).unwrap() += 1;
                }
            }
        }
    }

    // 2. 构建后继映射（前驱 → 后继列表）
    let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();
    for id in node_ids {
        if let Some(preds) = dependencies.get(id) {
            for pred in preds {
                if id_set.contains(pred.as_str()) {
                    successors
                        .entry(pred.as_str())
                        .or_default()
                        .push(id.as_str());
                }
            }
        }
    }

    // 3. Kahn BFS
    let mut queue: VecDeque<(&str, i32)> = VecDeque::new();
    for id in node_ids {
        if in_degree[id.as_str()] == 0 {
            queue.push_back((id.as_str(), 0));
        }
    }

    let mut processed: usize = 0;
    let mut max_layer: i32 = 0;
    while let Some((id, layer)) = queue.pop_front() {
        result.insert(id.to_string(), (layer, 0.0, 0.0));
        processed += 1;
        max_layer = max_layer.max(layer);
        if let Some(succs) = successors.get(id) {
            for succ in succs {
                let d = in_degree.get_mut(*succ).unwrap();
                *d -= 1;
                if *d == 0 {
                    queue.push_back((*succ, layer + 1));
                }
            }
        }
    }

    // 4. 环检测：未处理的节点放到最底层
    if processed < node_ids.len() {
        let bottom_layer = max_layer + 1;
        for id in node_ids {
            if !result.contains_key(id.as_str()) {
                result.insert(id.clone(), (bottom_layer, 0.0, 0.0));
            }
        }
    }

    // 5. 同层水平均分
    let mut by_layer: HashMap<i32, Vec<String>> = HashMap::new();
    for id in node_ids {
        if let Some((layer, _, _)) = result.get(id) {
            by_layer.entry(*layer).or_default().push(id.clone());
        }
    }
    for (layer, ids) in &by_layer {
        let count = ids.len();
        let y = config.top_margin + (*layer as f64) * config.layer_height;
        let usable_width = config.width - 2.0 * config.side_margin;
        for (i, id) in ids.iter().enumerate() {
            let x = if count == 1 {
                config.width / 2.0
            } else {
                config.side_margin + (i as f64 + 0.5) * usable_width / count as f64
            };
            if let Some(entry) = result.get_mut(id) {
                entry.1 = x;
                entry.2 = y;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LayeredLayoutConfig {
        LayeredLayoutConfig {
            width: 600.0,
            height: 400.0,
            top_margin: 40.0,
            layer_height: 80.0,
            side_margin: 50.0,
        }
    }

    #[test]
    fn test_empty_input() {
        let deps = HashMap::new();
        let result = compute_layered_layout(&[], &deps, &cfg());
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_node_no_deps() {
        let ids = vec!["a".to_string()];
        let deps = HashMap::new();
        let result = compute_layered_layout(&ids, &deps, &cfg());
        let (layer, x, y) = result["a"];
        assert_eq!(layer, 0);
        assert!((x - 300.0).abs() < 0.01, "单节点应居中: x={}", x);
        assert!((y - 40.0).abs() < 0.01, "单节点 y 应为 top_margin: y={}", y);
    }

    #[test]
    fn test_linear_chain() {
        // a → b → c (b 依赖 a, c 依赖 b)
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut deps = HashMap::new();
        deps.insert("b".to_string(), vec!["a".to_string()]);
        deps.insert("c".to_string(), vec!["b".to_string()]);
        let result = compute_layered_layout(&ids, &deps, &cfg());
        assert_eq!(result["a"].0, 0);
        assert_eq!(result["b"].0, 1);
        assert_eq!(result["c"].0, 2);
    }

    #[test]
    fn test_parallel_nodes_all_layer_zero() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let deps = HashMap::new();
        let result = compute_layered_layout(&ids, &deps, &cfg());
        for id in &ids {
            assert_eq!(result[id].0, 0, "{} 应在 layer 0", id);
        }
        // 水平均分
        let xs: Vec<f64> = ids.iter().map(|id| result[id].1).collect();
        assert!(
            xs.iter().all(|x| *x >= 50.0 && *x <= 550.0),
            "x 应在 side_margin 内"
        );
    }

    #[test]
    fn test_diamond_dependency() {
        // a → b, a → c, b → d, c → d
        let ids = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let mut deps = HashMap::new();
        deps.insert("b".to_string(), vec!["a".to_string()]);
        deps.insert("c".to_string(), vec!["a".to_string()]);
        deps.insert("d".to_string(), vec!["b".to_string(), "c".to_string()]);
        let result = compute_layered_layout(&ids, &deps, &cfg());
        assert_eq!(result["a"].0, 0);
        assert_eq!(result["b"].0, 1);
        assert_eq!(result["c"].0, 1);
        assert_eq!(result["d"].0, 2);
        // b 和 c 同层，x 应不同
        assert_ne!(result["b"].1, result["c"].1, "同层节点 x 应不同");
    }

    #[test]
    fn test_cycle_detection() {
        // a 依赖 b, b 依赖 a（成环）
        let ids = vec!["a".to_string(), "b".to_string()];
        let mut deps = HashMap::new();
        deps.insert("a".to_string(), vec!["b".to_string()]);
        deps.insert("b".to_string(), vec!["a".to_string()]);
        let result = compute_layered_layout(&ids, &deps, &cfg());
        // 环上节点应被放到最底层（layer=0 因为没有入度为 0 的节点，max_layer=0, bottom=1）
        // 实际上：没有节点入度为 0，queue 为空，processed=0，全部放到 bottom_layer = 0+1 = 1
        assert!(
            result["a"].0 >= 1,
            "环上节点应放最底层: a layer={}",
            result["a"].0
        );
        assert!(
            result["b"].0 >= 1,
            "环上节点应放最底层: b layer={}",
            result["b"].0
        );
    }

    #[test]
    fn test_skipped_dependencies_ignored() {
        // b 依赖 x，但 x 不在 node_ids 中
        let ids = vec!["a".to_string(), "b".to_string()];
        let mut deps = HashMap::new();
        deps.insert("b".to_string(), vec!["x".to_string()]); // x 不在 ids
        let result = compute_layered_layout(&ids, &deps, &cfg());
        // a 和 b 都没有有效前驱，都在 layer 0
        assert_eq!(result["a"].0, 0);
        assert_eq!(result["b"].0, 0);
    }
}
