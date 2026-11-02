<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 12-基础设施与部署

> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。
>
> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。

本模块共 89 篇：A 设计 5 · B 落地方案 5 · C 知识 79。

## A 设计（design）

- [common 统一错误类型设计](<A-设计/common_error_type.md>) — `已废弃`
- [日志系统设计文档](<A-设计/logging_design.md>) — `现行`
- [模型访问模式（access_mode）UI 设计规范](<A-设计/model_provider_access_mode_design.md>) — `现行`
- [Seed 配置迁移中心](<A-设计/seed_config_migration.md>) — `已废弃`
- [SQLx 0.8 + SQLite 开发规范与最佳实践](<A-设计/sqlx_guide.md>) — `现行`

## B 落地方案（plan）

- [Clippy Warning Cleanup Implementation Plan](<B-落地方案/Clippy Warning Cleanup（执行蓝图）.md>) — `已完成`
- [Seed 配置迁移中心 实施计划](<B-落地方案/Seed 配置迁移中心 实施计划（执行蓝图）.md>) — `已完成`
- [容器化与网关部署方案](<B-落地方案/容器化与网关部署方案.md>) — `已完成`
- [系统初始化模型配置策略调整](<B-落地方案/系统初始化模型配置策略调整.md>) — `已完成`
- [系统管理工具（备份 + 日志查询 + 角色权限）实现计划](<B-落地方案/系统管理工具（备份 + 日志查询 + 角色权限）实现计划（执行蓝图）.md>) — `已完成`

## C 知识（wiki）

