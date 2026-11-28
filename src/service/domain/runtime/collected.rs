//! run 级业务产出的收集器（随图上下文创建、随唤醒结果移交）
//!
//! ## 为什么单独一个类型
//!
//! 「执行节点侧收集、循环结束吐出」的数据必须有一个**显式归属**。裁决判据
//! 见 `docs/modules/02-Agent运行时/A-设计/runtime_data_bus_design.md`，本模块只承接其中一格：
//!
//! | 数据 | 载体 |
//! |---|---|
//! | 图内节点互读写 | `RoundState` / `ThinkBus` |
//! | **出图即被业务消费** | **本类型，随 `AwakeningResult` 移交** |
//! | 长于图（Err / 超时也要） | 调用方持有的 `Arc` 句柄 |
//! | 进程外的读者（前端 / 查询接口） | 写穿落盘，不进内存 |
//!
//! 本类型由图级运行总线实例（`GraphRuntime`）持有，dispatch 节点经总线方法
//! `record_resources` 写入，图结束后由 `take_collected` 取出、随
//! `AwakeningResult` 移交给消费侧。它**不是**进程级键控表：所有权一路显式，
//! 图结束随运行时实例回收。
//!
//! ## 为什么「解析」也在这里
//!
//! [`collect_produced`] 解析工具返回、判断某次调用产出了什么 —— 这是**执行层**
//! 对自身产出的认知，与容器同属一格。值是**跨层共用**的（消息域要拿它拼正文），
//! 因此值对象 [`ProducedResource`] 落在 `models`，两个域各自引用它，域之间没有调用。
//!
//! ## 扩展方式
//!
//! 新增一类业务事实 = 加一个字段 + 一处写入 + 一处读取，**不需要**再穿过
//! `RoundState → ThinkLoopResult → Artifacts → LegacyAwakenOutcome` 四层结构体。

use serde_json::Value;
use std::sync::{Arc, Mutex};

/// 一次唤醒内产生的业务事实
///
/// 只放「丢失会改变用户可见结果」的数据：本类型随返回值移交给消费侧，
/// 不参与路由/投递决策（那些必须是 `ThinkLoopResult` 上的强类型字段，
/// 不允许经由收集器传递）。
#[derive(Debug, Clone, Default)]
pub struct RoundCollected {
    /// 本轮创建出来的附件 / 产物（final 正文漏写引用时按此查漏补缺）
    ///
    /// 跨压缩周期累计：同一次唤醒内多次 think loop 产出合并到一份清单，
    /// 消费侧一次读完。
    pub resources: Vec<ProducedResource>,
}

impl RoundCollected {
    /// 创建共享句柄（与 `WakeUsage::shared` 同一范式）
    pub fn shared() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }

    /// 追加一批产出（空批次天然无副作用）
    ///
    /// 跨周期累加**不靠**「合并两份收集器」：同一次唤醒内的多次 think loop
    /// （deliberate → compact 重跑）共享同一个 `Arc` 句柄，写入即累加，
    /// 因此本类型刻意不提供 `merge` 一类的 API。
    pub fn record_resources(&mut self, items: impl IntoIterator<Item = ProducedResource>) {
        self.resources.extend(items);
    }
}

/// 本轮工具产出的一条资源：scheme + id + 展示名
///
/// 值对象本体在 `models`（执行层与消息域都要用，见
/// [`crate::models::produced_resource`]）。
use crate::models::produced_resource::ProducedResource;

/// 产出型工具 id → (scheme, id 字段名, name 字段名)
///
/// 覆盖全部会「创建附件 / 创建产物」的工具。字段名取自各工具返回结构：
/// - 附件类返回 `AttachmentDetail { id, original_name, .. }`
/// - 产物类返回 `ArtifactDetail { id, name, .. }`，`mark_artifact` 特例为 `artifact_id`
const PRODUCER_TOOLS: &[(&str, &str, &str, &str)] = &[
    (
        "create_attachment_from_path",
        "attachment",
        "id",
        "original_name",
    ),
    (
        "create_text_attachment",
        "attachment",
        "id",
        "original_name",
    ),
    ("create_artifact", "artifact", "id", "name"),
    ("create_text_artifact", "artifact", "id", "name"),
    ("register_artifact_from_path", "artifact", "id", "name"),
    ("mark_artifact", "artifact", "artifact_id", "name"),
];

