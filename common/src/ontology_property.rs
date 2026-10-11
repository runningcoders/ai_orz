//! 属性值模型与校验（前后端共享的纯函数层）
//!
//! ## 分层
//!
//! 本模块是**纯计算层**：属性值形态、取值约束解析、合法性判定与值列互转，
//! 无 IO、无时钟、无副作用。消费方是 Domain 写侧校验（拦非法值）与 DAO
//! 落库（值 ↔ 列互转）；前端复用同一套判定做属性编辑器即时反馈。
//!
//! ## 为什么值是四选一枚举而不是一个 JSON 字符串
//!
//! `knowledge_property` 用四个类型化值列（`value_text` / `value_num` /
//! `value_bool` / `value_ref`）分别承载不同数据类型，目的是让数值属性可直接
//! `WHERE value_num > ?` 走索引、引用属性可直接 JOIN 回节点表。
//! [`PropertyValue`] 是这个「四选一」在内存中的镜像：由
//! [`PropertyDataType`] 决定读哪一列，未使用的列落 NULL。
//!
//! ## 软门禁哲学
//!
//! 校验失败返回 [`PropertyValueIssue`] 描述原因，**由调用方决定拦还是放**。
//! 本体层的既有约定是软门禁（漂移不拦截、不撤回，只标记待审），属性值校验
//! 沿用同一口径：非法值写侧拒绝（类型错属于写错，不是认知分歧），而本体
//! 定义侧的脏数据（`value_range` 非法 JSON）一律退化为「无约束」，
//! 不 panic 不阻断。
//!
//! ## 与基数（Cardinality）的关系
//!
//! 基数约束「一个节点该属性能挂几条值」，需要先知道已有条数（IO），因此
//! 本模块只提供纯判定 [`allows_append`]，取数由调用方完成。

use crate::enums::ontology::{Cardinality, PropertyDataType, PropertyTargetType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 属性值（与 `knowledge_property` 四个类型化值列同构）
///
/// 变体与 [`PropertyDataType`] 一一对应；序列化为 snake_case 标签，
/// 与枚举 `data_type` 取值同源，前端无需二次映射。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PropertyValue {
    /// 文本值 → 落 `value_text`
    Text(String),
    /// 数值 → 落 `value_num`（整数与浮点共用 REAL 列）
    Number(f64),
    /// 布尔 → 落 `value_bool`
    Boolean(bool),
    /// 日期 → 落 `value_text`（ISO-8601 或秒级时间戳字符串，口径由属性定义方声明）
    Date(String),
    /// 引用 → 落 `value_ref`（指向另一个知识节点 id 或外部实体标识）
    Reference(String),
    /// 枚举 → 落 `value_text`（取值受白名单约束）
    Enum(String),
}

/// 属性值的四列拆解形态（DAO 落库用）
///
/// 与 `knowledge_property` 的四个值列逐字段对应；由 [`PropertyValue`]
/// 拆出后只有 [`PropertyDataType`] 指派的那一个字段为 `Some`，其余为 `None`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertyColumns {
    /// 文本列（`text` / `date` / `enum` 型承载）
    pub value_text: Option<String>,
    /// 数值列（`number` 型承载）
    pub value_num: Option<f64>,
    /// 布尔列（`boolean` 型承载；落库为 0 / 1）
    pub value_bool: Option<i32>,
    /// 引用列（`reference` 型承载）
    pub value_ref: Option<String>,
}

impl PropertyValue {
    /// 本值所属的数据类型（与 DDL `data_type` 列同口径）
    pub fn data_type(&self) -> PropertyDataType {
        match self {
            Self::Text(_) => PropertyDataType::String,
            Self::Number(_) => PropertyDataType::Number,
            Self::Boolean(_) => PropertyDataType::Boolean,
            Self::Date(_) => PropertyDataType::Date,
            Self::Reference(_) => PropertyDataType::Reference,
            Self::Enum(_) => PropertyDataType::Enum,
        }
    }

    /// 拆成四个值列（未使用的列恒 `None`）
    pub fn into_columns(self) -> PropertyColumns {
        let mut cols = PropertyColumns::default();
        match self {
            Self::Text(v) | Self::Date(v) | Self::Enum(v) => cols.value_text = Some(v),
            Self::Number(v) => cols.value_num = Some(v),
            Self::Boolean(v) => cols.value_bool = Some(if v { 1 } else { 0 }),
            Self::Reference(v) => cols.value_ref = Some(v),
        }
        cols
    }

