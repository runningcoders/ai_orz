//! 本体域枚举

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 本体词表条目状态（对齐全库软删除约定）
///
/// 只表达词表条目自身的生命周期：1 正常（可被新写入引用）/ 0 退役。
/// ⚠️ **退役 ≠ 删除**：退役词的历史图谱存量引用仍需可解释（读侧照常展示、
/// 照常参与 resolve 之外的解释），只是写侧不再允许新引用 —— 管理页下架
/// 一个关系词，不能让既有连线一夜之间"消失"或退化成无语义的裸边。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum OntologyStatus {
    /// 退役 - 0：写侧不再允许新引用，历史存量引用保留可解释
    Retired = 0,
    /// 正常 - 1：可被新写入引用
    #[default]
    Active = 1,
}

impl From<i32> for OntologyStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => OntologyStatus::Retired,
            _ => OntologyStatus::Active,
        }
    }
}

impl From<i64> for OntologyStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl OntologyStatus {
    /// 转换为 i32 用于数据库存储
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

/// 属性值的数据类型（本体属性定义 + 节点属性值共用）
///
/// 落库形态为小写字符串（`"string"` / `"number"` / ...），与迁移 DDL 的
/// `CHECK (data_type IN (...))` 取值逐字对齐；序列化小写由 `rename_all` 保证。
///
/// 它决定属性值读哪一列：`knowledge_property` 的四个类型化值列
/// （`value_text` / `value_num` / `value_bool` / `value_ref`）四选一，
/// 由本枚举指派——未使用的列落 NULL，不出现「一列存所有类型」的歧义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PropertyDataType {
    /// 文本（默认）
    #[default]
    String,
    /// 数值（整数与浮点共用 REAL 列，支持范围查询）
    Number,
    /// 布尔（0 / 1）
    Boolean,
    /// 日期时间（存文本，ISO-8601 或秒级时间戳字符串，口径由属性定义方声明）
    Date,
    /// 引用（指向另一个知识节点 id 或外部实体标识）
    Reference,
    /// 枚举（取值受属性定义 `value_range` 白名单约束）
    Enum,
}

impl PropertyDataType {
    /// 落库 / 序列化字符串（与 DDL CHECK 枚举逐字一致）
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Date => "date",
            Self::Reference => "reference",
            Self::Enum => "enum",
        }
    }

    /// 合法取值清单（供错误提示列出，让调用方能自我纠正）
    ///
    /// 与 DDL `CHECK (data_type IN (...))` 同源：新增变体时**两处都要改**，
    /// 否则写侧校验与落库约束会错位（先过校验后撞 CHECK，报的是 500 而非 400）。
    pub const ACCEPTED_VALUES: &'static str = "string, number, boolean, date, reference, enum";
}

impl std::str::FromStr for PropertyDataType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "string" => Ok(Self::String),
            "number" => Ok(Self::Number),
            "boolean" => Ok(Self::Boolean),
            "date" => Ok(Self::Date),
            "reference" => Ok(Self::Reference),
            "enum" => Ok(Self::Enum),
            _ => Err(()),
        }
    }
}

impl PropertyDataType {
    /// 词表行脏数据兜底解析：非法值一律退 [`PropertyDataType::String`]
    /// （与本体「软门禁」哲学一致：脏数据退化为最宽松类型，不 panic 不阻断）
    pub fn parse_or_default(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

/// 属性值的挂载载体类型（`knowledge_property.target_type`）
///
/// 属性值表是**图谱域内通用**的：一张 `knowledge_property` 同时承载节点属性
/// 与边属性，靠本枚举区分 `target_id` 指向哪张表。
///
/// ⚠️ 作用域边界：只有**知识图谱域内**的实体（节点 / 边）可以挂载。
/// 不扩展到 agent / task / project —— 那些表带 `organization_id`，
/// 而图谱与本体表一律不带（部署单元即作用域），混进一张表会撕裂作用域约定。
///
/// 与 `target_id` 始终成对出现：单独用 `target_id` 无法判定归属表
/// （全库无外键，靠应用层解释）。新增载体类型时须同步迁移的 CHECK 约束。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PropertyTargetType {
    /// 知识节点 → `long_term_knowledge_node.id`
    #[default]
    Node,
    /// 知识关系边 → `knowledge_node_relation.id`
    Relation,
}

impl PropertyTargetType {
    /// 落库 / 序列化字符串（与 DDL CHECK 枚举逐字一致）
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Relation => "relation",
        }
    }

    /// 合法取值清单（供错误提示列出）
    ///
    /// 与 DDL `CHECK (target_type IN (...))` 同源；新增载体时须同步迁移的 CHECK。
    pub const ACCEPTED_VALUES: &'static str = "node, relation";
}

impl std::str::FromStr for PropertyTargetType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "node" => Ok(Self::Node),
            "relation" => Ok(Self::Relation),
            _ => Err(()),
        }
    }
}

impl PropertyTargetType {
    /// 行数据兜底解析：非法值一律退 [`PropertyTargetType::Node`]
    /// （与本体「软门禁」哲学一致：脏行退化为默认载体，不 panic 不阻断）
    pub fn parse_or_default(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

/// 属性基数（单值 / 多值）
///
/// 决定 `knowledge_property.ordinal` 的用法：单值恒 0（唯一索引
/// `(target_type, target_id, term_key, ordinal)` 因此退化为
/// 「一实体一属性一条生效」）；多值按槽位递增，同一属性可挂多条生效行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum Cardinality {
    /// 单值（默认）：一个节点该属性最多一条生效值
    #[default]
    Single,
    /// 多值：一个节点该属性可有多条生效值（按 ordinal 排序）
    Many,
}

impl Cardinality {
    /// 落库 / 序列化字符串（与 DDL CHECK 枚举逐字一致）
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Many => "many",
        }
    }

    /// 合法取值清单（供错误提示列出）
    ///
    /// 与 DDL `CHECK (cardinality IN (...))` 同源。
    pub const ACCEPTED_VALUES: &'static str = "single, many";
}

impl std::str::FromStr for Cardinality {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "single" => Ok(Self::Single),
            "many" => Ok(Self::Many),
            _ => Err(()),
        }
    }
}

impl Cardinality {
    /// 词表行脏数据兜底解析：非法值一律退 [`Cardinality::Single`]（保守取值，
    /// 避免脏数据被解释为多值后放开写入量）
    pub fn parse_or_default(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}
