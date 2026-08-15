# AI Orz - Code Wiki（入口页）

> 🎯 **本文档定位**：代码现状百科入口页（由 IDE 自动生成，随代码演进重生成；手工修改下次重生成会覆盖）——用于快速查文件/模块作用、docs 四象限索引、模块速查表
>
> 状态：IDE 自动生成 + 入口页手工维护（四象限与速查表定期人工复核）
>
> 查阅场景：
> - 不记得某模块在哪个文件 → 查模块速查表
> - 不知道该读 AGENTS/ARCHITECTURE/design/plan/wiki 哪份文档 → 查 §docs 内容脉络四象限表
> - 需要字段级实现细节 → 跳 [wiki/](./wiki/) 自动生成内容或直接打开对应源文件
>
> 关联文档：
> - [AGENTS.md](../AGENTS.md) — 架构规范与开发约定强制执行手册
> - [ARCHITECTURE.md](./ARCHITECTURE.md) — 唯一权威架构总纲（概念/关系/分层边界）

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

### Wiki 主分类导航（IDE 生成，随代码演进）

wiki 位于 `docs/wiki/ → ../.qoder/repowiki`（软链接），按主题分中文内容在 `zh/content/`：

| 分类 | 路径（相对 docs/wiki） | 包含内容 |
|------|----------------------|---------|
| 项目总览 | [zh/content/项目概述](./wiki/zh/content/项目概述) | 项目介绍与目标、核心功能特性、快速开始指南、技术栈概览、项目结构说明 |
| 架构设计 | [zh/content/架构设计](./wiki/zh/content/架构设计) | 整体架构概览、记忆系统架构、数据存储架构、安全架构、分层架构设计（Domain/DAL/DAO/Handler 编排）、AOP 事件系统架构、API 协议规范 |
| 核心模块 | [zh/content/核心模块](./wiki/zh/content/核心模块) | 服务层（DAO/DAL/Domain）、处理器层、路由中间件、存储系统（向量+全文）、工具注册表、AOP 事件系统 |
| 基础设施 | [zh/content/基础设施](./wiki/zh/content/基础设施) | AOP 事件中心（生产者/消费者/队列/监控）、存储系统（LanceDB/HNSW/InMemory/SQLite VSS 四后端）、工具注册表、日志、请求上下文、优雅关闭、CI/Release |
| 功能模块 | [zh/content/功能模块](./wiki/zh/content/功能模块) | 用户组织、Agent/工具/技能、消息系统、项目管理、系统管理（AOP/定时/备份/日志/健康）、模型提供商、MCP 集成 |
| 数据模型 | [zh/content/数据模型](./wiki/zh/content/数据模型) | Agent+技能、消息+记忆、项目+任务、系统模型、文件+存储 |
| 前端应用 | [zh/content/前端应用](./wiki/zh/content/前端应用) | 页面模块、组件系统、钩子、API 客户端、UI 样式与主题、前端架构 |
| API 参考 | [zh/content/API 参考](./wiki/zh/content/API%20参考) | RESTful 各业务域、A2A 协议、WebSocket/SSE、认证授权、错误码 |
| 测试与运维 | [zh/content/测试指南](./wiki/zh/content/测试指南) / [开发指南](./wiki/zh/content/开发指南.md) / [故障排除与监控](./wiki/zh/content/故障排除与监控.md) / [配置与部署](./wiki/zh/content/配置与部署.md) | E2E 基础设施、开发规范、排障手册、部署方法 |
