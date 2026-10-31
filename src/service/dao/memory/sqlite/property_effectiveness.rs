//! 属性有效性 DAO：属性取值分布聚合（众数占比统计的数据源）
//!
//! 与 [`super::drift`] 的属性域聚合互补：那边按 `(term_key, node_type)` 计数
//! （回答「挂得对不对」），这边在同一组合内**再按取值分组**（回答「值变不变」）。

use super::MemoryDaoSqliteImpl;
use crate::pkg::RequestContext;
use crate::service::dao::memory::PropertyValueFreqRow;
use common::error::Result;
use common::ontology::normalize;
use sqlx::QueryBuilder;

/// 取值分布聚合基础 SQL（过滤条件由 [`QueryBuilder`] 拼在 WHERE 之后）
///
/// 抽成常量而非内联字面量：证据/联调用例可以复用**同一段** SQL（口径零漂移），
/// 不必各抄一份。
///
/// 取值的「一把尺子」：`knowledge_property` 是四选一值列的泛化表，同一个 term_key
/// 的值可能落在任一列，直接按列分组会把同一语义值拆成多组，故先在子查询里把值
/// 统一序列化成文本（列别名 `value_text`）再分组：
///
/// - `value_text`（string / date / enum 型）→ 原文（**本表无 value_date 列**）
/// - `value_num`（number 型）→ 整数化去尾零：`1.0` → `1`、`0.9` → `0.9`
/// - `value_bool`（boolean 型，列内落 1/0）→ `true` / `false`
/// - `value_ref`（reference 型）→ 引用目标 ID 原文
/// - 四列全 NULL（脏数据）→ 空串：留一个可见的异常组，不静默丢弃
///
/// strict 口径三条，与 `property_domain_freq` 严格一致（缺一不可）：
/// - `p.target_type = 'node'`：边属性（relation）不属于本维度
/// - `p.status = 1`：被取代的历史值不计（同载体同槽位只留一条生效行）
/// - `n.status = 1`：已遗忘节点上的悬挂值不计
pub(super) const PROPERTY_VALUE_FREQ_SQL: &str = r#"
SELECT term_key, node_type, value_text, COUNT(*) AS count
FROM (
    SELECT p.term_key AS term_key,
           n.node_type AS node_type,
           CASE
               WHEN p.value_text IS NOT NULL THEN p.value_text
               WHEN p.value_num IS NOT NULL THEN
                   CASE
                       WHEN p.value_num = CAST(p.value_num AS INTEGER)
                           THEN CAST(CAST(p.value_num AS INTEGER) AS TEXT)
                       ELSE CAST(p.value_num AS TEXT)
                   END
               WHEN p.value_bool IS NOT NULL THEN
                   CASE WHEN p.value_bool <> 0 THEN 'true' ELSE 'false' END
               WHEN p.value_ref IS NOT NULL THEN p.value_ref
               ELSE ''
           END AS value_text
    FROM knowledge_property p
    JOIN long_term_knowledge_node n ON n.id = p.target_id
    WHERE p.target_type = 'node' AND p."status" = 1 AND n."status" = 1"#;

/// 聚合尾部：闭合子查询 + 分组 + 确定性排序（过滤条件插在两者之间）
pub(super) const PROPERTY_VALUE_FREQ_SQL_TAIL: &str = r#"
)
GROUP BY term_key, node_type, value_text
ORDER BY term_key ASC, node_type ASC, count DESC, value_text ASC"#;

impl MemoryDaoSqliteImpl {
    /// 属性取值分布聚合：`(term_key, node_type, 取值)` → 生效条数
    ///
    /// 运行时 `query_as::<_, Row>` 而非编译期 `query_as!`：与 `drift.rs` 既有聚合
    /// 同口径（动态过滤 + 避免为一处 SQL 刷新 `.sqlx` 离线产物）。
    pub(super) async fn property_value_freq(
        &self,
        ctx: RequestContext,
        term_key: Option<String>,
        node_type: Option<String>,
    ) -> Result<Vec<PropertyValueFreqRow>> {
        let pool = self.pool(ctx);
        let mut builder = QueryBuilder::new(PROPERTY_VALUE_FREQ_SQL);
        // 过滤条件在入库规范形上比对：入口先 normalize，与写入侧单点归一同一口径
        if let Some(term) = term_key.as_deref().map(normalize).filter(|s| !s.is_empty()) {
            builder.push(" AND p.term_key = ");
            builder.push_bind(term);
        }
        if let Some(class) = node_type
            .as_deref()
            .map(normalize)
            .filter(|s| !s.is_empty())
        {
            builder.push(" AND n.node_type = ");
            builder.push_bind(class);
        }
        builder.push(PROPERTY_VALUE_FREQ_SQL_TAIL);
        let rows = builder
            .build_query_as::<PropertyValueFreqRow>()
            .fetch_all(&pool)
            .await?;
        Ok(rows)
    }
}