    /// 从四个值列按声明类型还原（脏数据兜底：指派列缺失返 `None`）
    ///
    /// 只认 `data_type` 指派的那一列，其余列即使有值也忽略——避免历史行
    /// 改类型后残留旧列值被误读。缺失即 [`None`]，不 panic 不猜默认值。
    pub fn from_columns(data_type: PropertyDataType, cols: PropertyColumns) -> Option<Self> {
        match data_type {
            PropertyDataType::String => cols.value_text.map(Self::Text),
            PropertyDataType::Number => cols.value_num.map(Self::Number),
            PropertyDataType::Boolean => cols.value_bool.map(|v| Self::Boolean(v != 0)),
            PropertyDataType::Date => cols.value_text.map(Self::Date),
            PropertyDataType::Reference => cols.value_ref.map(Self::Reference),
            PropertyDataType::Enum => cols.value_text.map(Self::Enum),
        }
    }
}

/// 取值约束（与 `ontology_properties.value_range` 列的 JSON 同构）
///
/// 两种合法 JSON 形态：
/// - 数组 → 枚举白名单（如 `["gpt-4","claude"]`）；
/// - 对象 → 数值区间 `{"min": 0.0, "max": 1.0}`，也可带 `enum` 键给白名单。
///
/// 全 [`Default`]（min/max 皆 None、白名单为空）= 无约束。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValueRange {
    /// 数值下界（含）；`None` = 不限
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// 数值上界（含）；`None` = 不限
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// 枚举取值白名单；空 = 不限
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<String>,
}

/// 取值提示里枚举白名单最多展示的项数（超出以 `…(共N项)` 收尾）
///
/// 提示串会进提示词（受本体词表注入预算约束）：一个管理员塞了 200 项的枚举
/// 不应吃掉整段预算，故截断并保留总数信息。
const PROPERTY_HINT_MAX_ENUM: usize = 8;

impl ValueRange {
    /// 是否无任何约束
    pub fn is_empty(&self) -> bool {
        self.min.is_none() && self.max.is_none() && self.enum_values.is_empty()
    }

    /// 渲染为提示词用的取值提示短串（无约束 → 空串）
    ///
    /// 形态：数值区间 `0~1` / `>=0` / `<=1`，枚举白名单 `取值取其一: a|b|c`；
    /// 两段并存时用「，」连接。整数值不带小数点（`1` 而非 `1.0`）。
    pub fn to_hint(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        match (self.min, self.max) {
            (Some(min), Some(max)) => parts.push(format!("{}~{}", fmt_num(min), fmt_num(max))),
            (Some(min), None) => parts.push(format!(">={}", fmt_num(min))),
            (None, Some(max)) => parts.push(format!("<={}", fmt_num(max))),
            (None, None) => {}
        }
        if !self.enum_values.is_empty() {
            let shown: Vec<&str> = self
                .enum_values
                .iter()
                .take(PROPERTY_HINT_MAX_ENUM)
                .map(String::as_str)
                .collect();
            let mut s = format!("取值取其一: {}", shown.join("|"));
            if self.enum_values.len() > PROPERTY_HINT_MAX_ENUM {
                s.push_str(&format!("|…(共{}项)", self.enum_values.len()));
            }
            parts.push(s);
        }
        parts.join("，")
    }
}

/// 数值短串（整数不带小数点：`1` 而非 `1.0`）
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 {
        (v as i64).to_string()
    } else {
        v.to_string()
    }
}

/// 解析取值约束（非法 JSON / 空串 / 形态不符一律退化为「无约束」）
///
/// 软门禁：属性定义是管理员维护的共享约定，脏定义不应让写值链路整体失败；
/// 退化后该属性等同「只校验数据类型」，写侧照常工作。
pub fn parse_value_range(raw: &str) -> ValueRange {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ValueRange::default();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return ValueRange::default();
    };
    match value {
        serde_json::Value::Array(items) => ValueRange {
            enum_values: items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect(),
            ..Default::default()
        },
        serde_json::Value::Object(map) => ValueRange {
            min: map.get("min").and_then(serde_json::Value::as_f64),
            max: map.get("max").and_then(serde_json::Value::as_f64),
            enum_values: map
                .get("enum")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        },
        _ => ValueRange::default(),
    }
}

/// 属性值校验不通过的原因
///
/// 纯描述，不含 IO 上下文；调用方（Domain）按需翻译成用户可见文案。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyValueIssue {
    /// 值形态与属性声明的数据类型不匹配（写错，应拒绝）
    TypeMismatch {
        /// 属性声明的类型
        expected: PropertyDataType,
    },
    /// 数值越界（含 NaN / 无穷大等非有限值）
    NumberOutOfRange,
    /// 枚举值不在白名单内
    NotInEnumValue,
}

