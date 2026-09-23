//! 面向 LLM 的工具参数 schema 收敛
//!
//! # 为什么需要这一层
//!
//! `schemars` 按 JSON Schema 规范生成，规范写得没错，但**不是为 LLM function calling 设计的**：
//!
//! - `Option<T>` 遇到引用类型只能写成 `anyOf: [{$ref}, {type:"null"}]`
//!   （draft-07 里 `$ref` 旁不能放兄弟键），取值词表被挡在 `definitions` 后面；
//! - 带 doc comment 的 unit enum 会拆成 `oneOf: [{enum:["A"]}, {enum:["B"]}]`，
//!   模型要跨多个分支才能拼出全部取值。
//!
//! 实测（2026-09-23，模型 `glm-5.3-flash`）：同为 `status` 字段、同样填大写枚举值，
//! `anyOf+$ref` 形状 **25 次调用 25 次把枚举值写成未加引号的裸标识符**
//! （整段 arguments 变非法 JSON → 降级为 null → 下游报 `empty tool arguments`），
//! 而属性层直接可见 `type`/`enum` 的形状 0 次出错。
//!
//! 本模块把 schema 收敛成「属性层自解释」的形状：
//!
//! ```json
//! {"description":"取值：Pending=待审批；Active=已批准生效",
//!  "type":["string","null"],"enum":["Pending","Active",null]}
//! ```
//!
//! # 落位
//!
//! - 生成侧（内置工具）：`schema_for_llm::<T>()`
//! - 出站侧（用户自填 / 外部 MCP 同步的来源不走生成侧）：`to_llm_schema(Value)`
//!
//! 两处共用同一套规则，禁止各自漂移。

use schemars::JsonSchema;
use schemars::generate::{SchemaGenerator, SchemaSettings};
use schemars::transform::Transform;
use serde_json::{Map, Value};

/// 递归深度上限。工具参数 schema 由 Rust 类型派生、不会有环，
/// 这里只是防御外部来源（MCP `inputSchema` 是**不可信输入**）构造出的畸形嵌套。
const MAX_DEPTH: usize = 32;

// ==================== 公开 API ====================

/// 生成「LLM 友好」的参数 schema（内置工具注册时用）
pub fn schema_for_llm<T: JsonSchema + ?Sized>() -> Value {
    let settings = SchemaSettings::draft07()
        .with(|s| {
            // 展开 $ref：让取值词表在属性层可见
            s.inline_subschemas = true;
            // 顶层 $schema 对模型无意义，省 token
            s.meta_schema = None;
        })
        .with_transform(LlmSchemaFlattener);

    let mut generator: SchemaGenerator = settings.into_generator();
    let mut value = generator.root_schema_for::<T>().to_value();
    strip_root_noise(&mut value);
    value
}

/// 对已存在的 schema 做同样的收敛（出站侧用）
///
/// 覆盖三种来源：内置工具（生成侧已处理，再跑一次幂等）、HTTP 自建工具、
/// 外部 MCP server 同步的 `inputSchema`。
pub fn to_llm_schema(mut schema: Value) -> Value {
    flatten_value(&mut schema, 0);
    strip_root_noise(&mut schema);
    schema
}

/// 套在 schemars `SchemaSettings` 上的官方扩展点
#[derive(Debug, Clone, Copy, Default)]
pub struct LlmSchemaFlattener;

impl Transform for LlmSchemaFlattener {
    fn transform(&mut self, schema: &mut schemars::Schema) {
        // Schema 是 Value 的薄封装，直接就地改它的 object，不必来回转换
        if let Some(obj) = schema.as_object_mut() {
            flatten_object(obj, 0);
        }
    }
}

// ==================== 递归骨架 ====================

fn flatten_value(value: &mut Value, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    if let Some(obj) = value.as_object_mut() {
        flatten_object(obj, depth);
    }
}

fn flatten_object(obj: &mut Map<String, Value>, depth: usize) {
    // 先递归处理子 schema，再折叠本层 —— 顺序不能反：
    // 本层的 anyOf 分支里可能还套着 oneOf，要先被压平才能合并成单一 enum。
    recurse_subschemas(obj, depth);
    flatten_one_of(obj);
    collapse_nullable_any_of(obj);
}

