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
//! # 不止 schema：`function.name` 同样要收敛
//!
//! tools 数组的 `function.name` 也受同一套逻辑管辖：DB 的 `tools.name` 是
//! **人类显示名**（`List Tool Tags`，带空格 / 斜杠 / 括号），MCP 工具名
//! 形如 `mcp.<server-uuid>.<tool>`（带点号），二者都超出最严端点
//! （火山方舟）的 `^[a-zA-Z0-9_-]{1,128}$`。出站一律经
//! [`to_wire_tool_name`] 从**工具 id** 派生，回站按同一函数现算后比较。
//!
//! # 落位
//!
//! - 生成侧（内置工具）：`schema_for_llm::<T>()`
//! - 出站侧（用户自填 / 外部 MCP 同步的来源不走生成侧）：`to_llm_schema(Value)`
//! - name 出站侧：`to_wire_tool_name(&tool.po.id)`（`ToolDescriptor::from`）
//!
//! 两处共用同一套规则，禁止各自漂移。

use schemars::JsonSchema;
use schemars::generate::{SchemaGenerator, SchemaSettings};
use schemars::transform::Transform;
use serde_json::{Map, Value};

/// 递归深度上限。工具参数 schema 由 Rust 类型派生、不会有环，
/// 这里只是防御外部来源（MCP `inputSchema` 是**不可信输入**）构造出的畸形嵌套。
const MAX_DEPTH: usize = 32;

/// 单次转换允许展开的内部 `$ref` 数量上限。
///
/// 外部来源 schema 属**不可信输入**：一个被上百处引用的大 `$defs` 项会被复制
/// 上百份直送模型，足以吃满上下文窗口。超预算即停止展开（残留 `$ref` 由
/// `$defs` 兜底）——宁可让模型少看见几个字段，也不让 prompt 膨胀失控。
const MAX_INLINED_REFS: usize = 64;

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
    inline_local_refs(&mut schema);
    flatten_value(&mut schema, 0);
    strip_root_noise(&mut schema);
    schema
}

// ==================== function.name 收敛 ====================

/// `function.name` 的跨 provider 安全字符集：ASCII 字母、数字、下划线、连字符。
///
/// 实证（2026-10-21，火山方舟 `deepseek-v4.1-flash`）：tools 数组里任一工具的
/// name 含空格 / 斜杠 / 括号 / 点号，整个请求 400 `InvalidParameter:
/// tools.N.function.name: expected 1-128 ASCII letters, digits, underscores
/// or hyphens`，且**报错指不出是哪个工具**。症状：该 Agent 持有的每一轮模型
/// 调用全挂（毒工具效应——只要在册，与被不被调用无关）。
/// OpenAI / GLM 端点宽松不报错，但那是运气不是设计：LCD 按最严端点收敛。
const WIRE_NAME_MAX_LEN: usize = 128;

/// 把工具标识符收敛成跨 provider 安全的 `function.name`。
///
/// 规则：
/// - 非 `[a-zA-Z0-9_-]` 字符一律替换为 `_`（显示名空格、MCP 名前缀点号等）；
/// - 超 128 字符截断，拼短哈希后缀防同批撞名；
/// - 空结果兜底 `tool`（空 name 同样被端点拒）。
///
/// **不是单射**（`a.b` 与 `a_b` 同结果）—— 回站查找不依赖反推：调用方对
/// 候选工具集逐个现算后比较，语义与原先按显示名精确匹配同构。
/// 入参应传工具 **id**（天然唯一；内置 / MCP / 自建三来源都带得起来），
/// **不要传 DB 的 `name` 显示名列**——那正是本函数要防的东西。
pub fn to_wire_tool_name(tool_id: &str) -> String {
    if tool_id.chars().count() <= WIRE_NAME_MAX_LEN {
        let out: String = tool_id.chars().map(sanitize_wire_char).collect();
        return if out.is_empty() {
            "tool".to_string()
        } else {
            out
        };
    }
    // 超长：保留前缀 + "_" + 8 位短哈希（1 + 8 + keep == MAX_LEN）
    let keep = WIRE_NAME_MAX_LEN - 9;
    let mut out: String = tool_id.chars().map(sanitize_wire_char).take(keep).collect();
    out.push('_');
    out.push_str(&format!("{:08x}", fnv1a32(tool_id)));
    out
}

