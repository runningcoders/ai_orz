-- 关系强度（knowledge_node_relation.weight）
--
-- 背景：关系边此前只有 relation_type 一列，图上所有连线长得一模一样 ——
-- 「A 依赖 B」和「A 与 B 顺带相关」视觉上无法区分，用户看不出哪条关联更值得看。
-- 新增 weight 表达「这条关联有多强」（0.0~1.0），图谱按它调线宽与浓淡，
-- hover 时展示数值。
--
-- 为什么是「写入方声明」而不是「框架派生」：
--   - 可派生的候选（共现证据数）依赖 knowledge_reference，而节点写入路径
--     （save_long_term_memory / create_memory）恒传 `references: vec![]`，
--     该表实际为空 → 派生值恒 0，等于没信息；
--   - 唯一知道「这两个概念关联有多强」的是产出这条关系的模型，
--     所以由 save_long_term_memory 的 relations[].weight 声明，
--     缺省不落值（NULL = 未标注），前端渲染基准线宽而不是伪造一个 0.5。
--
-- 可空、无默认值：存量行一律 NULL，语义为「未标注」，与「强度 0」区分开。
-- 写入侧在 handler 里夹紧到 0.0~1.0（DB 不做约束，避免非法值直接写库报错）。

ALTER TABLE knowledge_node_relation ADD COLUMN weight REAL;