/// 校验属性值是否符合属性定义
///
/// 判定顺序：类型匹配 → 数值区间 → 枚举白名单。纯函数：同一输入 + 同一约束
/// ⇒ 同一结论。
pub fn validate_property_value(
    data_type: PropertyDataType,
    value: &PropertyValue,
    range: &ValueRange,
) -> Result<(), PropertyValueIssue> {
    if value.data_type() != data_type {
        return Err(PropertyValueIssue::TypeMismatch {
            expected: data_type,
        });
    }
    match value {
        PropertyValue::Number(v) => {
            // NaN / 无穷大与任何区间的比较都为 false，必须显式排除，
            // 否则「限 0~1」的属性会被 NaN 静默写穿
            if !v.is_finite() {
                return Err(PropertyValueIssue::NumberOutOfRange);
            }
            if range.min.is_some_and(|min| *v < min) || range.max.is_some_and(|max| *v > max) {
                return Err(PropertyValueIssue::NumberOutOfRange);
            }
        }
        PropertyValue::Enum(v) => {
            if !range.enum_values.is_empty() && !range.enum_values.iter().any(|e| e == v) {
                return Err(PropertyValueIssue::NotInEnumValue);
            }
        }
        PropertyValue::Text(_)
        | PropertyValue::Boolean(_)
        | PropertyValue::Date(_)
        | PropertyValue::Reference(_) => {}
    }
    Ok(())
}

/// 基数判定：已有 `existing` 条生效值时，是否还允许再挂一条
///
/// 纯判定，取数由调用方完成（基数约束需要 IO）。
pub fn allows_append(cardinality: Cardinality, existing: usize) -> bool {
    match cardinality {
        Cardinality::Single => existing == 0,
        Cardinality::Many => true,
    }
}

/// 解析属性定义的适用载体清单（`ontology_properties.applies_to` 的 JSON 数组）
///
/// 非法 JSON / 空串 / 非数组一律退化为**空集**；空集语义是「不限载体」，
/// 与 `domain_classes`（空数组 = 不限实体类）、`value_range`（非法 = 无约束）
/// 的退化口径一致——脏定义不应让写值链路整体失败。
///
/// ⚠️ 数组内的非法载体名静默丢弃（如 `"nodes"` 拼错），因此拼错的结果是
/// 「该属性谁的都不让挂」而非「全放行」——窄化比放宽安全。
pub fn parse_applies_to(raw: &str) -> Vec<PropertyTargetType> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(trimmed) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_str()?.parse().ok())
        .collect()
}

/// 载体门闩：该属性是否允许挂到指定载体
///
/// 空清单 = 未声明 = 不限载体；非空清单则必须命中，否则拒绝。
/// 纯判定：属性定义取数由调用方完成（需要 IO）。
pub fn allows_target(applies_to: &[PropertyTargetType], target: PropertyTargetType) -> bool {
    applies_to.is_empty() || applies_to.contains(&target)
}

/// 待写入的一条属性（写侧入参，与落库 PO 解耦）
///
/// 刻意不复用 `KnowledgePropertyPo`：PO 需要 id / 时间戳 / status 等落库字段，
/// 在 Domain 层手工拼装容易造出「时间归零」这类脏行（项目既有红线：
/// 构造 PO 一律走 `new()`）。这里只带业务三元组，落库形态由 DAL 决定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PropertyEntry {
    /// 属性语义锚点（`ontology_properties.term_key`）
    pub term_key: String,
    /// 待写入的值
    pub value: PropertyValue,
    /// 多值属性的槽位；**单值属性此值会被 DAL 强制归一为 0**
    pub ordinal: i32,
}

/// 一条属性写入的判定问题
///
/// 「值域 / 载体 / 未定义」三类沿用本体层**软门禁**哲学：记录后照常写入，
/// 不阻断调用方——本体是包容的抽象，认知分歧不该由写侧裁决。
/// 只有「类型错」是**写错**（值形态与声明不符，没有解释空间），硬拦。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PropertyWriteIssue {
    /// 值形态与属性声明的数据类型不符 → **拒绝写入**
    TypeMismatch {
        /// 属性声明的类型
        expected: PropertyDataType,
    },
    /// 数值越界（含 NaN / 无穷大）→ 记录后写入
    NumberOutOfRange,
    /// 枚举值不在白名单内 → 记录后写入
    NotInEnumValue,
    /// 基数超限（单值属性重复写）→ **拒绝写入**（无法安置）
    CardinalityExceeded,
    /// 属性定义不允许挂到该载体 → 记录后写入
    TargetNotAllowed,
    /// 本体尚未定义该属性 → 记录后按值自身类型写入
    UndefinedProperty,
}

