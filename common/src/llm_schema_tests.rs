//! tests 单元测试（拆分自 llm_schema.rs，拆分前 969 行）
//!
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 本模块为手工拆分：`tools/split_inline_tests.py` 要求文件以配平 `}` 结尾，
//! 而 llm_schema.rs 末尾已有 `mod wire_name_tests;` 的 path 声明，故按同一手法手工拆。

use super::*;
use schemars::JsonSchema;

#[derive(JsonSchema)]
#[allow(dead_code)]
enum Status {
    /// 待审批
    Pending,
    /// 已批准生效
    Active,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct Probe {
    /// 按状态过滤
    status: Option<Status>,
    /// 必填枚举
    required_status: Status,
    /// 按申请人过滤
    agent_id: Option<String>,
}

#[test]
fn 可选枚举折叠为属性层可见的_type_与_enum() {
    let schema = schema_for_llm::<Probe>();
    let status = &schema["properties"]["status"];
    assert_eq!(status["type"], serde_json::json!(["string", "null"]));
    assert_eq!(
        status["enum"],
        serde_json::json!(["Pending", "Active", null])
    );
    assert!(status["description"].as_str().unwrap().contains("待审批"));
}

#[test]
fn 折叠后不再残留_anyof_oneof_ref() {
    let schema = schema_for_llm::<Probe>();
    // 复用与折叠规则同源的判定，避免测试自己另写一套而漂移
    assert!(
        unflattened_positions(&schema).is_empty(),
        "{:?}",
        unflattened_positions(&schema)
    );
}

#[test]
fn 必填枚举不带_null() {
    let schema = schema_for_llm::<Probe>();
    let s = &schema["properties"]["required_status"];
    assert_eq!(s["type"], serde_json::json!("string"));
    assert_eq!(s["enum"], serde_json::json!(["Pending", "Active"]));
}

#[test]
fn 可选基础类型保持原状() {
    let schema = schema_for_llm::<Probe>();
    assert_eq!(
        schema["properties"]["agent_id"]["type"],
        serde_json::json!(["string", "null"])
    );
}

#[test]
fn 折叠是幂等的() {
    let once = schema_for_llm::<Probe>();
    let twice = to_llm_schema(once.clone());
    assert_eq!(once, twice);
}

#[test]
fn 名为_type_的字段不会被误判成_schema_关键字() {
    let tricky = serde_json::json!({
        "type": "object",
        "properties": {
            "type": {"type": "string", "enum": ["env"], "description": "凭据类型"},
            "items": {"type": "string"}
        }
    });
    let out = to_llm_schema(tricky);
    assert_eq!(
        out["properties"]["type"]["enum"],
        serde_json::json!(["env"])
    );
    assert_eq!(
        out["properties"]["items"]["type"],
        serde_json::json!("string")
    );
}

#[test]
fn 真实授权查询请求_折叠后取值词表在属性层可见() {
    let schema = schema_for_llm::<crate::api::tool::AuthorizationQueryRequest>();
    assert!(
        unflattened_positions(&schema).is_empty(),
        "{:?}",
        unflattened_positions(&schema)
    );
    let status = &schema["properties"]["status"];
    assert_eq!(status["type"], serde_json::json!(["string", "null"]));
    // 六个状态全部在属性层列出，且补了 null 保证「不传/传 null」仍合法
    assert_eq!(
        status["enum"],
        serde_json::json!([
            "Pending", "Active", "Expired", "Revoked", "Rejected", "Consumed", null
        ])
    );
    assert!(status["description"].as_str().unwrap().contains("Pending"));
}

#[test]
fn 非两分支或非可空的_anyof_不动_避免破坏语义() {
    let src = serde_json::json!({
        "anyOf": [{"type": "string"}, {"type": "integer"}, {"type": "null"}]
    });
    assert_eq!(to_llm_schema(src.clone()), src);
}

// ==================== $ref 内联（出站侧）====================

/// 真实 MCP 工具 `add_transaction` 的 inputSchema（2026-10-18 从运行库抓取，
/// 已裁掉与问题无关的字段）。
///
/// 这份夹具的价值在于它**不是**事后编出来的：`location` 的
/// `anyOf:[{$ref},{type:null}]` 让模型把 `"null"` 当值传了 7 次，
/// `LocationInfo` 的 9 个键一个都没到，而 MCP server 侧宽松接受 ⇒
/// 状态 Completed、日志零报错。
fn add_transaction_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "title": "add_transaction",
        "properties": {
            "txn_type": {
                "type": "string",
                "enum": ["expense", "income", "transfer"],
                "title": "Txn Type"
            },
            "amount": {"type": "string", "title": "Amount"},
            "tags": {
                "anyOf": [{"type": "array", "items": {"type": "string"}}, {"type": "null"}],
                "default": null
            },
            "location": {
                "anyOf": [{"$ref": "#/$defs/LocationInfo"}, {"type": "null"}],
                "default": null
            }
        },
        "required": ["txn_type", "amount"],
        "$defs": {
            "LocationInfo": {
                "type": "object",
                "title": "LocationInfo",
                "description": "地点信息结构体。全键 Optional，整体 None=无地点。",
                "properties": {
                    "name": {"anyOf": [{"type": "string", "maxLength": 128}, {"type": "null"}], "default": null},
                    "addr": {"anyOf": [{"type": "string", "maxLength": 256}, {"type": "null"}], "default": null},
                    "city": {"anyOf": [{"type": "string"}, {"type": "null"}], "default": null},
                    "point": {"anyOf": [{"type": "string"}, {"type": "null"}], "default": null},
                    "poi_id": {"anyOf": [{"type": "string"}, {"type": "null"}], "default": null},
                    "adcode": {"anyOf": [{"type": "string"}, {"type": "null"}], "default": null},
                    "biz": {"anyOf": [{"type": "string"}, {"type": "null"}], "default": null},
                    "raw": {"anyOf": [{"type": "string"}, {"type": "null"}], "default": null},
                    "type": {"anyOf": [{"type": "string"}, {"type": "null"}], "default": null}
                }
            }
        }
    })
}