- [A2A 轮询生产者](<C-知识/A2A 轮询生产者.md>)
- [AOP 核心架构](<C-知识/AOP 核心架构.md>)
- [AOP监控处理器](<C-知识/AOP监控处理器.md>)
- [Agent 循环消费者（框架层）](<C-知识/Agent 循环消费者.md>)
- [Doctor 依赖预检](<C-知识/Doctor 依赖预检/Doctor 依赖预检.md>)
- [Doctor 依赖预检](<C-知识/Doctor 依赖预检.md>)
- [Finance模块处理器](<C-知识/Finance模块处理器.md>)
- [HNSW 索引存储后端](<C-知识/HNSW 索引存储后端.md>)
- [LanceDB 存储后端](<C-知识/LanceDB 存储后端.md>)
- [MCP 服务器管理 API](<C-知识/MCP 服务器管理 API.md>)
- [MCP服务器管理处理器](<C-知识/MCP服务器管理处理器.md>)
- [Rust Workspace + NPM 多语言依赖管理（Cargo.lock / package-lock.json / SQLx 离线构建）](<C-知识/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）.md>)
- [SQLite VSS 存储后端](<C-知识/SQLite VSS 存储后端.md>)
- [System模块处理器](<C-知识/System模块处理器.md>)
- [AI Orz 设计与落地方案文档维护 Skill（规范全文版）](<C-知识/ai-orz-doc-maintainer.md>)
- [AI Orz Wiki 维护 Skill（规范全文版）](<C-知识/ai-orz-wiki-maintainer.md>)
- [pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语](<C-知识/pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语/pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语.md>)
- [scripts/ 归置 + Makefile 统一入口 + 60+ 引用全量修正](<C-知识/scripts 归置 + Makefile 统一入口 + 60+ 引用全量修正/scripts 归置 + Makefile 统一入口 + 60+ 引用全量修正.md>)
- [任务管理 API](<C-知识/任务管理 API.md>)
- [任务管理处理器](<C-知识/任务管理处理器.md>)
- [优雅关闭系统](<C-知识/优雅关闭系统.md>)
- [内存向量存储后端](<C-知识/内存向量存储后端.md>)
- [后台任务系统](<C-知识/后台任务系统.md>)
- [后台进程管理系统](<C-知识/后台进程管理系统.md>)
- [向量存储（框架层）](<C-知识/向量存储.md>)
- [向量存储抽象接口（框架层）](<C-知识/向量存储抽象接口.md>)
- [基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）](<C-知识/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）.md>)
- [基础设施](<C-知识/基础设施.md>)
- [备份恢复 API](<C-知识/备份恢复 API.md>)
- [备份恢复管理（业务功能层）](<C-知识/备份恢复管理.md>)
- [备份管理处理器](<C-知识/备份管理处理器.md>)
- [存储系统（框架层）](<C-知识/存储系统.md>)
- [存储配置](<C-知识/存储配置.md>)
- [定时任务 API](<C-知识/定时任务 API.md>)
- [定时任务处理器](<C-知识/定时任务处理器.md>)
- [容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像](<C-知识/容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像/容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像.md>)
- [工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆](<C-知识/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆.md>)
- [工具执行日志消费者](<C-知识/工具执行日志消费者.md>)
- [工具执行统计消费者](<C-知识/工具执行统计消费者.md>)
- [工具管理 API（API 参考层）](<C-知识/工具管理 API.md>)
- [工具管理处理器](<C-知识/工具管理处理器.md>)
- [思考轮次统计消费者](<C-知识/思考轮次统计消费者.md>)
- [持续集成与发布工作流](<C-知识/持续集成与发布工作流.md>)
- [数据库迁移管理](<C-知识/数据库迁移管理/数据库迁移管理.md>)
- [文档链接质量门禁](<C-知识/文档链接质量门禁.md>)
- [日志查询 API](<C-知识/日志查询 API.md>)
- [日志查询处理器](<C-知识/日志查询处理器.md>)
- [日志管理系统（系统管理层）](<C-知识/日志管理系统.md>)
- [日志系统](<C-知识/日志系统.md>)
- [时间戳规范](<C-知识/时间戳规范/时间戳规范.md>)
- [服务层](<C-知识/服务层.md>)
- [本地文件安全与工作区隔离](<C-知识/本地文件安全与工作区隔离.md>)
- [模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片](<C-知识/模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片/模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片.md>)
- [模型提供商管理 API](<C-知识/模型提供商管理 API.md>)
- [模型提供商管理（业务功能层）](<C-知识/模型提供商管理.md>)
- [模型提供商管理处理器](<C-知识/模型提供商管理处理器.md>)
- [模型提供商配置](<C-知识/模型提供商配置.md>)
- [模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析](<C-知识/模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析/模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析.md>)
- [消息处理 API](<C-知识/消息处理 API.md>)
- [消息消费者（框架层）](<C-知识/消息消费者.md>)
- [消息渠道管理 API](<C-知识/消息渠道管理 API.md>)
- [消息渠道管理处理器](<C-知识/消息渠道管理处理器.md>)
- [消息系统处理器](<C-知识/消息系统处理器.md>)
- [消息通道生产者](<C-知识/消息通道生产者.md>)
- [监控指标 API](<C-知识/监控指标 API.md>)
- [种子数据处理器](<C-知识/种子数据处理器.md>)
- [种子数据管理 API](<C-知识/种子数据管理 API.md>)
- [种子数据管理（业务功能层）](<C-知识/种子数据管理.md>)
- [种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离](<C-知识/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发.md>)
- [系统初始化](<C-知识/系统初始化.md>)
- [系统管理](<C-知识/系统管理.md>)
- [系统管理模块 API](<C-知识/系统管理模块 API.md>)
- [脱敏引擎下沉 common](<C-知识/脱敏引擎下沉 common.md>)
- [脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏](<C-知识/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏.md>)
- [财务管理模块 API](<C-知识/财务管理模块 API.md>)
- [配置与部署](<C-知识/配置与部署.md>)
- [配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入](<C-知识/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入.md>)
- [附件管理 API](<C-知识/附件管理 API.md>)
- [附件管理处理器](<C-知识/附件管理处理器.md>)
