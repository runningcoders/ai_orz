<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 模块知识索引（docs/modules）

> **自动生成**：生成器 `tools/wiki_index.py` · 门禁 `make wiki-lint` · 重建 `make wiki-index`
>
> 覆盖三个层：`A-设计`（为什么）· `B-落地方案`（要去哪）· `C-知识`（是什么）。
> 机器可读版见 `INDEX.json`；Agent 实现需求请从 `docs/AGENT_GUIDE.md` 进入。

**合计 671 篇 / 14,103,964 B**，分布于 15 个模块。

## 模块概览

| 模块 | A 设计 | B 落地方案 | C 知识 | 合计 | 字节 |
|---|---:|---:|---:|---:|---:|
| [00-总览与架构](#00-总览与架构) | 0 | 2 | 35 | 37 | 652,240 |
| [01-组织与权限](#01-组织与权限) | 4 | 7 | 36 | 47 | 904,755 |
| [02-Agent运行时](#02-agent运行时) | 10 | 16 | 19 | 45 | 1,233,155 |
| [03-记忆系统](#03-记忆系统) | 1 | 4 | 13 | 18 | 385,177 |
| [04-对话与消息](#04-对话与消息) | 6 | 9 | 21 | 36 | 823,727 |
| [05-任务与项目](#05-任务与项目) | 5 | 6 | 53 | 64 | 1,175,298 |
| [06-工具与技能](#06-工具与技能) | 11 | 14 | 52 | 77 | 1,775,774 |
| [07-外部Agent(A2A)](#07-外部agenta2a) | 3 | 3 | 20 | 26 | 515,641 |
| [08-搜索](#08-搜索) | 5 | 10 | 21 | 36 | 850,094 |
| [09-统计与监控](#09-统计与监控) | 3 | 6 | 8 | 17 | 299,639 |
| [10-异步基建(AOP)](#10-异步基建aop) | 3 | 3 | 32 | 38 | 787,588 |
| [11-前端应用](#11-前端应用) | 3 | 30 | 49 | 82 | 1,934,883 |
| [12-基础设施与部署](#12-基础设施与部署) | 5 | 5 | 79 | 89 | 1,670,953 |
| [13-数据模型](#13-数据模型) | 5 | 4 | 39 | 48 | 881,846 |
| [14-测试](#14-测试) | 2 | 5 | 4 | 11 | 213,194 |

## 各模块文档

### 00-总览与架构

#### B 落地方案（plan，2 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [superpowers/plans 执行蓝图归档（2026-08-15）](<00-总览与架构/B-落地方案/superpowers 执行蓝图说明.md>) | 已完成 | 835 |
| 2 | [项目遗留事项 TODO](<00-总览与架构/B-落地方案/项目遗留事项 TODO（2026-08-15）.md>) | 已完成 | 6,184 |

#### C 知识（wiki，35 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 事件系统架构](<00-总览与架构/C-知识/AOP 事件系统架构.md>) | 20,475 |
| 2 | [API协议规范](<00-总览与架构/C-知识/API协议规范.md>) | 17,199 |
| 3 | [DAL 层组合](<00-总览与架构/C-知识/DAL 层组合.md>) | 20,010 |
| 4 | [Domain 层编排](<00-总览与架构/C-知识/Domain 层编排.md>) | 18,060 |
| 5 | [Finance 领域编排](<00-总览与架构/C-知识/Finance 领域编排.md>) | 19,324 |
| 6 | [HR 领域编排](<00-总览与架构/C-知识/HR 领域编排.md>) | 19,948 |
| 7 | [HR领域](<00-总览与架构/C-知识/HR领域.md>) | 20,410 |
| 8 | [Handler 层设计](<00-总览与架构/C-知识/Handler 层设计.md>) | 14,149 |
| 9 | [Message 领域编排](<00-总览与架构/C-知识/Message 领域编排.md>) | 15,380 |
| 10 | [Organization 领域编排](<00-总览与架构/C-知识/Organization 领域编排.md>) | 19,344 |
| 11 | [PO 与业务实体分层](<00-总览与架构/C-知识/PO 与业务实体分层.md>) | 14,821 |
| 12 | [Project 领域编排](<00-总览与架构/C-知识/Project 领域编排.md>) | 13,296 |
| 13 | [Runtime 领域编排](<00-总览与架构/C-知识/Runtime 领域编排.md>) | 39,527 |
| 14 | [System 领域编排](<00-总览与架构/C-知识/System 领域编排.md>) | 16,354 |
| 15 | [分层架构设计](<00-总览与架构/C-知识/分层架构设计.md>) | 15,767 |
| 16 | [分页与计数规范](<00-总览与架构/C-知识/分页与计数规范.md>) | 12,402 |
| 17 | [前端架构设计](<00-总览与架构/C-知识/前端架构设计.md>) | 38,856 |
| 18 | [功能模块](<00-总览与架构/C-知识/功能模块.md>) | 15,675 |
| 19 | [处理器层](<00-总览与架构/C-知识/处理器层.md>) | 15,227 |
| 20 | [安全架构设计](<00-总览与架构/C-知识/安全架构设计.md>) | 13,907 |
| 21 | [工具系统三层调用架构](<00-总览与架构/C-知识/工具系统三层调用架构/工具系统三层调用架构.md>) | 5,227 |
| 22 | [开发指南](<00-总览与架构/C-知识/开发指南.md>) | 16,108 |
| 23 | [思维轨迹架构](<00-总览与架构/C-知识/思维轨迹架构.md>) | 5,377 |
| 24 | [技术栈概览](<00-总览与架构/C-知识/技术栈概览.md>) | 18,804 |
| 25 | [数据存储架构](<00-总览与架构/C-知识/数据存储架构.md>) | 29,357 |
| 26 | [数据库迁移管理](<00-总览与架构/C-知识/数据库迁移管理.md>) | 33,563 |
| 27 | [整体架构概览](<00-总览与架构/C-知识/整体架构概览.md>) | 13,801 |
| 28 | [时间戳处理规范](<00-总览与架构/C-知识/时间戳处理规范.md>) | 19,987 |
| 29 | [本体子系统架构](<00-总览与架构/C-知识/本体子系统架构.md>) | 6,645 |
| 30 | [架构设计](<00-总览与架构/C-知识/架构设计.md>) | 15,619 |
| 31 | [核心模块](<00-总览与架构/C-知识/核心模块.md>) | 14,086 |
| 32 | [系统领域](<00-总览与架构/C-知识/系统领域.md>) | 14,007 |
| 33 | [统一工具调用架构](<00-总览与架构/C-知识/统一工具调用架构.md>) | 24,008 |
| 34 | [记忆系统架构](<00-总览与架构/C-知识/记忆系统架构.md>) | 30,200 |
| 35 | [领域层](<00-总览与架构/C-知识/领域层.md>) | 18,301 |

### 01-组织与权限

#### A 设计（design，4 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Agent 入职流程设计文档](<01-组织与权限/A-设计/agent_onboarding_design.md>) | 已废弃 | 14,010 |
| 2 | [组织初始化与用户认证模块设计文档](<01-组织与权限/A-设计/organization_design.md>) | 已废弃 | 10,069 |
| 3 | [RequestContext 设计文档](<01-组织与权限/A-设计/request_context_design.md>) | 已废弃 | 20,413 |
| 4 | [用户身份凭证独立表设计（user_credentials）](<01-组织与权限/A-设计/user_credentials_design.md>) | 现行 | 29,300 |

#### B 落地方案（plan，7 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [用户偏好双源沉淀机制（用户表自述 + 组织级图谱总结）](<01-组织与权限/B-落地方案/用户偏好双源设计.md>) | 已完成 | 11,127 |
| 2 | [用户身份凭证独立表落地](<01-组织与权限/B-落地方案/用户身份凭证独立表落地.md>) | 已完成 | 10,352 |
| 3 | [组织组网与去中心化联邦方案（评审稿）](<01-组织与权限/B-落地方案/组织组网与去中心化联邦方案.md>) | 草案 | 17,930 |
| 4 | [联邦鉴权升级方案（静态 token → 组织公钥签名）](<01-组织与权限/B-落地方案/联邦鉴权升级方案.md>) | 进行中 | 34,079 |
| 5 | [RequestContext 调用者类型上下文扩展](<01-组织与权限/B-落地方案/调用者类型上下文.md>) | 已完成 | 13,879 |
| 6 | [跨组织业务调用方案（Phase 2 主体）](<01-组织与权限/B-落地方案/跨组织业务调用方案.md>) | 已完成 | 40,657 |
| 7 | [身份凭证 Domain 层统一 CRUD 重构](<01-组织与权限/B-落地方案/身份凭证Domain统一CRUD重构.md>) | 已完成 | 9,637 |

#### C 知识（wiki，36 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Agent 管理 API](<01-组织与权限/C-知识/Agent 管理 API.md>) | 18,872 |
| 2 | [GitHub 集成](<01-组织与权限/C-知识/GitHub 集成.md>) | 23,369 |
| 3 | [HR模块处理器](<01-组织与权限/C-知识/HR模块处理器.md>) | 19,321 |
| 4 | [IdentityCredentials 身份凭证扩展：WechatIlink 扫码授权 + 凭证整组轮换 + 与通用 CRUD 框架的集成点](<01-组织与权限/C-知识/IdentityCredentials 身份凭证扩展：授权流程 + QR登录 + 多渠道凭证管理/IdentityCredentials 身份凭证扩展：授权流程 + QR登录 + 多渠道凭证管理.md>) | 11,747 |
| 5 | [Organization模块处理器](<01-组织与权限/C-知识/Organization模块处理器.md>) | 19,639 |
| 6 | [Skill处理器](<01-组织与权限/C-知识/Skill处理器.md>) | 21,852 |
| 7 | [User模块处理器](<01-组织与权限/C-知识/User模块处理器.md>) | 14,977 |
| 8 | [人力资源模块 API](<01-组织与权限/C-知识/人力资源模块 API.md>) | 28,183 |
| 9 | [当前组织信息处理器](<01-组织与权限/C-知识/当前组织信息处理器.md>) | 19,424 |
| 10 | [技能包管理 API](<01-组织与权限/C-知识/技能包管理 API.md>) | 23,394 |
| 11 | [用户与组织管理](<01-组织与权限/C-知识/用户与组织管理.md>) | 20,808 |
| 12 | [用户管理](<01-组织与权限/C-知识/用户管理.md>) | 19,428 |
| 13 | [用户管理处理器](<01-组织与权限/C-知识/用户管理处理器.md>) | 21,028 |
| 14 | [用户管理模块 API](<01-组织与权限/C-知识/用户管理模块 API.md>) | 19,565 |
| 15 | [用户认证与授权](<01-组织与权限/C-知识/用户认证与授权.md>) | 19,295 |
| 16 | [系统初始化处理器](<01-组织与权限/C-知识/系统初始化处理器.md>) | 16,804 |
| 17 | [组织权限与用户偏好：Organization 多级组织 + UserRole 并查集继承 + JWT Cookie+Bearer 双模式 + 偏好双源沉淀 + Agent入职流程](<01-组织与权限/C-知识/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步.md>) | 28,769 |
| 18 | [组织管理](<01-组织与权限/C-知识/组织管理.md>) | 20,454 |
| 19 | [组织管理处理器](<01-组织与权限/C-知识/组织管理处理器.md>) | 23,559 |
| 20 | [组织管理模块 API](<01-组织与权限/C-知识/组织管理模块 API.md>) | 17,972 |
| 21 | [组织组网与联邦](<01-组织与权限/C-知识/组织组网与联邦.md>) | 23,575 |
| 22 | [组织领域](<01-组织与权限/C-知识/组织领域.md>) | 17,594 |
| 23 | [聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成](<01-组织与权限/C-知识/聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成/聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成.md>) | 8,248 |
| 24 | [联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断](<01-组织与权限/C-知识/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断.md>) | 13,724 |
| 25 | [联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接](<01-组织与权限/C-知识/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接.md>) | 32,095 |
| 26 | [认证与授权 API](<01-组织与权限/C-知识/认证与授权 API.md>) | 15,387 |
| 27 | [认证与授权](<01-组织与权限/C-知识/认证与授权.md>) | 15,347 |
| 28 | [认证处理器](<01-组织与权限/C-知识/认证处理器.md>) | 15,044 |
| 29 | [记忆管理 API](<01-组织与权限/C-知识/记忆管理 API.md>) | 16,527 |
| 30 | [跨组织业务调用鉴权](<01-组织与权限/C-知识/跨组织业务调用鉴权.md>) | 20,959 |
| 31 | [跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌](<01-组织与权限/C-知识/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities.md>) | 21,868 |
| 32 | [身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用](<01-组织与权限/C-知识/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用.md>) | 13,841 |
| 33 | [身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式](<01-组织与权限/C-知识/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式.md>) | 11,737 |
| 34 | [身份凭证与授权流程](<01-组织与权限/C-知识/身份凭证与授权流程.md>) | 29,206 |
| 35 | [身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立](<01-组织与权限/C-知识/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立.md>) | 12,499 |
| 36 | [身份凭证统一链路（总卡）](<01-组织与权限/C-知识/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）.md>) | 17,191 |

### 02-Agent运行时

#### A 设计（design，10 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Agent 循环驱动引擎设计](<02-Agent运行时/A-设计/agent_loop_engine_design.md>) | 已废弃 | 36,120 |
| 2 | [决策型策略设计（策略引擎承载小脑场景）](<02-Agent运行时/A-设计/cerebellum_decision_strategy_design.md>) | 现行 | 24,529 |
| 3 | [思考循环中的新输入收集与意图回退设计（可中断唤醒）](<02-Agent运行时/A-设计/inbox_poll_and_intent_revision_design.md>) | 现行 | 19,854 |
| 4 | [意图感知两阶段唤醒设计](<02-Agent运行时/A-设计/intent_aware_two_stage_awaken_design.md>) | 已废弃 | 27,455 |
| 5 | [Runtime Domain 设计](<02-Agent运行时/A-设计/runtime_design.md>) | 现行 | 142,125 |
| 6 | [Runtime Domain 推进路线图](<02-Agent运行时/A-设计/runtime_domain_roadmap.md>) | 已废弃 | 31,664 |
| 7 | [思考生命周期设计（Think Lifecycle）](<02-Agent运行时/A-设计/think_lifecycle_design.md>) | 现行 | 43,584 |
| 8 | [思考图引擎改造设计（Awaken 主思考路径节点化）](<02-Agent运行时/A-设计/thinking_graph_engine_design.md>) | 落地中 | 10,207 |
| 9 | [Agent 思考运行时 + 策略引擎设计](<02-Agent运行时/A-设计/thinking_task_policy_engine_design.md>) | 现行 | 47,942 |
| 10 | [两阶段唤醒 Prompt 追加化与前缀缓存设计](<02-Agent运行时/A-设计/two_stage_append_only_prompt_design.md>) | 现行 | 23,238 |

#### B 落地方案（plan，16 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Agent Loop Alignment + AOP Sync Hooks Implementation Plan](<02-Agent运行时/B-落地方案/Agent Loop Alignment + AOP Sync Hooks（执行蓝图）.md>) | 已完成 | 62,565 |
| 2 | [Agent 循环驱动引擎 Implementation Plan](<02-Agent运行时/B-落地方案/Agent 循环驱动引擎实施计划.md>) | 已完成 | 13,799 |
| 3 | [AgentRuntimeInfo 业务上下文扩展 Implementation Plan](<02-Agent运行时/B-落地方案/AgentRuntimeInfo 业务上下文扩展（执行蓝图）.md>) | 已完成 | 18,189 |
| 4 | [Agent运行时上下文扩展](<02-Agent运行时/B-落地方案/Agent运行时上下文扩展.md>) | 已完成 | 8,359 |
| 5 | [Rig 0.34 → 0.41 Upgrade Implementation Plan](<02-Agent运行时/B-落地方案/Rig 0.34 → 0.41 Upgrade（执行蓝图）.md>) | 已完成 | 36,316 |
| 6 | [Runtime API + exit_reason 持久化 Implementation Plan](<02-Agent运行时/B-落地方案/Runtime API + exit_reason 持久化（执行蓝图）.md>) | 已完成 | 27,084 |
| 7 | [两阶段唤醒Prompt追加化](<02-Agent运行时/B-落地方案/两阶段唤醒Prompt追加化.md>) | 已完成 | 11,715 |
| 8 | [两阶段唤醒：IntentAnalyze+Awaken正式执行](<02-Agent运行时/B-落地方案/两阶段唤醒：IntentAnalyze+Awaken正式执行.md>) | 已完成 | 12,615 |
| 9 | [唤醒上下文注入 + 沉睡沉淀约束改造](<02-Agent运行时/B-落地方案/唤醒上下文与睡眠约束.md>) | 已完成 | 20,586 |
| 10 | [思考图引擎改造实施计划](<02-Agent运行时/B-落地方案/思考图引擎改造.md>) | 进行中 | 34,415 |
| 11 | [意图感知两阶段唤醒 Implementation Plan](<02-Agent运行时/B-落地方案/意图感知两阶段唤醒（执行蓝图）.md>) | 已完成 | 67,595 |
| 12 | [移除 rig 依赖与 Cortex 层扁平化重构](<02-Agent运行时/B-落地方案/移除rig依赖与向量存储后端解耦.md>) | 已完成 | 19,703 |
| 13 | [策略引擎 + AgentThinkRuntime Implementation Plan](<02-Agent运行时/B-落地方案/策略引擎 + AgentThinkRuntime（执行蓝图）.md>) | 已完成 | 37,226 |
| 14 | [策略引擎与思考运行时策略增强](<02-Agent运行时/B-落地方案/策略引擎与思考运行时策略增强.md>) | 已完成 | 14,329 |
| 15 | [运行时API可观测与退出原因字段](<02-Agent运行时/B-落地方案/运行时API可观测与退出原因字段.md>) | 已完成 | 14,562 |
| 16 | [运行时问题修复](<02-Agent运行时/B-落地方案/运行时问题修复.md>) | 已完成 | 12,191 |

#### C 知识（wiki，19 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A Client + 外部 Agent Runtime（出站调用 + 内部桥接）](<02-Agent运行时/C-知识/A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送/A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送.md>) | 16,334 |
| 2 | [AI Agent 管理](<02-Agent运行时/C-知识/AI Agent 管理.md>) | 18,362 |
| 3 | [Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报](<02-Agent运行时/C-知识/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报.md>) | 8,818 |
| 4 | [Agent 搜索与推荐](<02-Agent运行时/C-知识/Agent 搜索与推荐.md>) | 13,286 |
| 5 | [Agent 生命周期管理](<02-Agent运行时/C-知识/Agent 生命周期管理.md>) | 28,090 |
| 6 | [Agent处理器](<02-Agent运行时/C-知识/Agent处理器.md>) | 24,451 |
| 7 | [Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2](<02-Agent运行时/C-知识/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联.md>) | 27,874 |
| 8 | [ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板](<02-Agent运行时/C-知识/ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板/ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板.md>) | 13,281 |
| 9 | [小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类](<02-Agent运行时/C-知识/小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类/小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类.md>) | 17,916 |
| 10 | [小脑快判断（System One）](<02-Agent运行时/C-知识/小脑快判断（System One）.md>) | 30,754 |
| 11 | [工具绑定管理](<02-Agent运行时/C-知识/工具绑定管理.md>) | 15,638 |
| 12 | [思考循环异常退出兜底总结与错误上报结构化](<02-Agent运行时/C-知识/思考循环异常退出兜底总结与错误上报结构化/思考循环异常退出兜底总结与错误上报结构化.md>) | 4,735 |
| 13 | [思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件](<02-Agent运行时/C-知识/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件.md>) | 13,082 |
| 14 | [技能包管理](<02-Agent运行时/C-知识/技能包管理.md>) | 30,463 |
| 15 | [策略引擎与 Shell 拦截层](<02-Agent运行时/C-知识/策略引擎与 Shell 拦截层.md>) | 31,774 |
| 16 | [策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层](<02-Agent运行时/C-知识/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层.md>) | 15,003 |
| 17 | [记忆系统管理](<02-Agent运行时/C-知识/记忆系统管理.md>) | 36,203 |
| 18 | [运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化](<02-Agent运行时/C-知识/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化.md>) | 13,011 |
| 19 | [运行时领域](<02-Agent运行时/C-知识/运行时领域.md>) | 56,113 |

### 03-记忆系统

#### A 设计（design，1 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [记忆系统增强（工具拆分 + 定时触发器 + 休息沉淀）设计](<03-记忆系统/A-设计/memory_system_enhancement_design.md>) | 已废弃 | 13,820 |

#### B 落地方案（plan，4 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [代码异味修复 + sleep_and_settle 架构对齐 Implementation Plan](<03-记忆系统/B-落地方案/代码异味修复 + sleep_and_settle 架构对齐（执行蓝图）.md>) | 已完成 | 22,815 |
| 2 | [知识图谱推荐起点与组件复用重构](<03-记忆系统/B-落地方案/知识图谱推荐起点与组件复用重构.md>) | 已完成 | 15,668 |
| 3 | [知识图谱蜂巢共享（published 标签）实施计划](<03-记忆系统/B-落地方案/知识图谱蜂巢共享（published 标签）实施计划（执行蓝图）.md>) | 已完成 | 5,531 |
| 4 | [蜂巢知识共享 Implementation Plan](<03-记忆系统/B-落地方案/蜂巢知识共享（执行蓝图）.md>) | 已完成 | 37,369 |

#### C 知识（wiki，13 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Agent 记忆系统](<03-记忆系统/C-知识/Agent 记忆系统.md>) | 20,457 |
| 2 | [Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle](<03-记忆系统/C-知识/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md>) | 32,090 |
| 3 | [RESTful API](<03-记忆系统/C-知识/RESTful API.md>) | 35,584 |
| 4 | [原始记忆追踪](<03-记忆系统/C-知识/原始记忆追踪.md>) | 22,490 |
| 5 | [四层记忆系统](<03-记忆系统/C-知识/四层记忆系统.md>) | 26,289 |
| 6 | [工作记忆 (Working Memory)](<03-记忆系统/C-知识/工作记忆 (Working Memory).md>) | 20,287 |
| 7 | [核心记忆 (Core Memory)](<03-记忆系统/C-知识/核心记忆 (Core Memory).md>) | 17,586 |
| 8 | [短期记忆（Short-term Memory）](<03-记忆系统/C-知识/短期记忆 (Short-term Memory).md>) | 21,253 |
| 9 | [短期记忆索引](<03-记忆系统/C-知识/短期记忆索引.md>) | 16,653 |
| 10 | [记忆搜索机制](<03-记忆系统/C-知识/记忆搜索机制.md>) | 21,005 |
| 11 | [记忆沉淀机制](<03-记忆系统/C-知识/记忆沉淀机制.md>) | 28,159 |
| 12 | [记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开](<03-记忆系统/C-知识/记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开/记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开.md>) | 6,246 |
| 13 | [长期记忆（Long-term Memory）](<03-记忆系统/C-知识/长期记忆 (Long-term Memory).md>) | 21,875 |

### 04-对话与消息

#### A 设计（design，6 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [飞书（Lark）渠道集成设计 —— 长连接协议对齐版](<04-对话与消息/A-设计/lark_channel_integration_design.md>) | 现行 | 8,859 |
| 2 | [lark-cli 集成与飞书渠道多应用化设计](<04-对话与消息/A-设计/lark_cli_integration.md>) | 已废弃 | 16,194 |
| 3 | [消息渠道设计文档](<04-对话与消息/A-设计/message_channel_design.md>) | 已废弃 | 30,889 |
| 4 | [用户-Agent 消息交互设计](<04-对话与消息/A-设计/message_interaction_design.md>) | 已废弃 | 19,623 |
| 5 | [消息撤回与取消能力设计](<04-对话与消息/A-设计/message_recall_design.md>) | 现行 | 29,408 |
| 6 | [微信渠道接入设计（iLink ClawBot / 企微智能机器人）](<04-对话与消息/A-设计/wechat_channel_integration_design.md>) | 草案 | 59,531 |

#### B 落地方案（plan，9 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [SSE 消息推送实现计划](<04-对话与消息/B-落地方案/SSE 消息推送实现计划（执行蓝图）.md>) | 已完成 | 19,923 |
| 2 | [lark-cli 集成二期：绑定体验 + 用户身份 + WS 稳定性](<04-对话与消息/B-落地方案/lark-cli_集成二期.md>) | 已完成 | 13,478 |
| 3 | [微信 iLink 协议对齐重构方案](<04-对话与消息/B-落地方案/微信iLink协议对齐重构方案.md>) | 进行中 | 34,378 |
| 4 | [消息撤回与取消能力落地](<04-对话与消息/B-落地方案/消息撤回与取消能力.md>) | 已完成 | 19,985 |
| 5 | [聊天MVP](<04-对话与消息/B-落地方案/聊天MVP.md>) | 已完成 | 13,992 |
| 6 | [飞书P2P消息集成](<04-对话与消息/B-落地方案/飞书P2P消息集成.md>) | 已完成 | 14,869 |
| 7 | [飞书出站引用回复 Implementation Plan](<04-对话与消息/B-落地方案/飞书出站引用回复方案.md>) | 已完成 | 34,238 |
| 8 | [飞书链路对齐审查](<04-对话与消息/B-落地方案/飞书链路对齐审查.md>) | 已完成 | 51,829 |
| 9 | [飞书链路对齐重构方案](<04-对话与消息/B-落地方案/飞书链路对齐重构方案.md>) | 进行中 | 26,751 |

#### C 知识（wiki，21 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [@提及功能（mention）](<04-对话与消息/C-知识/@提及功能（mention）.md>) | 23,363 |
| 2 | [@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染](<04-对话与消息/C-知识/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染.md>) | 6,024 |
| 3 | [AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容](<04-对话与消息/C-知识/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容.md>) | 11,765 |
| 4 | [Agent消息集成](<04-对话与消息/C-知识/Agent消息集成.md>) | 18,947 |
| 5 | [ChatMessage::System 消息角色：人设/指令/规则 与 对话内容 分层传递给 Chat Completions API](<04-对话与消息/C-知识/ChatMessage::System 消息角色：人设·指令·规则 与 对话内容分层传递给 Chat Completions API/ChatMessage::System 消息角色：人设·指令·规则 与 对话内容分层传递给 Chat Completions API.md>) | 4,549 |
| 6 | [InboundState 入站运行状态：动态游标 + 会话滚动刷新 + fail-open 游标损坏恢复](<04-对话与消息/C-知识/InboundState 入站运行状态：动态游标 + 会话滚动刷新/InboundState 入站运行状态：动态游标 + 会话滚动刷新.md>) | 8,474 |
| 7 | [WebSocket 与 SSE](<04-对话与消息/C-知识/WebSocket 与 SSE.md>) | 15,293 |
| 8 | [多渠道消息系统](<04-对话与消息/C-知识/多渠道消息系统.md>) | 25,278 |
| 9 | [实时推送](<04-对话与消息/C-知识/实时推送.md>) | 18,046 |
| 10 | [微信 iLink 专属渠道](<04-对话与消息/C-知识/微信 iLink 专属渠道.md>) | 38,871 |
| 11 | [微信 iLink 专属渠道闭环（wechat_dal + ilink_dao + inbound_state + 授权流程）](<04-对话与消息/C-知识/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程.md>) | 23,805 |
| 12 | [消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类](<04-对话与消息/C-知识/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类.md>) | 34,261 |
| 13 | [消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级](<04-对话与消息/C-知识/消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级/消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级.md>) | 8,692 |
| 14 | [消息处理核心](<04-对话与消息/C-知识/消息处理核心.md>) | 21,399 |
| 15 | [消息渠道入站适配中台（MessageInboundAdapter + Registry + 生命周期）](<04-对话与消息/C-知识/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期.md>) | 36,488 |
| 16 | [消息渠道管理（业务功能层）](<04-对话与消息/C-知识/消息渠道管理.md>) | 32,206 |
| 17 | [消息渠道适配器](<04-对话与消息/C-知识/消息渠道适配器.md>) | 22,965 |
| 18 | [消息管理](<04-对话与消息/C-知识/消息管理.md>) | 19,205 |
| 19 | [消息系统（业务功能层）](<04-对话与消息/C-知识/消息系统.md>) | 22,683 |
| 20 | [消息领域](<04-对话与消息/C-知识/消息领域.md>) | 25,572 |
| 21 | [邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型](<04-对话与消息/C-知识/邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型/邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型.md>) | 11,894 |

### 05-任务与项目

#### A 设计（design，5 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [聊天侧栏「项目内 Agent 列表」与「工具调用身份展示」设计说明](<05-任务与项目/A-设计/chat_project_agents_and_tool_call_identity_design.md>) | 现行 | 11,788 |
| 2 | [项目系统设计文档](<05-任务与项目/A-设计/project_design.md>) | 已废弃 | 6,585 |
| 3 | [项目管理系统设计文档](<05-任务与项目/A-设计/project_management_design.md>) | 已废弃 | 30,167 |
| 4 | [任务系统设计文档](<05-任务与项目/A-设计/task_design.md>) | 已废弃 | 7,196 |
| 5 | [定时任务系统设计文档](<05-任务与项目/A-设计/task_scheduler_design.md>) | 已废弃 | 22,674 |

#### B 落地方案（plan，6 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Agent Artifact 创建能力实施 Plan](<05-任务与项目/B-落地方案/Agent Artifact 创建能力实施（执行蓝图）.md>) | 已完成 | 38,189 |
| 2 | [Agent Artifact 创建能力方案（定稿）](<05-任务与项目/B-落地方案/Agent Artifact 创建能力方案（定稿）（执行蓝图）.md>) | 已完成 | 13,824 |
| 3 | [Artifact 编辑功能实施 Plan](<05-任务与项目/B-落地方案/Artifact 编辑功能实施（执行蓝图）.md>) | 已完成 | 34,450 |
| 4 | [任务管理核心功能实现计划](<05-任务与项目/B-落地方案/任务管理核心功能实现计划（执行蓝图）.md>) | 已完成 | 45,213 |
| 5 | [独立任务管理页面 + 看板视图实现计划](<05-任务与项目/B-落地方案/独立任务管理页面 + 看板视图实现计划（执行蓝图）.md>) | 已完成 | 28,928 |
| 6 | [项目任务增强](<05-任务与项目/B-落地方案/项目任务增强.md>) | 已完成 | 13,326 |

#### C 知识（wiki，53 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息](<05-任务与项目/C-知识/A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息/A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息.md>) | 9,450 |
| 2 | [Agent 全生命周期管理](<05-任务与项目/C-知识/Agent 全生命周期管理.md>) | 24,467 |
| 3 | [Agent 创建与配置](<05-任务与项目/C-知识/Agent 创建与配置.md>) | 16,501 |
| 4 | [Agent 搜索与查询](<05-任务与项目/C-知识/Agent 搜索与查询.md>) | 13,604 |
| 5 | [Agent 状态管理](<05-任务与项目/C-知识/Agent 状态管理.md>) | 18,494 |
| 6 | [Agent 维度统计](<05-任务与项目/C-知识/Agent 维度统计.md>) | 16,601 |
| 7 | [AgentRuntimeInfo 状态机 + BusyGuard RAII：Idle/Busy/Resting 三态转换 + task_id/project_id 业务上下文 + 前端 runtime-list 过滤](<05-任务与项目/C-知识/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视.md>) | 17,679 |
| 8 | [HTTP 请求工具 (http_fetch)](<05-任务与项目/C-知识/HTTP 请求工具 (http_fetch).md>) | 19,283 |
| 9 | [ModelProvider 维度统计](<05-任务与项目/C-知识/ModelProvider 维度统计.md>) | 14,709 |
| 10 | [Project 维度统计](<05-任务与项目/C-知识/Project 维度统计.md>) | 16,029 |
| 11 | [Project模块处理器](<05-任务与项目/C-知识/Project模块处理器.md>) | 19,537 |
| 12 | [Task 维度统计](<05-任务与项目/C-知识/Task 维度统计.md>) | 16,293 |
| 13 | [任务协作与执行计划](<05-任务与项目/C-知识/任务协作与执行计划.md>) | 23,708 |
| 14 | [任务处理器](<05-任务与项目/C-知识/任务处理器.md>) | 20,994 |
| 15 | [任务状态机 + 项目聚合进度追踪：TaskStatus 4 态流转 + execution_plan_result 结构化存储 + TaskGraph](<05-任务与项目/C-知识/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG.md>) | 14,098 |
| 16 | [任务生命周期管理](<05-任务与项目/C-知识/任务生命周期管理.md>) | 16,444 |
| 17 | [任务管理 API 参考](<05-任务与项目/C-知识/任务管理 API 参考.md>) | 16,929 |
| 18 | [任务管理（业务功能层）](<05-任务与项目/C-知识/任务管理.md>) | 22,906 |
| 19 | [任务管理接口](<05-任务与项目/C-知识/任务管理接口.md>) | 17,029 |
| 20 | [多 Agent 协作机制](<05-任务与项目/C-知识/多 Agent 协作机制.md>) | 18,441 |
| 21 | [多维统计系统](<05-任务与项目/C-知识/多维统计系统.md>) | 27,108 |
| 22 | [定时任务管理](<05-任务与项目/C-知识/定时任务管理.md>) | 21,259 |
| 23 | [定时任务调度（业务功能层）](<05-任务与项目/C-知识/定时任务调度.md>) | 17,196 |
| 24 | [工件处理器](<05-任务与项目/C-知识/工件处理器.md>) | 16,841 |
| 25 | [工件管理](<05-任务与项目/C-知识/工件管理.md>) | 18,275 |
| 26 | [工件管理接口](<05-任务与项目/C-知识/工件管理接口.md>) | 18,573 |
| 27 | [工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡](<05-任务与项目/C-知识/工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡/工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡.md>) | 9,378 |
| 28 | [工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询](<05-任务与项目/C-知识/工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询/工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询.md>) | 4,277 |
| 29 | [工具安全控制](<05-任务与项目/C-知识/工具安全控制.md>) | 20,028 |
| 30 | [工具调用模式](<05-任务与项目/C-知识/工具调用模式.md>) | 17,574 |
| 31 | [快速开始指南](<05-任务与项目/C-知识/快速开始指南.md>) | 15,052 |
| 32 | [执行计划与结果追踪](<05-任务与项目/C-知识/执行计划与结果追踪.md>) | 19,162 |
| 33 | [数据备份与恢复](<05-任务与项目/C-知识/数据备份与恢复.md>) | 16,894 |
| 34 | [文件系统写入工具 (fs_write)](<05-任务与项目/C-知识/文件系统写入工具 (fs_write).md>) | 15,141 |
| 35 | [文件系统读取工具 (fs_read)](<05-任务与项目/C-知识/文件系统读取工具 (fs_read).md>) | 12,138 |
| 36 | [日志管理系统（入门概览层）](<05-任务与项目/C-知识/日志管理系统.md>) | 19,426 |
| 37 | [核心功能特性](<05-任务与项目/C-知识/核心功能特性.md>) | 32,306 |
| 38 | [知识关系管理](<05-任务与项目/C-知识/知识关系管理.md>) | 25,256 |
| 39 | [知识节点管理](<05-任务与项目/C-知识/知识节点管理.md>) | 16,327 |
| 40 | [系统健康检查](<05-任务与项目/C-知识/系统健康检查.md>) | 24,038 |
| 41 | [系统管理功能](<05-任务与项目/C-知识/系统管理功能.md>) | 16,576 |
| 42 | [综合搜索能力](<05-任务与项目/C-知识/综合搜索能力.md>) | 22,785 |
| 43 | [聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文](<05-任务与项目/C-知识/聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文/聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文.md>) | 5,957 |
| 44 | [自定义工具开发](<05-任务与项目/C-知识/自定义工具开发.md>) | 16,604 |
| 45 | [通过 common DTO 补全 Project/Task execution_plan/result 读取链路](<05-任务与项目/C-知识/通过 common DTO 补全 Project_Task execution_plan_result 读取链路/通过 common DTO 补全 Project_Task execution_plan_result 读取链路.md>) | 1,666 |
| 46 | [项目 CRUD 接口](<05-任务与项目/C-知识/项目 CRUD 接口.md>) | 17,557 |
| 47 | [项目 CRUD 操作](<05-任务与项目/C-知识/项目 CRUD 操作.md>) | 20,431 |
| 48 | [项目和任务模型](<05-任务与项目/C-知识/项目和任务模型.md>) | 20,898 |
| 49 | [项目处理器](<05-任务与项目/C-知识/项目处理器.md>) | 22,202 |
| 50 | [项目管理（业务功能层）](<05-任务与项目/C-知识/项目管理.md>) | 18,489 |
| 51 | [项目管理模块 API](<05-任务与项目/C-知识/项目管理模块 API.md>) | 16,270 |
| 52 | [项目领域](<05-任务与项目/C-知识/项目领域.md>) | 14,991 |
| 53 | [项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合](<05-任务与项目/C-知识/项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合/项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合.md>) | 9,087 |

### 06-工具与技能

#### A 设计（design，11 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [HTTP Tool Runtime 设计文档](<06-工具与技能/A-设计/builtins_http_tool_design.md>) | 已废弃 | 16,582 |
| 2 | [通用 Builtin 工具设计 - 文件读写 & HTTP Fetch](<06-工具与技能/A-设计/generic_builtin_tools_design.md>) | 已废弃 | 18,356 |
| 3 | [工具注册宏设计文档](<06-工具与技能/A-设计/handler_tool_registration_macro.md>) | 已废弃 | 29,032 |
| 4 | [MCP Tool Runtime 设计文档](<06-工具与技能/A-设计/mcp_tool_design.md>) | 已废弃 | 68,660 |
| 5 | [技能系统设计文档](<06-工具与技能/A-设计/skill_design.md>) | 已废弃 | 31,514 |
| 6 | [技能导入方式扩展与统一导入框架设计](<06-工具与技能/A-设计/skill_import_methods_design.md>) | 草案 | 21,670 |
| 7 | [技能系统增强（tag 过滤 + 技能包 + 唤醒注入）设计](<06-工具与技能/A-设计/skill_system_enhancement_design.md>) | 已废弃 | 11,625 |
| 8 | [共享工具凭据增强器设计（MCP / HTTP 工具接入身份凭证体系）](<06-工具与技能/A-设计/tool_credential_requirement_design.md>) | 现行 | 56,092 |
| 9 | [ai_orz 工具模块设计与开发总结](<06-工具与技能/A-设计/tool_design.md>) | 已废弃 | 76,360 |
| 10 | [工具输出三分边界设计（运行时输出 / 最终结果 / 产物）](<06-工具与技能/A-设计/tool_output_boundary_design.md>) | 现行 | 13,962 |
| 11 | [网络搜索与浏览器工具设计](<06-工具与技能/A-设计/web_search_and_browser_tools_design.md>) | 现行 | 16,848 |

#### B 落地方案（plan，14 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Tool/Skill 搜索式安装 Implementation Plan](<06-工具与技能/B-落地方案/ToolSkill 搜索式安装（执行蓝图）.md>) | 已完成 | 60,502 |
| 2 | [generate_http_handler 宏 (true, true) 分支修复 Implementation Plan](<06-工具与技能/B-落地方案/generate_http_handler 宏 (true, true) 分支修复（执行蓝图）.md>) | 已完成 | 41,213 |
| 3 | [generate_http_handler 宏 path 参数解析修复 Implementation Plan](<06-工具与技能/B-落地方案/generate_http_handler 宏 path 参数解析修复（执行蓝图）.md>) | 已完成 | 42,801 |
| 4 | [共享工具凭据增强器落地](<06-工具与技能/B-落地方案/共享工具凭据增强器落地.md>) | 已完成 | 29,493 |
| 5 | [技能列表显示修复与技能同步过期处理 实现计划](<06-工具与技能/B-落地方案/技能列表显示修复与技能同步过期处理 实现计划（执行蓝图）.md>) | 已完成 | 56,325 |
| 6 | [技能列表显示修复与技能同步过期处理](<06-工具与技能/B-落地方案/技能列表显示修复与技能同步过期处理.md>) | 已完成 | 10,791 |
| 7 | [技能导入方式扩展与统一导入框架落地](<06-工具与技能/B-落地方案/技能导入方式扩展与统一导入框架落地.md>) | 草案 | 22,299 |
| 8 | [文档路径引用统一化 + DocLinkClassifier 通用组件 Implementation Plan (v2)](<06-工具与技能/B-落地方案/文档路径引用统一化 + DocLinkClassifier 通用组件 Implementation Plan (v2)（执行蓝图）.md>) | 已完成 | 43,395 |
| 9 | [文档链接统一与DocLinkClassifier工具链](<06-工具与技能/B-落地方案/文档链接统一与DocLinkClassifier工具链.md>) | 已完成 | 17,956 |
| 10 | [浏览器工具与环境就绪提示（阶段二：browser + tool_readiness）](<06-工具与技能/B-落地方案/浏览器工具与环境就绪提示.md>) | 进行中 | 11,877 |
| 11 | [统一工具调用与装饰器收敛 Implementation Plan](<06-工具与技能/B-落地方案/统一工具调用与装饰器收敛（执行蓝图）.md>) | 已完成 | 23,258 |
| 12 | [网络搜索工具（阶段一：tavily_search）](<06-工具与技能/B-落地方案/网络搜索工具.md>) | 草案 | 9,874 |
| 13 | [统一后台进程管理 + shell_exec 超时移交修复](<06-工具与技能/B-落地方案/进程管理与shell_exec修复.md>) | 已完成 | 16,507 |
| 14 | [预置基础技能导入重构](<06-工具与技能/B-落地方案/预置基础技能导入重构.md>) | 已完成 | 11,825 |

#### C 知识（wiki，52 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配](<06-工具与技能/C-知识/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配.md>) | 12,454 |
| 2 | [DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链](<06-工具与技能/C-知识/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链.md>) | 15,373 |
| 3 | [GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页](<06-工具与技能/C-知识/GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页/GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页.md>) | 15,784 |
| 4 | [HTTP 客户端工具](<06-工具与技能/C-知识/HTTP 客户端工具.md>) | 22,252 |
| 5 | [HTTP 工具（框架层）](<06-工具与技能/C-知识/HTTP 工具.md>) | 16,520 |
| 6 | [HTTP 工具创建表单限定 method 白名单为 GET/POST](<06-工具与技能/C-知识/HTTP 工具创建表单限定 method 白名单为 GET_POST/HTTP 工具创建表单限定 method 白名单为 GET_POST.md>) | 1,283 |
| 7 | [HTTP请求工具](<06-工具与技能/C-知识/HTTP请求工具.md>) | 18,483 |
| 8 | [MCP 协议工具](<06-工具与技能/C-知识/MCP 协议工具.md>) | 17,838 |
| 9 | [MCP 工具集成（代码落地层）](<06-工具与技能/C-知识/MCP 工具集成.md>) | 15,825 |
| 10 | [MCP 服务器集成](<06-工具与技能/C-知识/MCP 服务器集成.md>) | 19,624 |
| 11 | [Shell 工具全链路：shell_exec 内置执行 + shell_policy 策略拦截 + shell_tool 声明式注册](<06-工具与技能/C-知识/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行.md>) | 16,988 |
| 12 | [Shell 执行工具 (shell_exec)](<06-工具与技能/C-知识/Shell 执行工具 (shell_exec).md>) | 17,514 |
| 13 | [Shell 执行工具（业务功能层）](<06-工具与技能/C-知识/Shell 执行工具.md>) | 28,491 |
| 14 | [Shell执行工具](<06-工具与技能/C-知识/Shell执行工具.md>) | 20,046 |
| 15 | [§1 概述（一句话定位 + 解决什么问题）](<06-工具与技能/C-知识/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断.md>) | 43,394 |
| 16 | [Tool 维度统计](<06-工具与技能/C-知识/Tool 维度统计.md>) | 17,234 |
| 17 | [pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）](<06-工具与技能/C-知识/pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）/pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）.md>) | 13,782 |
| 18 | [pkg 基建统一](<06-工具与技能/C-知识/pkg 基建统一.md>) | 11,374 |
| 19 | [sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）](<06-工具与技能/C-知识/sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）/sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）.md>) | 8,975 |
| 20 | [共享工具凭据增强器](<06-工具与技能/C-知识/共享工具凭据增强器.md>) | 37,112 |
| 21 | [共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例](<06-工具与技能/C-知识/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md>) | 15,540 |
| 22 | [内置工具](<06-工具与技能/C-知识/内置工具.md>) | 22,278 |
| 23 | [内置工具系统（框架层）](<06-工具与技能/C-知识/内置工具系统.md>) | 18,968 |
| 24 | [内置工具集](<06-工具与技能/C-知识/内置工具集.md>) | 22,126 |
| 25 | [Shell 执行工具（框架层）](<06-工具与技能/C-知识/基础设施/工具注册表/Shell 执行工具.md>) | 22,917 |
| 26 | [工具安全控制](<06-工具与技能/C-知识/工具安全控制.md>) | 22,309 |
| 27 | [工具执行引擎](<06-工具与技能/C-知识/工具执行引擎.md>) | 21,548 |
| 28 | [工具注册与发现](<06-工具与技能/C-知识/工具注册与发现.md>) | 19,785 |
| 29 | [工具注册表（框架层）](<06-工具与技能/C-知识/工具注册表.md>) | 21,871 |
| 30 | [工具生态系统](<06-工具与技能/C-知识/工具生态系统.md>) | 23,046 |
| 31 | [工具管理 API（业务功能层）](<06-工具与技能/C-知识/工具管理 API.md>) | 18,614 |
| 32 | [工具系统三层调用架构：CoreTool trait + Builtin/HTTP/MCP 三协议路由 + register_handler_tool 宏](<06-工具与技能/C-知识/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md>) | 22,092 |
| 33 | [工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复](<06-工具与技能/C-知识/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复.md>) | 7,352 |
| 34 | [工具输出与安全治理](<06-工具与技能/C-知识/工具输出与安全治理/工具输出与安全治理.md>) | 14,551 |
| 35 | [工具输出与安全治理](<06-工具与技能/C-知识/工具输出与安全治理.md>) | 31,400 |
| 36 | [技能与工具绑定](<06-工具与技能/C-知识/技能与工具绑定.md>) | 17,803 |
| 37 | [技能和工具系统](<06-工具与技能/C-知识/技能和工具系统.md>) | 18,729 |
| 38 | [技能系统](<06-工具与技能/C-知识/技能系统.md>) | 29,703 |
| 39 | [文件写入工具](<06-工具与技能/C-知识/文件写入工具.md>) | 17,708 |
| 40 | [文件系统工具](<06-工具与技能/C-知识/文件系统工具.md>) | 20,180 |
| 41 | [文件读取工具](<06-工具与技能/C-知识/文件读取工具.md>) | 18,424 |
| 42 | [核心架构与注册机制（框架层）](<06-工具与技能/C-知识/核心架构与注册机制.md>) | 19,722 |
| 43 | [HTTP 工具（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/HTTP 工具.md>) | 19,372 |
| 44 | [Shell 执行工具（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/Shell 执行工具.md>) | 20,900 |
| 45 | [内置工具系统（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/内置工具系统.md>) | 27,038 |
| 46 | [工具安全控制](<06-工具与技能/C-知识/核心模块/工具注册表/工具安全控制.md>) | 26,759 |
| 47 | [工具注册表（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/工具注册表.md>) | 23,631 |
| 48 | [核心架构与注册机制（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/核心架构与注册机制.md>) | 20,860 |
| 49 | [用户维度工作区路径改造 + 工具调用安全边界检查（路径逃逸 / 跨用户访问 / 相对路径阻断）](<06-工具与技能/C-知识/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断.md>) | 19,823 |
| 50 | [运行时诊断工具组](<06-工具与技能/C-知识/运行时诊断工具组.md>) | 23,092 |
| 51 | [进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤](<06-工具与技能/C-知识/进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤/进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤.md>) | 1,985 |
| 52 | [MCP 工具集成（入门概览层）](<06-工具与技能/C-知识/项目概述/核心功能特性/统一工具调用架构/MCP 工具集成.md>) | 16,485 |

### 07-外部Agent(A2A)

#### A 设计（design，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [A2A Server 架构设计（agent 与 project 维度分离）](<07-外部Agent(A2A)/A-设计/a2a_server_architecture_design.md>) | 已废弃 | 14,356 |
| 2 | [A2A Server 设计文档](<07-外部Agent(A2A)/A-设计/a2a_server_design.md>) | 已废弃 | 20,842 |
| 3 | [外部 Agent 接入设计](<07-外部Agent(A2A)/A-设计/external_agent_design.md>) | 已废弃 | 13,596 |

#### B 落地方案（plan，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [A2A Server 实施计划](<07-外部Agent(A2A)/B-落地方案/A2A Server 实施计划（执行蓝图）.md>) | 已完成 | 86,590 |
| 2 | [A2A 异步回调与轮询 Producer 实现计划](<07-外部Agent(A2A)/B-落地方案/A2A 异步回调与轮询 Producer 实现计划（执行蓝图）.md>) | 已完成 | 31,197 |
| 3 | [A2A 外部 Agent 集成（CLI 子进程 + 通用远程）](<07-外部Agent(A2A)/B-落地方案/A2A外部Agent集成.md>) | 已完成 | 21,067 |

#### C 知识（wiki，20 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A Client 模式](<07-外部Agent(A2A)/C-知识/A2A Client 模式.md>) | 19,118 |
| 2 | [A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建](<07-外部Agent(A2A)/C-知识/A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建/A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建.md>) | 10,700 |
| 3 | [A2A Server 模式](<07-外部Agent(A2A)/C-知识/A2A Server 模式.md>) | 16,873 |
| 4 | [A2A 协议](<07-外部Agent(A2A)/C-知识/A2A 协议.md>) | 17,064 |
| 5 | [A2A 协议支持](<07-外部Agent(A2A)/C-知识/A2A 协议支持.md>) | 19,536 |
| 6 | [A2A 协议规范](<07-外部Agent(A2A)/C-知识/A2A 协议规范.md>) | 15,553 |
| 7 | [A2A 集成指南](<07-外部Agent(A2A)/C-知识/A2A 集成指南.md>) | 15,638 |
| 8 | [A2A协议处理器](<07-外部Agent(A2A)/C-知识/A2A协议处理器.md>) | 19,684 |
| 9 | [Agent 发现机制](<07-外部Agent(A2A)/C-知识/Agent 发现机制.md>) | 16,144 |
| 10 | [Agent卡片发现](<07-外部Agent(A2A)/C-知识/Agent卡片发现.md>) | 14,279 |
| 11 | [JSON-RPC 通信协议](<07-外部Agent(A2A)/C-知识/JSON-RPC 通信协议.md>) | 19,586 |
| 12 | [JSON-RPC核心](<07-外部Agent(A2A)/C-知识/JSON-RPC核心.md>) | 17,948 |
| 13 | [任务创建](<07-外部Agent(A2A)/C-知识/任务创建.md>) | 14,418 |
| 14 | [任务取消](<07-外部Agent(A2A)/C-知识/任务取消.md>) | 13,855 |
| 15 | [任务查询](<07-外部Agent(A2A)/C-知识/任务查询.md>) | 14,884 |
| 16 | [任务生命周期](<07-外部Agent(A2A)/C-知识/任务生命周期.md>) | 16,027 |
| 17 | [任务管理](<07-外部Agent(A2A)/C-知识/任务管理.md>) | 14,816 |
| 18 | [回调机制](<07-外部Agent(A2A)/C-知识/回调机制.md>) | 16,195 |
| 19 | [回调系统](<07-外部Agent(A2A)/C-知识/回调系统.md>) | 16,503 |
| 20 | [任务管理（代码落地层）](<07-外部Agent(A2A)/C-知识/核心模块/处理器层/A2A协议处理器/任务管理.md>) | 19,172 |

### 08-搜索

#### A 设计（design，5 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [实体列表/查询/搜索接口设计规范](<08-搜索/A-设计/entity_list_query_search_design.md>) | 已废弃 | 22,947 |
| 2 | [全实体 FTS5 全文搜索统一设计](<08-搜索/A-设计/full_entity_fts5_search_design.md>) | 已废弃 | 9,609 |
| 3 | [记忆搜索 FTS5 增强与综合搜索设计](<08-搜索/A-设计/memory_search_enhancement_design.md>) | 已废弃 | 9,750 |
| 4 | [本体论驱动知识沉淀设计（Ontology-Driven Knowledge Sedimentation）](<08-搜索/A-设计/ontology_knowledge_sedimentation_design.md>) | 现行 | 37,339 |
| 5 | [向量搜索与混合搜索架构设计文档](<08-搜索/A-设计/vector_search_architecture.md>) | 已废弃 | 22,467 |

#### B 落地方案（plan，10 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Agent搜索查询三场景统一重构](<08-搜索/B-落地方案/Agent搜索查询三场景统一重构.md>) | 已完成 | 17,034 |
| 2 | [HNSW持久化与索引重建异步化重构](<08-搜索/B-落地方案/HNSW持久化与索引重建异步化重构.md>) | 已完成 | 17,179 |
| 3 | [Project 实体搜索/查询接口统一实施计划](<08-搜索/B-落地方案/Project 实体搜索查询接口统一实施计划（执行蓝图）.md>) | 已完成 | 23,799 |
| 4 | [Search and Knowledge Graph Implementation Plan](<08-搜索/B-落地方案/Search and Knowledge Graph（执行蓝图）.md>) | 已完成 | 39,657 |
| 5 | [Task 实体搜索/查询接口统一实施计划](<08-搜索/B-落地方案/Task 实体搜索查询接口统一实施计划（执行蓝图）.md>) | 已完成 | 19,232 |
| 6 | [向量搜索 Pre-Filter 改造：Payload 落库与谓词下推 实施计划](<08-搜索/B-落地方案/向量搜索Pre-Filter改造.md>) | 进行中 | 79,265 |
| 7 | [向量搜索增强计划](<08-搜索/B-落地方案/向量搜索增强计划（执行蓝图）.md>) | 已完成 | 13,460 |
| 8 | [知识图谱遍历查询优化（DFS 批量化 + IN 分块）](<08-搜索/B-落地方案/图谱遍历查询优化.md>) | 已完成 | 10,220 |
| 9 | [本体论知识沉淀落地](<08-搜索/B-落地方案/本体论知识沉淀落地.md>) | 草案 | 25,670 |
| 10 | [统一其他实体搜索/查询接口实施计划](<08-搜索/B-落地方案/统一其他实体搜索查询接口实施计划（执行蓝图）.md>) | 已完成 | 36,227 |

#### C 知识（wiki，21 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕](<08-搜索/C-知识/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕.md>) | 48,546 |
| 2 | [Embedding Provider 生命周期与重建触发条件](<08-搜索/C-知识/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵.md>) | 23,915 |
| 3 | [Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T](<08-搜索/C-知识/Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分/Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分.md>) | 8,513 |
| 4 | [FTS5 关键词搜索](<08-搜索/C-知识/FTS5 关键词搜索.md>) | 18,064 |
| 5 | [PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序](<08-搜索/C-知识/PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序/PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序.md>) | 18,978 |
| 6 | [RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配](<08-搜索/C-知识/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配.md>) | 12,901 |
| 7 | [SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离](<08-搜索/C-知识/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离.md>) | 7,452 |
| 8 | [全文搜索（框架层）](<08-搜索/C-知识/全文搜索.md>) | 16,308 |
| 9 | [向量存储抽象 + Vectorizable trait 统一索引](<08-搜索/C-知识/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity.md>) | 31,053 |
| 10 | [向量语义搜索](<08-搜索/C-知识/向量语义搜索.md>) | 15,926 |
| 11 | [本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查](<08-搜索/C-知识/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查.md>) | 18,477 |
| 12 | [本体属性管理](<08-搜索/C-知识/本体属性管理.md>) | 32,881 |
| 13 | [本体词表与漂移治理](<08-搜索/C-知识/本体词表与漂移治理.md>) | 30,736 |
| 14 | [本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账](<08-搜索/C-知识/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账.md>) | 22,057 |
| 15 | [混合搜索引擎](<08-搜索/C-知识/混合搜索引擎.md>) | 16,492 |
| 16 | [知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出](<08-搜索/C-知识/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md>) | 15,797 |
| 17 | [知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头](<08-搜索/C-知识/知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头/知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头.md>) | 23,435 |
| 18 | [知识图谱可视化](<08-搜索/C-知识/知识图谱可视化.md>) | 45,395 |
| 19 | [知识图谱搜索](<08-搜索/C-知识/知识图谱搜索.md>) | 28,060 |
| 20 | [知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导](<08-搜索/C-知识/知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导/知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导.md>) | 16,944 |
| 21 | [记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 度数排序推荐](<08-搜索/C-知识/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md>) | 14,309 |

### 09-统计与监控

#### A 设计（design，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [统计模块设计 — 双层实现：DuckDB 持久化 + 内存实时](<09-统计与监控/A-设计/stats_module_design.md>) | 已废弃 | 33,692 |
| 2 | [统计查询模块设计方案](<09-统计与监控/A-设计/stats_query_design.md>) | 已废弃 | 31,129 |
| 3 | [工作台顶栏聚合指标设计](<09-统计与监控/A-设计/workspace_topbar_metrics_design.md>) | 现行 | 8,645 |

#### B 落地方案（plan，6 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Finance 域统计面板补充计划](<09-统计与监控/B-落地方案/Finance 域统计面板补充计划（执行蓝图）.md>) | 已完成 | 4,189 |
| 2 | [Responses API 接入与推理统计扩展 实施计划](<09-统计与监控/B-落地方案/Responses API 接入与推理统计扩展.md>) | 进行中 | 23,905 |
| 3 | [实体统计注入](<09-统计与监控/B-落地方案/实体统计注入.md>) | 已完成 | 15,448 |
| 4 | [统计图表Phase1基础设施与时序图展示重构](<09-统计与监控/B-落地方案/统计图表Phase1基础设施与时序图展示重构.md>) | 已完成 | 15,939 |
| 5 | [统计图表 Phase 2：环形图 + Project 任务状态分布](<09-统计与监控/B-落地方案/统计图表Phase2.md>) | 已完成 | 12,495 |
| 6 | [统计图表第三期（AOP 实时内存统计 + 轮询渲染）](<09-统计与监控/B-落地方案/统计图表第三期.md>) | 已完成 | 24,064 |

#### C 知识（wiki，8 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表](<09-统计与监控/C-知识/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md>) | 9,583 |
| 2 | [Lark P2P WS 私信入站（凭证引用解析 + 多渠道按 app_id 聚合 + 用户身份自动映射 + 健康指标）](<09-统计与监控/C-知识/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标.md>) | 14,187 |
| 3 | [健康检查处理器](<09-统计与监控/C-知识/健康检查处理器.md>) | 14,076 |
| 4 | [底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（LanceDB 默认生产 → InMemory/Hnsw 测试备选）](<09-统计与监控/C-知识/底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（LanceDB 默认生产 _ InMemory_Hnsw 测试备选）/底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（InMemory_Hnsw 默认 _ LanceDB 大规模）.md>) | 12,623 |
| 5 | [故障排除与监控](<09-统计与监控/C-知识/故障排除与监控.md>) | 17,803 |
| 6 | [系统监控与健康检查](<09-统计与监控/C-知识/系统监控与健康检查.md>) | 15,950 |
| 7 | [统计与监控](<09-统计与监控/C-知识/统计与监控.md>) | 25,531 |
| 8 | [统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示](<09-统计与监控/C-知识/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示.md>) | 20,380 |

### 10-异步基建(AOP)

#### A 设计（design，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [AOP 生产者-消费者契约重构设计](<10-异步基建(AOP)/A-设计/aop_producer_consumer_contract_design.md>) | 现行 | 83,831 |
| 2 | [消费者与生产者架构设计文档](<10-异步基建(AOP)/A-设计/consumer_architecture.md>) | 已废弃 | 18,836 |
| 3 | [事件总线设计（已归档）](<10-异步基建(AOP)/A-设计/event_design.md>) | 已废弃 | 8,148 |

#### B 落地方案（plan，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [AOP 队列监控功能实现计划](<10-异步基建(AOP)/B-落地方案/AOP 队列监控功能实现计划（执行蓝图）.md>) | 已完成 | 21,589 |
| 2 | [AOP 生产-消费事件中心重构](<10-异步基建(AOP)/B-落地方案/AOP生产消费事件中心重构.md>) | 已完成 | 12,552 |
| 3 | [通用后台任务模块与Seed异步化重构](<10-异步基建(AOP)/B-落地方案/通用后台任务模块与Seed异步化重构.md>) | 已完成 | 15,990 |

#### C 知识（wiki，32 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 事件系统（系统管理层）](<10-异步基建(AOP)/C-知识/AOP 事件系统.md>) | 22,572 |
| 2 | [AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册](<10-异步基建(AOP)/C-知识/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md>) | 15,822 |
| 3 | [Agent 循环消费者（代码落地层）](<10-异步基建(AOP)/C-知识/Agent 循环消费者.md>) | 18,847 |
| 4 | [Cron 任务调度与系统启动顺序：5 字段 cron 解析 + ensure_system_cron_triggers 2 条注入 + CronTriggerProducer](<10-异步基建(AOP)/C-知识/Cron 任务调度与系统启动顺序：5字段cron解析 + ensure_system_cron_triggers 2条注入 + CronTriggerConsumer 事件 + 两阶段init/aop严格分离/Cron 任务调度与系统启动顺序：5字段cron解析 + ensure_system_cron_triggers 2条注入 + CronTriggerConsumer 事件 + 两阶段init/aop严格分离.md>) | 20,322 |
| 5 | [Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅](<10-异步基建(AOP)/C-知识/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅.md>) | 12,424 |
| 6 | [事件持久化](<10-异步基建(AOP)/C-知识/事件持久化.md>) | 17,551 |
| 7 | [事件消费者](<10-异步基建(AOP)/C-知识/事件消费者.md>) | 19,188 |
| 8 | [事件生产者](<10-异步基建(AOP)/C-知识/事件生产者.md>) | 22,483 |
| 9 | [事件系统核心](<10-异步基建(AOP)/C-知识/事件系统核心.md>) | 19,652 |
| 10 | [事件调度器](<10-异步基建(AOP)/C-知识/事件调度器.md>) | 20,162 |
| 11 | [事件队列管理](<10-异步基建(AOP)/C-知识/事件队列管理.md>) | 20,149 |
| 12 | [事件队列系统](<10-异步基建(AOP)/C-知识/事件队列系统.md>) | 19,796 |
| 13 | [任务事件消费者（框架层）](<10-异步基建(AOP)/C-知识/任务事件消费者.md>) | 14,385 |
| 14 | [唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发](<10-异步基建(AOP)/C-知识/唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发/唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发.md>) | 10,184 |
| 15 | [AOP 事件系统（框架层）](<10-异步基建(AOP)/C-知识/基础设施/AOP 事件系统/AOP 事件系统.md>) | 22,985 |
| 16 | [定时任务和事件系统](<10-异步基建(AOP)/C-知识/定时任务和事件系统.md>) | 22,661 |
| 17 | [定时任务消费者](<10-异步基建(AOP)/C-知识/定时任务消费者.md>) | 17,084 |
| 18 | [定时触发生产者](<10-异步基建(AOP)/C-知识/定时触发生产者.md>) | 21,051 |
| 19 | [工具执行消费者](<10-异步基建(AOP)/C-知识/工具执行消费者.md>) | 18,629 |
| 20 | [思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路](<10-异步基建(AOP)/C-知识/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路.md>) | 7,340 |
| 21 | [核心概念与架构](<10-异步基建(AOP)/C-知识/核心概念与架构.md>) | 23,966 |
| 22 | [AOP 事件系统（代码落地层）](<10-异步基建(AOP)/C-知识/核心模块/AOP 事件系统/AOP 事件系统.md>) | 23,032 |
| 23 | [任务事件消费者（代码落地层）](<10-异步基建(AOP)/C-知识/核心模块/AOP 事件系统/消费者框架/任务事件消费者.md>) | 22,665 |
| 24 | [注册中心与调度器](<10-异步基建(AOP)/C-知识/注册中心与调度器.md>) | 21,035 |
| 25 | [消息消费者（代码落地层）](<10-异步基建(AOP)/C-知识/消息消费者.md>) | 24,489 |
| 26 | [消费者接口规范](<10-异步基建(AOP)/C-知识/消费者接口规范.md>) | 22,954 |
| 27 | [消费者框架](<10-异步基建(AOP)/C-知识/消费者框架.md>) | 18,485 |
| 28 | [生产者机制](<10-异步基建(AOP)/C-知识/生产者机制.md>) | 23,162 |
| 29 | [生产者消费者模式](<10-异步基建(AOP)/C-知识/生产者消费者模式.md>) | 23,783 |
| 30 | [监控与统计集成](<10-异步基建(AOP)/C-知识/监控与统计集成.md>) | 21,666 |
| 31 | [调度器消费者](<10-异步基建(AOP)/C-知识/调度器消费者.md>) | 18,154 |
| 32 | [队列实现](<10-异步基建(AOP)/C-知识/队列实现.md>) | 19,964 |

### 11-前端应用

#### A 设计（design，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Canvas 渲染基础设施复用指南](<11-前端应用/A-设计/canvas_rendering_playbook.md>) | 已废弃 | 14,117 |
| 2 | [前端架构设计](<11-前端应用/A-设计/frontend_architecture.md>) | 现行 | 31,198 |
| 3 | [AI Orz HUD 设计系统](<11-前端应用/A-设计/ui_design_system.md>) | 现行 | 15,355 |

#### B 落地方案（plan，30 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [4 个 CRUD Edit Modal Implementation Plan](<11-前端应用/B-落地方案/4 个 CRUD Edit Modal（执行蓝图）.md>) | 已完成 | 31,189 |
| 2 | [AI Orz 前端重构实施计划](<11-前端应用/B-落地方案/AI Orz 前端重构实施计划（执行蓝图）.md>) | 已完成 | 141,753 |
| 3 | [Agent 记忆面板 + 对话体验打磨实现计划](<11-前端应用/B-落地方案/Agent 记忆面板 + 对话体验打磨实现计划（执行蓝图）.md>) | 已完成 | 21,050 |
| 4 | [Agent 详情页过期技能独立区 + 恢复按钮 与 DTO 清理 实现计划](<11-前端应用/B-落地方案/Agent详情页过期技能区与恢复按钮.md>) | 已完成 | 32,518 |
| 5 | [Canvas 动态渲染增强实施计划（阶段 2）](<11-前端应用/B-落地方案/Canvas 动态渲染增强实施计划（阶段 2）（执行蓝图）.md>) | 已完成 | 39,479 |
| 6 | [Canvas 改造扩展（高/中优先级页面）实施计划](<11-前端应用/B-落地方案/Canvas 改造扩展（高中优先级页面）实施计划（执行蓝图）.md>) | 已完成 | 69,342 |
| 7 | [Canvas 渲染基础设施实施计划（阶段 1）](<11-前端应用/B-落地方案/Canvas 渲染基础设施实施计划（阶段 1）（执行蓝图）.md>) | 已完成 | 19,236 |
| 8 | [Canvas 粒子系统与视觉增强实施计划（阶段 3）](<11-前端应用/B-落地方案/Canvas 粒子系统与视觉增强实施计划（阶段 3）（执行蓝图）.md>) | 已完成 | 46,311 |
| 9 | [Frontend Page Completion Implementation Plan](<11-前端应用/B-落地方案/Frontend Page Completion（执行蓝图）.md>) | 已完成 | 46,192 |
| 10 | [Skill 详情页 + 任务 Edit 入口 Implementation Plan](<11-前端应用/B-落地方案/Skill 详情页 + 任务 Edit 入口（执行蓝图）.md>) | 已完成 | 31,892 |
| 11 | [Tailwind CSS + DaisyUI 集成迁移](<11-前端应用/B-落地方案/TailwindDaisyUI迁移.md>) | 已完成 | 11,795 |
| 12 | [Workspace 工作台驾驶舱 Implementation Plan](<11-前端应用/B-落地方案/Workspace 工作台驾驶舱（执行蓝图）.md>) | 已完成 | 38,949 |
| 13 | [Workspace 第二期：Task DAG 分层布局 Plan](<11-前端应用/B-落地方案/Workspace 第二期：Task DAG 分层布局（执行蓝图）.md>) | 已完成 | 1,437 |
| 14 | [三个详情页新增关系图 Tab 实施计划](<11-前端应用/B-落地方案/三个详情页新增关系图 Tab 实施计划（执行蓝图）.md>) | 已完成 | 34,419 |
| 15 | [全局通知系统 (Toast) 实现计划](<11-前端应用/B-落地方案/全局通知系统 (Toast) 实现计划（执行蓝图）.md>) | 已完成 | 15,835 |
| 16 | [前端 Agent 运行时面板 + 取消按钮 + 全局运行中列表 实施计划](<11-前端应用/B-落地方案/前端 Agent 运行时面板 + 取消按钮 + 全局运行中列表 实施计划（执行蓝图）.md>) | 已完成 | 28,072 |
| 17 | [前端 Markdown 渲染全覆盖](<11-前端应用/B-落地方案/前端_Markdown_渲染全覆盖.md>) | 已完成 | 13,683 |
| 18 | [前端优化路线图](<11-前端应用/B-落地方案/前端优化路线图.md>) | 已完成 | 13,876 |
| 19 | [前端功能补全 — Plan 索引](<11-前端应用/B-落地方案/前端功能补全 — Plan 索引（执行蓝图）.md>) | 已完成 | 3,255 |
| 20 | [HTTP 工具表单 + 进程管理页面 + ChatSidePanel 工具调用 Tab](<11-前端应用/B-落地方案/前端工具与进程管理.md>) | 已完成 | 13,861 |
| 21 | [前端移动端响应式适配](<11-前端应用/B-落地方案/前端移动端响应式适配.md>) | 已完成 | 10,932 |
| 22 | [前端统计数据集成实现计划](<11-前端应用/B-落地方案/前端统计数据集成实现计划（执行蓝图）.md>) | 已完成 | 18,993 |
| 23 | [前端运行时观测面板](<11-前端应用/B-落地方案/前端运行时观测面板.md>) | 已完成 | 8,025 |
| 24 | [后台任务管理页面与列表清理接口重构](<11-前端应用/B-落地方案/后台任务管理页面与列表清理接口重构.md>) | 已完成 | 18,158 |
| 25 | [定时触发器前端体验优化计划](<11-前端应用/B-落地方案/定时触发器前端体验优化计划（执行蓝图）.md>) | 已完成 | 13,020 |
| 26 | [知识图谱 tags 展示与过滤 实施计划](<11-前端应用/B-落地方案/知识图谱 tags 展示与过滤 实施计划（执行蓝图）.md>) | 已完成 | 41,216 |
| 27 | [知识图谱交互完善 Implementation Plan](<11-前端应用/B-落地方案/知识图谱交互完善（执行蓝图）.md>) | 已完成 | 23,373 |
| 28 | [管理页面补全实施计划（模块化方案）](<11-前端应用/B-落地方案/管理页面补全实施计划（模块化方案）（执行蓝图）.md>) | 已完成 | 22,013 |
| 29 | [聊天页项目信息侧栏（ChatSidePanel）](<11-前端应用/B-落地方案/聊天页项目信息侧栏.md>) | 已完成 | 10,800 |
| 30 | [详情页补全 + 工具调用记录查询页 Implementation Plan](<11-前端应用/B-落地方案/详情页补全 + 工具调用记录查询页（执行蓝图）.md>) | 已完成 | 58,405 |

#### C 知识（wiki，49 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 监控面板](<11-前端应用/C-知识/AOP 监控面板.md>) | 19,675 |
| 2 | [API 客户端](<11-前端应用/C-知识/API 客户端.md>) | 27,449 |
| 3 | [Agent 管理功能](<11-前端应用/C-知识/Agent 管理功能.md>) | 26,969 |
| 4 | [Finance 管理页面](<11-前端应用/C-知识/Finance 管理页面.md>) | 21,204 |
| 5 | [HR 管理页面](<11-前端应用/C-知识/HR 管理页面.md>) | 16,133 |
| 6 | [HTTP工具创建界面](<11-前端应用/C-知识/HTTP工具创建界面.md>) | 17,852 |
| 7 | [MCP 服务器管理](<11-前端应用/C-知识/MCP 服务器管理.md>) | 19,241 |
| 8 | [Markdown渲染组件](<11-前端应用/C-知识/Markdown渲染组件.md>) | 21,272 |
| 9 | [Mermaid图表支持](<11-前端应用/C-知识/Mermaid图表支持.md>) | 17,328 |
| 10 | [Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线](<11-前端应用/C-知识/Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线/Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线.md>) | 16,397 |
| 11 | [Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格](<11-前端应用/C-知识/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格.md>) | 14,279 |
| 12 | [UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束](<11-前端应用/C-知识/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束.md>) | 23,207 |
| 13 | [UI 样式与主题](<11-前端应用/C-知识/UI 样式与主题.md>) | 32,007 |
| 14 | [recommend_seed_nodes 种子节点推荐：度数排序（入边+出边）Top N + published 度数持平决胜 + KnowledgeGraph 组件两端复用 + Agent 神经工具免绑定](<11-前端应用/C-知识/recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用/recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用.md>) | 17,143 |
| 15 | [业务组件](<11-前端应用/C-知识/业务组件.md>) | 24,297 |
| 16 | [任务管理功能](<11-前端应用/C-知识/任务管理功能.md>) | 23,204 |
| 17 | [前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发](<11-前端应用/C-知识/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发.md>) | 17,390 |
| 18 | [前端应用](<11-前端应用/C-知识/前端应用.md>) | 46,286 |
| 19 | [前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层](<11-前端应用/C-知识/前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层/前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层.md>) | 13,520 |
| 20 | [图表组件](<11-前端应用/C-知识/图表组件.md>) | 21,699 |
| 21 | [基础组件](<11-前端应用/C-知识/基础组件.md>) | 19,425 |
| 22 | [备份恢复管理（前端 UI 层）](<11-前端应用/C-知识/备份恢复管理.md>) | 14,975 |
| 23 | [定时任务调度（前端 UI 层）](<11-前端应用/C-知识/定时任务调度.md>) | 19,066 |
| 24 | [工件管理功能](<11-前端应用/C-知识/工件管理功能.md>) | 19,982 |
| 25 | [工具管理](<11-前端应用/C-知识/工具管理.md>) | 19,650 |
| 26 | [工具调用标签页](<11-前端应用/C-知识/工具调用标签页.md>) | 19,528 |
| 27 | [布局组件](<11-前端应用/C-知识/布局组件.md>) | 18,237 |
| 28 | [思考运行时面板观测接口](<11-前端应用/C-知识/思考运行时面板观测接口.md>) | 24,805 |
| 29 | [技能管理系统](<11-前端应用/C-知识/技能管理系统.md>) | 25,708 |
| 30 | [文档中心](<11-前端应用/C-知识/文档中心.md>) | 12,629 |
| 31 | [日志管理系统（前端 UI 层）](<11-前端应用/C-知识/日志管理系统.md>) | 17,970 |
| 32 | [模型提供商管理（前端 UI 层）](<11-前端应用/C-知识/模型提供商管理.md>) | 27,191 |
| 33 | [消息与工作区页面](<11-前端应用/C-知识/消息与工作区页面.md>) | 28,974 |
| 34 | [消息渠道管理（前端 UI 层）](<11-前端应用/C-知识/消息渠道管理.md>) | 17,001 |
| 35 | [用户与组织页面](<11-前端应用/C-知识/用户与组织页面.md>) | 18,217 |
| 36 | [种子数据管理（前端 UI 层）](<11-前端应用/C-知识/种子数据管理.md>) | 17,899 |
| 37 | [系统健康检查](<11-前端应用/C-知识/系统健康检查.md>) | 16,170 |
| 38 | [系统管理页面](<11-前端应用/C-知识/系统管理页面.md>) | 18,918 |
| 39 | [组件系统](<11-前端应用/C-知识/组件系统.md>) | 29,533 |
| 40 | [统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段](<11-前端应用/C-知识/统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段/统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段.md>) | 4,200 |
| 41 | [聊天侧面板](<11-前端应用/C-知识/聊天侧面板.md>) | 26,608 |
| 42 | [记忆搜索功能](<11-前端应用/C-知识/记忆搜索功能.md>) | 17,718 |
| 43 | [路由和中间件](<11-前端应用/C-知识/路由和中间件.md>) | 19,697 |
| 44 | [进程详情采用共享组件 + Modal 弹窗，不新建独立路由页](<11-前端应用/C-知识/进程详情采用共享组件 + Modal 弹窗，不新建独立路由页/进程详情采用共享组件 + Modal 弹窗，不新建独立路由页.md>) | 1,676 |
| 45 | [钩子系统](<11-前端应用/C-知识/钩子系统.md>) | 16,526 |
| 46 | [附件管理](<11-前端应用/C-知识/附件管理.md>) | 17,052 |
| 47 | [页面模块](<11-前端应用/C-知识/页面模块.md>) | 23,095 |
| 48 | [项目管理功能](<11-前端应用/C-知识/项目管理功能.md>) | 21,600 |
| 49 | [项目管理页面](<11-前端应用/C-知识/项目管理页面.md>) | 24,532 |

### 12-基础设施与部署

#### A 设计（design，5 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [common 统一错误类型设计](<12-基础设施与部署/A-设计/common_error_type.md>) | 已废弃 | 10,974 |
| 2 | [日志系统设计文档](<12-基础设施与部署/A-设计/logging_design.md>) | 现行 | 14,396 |
| 3 | [模型访问模式（access_mode）UI 设计规范](<12-基础设施与部署/A-设计/model_provider_access_mode_design.md>) | 现行 | 9,281 |
| 4 | [Seed 配置迁移中心](<12-基础设施与部署/A-设计/seed_config_migration.md>) | 已废弃 | 6,627 |
| 5 | [SQLx 0.8 + SQLite 开发规范与最佳实践](<12-基础设施与部署/A-设计/sqlx_guide.md>) | 现行 | 15,173 |

#### B 落地方案（plan，5 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Clippy Warning Cleanup Implementation Plan](<12-基础设施与部署/B-落地方案/Clippy Warning Cleanup（执行蓝图）.md>) | 已完成 | 27,770 |
| 2 | [Seed 配置迁移中心 实施计划](<12-基础设施与部署/B-落地方案/Seed 配置迁移中心 实施计划（执行蓝图）.md>) | 已完成 | 89,743 |
| 3 | [容器化与网关部署方案](<12-基础设施与部署/B-落地方案/容器化与网关部署方案.md>) | 已完成 | 23,403 |
| 4 | [系统初始化模型配置策略调整](<12-基础设施与部署/B-落地方案/系统初始化模型配置策略调整.md>) | 已完成 | 10,808 |
| 5 | [系统管理工具（备份 + 日志查询 + 角色权限）实现计划](<12-基础设施与部署/B-落地方案/系统管理工具（备份 + 日志查询 + 角色权限）实现计划（执行蓝图）.md>) | 已完成 | 29,350 |

#### C 知识（wiki，79 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A 轮询生产者](<12-基础设施与部署/C-知识/A2A 轮询生产者.md>) | 15,540 |
| 2 | [AOP 核心架构](<12-基础设施与部署/C-知识/AOP 核心架构.md>) | 23,455 |
| 3 | [AOP监控处理器](<12-基础设施与部署/C-知识/AOP监控处理器.md>) | 17,536 |
| 4 | [Agent 循环消费者（框架层）](<12-基础设施与部署/C-知识/Agent 循环消费者.md>) | 14,369 |
| 5 | [Doctor 依赖预检](<12-基础设施与部署/C-知识/Doctor 依赖预检/Doctor 依赖预检.md>) | 5,899 |
| 6 | [Doctor 依赖预检](<12-基础设施与部署/C-知识/Doctor 依赖预检.md>) | 5,347 |
| 7 | [Finance模块处理器](<12-基础设施与部署/C-知识/Finance模块处理器.md>) | 34,066 |
| 8 | [HNSW 索引存储后端](<12-基础设施与部署/C-知识/HNSW 索引存储后端.md>) | 15,601 |
| 9 | [LanceDB 存储后端](<12-基础设施与部署/C-知识/LanceDB 存储后端.md>) | 17,243 |
| 10 | [MCP 服务器管理 API](<12-基础设施与部署/C-知识/MCP 服务器管理 API.md>) | 15,775 |
| 11 | [MCP服务器管理处理器](<12-基础设施与部署/C-知识/MCP服务器管理处理器.md>) | 16,142 |
| 12 | [Rust Workspace + NPM 多语言依赖管理（Cargo.lock / package-lock.json / SQLx 离线构建）](<12-基础设施与部署/C-知识/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）.md>) | 9,616 |
| 13 | [SQLite VSS 存储后端](<12-基础设施与部署/C-知识/SQLite VSS 存储后端.md>) | 15,198 |
| 14 | [System模块处理器](<12-基础设施与部署/C-知识/System模块处理器.md>) | 26,161 |
| 15 | [AI Orz 设计与落地方案文档维护 Skill（规范全文版）](<12-基础设施与部署/C-知识/ai-orz-doc-maintainer.md>) | 23,454 |
| 16 | [AI Orz Wiki 维护 Skill（规范全文版）](<12-基础设施与部署/C-知识/ai-orz-wiki-maintainer.md>) | 22,154 |
| 17 | [pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语](<12-基础设施与部署/C-知识/pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语/pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语.md>) | 15,359 |
| 18 | [scripts/ 归置 + Makefile 统一入口 + 60+ 引用全量修正](<12-基础设施与部署/C-知识/scripts 归置 + Makefile 统一入口 + 60+ 引用全量修正/scripts 归置 + Makefile 统一入口 + 60+ 引用全量修正.md>) | 17,077 |
| 19 | [任务管理 API](<12-基础设施与部署/C-知识/任务管理 API.md>) | 16,158 |
| 20 | [任务管理处理器](<12-基础设施与部署/C-知识/任务管理处理器.md>) | 18,278 |
| 21 | [优雅关闭系统](<12-基础设施与部署/C-知识/优雅关闭系统.md>) | 11,944 |
| 22 | [内存向量存储后端](<12-基础设施与部署/C-知识/内存向量存储后端.md>) | 15,744 |
| 23 | [后台任务系统](<12-基础设施与部署/C-知识/后台任务系统.md>) | 20,759 |
| 24 | [后台进程管理系统](<12-基础设施与部署/C-知识/后台进程管理系统.md>) | 24,551 |
| 25 | [向量存储（框架层）](<12-基础设施与部署/C-知识/向量存储.md>) | 19,437 |
| 26 | [向量存储抽象接口（框架层）](<12-基础设施与部署/C-知识/向量存储抽象接口.md>) | 19,568 |
| 27 | [基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）](<12-基础设施与部署/C-知识/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）.md>) | 10,599 |
| 28 | [基础设施](<12-基础设施与部署/C-知识/基础设施.md>) | 18,627 |
| 29 | [备份恢复 API](<12-基础设施与部署/C-知识/备份恢复 API.md>) | 13,589 |
| 30 | [备份恢复管理（业务功能层）](<12-基础设施与部署/C-知识/备份恢复管理.md>) | 14,834 |
| 31 | [备份管理处理器](<12-基础设施与部署/C-知识/备份管理处理器.md>) | 14,228 |
| 32 | [存储系统（框架层）](<12-基础设施与部署/C-知识/存储系统.md>) | 22,508 |
| 33 | [存储配置](<12-基础设施与部署/C-知识/存储配置.md>) | 14,626 |
| 34 | [定时任务 API](<12-基础设施与部署/C-知识/定时任务 API.md>) | 22,093 |
| 35 | [定时任务处理器](<12-基础设施与部署/C-知识/定时任务处理器.md>) | 21,186 |
| 36 | [容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像](<12-基础设施与部署/C-知识/容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像/容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像.md>) | 9,573 |
| 37 | [工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆](<12-基础设施与部署/C-知识/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆.md>) | 9,509 |
| 38 | [工具执行日志消费者](<12-基础设施与部署/C-知识/工具执行日志消费者.md>) | 17,700 |
| 39 | [工具执行统计消费者](<12-基础设施与部署/C-知识/工具执行统计消费者.md>) | 13,444 |
| 40 | [工具管理 API（API 参考层）](<12-基础设施与部署/C-知识/工具管理 API.md>) | 19,986 |
| 41 | [工具管理处理器](<12-基础设施与部署/C-知识/工具管理处理器.md>) | 20,050 |
| 42 | [思考轮次统计消费者](<12-基础设施与部署/C-知识/思考轮次统计消费者.md>) | 17,703 |
| 43 | [持续集成与发布工作流](<12-基础设施与部署/C-知识/持续集成与发布工作流.md>) | 29,447 |
| 44 | [数据库迁移管理](<12-基础设施与部署/C-知识/数据库迁移管理/数据库迁移管理.md>) | 6,019 |
| 45 | [文档链接质量门禁](<12-基础设施与部署/C-知识/文档链接质量门禁.md>) | 23,767 |
| 46 | [日志查询 API](<12-基础设施与部署/C-知识/日志查询 API.md>) | 15,969 |
| 47 | [日志查询处理器](<12-基础设施与部署/C-知识/日志查询处理器.md>) | 16,854 |
| 48 | [日志管理系统（系统管理层）](<12-基础设施与部署/C-知识/日志管理系统.md>) | 19,200 |
| 49 | [日志系统](<12-基础设施与部署/C-知识/日志系统.md>) | 20,484 |
| 50 | [时间戳规范](<12-基础设施与部署/C-知识/时间戳规范/时间戳规范.md>) | 5,185 |
| 51 | [服务层](<12-基础设施与部署/C-知识/服务层.md>) | 18,070 |
| 52 | [本地文件安全与工作区隔离](<12-基础设施与部署/C-知识/本地文件安全与工作区隔离.md>) | 23,232 |
| 53 | [模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片](<12-基础设施与部署/C-知识/模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片/模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片.md>) | 7,888 |
| 54 | [模型提供商管理 API](<12-基础设施与部署/C-知识/模型提供商管理 API.md>) | 23,711 |
| 55 | [模型提供商管理（业务功能层）](<12-基础设施与部署/C-知识/模型提供商管理.md>) | 24,049 |
| 56 | [模型提供商管理处理器](<12-基础设施与部署/C-知识/模型提供商管理处理器.md>) | 21,479 |
| 57 | [模型提供商配置](<12-基础设施与部署/C-知识/模型提供商配置.md>) | 15,010 |
| 58 | [模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析](<12-基础设施与部署/C-知识/模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析/模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析.md>) | 11,455 |
| 59 | [消息处理 API](<12-基础设施与部署/C-知识/消息处理 API.md>) | 20,237 |
| 60 | [消息消费者（框架层）](<12-基础设施与部署/C-知识/消息消费者.md>) | 18,322 |
| 61 | [消息渠道管理 API](<12-基础设施与部署/C-知识/消息渠道管理 API.md>) | 20,481 |
| 62 | [消息渠道管理处理器](<12-基础设施与部署/C-知识/消息渠道管理处理器.md>) | 17,847 |
| 63 | [消息系统处理器](<12-基础设施与部署/C-知识/消息系统处理器.md>) | 19,774 |
| 64 | [消息通道生产者](<12-基础设施与部署/C-知识/消息通道生产者.md>) | 15,614 |
| 65 | [监控指标 API](<12-基础设施与部署/C-知识/监控指标 API.md>) | 18,506 |
| 66 | [种子数据处理器](<12-基础设施与部署/C-知识/种子数据处理器.md>) | 20,757 |
| 67 | [种子数据管理 API](<12-基础设施与部署/C-知识/种子数据管理 API.md>) | 18,856 |
| 68 | [种子数据管理（业务功能层）](<12-基础设施与部署/C-知识/种子数据管理.md>) | 32,596 |
| 69 | [种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离](<12-基础设施与部署/C-知识/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发.md>) | 30,407 |
| 70 | [系统初始化](<12-基础设施与部署/C-知识/系统初始化.md>) | 25,660 |
| 71 | [系统管理](<12-基础设施与部署/C-知识/系统管理.md>) | 20,151 |
| 72 | [系统管理模块 API](<12-基础设施与部署/C-知识/系统管理模块 API.md>) | 19,066 |
| 73 | [脱敏引擎下沉 common](<12-基础设施与部署/C-知识/脱敏引擎下沉 common.md>) | 25,444 |
| 74 | [脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏](<12-基础设施与部署/C-知识/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏.md>) | 5,884 |
| 75 | [财务管理模块 API](<12-基础设施与部署/C-知识/财务管理模块 API.md>) | 23,365 |
| 76 | [配置与部署](<12-基础设施与部署/C-知识/配置与部署.md>) | 31,674 |
| 77 | [配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入](<12-基础设施与部署/C-知识/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入.md>) | 14,871 |
| 78 | [附件管理 API](<12-基础设施与部署/C-知识/附件管理 API.md>) | 18,021 |
| 79 | [附件管理处理器](<12-基础设施与部署/C-知识/附件管理处理器.md>) | 17,390 |

### 13-数据模型

#### A 设计（design，5 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [前后端 API 协议规范（common DTO 单一事实源）](<13-数据模型/A-设计/api_protocol_convention.md>) | 现行 | 6,400 |
| 2 | [产物与消息附件统一存储设计](<13-数据模型/A-设计/attachment_storage.md>) | 已废弃 | 27,593 |
| 3 | [查询接口分页与通用 count 规范](<13-数据模型/A-设计/pagination_and_count_convention.md>) | 现行 | 16,819 |
| 4 | [时间戳处理约定](<13-数据模型/A-设计/timestamp_convention.md>) | 现行 | 7,118 |
| 5 | [统一参数 IDL + 自动生成 HTTP handler 设计文档](<13-数据模型/A-设计/unified_idl_http_handler.md>) | 已废弃 | 17,789 |

#### B 落地方案（plan，4 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Handler 管理面 API 补齐方案](<13-数据模型/B-落地方案/Handler 管理面 API 补齐方案.md>) | 已完成 | 44,113 |
| 2 | [Query 接口分页与 List 接口简化重构](<13-数据模型/B-落地方案/Query接口分页与List接口简化重构.md>) | 已完成 | 16,198 |
| 3 | [前端 API 协议结构体统一改造](<13-数据模型/B-落地方案/前端API协议结构重构.md>) | 已完成 | 8,473 |
| 4 | [批量查询与通用Query接口增强重构](<13-数据模型/B-落地方案/批量查询与通用Query接口增强重构.md>) | 已完成 | 16,579 |

#### C 知识（wiki，39 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [API 参考](<13-数据模型/C-知识/API 参考.md>) | 19,264 |
| 2 | [Agent 和技能模型](<13-数据模型/C-知识/Agent 和技能模型.md>) | 18,999 |
| 3 | [Agent 实体](<13-数据模型/C-知识/Agent 实体.md>) | 16,167 |
| 4 | [DAO 层多态](<13-数据模型/C-知识/DAO 层多态.md>) | 16,330 |
| 5 | [HNSW向量存储](<13-数据模型/C-知识/HNSW向量存储.md>) | 15,577 |
| 6 | [Handler 宏元数据契约 + ToolPo config/parameters_schema 字段分离](<13-数据模型/C-知识/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位.md>) | 15,162 |
| 7 | [LanceDB向量存储](<13-数据模型/C-知识/LanceDB向量存储.md>) | 15,130 |
| 8 | [Mermaid 图表采用 vendor mermaid.js 的独立阶段实现](<13-数据模型/C-知识/Mermaid 图表采用 vendor mermaid.js 的独立阶段实现/Mermaid 图表采用 vendor mermaid.js 的独立阶段实现.md>) | 1,717 |
| 9 | [SQLite VSS向量存储](<13-数据模型/C-知识/SQLite VSS向量存储.md>) | 17,068 |
| 10 | [SQLite 数据访问对象](<13-数据模型/C-知识/SQLite 数据访问对象.md>) | 17,337 |
| 11 | [三位一体混合搜索（FTS5 + 向量 + 合并排序）](<13-数据模型/C-知识/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）.md>) | 14,593 |
| 12 | [全文搜索（代码落地层）](<13-数据模型/C-知识/全文搜索.md>) | 17,078 |
| 13 | [内存向量存储](<13-数据模型/C-知识/内存向量存储.md>) | 15,266 |
| 14 | [制品和附件](<13-数据模型/C-知识/制品和附件.md>) | 22,493 |
| 15 | [向量存储（代码落地层）](<13-数据模型/C-知识/向量存储.md>) | 17,975 |
| 16 | [向量存储抽象接口（代码落地层）](<13-数据模型/C-知识/向量存储抽象接口.md>) | 17,211 |
| 17 | [向量存储数据访问对象](<13-数据模型/C-知识/向量存储数据访问对象.md>) | 20,034 |
| 18 | [外部 API 数据访问对象](<13-数据模型/C-知识/外部 API 数据访问对象.md>) | 19,059 |
| 19 | [存储架构设计](<13-数据模型/C-知识/存储架构设计.md>) | 18,679 |
| 20 | [存储系统（代码落地层）](<13-数据模型/C-知识/存储系统.md>) | 21,862 |
| 21 | [数据对象层（DAO）](<13-数据模型/C-知识/数据对象层 (DAO).md>) | 19,685 |
| 22 | [数据模型](<13-数据模型/C-知识/数据模型.md>) | 28,067 |
| 23 | [数据访问层 (DAL)](<13-数据模型/C-知识/数据访问层 (DAL).md>) | 12,772 |
| 24 | [文件存储模型](<13-数据模型/C-知识/文件存储模型.md>) | 18,327 |
| 25 | [核心实体模型](<13-数据模型/C-知识/核心实体模型.md>) | 16,414 |
| 26 | [消息和记忆模型](<13-数据模型/C-知识/消息和记忆模型.md>) | 22,647 |
| 27 | [消息系统（数据模型层）](<13-数据模型/C-知识/消息系统.md>) | 22,483 |
| 28 | [消息系统数据访问对象](<13-数据模型/C-知识/消息系统数据访问对象.md>) | 15,260 |
| 29 | [系统模型](<13-数据模型/C-知识/系统模型.md>) | 19,796 |
| 30 | [统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系](<13-数据模型/C-知识/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系.md>) | 10,876 |
| 31 | [统计数据分析对象](<13-数据模型/C-知识/统计数据分析对象.md>) | 15,736 |
| 32 | [记忆和向量系统](<13-数据模型/C-知识/记忆和向量系统.md>) | 30,207 |
| 33 | [请求上下文](<13-数据模型/C-知识/请求上下文.md>) | 20,432 |
| 34 | [财务领域](<13-数据模型/C-知识/财务领域.md>) | 19,564 |
| 35 | [身份凭证管理（统一 Domain CRUD + AES-256-GCM 加密存储 + 生命周期联动）](<13-数据模型/C-知识/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md>) | 30,548 |
| 36 | [错误处理与状态码](<13-数据模型/C-知识/错误处理与状态码.md>) | 18,820 |
| 37 | [附件存储与DTO协议统一：Attachment Finance域统一资产 + PagedResult T map 全链路 + common::api 单一事实源 + count与query 复用 WHERE](<13-数据模型/C-知识/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE.md>) | 16,443 |
| 38 | [项目管理（数据模型层）](<13-数据模型/C-知识/项目管理.md>) | 21,355 |
| 39 | [飞书集成系统](<13-数据模型/C-知识/飞书集成系统.md>) | 24,331 |

### 14-测试

#### A 设计（design，2 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [浏览器操作验证用例设计](<14-测试/A-设计/browser_e2e_test_design.md>) | 已废弃 | 6,818 |
| 2 | [项目测试规范](<14-测试/A-设计/testing_guidelines.md>) | 已废弃 | 25,844 |

#### B 落地方案（plan，5 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Agent 智能层集成测试 Implementation Plan](<14-测试/B-落地方案/Agent 智能层集成测试（执行蓝图）.md>) | 已完成 | 39,690 |
| 2 | [Agent 管理集成测试落地](<14-测试/B-落地方案/Agent管理集成测试.md>) | 已完成 | 13,421 |
| 3 | [Test Speed Optimization Implementation Plan](<14-测试/B-落地方案/Test Speed Optimization（执行蓝图）.md>) | 已完成 | 22,718 |
| 4 | [测试补充方案 - 2026-05-14](<14-测试/B-落地方案/测试补充方案（2026-05-14）.md>) | 已完成 | 7,229 |
| 5 | [集成测试基础设施搭建（HTTP API 端到端）](<14-测试/B-落地方案/集成测试基础设施.md>) | 已完成 | 20,525 |

#### C 知识（wiki，4 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖](<14-测试/C-知识/Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖/Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖.md>) | 8,653 |
| 2 | [测试与质量工程：974测试100%通过 + 984后端82前端58common + 87集成测试19targets + cargo-llvm-cov PR38%/main45%门槛 + clippy -D warnings零容忍 + Playwright E2E](<14-测试/C-知识/测试与质量工程：1124测试100%通过 + 984后端82前端 + 87集成测试19targets + cargo-llvm-cov 38%/45%门槛 + clippy零容忍+Playwright E2E/测试与质量工程：1124测试100%通过 + 984后端82前端 + 87集成测试19targets + cargo-llvm-cov 38%/45%门槛 + clippy零容忍+Playwright E2E.md>) | 23,640 |
| 3 | [测试指南](<14-测试/C-知识/测试指南.md>) | 31,482 |
| 4 | [端到端测试基础设施](<14-测试/C-知识/端到端测试基础设施.md>) | 13,174 |
