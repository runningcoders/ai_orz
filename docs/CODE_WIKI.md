# AI Orz - Code Wiki（入口页）

> 🎯 **本文档是代码认知的入口页**，不再是全景正文。
>
> 代码现状的完整百科（模块职责、架构、技术栈、编码约定）由知识库 Wiki 承载；
> 本页只保留最小速查表与导航。完整版历史正文见 git 历史（2026-07-25 版）。

---

## docs 内容脉络（四象限）

| 目录 / 文档 | 回答的问题 | 性质与维护方式 |
|-------------|-----------|----------------|
| [wiki/](./wiki/) | **是什么**：代码现状百科 | IDE 生成，随代码演进再生成；**阅读第一站** |
| [ARCHITECTURE.md](./ARCHITECTURE.md) | **权威纲要**：核心概念与实体关系 | 手工维护，唯一权威架构总纲 |
| [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md) | **怎么做**：分层实践与避坑 | 手工维护，开发必读 |
| [design/](./design/) | **为什么**：当时的设计决策 | 决策快照，写定后不追赶现状 |
| [plan/](./plan/) | **要去哪**：规划与状态快照 | 按阶段追加 |
| [archive/](./archive/) | **被什么取代**：历史方案归档 | 只进不出 |

> ⚠️ 当 design/ 与实现不一致时，以 wiki/ 描述的现状为准；design/ 保留决策当时的上下文。

---

## 模块速查表

| 层级 | 位置 | 一句话职责 |
|------|------|-----------|
| 适配层 Handler | `src/handlers/` | 用户 API + 外部回调，协议解析 → Domain 调用 |
| 适配层 Producer | `src/producer/` | 轮询 / 外部 WS 事件接入 |
| Consumer | `src/consumer/` | 消费 Domain 产生的内部事件 |
| Domain | `src/service/domain/` | 核心业务编排，7 个领域 |
| DAL | `src/service/dal/` | 组合 DAO，PO ↔ 业务实体转换 |
| DAO | `src/service/dao/` | 单一数据源 CRUD + 外部 API 出站 |
| 基础设施 | `src/pkg/` | AOP / 存储 / 日志 / 工具注册等，业务无感知 |
| 前端 | `frontend/src/` | Dioxus 0.7 + Tailwind v4 + DaisyUI v5 |
| 公共 | `common/src/` | 前后端共享 DTO / 枚举 / 错误模型 |

## 深入阅读

- 架构全貌 → [ARCHITECTURE.md](./ARCHITECTURE.md) + [wiki/](./wiki/)
- 分层规范与避坑 → [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md)
- 强制开发规范 → [../AGENTS.md](../AGENTS.md)
- 某子系统的设计动机 → [design/](./design/) 对应文档