/// 只沿「承载子 schema」的键下钻，其余键名一律当数据。
///
/// ⚠️ 绝不可按键名泛化匹配：用户完全可能有一个字段就叫 `type` / `items` / `properties`，
/// 泛化匹配会把数据误判成 JSON Schema 关键字（参见 `llm-tool-schema-lcd-guard` 第六节）。
fn recurse_subschemas(obj: &mut Map<String, Value>, depth: usize) {
    let keys: Vec<String> = obj.keys().cloned().collect();
    for key in keys {
        match key.as_str() {
            // 名字 → schema 的映射
            "properties" | "patternProperties" | "definitions" | "$defs" | "dependentSchemas" => {
                if let Some(Value::Object(map)) = obj.get_mut(&key) {
                    let sub_keys: Vec<String> = map.keys().cloned().collect();
                    for sk in sub_keys {
                        if let Some(v) = map.get_mut(&sk) {
                            flatten_value(v, depth + 1);
                        }
                    }
                }
            }
            // schema 数组（组合子 / 元组）
            "anyOf" | "oneOf" | "allOf" | "prefixItems" => {
                if let Some(Value::Array(arr)) = obj.get_mut(&key) {
                    for v in arr.iter_mut() {
                        flatten_value(v, depth + 1);
                    }
                }
            }
            // 单 schema 位置
            "items"
            | "additionalProperties"
            | "propertyNames"
            | "contains"
            | "not"
            | "if"
            | "then"
            | "else" => {
                if let Some(v) = obj.get_mut(&key) {
                    flatten_value(v, depth + 1);
                }
            }
            _ => {}
        }
    }
}

// ==================== 两条折叠规则 ====================

/// 一个「简单枚举分支」：`{type:"string", enum:[X] | const:X, description?}`
///
/// 只在这种形状上压平；带其它关键字的分支一律放过，避免破坏语义。
fn is_simple_enum_branch(v: &Value) -> bool {
    let Some(o) = v.as_object() else {
        return false;
    };
    if !o.keys().all(|k| {
        matches!(
            k.as_str(),
            "type" | "enum" | "const" | "description" | "title"
        )
    }) {
        return false;
    }
    if !matches!(o.get("type"), Some(Value::String(s)) if s == "string") {
        return false;
    }
    o.contains_key("enum") || o.contains_key("const")
}

/// `oneOf:[{enum:[A]},{enum:[B]}]` → `{type:"string", enum:[A,B]}`，
/// 各分支的中文注释拼进 description（信息不能丢）。
fn flatten_one_of(obj: &mut Map<String, Value>) {
    let Some(Value::Array(branches)) = obj.get("oneOf").cloned() else {
        return;
    };
    if branches.is_empty() || !branches.iter().all(is_simple_enum_branch) {
        return;
    }

    let mut values: Vec<Value> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for branch in &branches {
        let o = branch.as_object().expect("已校验为对象");
        let name = o
            .get("enum")
            .and_then(|e| e.get(0))
            .and_then(|x| x.as_str())
            .or_else(|| o.get("const").and_then(|c| c.as_str()))
            .unwrap_or_default()
            .to_string();
        match o.get("enum") {
            Some(Value::Array(e)) => values.extend(e.iter().cloned()),
            _ => {
                if let Some(c) = o.get("const") {
                    values.push(c.clone());
                }
            }
        }
        if let Some(d) = o.get("description").and_then(|d| d.as_str()) {
            if name.is_empty() {
                notes.push(d.to_string());
            } else {
                notes.push(format!("{name}={d}"));
            }
        }
    }

    obj.remove("oneOf");
    obj.insert("type".to_string(), Value::String("string".to_string()));
    obj.insert("enum".to_string(), Value::Array(values));
    if !notes.is_empty() {
        let joined = notes.join("；");
        let desc = match obj.get("description").and_then(|d| d.as_str()) {
            Some(d) => format!("{d}。取值：{joined}"),
            None => format!("取值：{joined}"),
        };
        obj.insert("description".to_string(), Value::String(desc));
    }
}

fn is_null_branch(v: &Value) -> bool {
    match v.as_object() {
        Some(o) if o.len() == 1 => match o.get("type") {
            Some(Value::String(s)) => s == "null",
            Some(Value::Array(a)) => {
                a.len() == 1 && a.first() == Some(&Value::String("null".into()))
            }
            _ => false,
        },
        _ => false,
    }
}