#[test]
fn 可空引用对象_内联后嵌套字段在属性层可见() {
    let out = to_llm_schema(add_transaction_schema());
    let location = &out["properties"]["location"];
    // 关键防线：折叠后必须是「可空对象」，绝不能退化成「只能填 null」
    assert_eq!(location["type"], serde_json::json!(["object", "null"]));
    for key in [
        "name", "addr", "city", "point", "poi_id", "adcode", "biz", "raw",
    ] {
        assert_eq!(
            location["properties"][key]["type"],
            serde_json::json!(["string", "null"]),
            "location.{key} 必须在属性层可见且保持可空"
        );
    }
    // 全展开后 $defs 已被丢弃
    assert!(out.get("$defs").is_none());
}

#[test]
fn 外部_mcp_schema_收敛后无未收敛位置() {
    let out = to_llm_schema(add_transaction_schema());
    let gaps = unflattened_positions(&out);
    assert!(gaps.is_empty(), "{gaps:?}");
}

#[test]
fn 内联加折叠是幂等的() {
    let once = to_llm_schema(add_transaction_schema());
    let twice = to_llm_schema(once.clone());
    assert_eq!(once, twice);
}

#[test]
fn 目标缺失的_ref_不得反推成_null_类型() {
    let src = serde_json::json!({
        "properties": {
            "location": {"anyOf": [{"$ref": "#/$defs/Missing"}, {"type": "null"}]}
        },
        "$defs": {"LocationInfo": {"type": "object"}}
    });
    let out = to_llm_schema(src);
    let location = &out["properties"]["location"];
    assert!(
        location["type"].is_null(),
        "未知类型绝不许写成 type:[\"null\"]，实际为 {}",
        location["type"]
    );
    assert_eq!(location["$ref"], serde_json::json!("#/$defs/Missing"));
}

#[test]
fn 自引用_schema_不死循环_保留_defs_兜底() {
    let src = serde_json::json!({
        "properties": {"node": {"$ref": "#/$defs/Node"}},
        "$defs": {
            "Node": {
                "type": "object",
                "properties": {"child": {"$ref": "#/$defs/Node"}}
            }
        }
    });
    let out = to_llm_schema(src);
    // 外层照常展开
    assert_eq!(
        out["properties"]["node"]["type"],
        serde_json::json!("object")
    );
    // 内层的环保留 $ref，$defs 随之保留以免成为悬空指针
    assert_eq!(
        out["properties"]["node"]["properties"]["child"]["$ref"],
        serde_json::json!("#/$defs/Node")
    );
    assert!(out["$defs"]["Node"].is_object());
}