/// 校验 name 是否已落在 LCD 内。
///
/// 护栏测试与写入口必须共用这一个判定（禁另写一套，否则两侧规则各自漂移）。
pub fn is_wire_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= WIRE_NAME_MAX_LEN
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn sanitize_wire_char(c: char) -> char {
    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
        c
    } else {
        '_'
    }
}

/// FNV-1a 32 位：仅用于超长 name 的截断后缀，纯函数无随机源，测试可复现。
fn fnv1a32(s: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in s.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
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

// ==================== $ref 内联 ====================

/// 出站侧比生成侧少一道工序：内置工具靠 schemars 的 `inline_subschemas` 在生成
/// 时就把 `$ref` 摊平了，而用户自填 / 外部 MCP 同步的 schema 是**带着 `$defs`
/// 直接进库**的。不补这一刀有两层后果：
///
/// 1. 模型得自己跳到 `$defs` 才能看见嵌套字段 —— 多数模型不会跳，于是干脆不传；
/// 2. `anyOf:[{$ref}, {type:"null"}]` 折叠时 `$ref` 分支没有 `type`，
///    可空对象被写成 `type:["null"]`（见 `collapse_nullable_any_of`）——
///    模型照字面传字符串 `"null"`，嵌套字段全丢且不报错。
fn inline_local_refs(root: &mut Value) {
    let Some(root_obj) = root.as_object_mut() else {
        return;
    };
    let defs = collect_local_defs(root_obj);
    if defs.is_empty() {
        return;
    }
    let mut unresolved = false;
    let mut budget = MAX_INLINED_REFS;
    let mut stack: Vec<String> = Vec::new();
    inline_refs_in_object(root_obj, &defs, &mut stack, &mut budget, 0, &mut unresolved);
    // 仍有残留（环引用 / 外部 URI / 预算耗尽）时必须**留住** `$defs`，
    // 否则剩下的 `$ref` 会变成模型侧解不开的悬空指针。
    if !unresolved {
        root_obj.remove("$defs");
        root_obj.remove("definitions");
    }
}

/// 收集根上可解析的子 schema 表（`$defs` 优先于 `definitions`）
fn collect_local_defs(root: &Map<String, Value>) -> Map<String, Value> {
    let mut defs = Map::new();
    for container in ["$defs", "definitions"] {
        if let Some(Value::Object(subs)) = root.get(container) {
            for (name, schema) in subs {
                defs.entry(name.clone()).or_insert_with(|| schema.clone());
            }
        }
    }
    defs
}

/// 解析本地引用指针，返回指向的子 schema 名
///
/// 只认 `#/$defs/<name>` / `#/definitions/<name>` 两段形态。深层 pointer
/// （`#/$defs/A/properties/b`）与外部 URI 一律返回 `None`（保守不展开）。
fn resolve_local_ref(reference: &str) -> Option<String> {
    let mut parts = reference.strip_prefix("#/")?.split('/');
    let container = parts.next()?;
    if container != "$defs" && container != "definitions" {
        return None;
    }
    let name = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    // JSON Pointer 反转义：`~1` = `/`、`~0` = `~`
    Some(name.replace("~1", "/").replace("~0", "~"))
}

/// 沿 schema 承载位下钻并就地替换 `$ref`
///
/// `stack` 记录当前正在展开的 def 名，用于打破自引用 / 互相引用；
/// `budget` 控制总展开次数；`unresolved` 记录「存在一个最终没展开的 `$ref`」。
fn inline_refs_in_object(
    obj: &mut Map<String, Value>,
    defs: &Map<String, Value>,
    stack: &mut Vec<String>,
    budget: &mut usize,
    depth: usize,
    unresolved: &mut bool,
) {
    if depth > MAX_DEPTH {
        *unresolved = true;
        return;
    }

    if let Some(Value::String(reference)) = obj.get("$ref").cloned() {
        match resolve_local_ref(&reference) {
            // 环：这次不展开（保留 `$ref`），否则无限递归
            Some(name) if stack.contains(&name) => *unresolved = true,
            Some(name) => match defs.get(&name) {
                Some(Value::Object(target)) if *budget > 0 => {
                    *budget -= 1;
                    // def 的内容为主，`$ref` 旁的兄弟键补 def 没覆盖的部分
                    let mut merged = target.clone();
                    for (k, v) in obj.iter() {
                        if k == "$ref" {
                            continue;
                        }
                        merged.entry(k.clone()).or_insert(v.clone());
                    }
                    // 被展开的目标内部还可能挂着 `$ref`，此时继续下钻
                    stack.push(name);
                    inline_refs_in_object(&mut merged, defs, stack, budget, depth + 1, unresolved);
                    stack.pop();
                    *obj = merged;
                    // ⚠️ 必须就此返回：`merged` 的键（含原 `$ref` 旁的兄弟键）
                    // 上面已处理完。若接着走下方的下钻，`stack` 已出栈，自引用的
                    // 子树会被当成首次展开再来一遍，一路复制到 MAX_DEPTH
                    // （实测 Node.child 被复制了 30 层）。
                    return;
                }
                _ => *unresolved = true,
            },
            None => *unresolved = true,
        }
    }

    // 下钻承载位：与 `recurse_subschemas` 同一套判定，但**不进** `$defs`/`definitions`
    // ——那里是待查表，走进去会把环按倍率放大。
    let keys: Vec<String> = obj.keys().cloned().collect();
    for key in keys {
        match key.as_str() {
            "properties" | "patternProperties" | "dependentSchemas" => {
                if let Some(Value::Object(map)) = obj.get_mut(&key) {
                    let subs: Vec<String> = map.keys().cloned().collect();
                    for sk in subs {
                        if let Some(Value::Object(sub)) = map.get_mut(&sk) {
                            inline_refs_in_object(sub, defs, stack, budget, depth + 1, unresolved);
                        }
                    }
                }
            }
            "anyOf" | "oneOf" | "allOf" | "prefixItems" => {
                if let Some(Value::Array(arr)) = obj.get_mut(&key) {
                    for v in arr.iter_mut() {
                        if let Some(sub) = v.as_object_mut() {
                            inline_refs_in_object(sub, defs, stack, budget, depth + 1, unresolved);
                        }
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
            | "else" => {
                if let Some(Value::Object(sub)) = obj.get_mut(&key) {
                    inline_refs_in_object(sub, defs, stack, budget, depth + 1, unresolved);
                }
            }
            _ => {}
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

    // ⚠️ 只在 other 分支**显式给出** `type` 时才写回 `type`。
    // other 是纯 `$ref` 时 `types` 里其实只有我们补的 `"null"`，写出去就成了
    // `type: ["null"]`——「可空对象」被声明成「只能填 null」。模型照字面提交
    // 字符串 `"null"`，嵌套字段一个都到不了；而 MCP server 侧往往宽松接受，
    // 状态仍是 Completed ⇒ **日志零报错的静默丢参**。
    // （2026-10-18 实证：`add_transaction.location` 7/7 全传 `"null"`，
    //   `LocationInfo` 的 9 个键（name/addr/city/point/poi_id/adcode/biz/raw/type）全部丢失）
    let mut types: Vec<Value> = Vec::new();
    let mut typed = false;
    match other_obj.get("type") {
        Some(Value::String(s)) => {
            types.push(Value::String(s.clone()));
            typed = true;
        }
        Some(Value::Array(a)) => {
            types.extend(a.iter().cloned());
            typed = true;
        }
        _ => {}
    }
    if typed && !types.iter().any(|t| t == "null") {
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
    // 未 typed：宁可不写 `type`，也不能写只有 `"null"` 的伪类型
    if typed && !types.is_empty() {
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
#[path = "llm_schema_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "llm_schema_wire_name_tests.rs"]
mod wire_name_tests;