impl PropertyWriteIssue {
    /// 是否导致本条**不写库**
    ///
    /// 与「是否报错」是两件事：被拒的条只进报告，不中断整批——批量写入
    /// 的既有哲学是「一条坏不连累一批」。
    pub fn rejects_write(&self) -> bool {
        matches!(self, Self::TypeMismatch { .. } | Self::CardinalityExceeded)
    }

    /// 人类可读原因（写进报告、日志与前端提示，不另建文案表）
    pub fn describe(&self) -> String {
        match self {
            Self::TypeMismatch { expected } => {
                format!("值类型与属性声明不符（应为 {}）", expected.as_str())
            }
            Self::NumberOutOfRange => "数值超出属性定义的取值区间".to_string(),
            Self::NotInEnumValue => "枚举值不在属性定义的白名单内".to_string(),
            Self::CardinalityExceeded => "超出属性基数（单值属性不可挂多条）".to_string(),
            Self::TargetNotAllowed => "属性定义不允许挂到该载体".to_string(),
            Self::UndefinedProperty => "本体尚未定义该属性".to_string(),
        }
    }
}

/// 一条属性的判定结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PropertyIssue {
    /// 出问题的属性锚点
    pub term_key: String,
    /// 问题类别
    pub kind: PropertyWriteIssue,
}

/// 属性写入报告
///
/// ⚠️ 语义是「**部分成功**」：非法条目只跳过，不回滚已通过的条目，
/// 也不返回 `Err`。调用方（Domain / Handler）按需把 `issues` 翻成告警。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PropertySaveReport {
    /// 成功写入的条数
    pub accepted: usize,
    /// 判定问题清单（含被拒与仅告警两类）
    pub issues: Vec<PropertyIssue>,
}

impl PropertySaveReport {
    /// 是否全部通过（无告警无拒绝）
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }

    /// 被拒绝（未写库）的问题条
    pub fn rejected(&self) -> impl Iterator<Item = &PropertyIssue> {
        self.issues.iter().filter(|i| i.kind.rejects_write())
    }

    /// 仅告警（已写库，但值得看一眼）的问题条
    pub fn warnings(&self) -> impl Iterator<Item = &PropertyIssue> {
        self.issues.iter().filter(|i| !i.kind.rejects_write())
    }
}

/// 属性反查的比较运算符
///
/// ⚠️ 刻意不做 `Between` / `Contains` / `Like`：区间用 `Gte` + `Lte` 两条组合表达，
/// 模糊匹配留给既有的 FTS5 全文通道，不在这里开第二套文本检索。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PropertyFilterOp {
    /// 等于
    Eq,
    /// 大于（仅数值）
    Gt,
    /// 大于等于（仅数值）
    Gte,
    /// 小于（仅数值）
    Lt,
    /// 小于等于（仅数值）
    Lte,
}

/// 属性反查条件（「查所有 model=xxx 的 agent 节点」的结构化过滤入口）
///
/// 值用 [`PropertyValue`] 枚举承载而不是裸字符串：DAO 据此决定查哪一列
/// （`value_num` / `value_text` / `value_ref`），调用方不需要知道列映射。
///
/// 多个条件取 **AND**：调用方多次查询后对结果集取交集，不在 SQL 里堆动态条件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PropertyFilter {
    /// 属性语义锚点
    pub term_key: String,
    /// 限定载体（节点 / 边）；反查通常只查一种
    pub target_type: PropertyTargetType,
    /// 比较运算符
    pub op: PropertyFilterOp,
    /// 比较值；变体决定查哪个值列
    pub value: PropertyValue,
}

impl PropertyFilter {
    /// 运算符与值类型是否匹配
    ///
    /// 非数值类型只支持 `Eq`（文本/枚举/引用/日期没有可比较的序，布尔只有两个取值）。
    /// 不匹配的组合返回 `false`，**由调用方决定拦还是返回空结果**——本模块只做纯判定。
    pub fn is_supported(&self) -> bool {
        match self.op {
            PropertyFilterOp::Eq => true,
            _ => matches!(self.value, PropertyValue::Number(_)),
        }
    }
}

#[cfg(test)]
#[path = "ontology_property_tests.rs"]
mod tests;