/// 从一次成功的工具执行里收集产出资源；非产出型工具 / 字段缺失时返回空。
///
/// `tool_id` 是工具的内部 id（`tool.po.id`），`result` 是工具返回的完整 JSON。
/// 只认字符串类型的 id/name；数字 / 空 / 缺失一律跳过，绝不臆造。
pub fn collect_produced(tool_id: &str, result: &Value) -> Vec<ProducedResource> {
    let Some(&(_, scheme, id_key, name_key)) =
        PRODUCER_TOOLS.iter().find(|(id, ..)| *id == tool_id)
    else {
        return Vec::new();
    };
    let (Some(id), Some(name)) = (result.get(id_key), result.get(name_key)) else {
        return Vec::new();
    };
    let (Some(id), Some(name)) = (id.as_str(), name.as_str()) else {
        return Vec::new();
    };
    if id.is_empty() {
        return Vec::new();
    }
    vec![ProducedResource {
        scheme,
        id: id.to_string(),
        name: name.to_string(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resource(id: &str) -> ProducedResource {
        ProducedResource {
            scheme: "attachment",
            id: id.to_string(),
            name: format!("{id}.png"),
        }
    }

    #[test]
    fn shared_handle_accumulates_across_periods() {
        // 真实用法：唤醒图创建一次句柄，deliberate / compact 重跑共享它，
        // 写入即累加，顺序即产出顺序（消费侧按此顺序补引用）。
        let collected = RoundCollected::shared();
        collected
            .lock()
            .unwrap()
            .record_resources(vec![resource("att_1")]);
        Arc::clone(&collected)
            .lock()
            .unwrap()
            .record_resources(vec![resource("att_2")]);

        let snapshot = collected.lock().unwrap();
        assert_eq!(snapshot.resources.len(), 2, "跨周期共享句柄应累加");
        assert_eq!(snapshot.resources[0].id, "att_1");
        assert_eq!(snapshot.resources[1].id, "att_2", "保持产出顺序");
    }

    #[test]
    fn empty_batches_keep_resources_empty() {
        let mut collected = RoundCollected::default();
        assert!(collected.resources.is_empty());
        collected.record_resources(Vec::new());
        assert!(collected.resources.is_empty(), "空批次不应把收集器变成非空");
    }

    #[test]
    fn collect_produced_covers_all_producer_tools() {
        let cases: &[(&str, &str, &str, Value)] = &[
            (
                "create_attachment_from_path",
                "attachment",
                "att_1",
                json!({"id": "att_1", "original_name": "图.png"}),
            ),
            (
                "create_text_attachment",
                "attachment",
                "att_2",
                json!({"id": "att_2", "original_name": "notes.txt"}),
            ),
            (
                "create_artifact",
                "artifact",
                "art_1",
                json!({"id": "art_1", "name": "报告"}),
            ),
            (
                "create_text_artifact",
                "artifact",
                "art_2",
                json!({"id": "art_2", "name": "摘要"}),
            ),
            (
                "register_artifact_from_path",
                "artifact",
                "art_3",
                json!({"id": "art_3", "name": "数据"}),
            ),
            (
                "mark_artifact",
                "artifact",
                "art_4",
                json!({"success": true, "artifact_id": "art_4", "name": "日志产物"}),
            ),
        ];
        for (tool, scheme, id, result) in cases {
            let got = collect_produced(tool, result);
            assert_eq!(got.len(), 1, "工具 {tool} 应产出一条");
            assert_eq!(got[0].scheme, *scheme);
            assert_eq!(got[0].id, *id);
        }
    }

    #[test]
    fn collect_produced_ignores_non_producers_and_malformed() {
        assert!(collect_produced("send_message", &json!({"id": "x"})).is_empty());
        assert!(collect_produced("create_artifact", &json!({"id": "x"})).is_empty());
        assert!(collect_produced("create_artifact", &json!({"id": "", "name": "n"})).is_empty());
        assert!(collect_produced("create_artifact", &json!({"id": 123, "name": "n"})).is_empty());
    }
}
