# docs/ 文档地图

> 按「问题类型」找文档，四象限分工，避免现状描述与历史决策互相追赶。

## 四象限

| 目录 / 文档 | 回答的问题 | 性质与维护方式 |
|-------------|-----------|----------------|
| [wiki/](./wiki/) | **是什么**：代码现状百科 | IDE 生成（源自 `.qoder/repowiki`），随代码演进再生成；**阅读第一站** |
| [ARCHITECTURE.md](./ARCHITECTURE.md) | **权威纲要**：核心概念与实体关系 | 手工维护，唯一权威架构总纲 |
| [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md) | **怎么做**：分层实践与避坑 | 手工维护，开发必读 |
| [CODE_WIKI.md](./CODE_WIKI.md) | **从哪开始**：模块速查 + 导航 | 入口页 |
| [design/](./design/) | **为什么**：当时的设计决策 | 决策快照，写定后不追赶现状 |
| [plan/](./plan/) | **要去哪**：规划与状态快照 | 按阶段追加 |
| [archive/](./archive/) | **被什么取代**：历史方案归档 | 只进不出 |

## 维护规则

1. 代码现状变化 → 再生成 wiki；**不**回头改 design/ 旧文（不一致时以 wiki 为准）
2. 开发规范变化 → 更新根目录 AGENTS.md 与 LAYERED 实践文档
3. 架构核心概念变化 → 更新 ARCHITECTURE.md
4. 新功能设计 → 先写 design/ 新文档，落地后由 wiki 承接现状描述
5. 被取代的方案 → 移入 archive/，原位置不留副本
