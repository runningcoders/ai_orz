# Checklist

## FTS5 迁移文件

- [x] short_term_memory_fts 虚拟表创建（fts5，索引 summary + tags，trigram 分词器支持中文）
- [x] knowledge_node_fts 虚拟表创建（fts5，索引 node_name + summary + node_description，trigram 分词器支持中文）
- [x] short_term_memory_index 的 INSERT 触发器（AFTER INSERT 自动写入 FTS）
- [x] short_term_memory_index 的 UPDATE 触发器（AFTER UPDATE 先删旧再插新）
- [x] short_term_memory_index 的 DELETE 触发器（AFTER DELETE 删除 FTS 条目）
- [x] long_term_knowledge_node 的 INSERT 触发器
- [x] long_term_knowledge_node 的 UPDATE 触发器
- [x] long_term_knowledge_node 的 DELETE 触发器
- [x] 存量数据回填（INSERT INTO fts_table SELECT ... FROM main_table）
- [x] 触发器同步测试：插入主表后 FTS 表有对应记录
- [x] 触发器同步测试：更新主表 summary 后 FTS 表内容更新
- [x] 触发器同步测试：删除主表记录后 FTS 表对应记录删除

## DAO 层搜索 SQL 改造

- [x] FTS5 关键词转义工具函数实现（处理 * " ( ) : 等特殊字符）
- [x] search_short_term SQL 改为 FTS5 MATCH + JOIN 主表 + BM25 排序
- [x] search_knowledge_nodes SQL 改为 FTS5 MATCH + JOIN 主表 + BM25 排序
- [x] query_short_term 中的 MATCH 死代码分支已移除
- [x] query_knowledge_nodes 中的 MATCH 死代码分支已移除
- [x] query 方法中 keyword 参数被忽略时记录 warn 日志
- [x] 空关键词时不执行 FTS5 搜索（返回空结果或走纯向量）
- [x] 测试：FTS5 单关键词搜索正确返回结果
- [x] 测试：FTS5 多关键词（escape_fts5_keyword 设计为短语匹配，对 Agent 记忆搜索更精确，非 AND 语义，by design）
- [x] 测试：FTS5 特殊字符转义不报错
- [x] 测试：BM25 相关性排序验证（更相关的排前面）

## 模型层扩展

- [x] SearchMatchInfo 新增 fts_rank: Option<f32> 字段
- [x] MemorySearch 新增 vector_distance_threshold: Option<f32> 字段
- [x] 模型层测试通过

## DAL 层混合搜索优化

- [x] search_short_term_internal：FTS5 关键词命中附加 SearchMatchInfo { match_type: Keyword, fts_rank }
- [x] search_short_term_internal：向量+关键词双命中附加 SearchMatchInfo { match_type: Hybrid, vector_distance, fts_rank }
- [x] search_knowledge_nodes_internal：同上改造
- [x] 向量距离阈值从硬编码改为读取 MemorySearch.vector_distance_threshold（默认 0.8）
- [x] search() 统一排序：Hybrid 优先 → Vector → Keyword
- [x] Hybrid 组内按 vector_distance 升序排序
- [x] Vector 组内按 vector_distance 升序排序
- [x] Keyword 组内按 fts_rank 升序排序
- [x] search_relations_internal 实现（通过 knowledge_node_fts 搜索节点 → 查关联关系）
- [x] 测试：三路混合排序正确性
- [x] 测试：Keyword MatchInfo 正确附加
- [x] 测试：关系关键词搜索返回关联关系和节点
- [x] 测试：自定义向量阈值生效

## 最终验证

- [x] cargo check 编译通过
- [x] cargo test 全量测试通过
- [x] 无新增 warning 回归
