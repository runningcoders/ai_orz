<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 09-统计与监控

> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。
>
> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。

本模块共 17 篇：A 设计 3 · B 落地方案 6 · C 知识 8。

## A 设计（design）

- [统计模块设计 — 双层实现：DuckDB 持久化 + 内存实时](<A-设计/stats_module_design.md>) — `已废弃`
- [统计查询模块设计方案](<A-设计/stats_query_design.md>) — `已废弃`
- [工作台顶栏聚合指标设计](<A-设计/workspace_topbar_metrics_design.md>) — `现行`

## B 落地方案（plan）

- [Finance 域统计面板补充计划](<B-落地方案/Finance 域统计面板补充计划（执行蓝图）.md>) — `已完成`
- [Responses API 接入与推理统计扩展 实施计划](<B-落地方案/Responses API 接入与推理统计扩展.md>) — `进行中`
- [实体统计注入](<B-落地方案/实体统计注入.md>) — `已完成`
- [统计图表Phase1基础设施与时序图展示重构](<B-落地方案/统计图表Phase1基础设施与时序图展示重构.md>) — `已完成`
- [统计图表 Phase 2：环形图 + Project 任务状态分布](<B-落地方案/统计图表Phase2.md>) — `已完成`
- [统计图表第三期（AOP 实时内存统计 + 轮询渲染）](<B-落地方案/统计图表第三期.md>) — `已完成`

## C 知识（wiki）

- [DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表](<C-知识/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md>)
- [Lark P2P WS 私信入站（凭证引用解析 + 多渠道按 app_id 聚合 + 用户身份自动映射 + 健康指标）](<C-知识/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标.md>)
- [健康检查处理器](<C-知识/健康检查处理器.md>)
- [底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（LanceDB 默认生产 → InMemory/Hnsw 测试备选）](<C-知识/底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（LanceDB 默认生产 _ InMemory_Hnsw 测试备选）/底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（InMemory_Hnsw 默认 _ LanceDB 大规模）.md>)
- [故障排除与监控](<C-知识/故障排除与监控.md>)
- [系统监控与健康检查](<C-知识/系统监控与健康检查.md>)
- [统计与监控](<C-知识/统计与监控.md>)
- [统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示](<C-知识/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示.md>)