/// `anyOf:[X,{type:"null"}]` → 把 X 提上来，`type` 补 `"null"`、`enum` 补 `null`。
///
/// ⚠️ `enum` 必须补 `null`：否则显式传 `null`（表示「不过滤」）会被 enum 拒掉，
/// 那就把「可空」语义弄丢了。
fn collapse_nullable_any_of(obj: &mut Map<String, Value>) {
    let Some(Value::Array(branches)) = obj.get("anyOf").cloned() else {
        return;
    };
    if branches.len() != 2 {
        return;
    }
    let other_idx = if is_null_branch(&branches[0]) {
        1
    } else if is_null_branch(&branches[1]) {
        0
    } else {
        return;
    };
    let other = branches[other_idx].clone();
    let Some(other_obj) = other.as_object() else {
        return;
    };

    obj.remove("anyOf");

    let mut types: Vec<Value> = Vec::new();
    match other_obj.get("type") {
        Some(Value::String(s)) => types.push(Value::String(s.clone())),
        Some(Value::Array(a)) => types.extend(a.iter().cloned()),
        _ => {}
    }
    if !types.iter().any(|t| t == "null") {
        types.push(Value::String("null".to_string()));
    }

    let merged_enum = match other_obj.get("enum") {
        Some(Value::Array(e)) => {
            let mut e = e.clone();
            if !e.iter().any(|x| x.is_null()) {
                e.push(Value::Null);
            }
            Some(Value::Array(e))
        }
        Some(v) => Some(v.clone()),
        None => None,
    };

    for (k, v) in other_obj {
        if k == "type" || k == "enum" {
            continue;
        }
        obj.insert(k.clone(), v.clone());
    }
    if !types.is_empty() {
        obj.insert("type".to_string(), Value::Array(types));
    }
    if let Some(e) = merged_enum {
        obj.insert("enum".to_string(), e);
    }
}

/// 列出**仍未收敛**的位置（JSON Pointer 风格），供测试与护栏复用。
///
/// 只追究「本该被折叠却没折叠」的形状：
/// - `$ref`
/// - 全部分支都是简单字符串枚举的 `oneOf`（压平不丢语义，没压就是漏了）
/// - 含 `{type:"null"}` 分支的两元 `anyOf`
///
/// ⚠️ **不追究**判别联合：`#[serde(tag="type")]` 的 enum 各分支是带不同 `properties` 的 object
/// （如 `CredentialBinding` 的 `env`/`header`/`query`），压平会破坏语义 —— 那是设计内的保留。
///
/// 判定与折叠规则**同源**，两侧禁止各自漂移。
pub fn unflattened_positions(schema: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_unflattened(schema, String::new(), 0, &mut out);
    out
}

fn collect_unflattened(value: &Value, path: String, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(obj) = value.as_object() else {
        return;
    };
    if obj.contains_key("$ref") {
        out.push(format!("{path}/$ref"));
    }
    if let Some(Value::Array(b)) = obj.get("oneOf")
        && !b.is_empty()
        && b.iter().all(is_simple_enum_branch)
    {
        out.push(format!("{path}/oneOf(可压平的枚举)"));
    }
    if let Some(Value::Array(b)) = obj.get("anyOf")
        && b.len() == 2
        && (is_null_branch(&b[0]) || is_null_branch(&b[1]))
    {
        out.push(format!("{path}/anyOf(可折叠的可空枚举)"));
    }
    for (k, sub) in obj {
        match k.as_str() {
            "properties" | "patternProperties" | "definitions" | "$defs" | "dependentSchemas" => {
                if let Some(map) = sub.as_object() {
                    for (sk, sv) in map {
                        collect_unflattened(sv, format!("{path}/{k}/{sk}"), depth + 1, out);
                    }
                }
            }
            "anyOf" | "oneOf" | "allOf" | "prefixItems" => {
                if let Some(arr) = sub.as_array() {
                    for (i, sv) in arr.iter().enumerate() {
                        collect_unflattened(sv, format!("{path}/{k}/{i}"), depth + 1, out);
                    }
                }
            }
            "items"
            | "additionalProperties"
            | "propertyNames"
            | "contains"
            | "not"
            | "if"
            | "then"
            | "else" => collect_unflattened(sub, format!("{path}/{k}"), depth + 1, out),
            _ => {}
        }
    }
}

/// 顶层 `$schema` / `title` 对模型是纯噪音
fn strip_root_noise(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("$schema");
        obj.remove("title");
    }
}

#[cfg(test)]
mod tests {
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
}
