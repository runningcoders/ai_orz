<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 模块知识索引（docs/modules）

> **自动生成**：生成器 `tools/wiki_index.py` · 门禁 `make wiki-lint` · 重建 `make wiki-index`
>
> 覆盖三个层：`A-设计`（为什么）· `B-落地方案`（要去哪）· `C-知识`（是什么）。
> 机器可读版见 `INDEX.json`；Agent 实现需求请从 `docs/AGENT_GUIDE.md` 进入。

**合计 525 篇 / 10,422,278 B**，分布于 15 个模块。

## 模块概览

| 模块 | A 设计 | B 落地方案 | C 知识 | 合计 | 字节 |
|---|---:|---:|---:|---:|---:|
| [00-总览与架构](#00-总览与架构) | 0 | 0 | 35 | 35 | 644,551 |
| [01-组织与权限](#01-组织与权限) | 1 | 4 | 36 | 41 | 825,307 |
| [02-Agent运行时](#02-agent运行时) | 5 | 1 | 19 | 25 | 682,874 |
| [03-记忆系统](#03-记忆系统) | 0 | 0 | 13 | 13 | 289,578 |
| [04-对话与消息](#04-对话与消息) | 3 | 4 | 21 | 28 | 658,772 |
| [05-任务与项目](#05-任务与项目) | 1 | 0 | 53 | 54 | 932,831 |
| [06-工具与技能](#06-工具与技能) | 4 | 5 | 52 | 61 | 1,209,332 |
| [07-外部Agent(A2A)](#07-外部agenta2a) | 0 | 0 | 20 | 20 | 327,487 |
| [08-搜索](#08-搜索) | 1 | 2 | 21 | 24 | 606,791 |
| [09-统计与监控](#09-统计与监控) | 1 | 1 | 8 | 10 | 162,283 |
| [10-异步基建(AOP)](#10-异步基建aop) | 1 | 0 | 32 | 33 | 709,825 |
| [11-前端应用](#11-前端应用) | 2 | 0 | 49 | 51 | 1,026,931 |
| [12-基础设施与部署](#12-基础设施与部署) | 3 | 2 | 79 | 84 | 1,519,066 |
| [13-数据模型](#13-数据模型) | 3 | 0 | 39 | 42 | 749,872 |
| [14-测试](#14-测试) | 0 | 0 | 4 | 4 | 76,778 |

## 各模块文档

### 00-总览与架构

#### C 知识（wiki，35 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 事件系统架构](<00-总览与架构/C-知识/AOP 事件系统架构.md>) | 20,561 |
| 2 | [API协议规范](<00-总览与架构/C-知识/API协议规范.md>) | 17,159 |
| 3 | [DAL 层组合](<00-总览与架构/C-知识/DAL 层组合.md>) | 20,010 |
| 4 | [Domain 层编排](<00-总览与架构/C-知识/Domain 层编排.md>) | 18,013 |
| 5 | [Finance 领域编排](<00-总览与架构/C-知识/Finance 领域编排.md>) | 19,298 |
| 6 | [HR 领域编排](<00-总览与架构/C-知识/HR 领域编排.md>) | 19,923 |
| 7 | [HR领域](<00-总览与架构/C-知识/HR领域.md>) | 20,280 |
| 8 | [Handler 层设计](<00-总览与架构/C-知识/Handler 层设计.md>) | 14,149 |
| 9 | [Message 领域编排](<00-总览与架构/C-知识/Message 领域编排.md>) | 15,302 |
| 10 | [Organization 领域编排](<00-总览与架构/C-知识/Organization 领域编排.md>) | 19,292 |
| 11 | [PO 与业务实体分层](<00-总览与架构/C-知识/PO 与业务实体分层.md>) | 14,821 |
| 12 | [Project 领域编排](<00-总览与架构/C-知识/Project 领域编排.md>) | 13,244 |
| 13 | [Runtime 领域编排](<00-总览与架构/C-知识/Runtime 领域编排.md>) | 39,495 |
| 14 | [System 领域编排](<00-总览与架构/C-知识/System 领域编排.md>) | 16,354 |
| 15 | [分层架构设计](<00-总览与架构/C-知识/分层架构设计.md>) | 15,491 |
| 16 | [分页与计数规范](<00-总览与架构/C-知识/分页与计数规范.md>) | 12,402 |
| 17 | [前端架构设计](<00-总览与架构/C-知识/前端架构设计.md>) | 38,856 |
| 18 | [功能模块](<00-总览与架构/C-知识/功能模块.md>) | 15,675 |
| 19 | [处理器层](<00-总览与架构/C-知识/处理器层.md>) | 15,227 |
| 20 | [安全架构设计](<00-总览与架构/C-知识/安全架构设计.md>) | 13,907 |
| 21 | [工具系统三层调用架构](<00-总览与架构/C-知识/工具系统三层调用架构/工具系统三层调用架构.md>) | 5,227 |
| 22 | [开发指南](<00-总览与架构/C-知识/开发指南.md>) | 16,354 |
| 23 | [思维轨迹架构](<00-总览与架构/C-知识/思维轨迹架构.md>) | 5,377 |
| 24 | [技术栈概览](<00-总览与架构/C-知识/技术栈概览.md>) | 18,769 |
| 25 | [数据存储架构](<00-总览与架构/C-知识/数据存储架构.md>) | 29,357 |
| 26 | [数据库迁移管理](<00-总览与架构/C-知识/数据库迁移管理.md>) | 33,563 |
| 27 | [整体架构概览](<00-总览与架构/C-知识/整体架构概览.md>) | 13,801 |
| 28 | [时间戳处理规范](<00-总览与架构/C-知识/时间戳处理规范.md>) | 19,987 |
| 29 | [本体子系统架构](<00-总览与架构/C-知识/本体子系统架构.md>) | 6,645 |
| 30 | [架构设计](<00-总览与架构/C-知识/架构设计.md>) | 15,501 |
| 31 | [核心模块](<00-总览与架构/C-知识/核心模块.md>) | 14,086 |
| 32 | [系统领域](<00-总览与架构/C-知识/系统领域.md>) | 14,007 |
| 33 | [统一工具调用架构](<00-总览与架构/C-知识/统一工具调用架构.md>) | 23,917 |
| 34 | [记忆系统架构](<00-总览与架构/C-知识/记忆系统架构.md>) | 30,200 |
| 35 | [领域层](<00-总览与架构/C-知识/领域层.md>) | 18,301 |

### 01-组织与权限

#### A 设计（design，1 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [用户身份凭证独立表设计（user_credentials）](<01-组织与权限/A-设计/user_credentials_design.md>) | 现行 | 29,265 |

#### B 落地方案（plan，4 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [用户身份凭证独立表落地](<01-组织与权限/B-落地方案/用户身份凭证独立表落地.md>) | 已完成 | 10,358 |
| 2 | [组织组网与去中心化联邦方案（评审稿）](<01-组织与权限/B-落地方案/组织组网与去中心化联邦方案.md>) | 草案 | 17,930 |
| 3 | [联邦鉴权升级方案（静态 token → 组织公钥签名）](<01-组织与权限/B-落地方案/联邦鉴权升级方案.md>) | 进行中 | 34,079 |
| 4 | [跨组织业务调用方案（Phase 2 主体）](<01-组织与权限/B-落地方案/跨组织业务调用方案.md>) | 已完成 | 40,657 |

#### C 知识（wiki，36 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Agent 管理 API](<01-组织与权限/C-知识/Agent 管理 API.md>) | 18,872 |
| 2 | [GitHub 集成](<01-组织与权限/C-知识/GitHub 集成.md>) | 23,653 |
| 3 | [HR模块处理器](<01-组织与权限/C-知识/HR模块处理器.md>) | 19,321 |
| 4 | [IdentityCredentials 身份凭证扩展：WechatIlink 扫码授权 + 凭证整组轮换 + 与通用 CRUD 框架的集成点](<01-组织与权限/C-知识/IdentityCredentials 身份凭证扩展：授权流程 + QR登录 + 多渠道凭证管理/IdentityCredentials 身份凭证扩展：授权流程 + QR登录 + 多渠道凭证管理.md>) | 11,747 |
| 5 | [Organization模块处理器](<01-组织与权限/C-知识/Organization模块处理器.md>) | 19,639 |
| 6 | [Skill处理器](<01-组织与权限/C-知识/Skill处理器.md>) | 21,852 |
| 7 | [User模块处理器](<01-组织与权限/C-知识/User模块处理器.md>) | 14,977 |
| 8 | [人力资源模块 API](<01-组织与权限/C-知识/人力资源模块 API.md>) | 28,183 |
| 9 | [当前组织信息处理器](<01-组织与权限/C-知识/当前组织信息处理器.md>) | 19,424 |
| 10 | [技能包管理 API](<01-组织与权限/C-知识/技能包管理 API.md>) | 23,394 |
| 11 | [用户与组织管理](<01-组织与权限/C-知识/用户与组织管理.md>) | 20,646 |
| 12 | [用户管理](<01-组织与权限/C-知识/用户管理.md>) | 19,428 |
| 13 | [用户管理处理器](<01-组织与权限/C-知识/用户管理处理器.md>) | 21,028 |
| 14 | [用户管理模块 API](<01-组织与权限/C-知识/用户管理模块 API.md>) | 19,565 |
| 15 | [用户认证与授权](<01-组织与权限/C-知识/用户认证与授权.md>) | 19,293 |
| 16 | [系统初始化处理器](<01-组织与权限/C-知识/系统初始化处理器.md>) | 16,804 |
| 17 | [组织权限与用户偏好：Organization 多级组织 + UserRole 并查集继承 + JWT Cookie+Bearer 双模式 + 偏好双源沉淀 + Agent入职流程](<01-组织与权限/C-知识/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步.md>) | 28,654 |
| 18 | [组织管理](<01-组织与权限/C-知识/组织管理.md>) | 20,389 |
| 19 | [组织管理处理器](<01-组织与权限/C-知识/组织管理处理器.md>) | 23,559 |
| 20 | [组织管理模块 API](<01-组织与权限/C-知识/组织管理模块 API.md>) | 17,972 |
| 21 | [组织组网与联邦](<01-组织与权限/C-知识/组织组网与联邦.md>) | 23,571 |
| 22 | [组织领域](<01-组织与权限/C-知识/组织领域.md>) | 17,555 |
| 23 | [聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成](<01-组织与权限/C-知识/聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成/聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成.md>) | 8,248 |
| 24 | [联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断](<01-组织与权限/C-知识/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断.md>) | 13,724 |
| 25 | [联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接](<01-组织与权限/C-知识/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接.md>) | 32,095 |
| 26 | [认证与授权 API](<01-组织与权限/C-知识/认证与授权 API.md>) | 15,387 |
| 27 | [认证与授权](<01-组织与权限/C-知识/认证与授权.md>) | 15,347 |
| 28 | [认证处理器](<01-组织与权限/C-知识/认证处理器.md>) | 15,044 |
| 29 | [记忆管理 API](<01-组织与权限/C-知识/记忆管理 API.md>) | 16,524 |
| 30 | [跨组织业务调用鉴权](<01-组织与权限/C-知识/跨组织业务调用鉴权.md>) | 20,959 |
| 31 | [跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌](<01-组织与权限/C-知识/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities.md>) | 21,868 |
| 32 | [身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用](<01-组织与权限/C-知识/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用.md>) | 13,794 |
| 33 | [身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式](<01-组织与权限/C-知识/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式.md>) | 11,716 |
| 34 | [身份凭证与授权流程](<01-组织与权限/C-知识/身份凭证与授权流程.md>) | 29,185 |
| 35 | [身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立](<01-组织与权限/C-知识/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立.md>) | 12,452 |
| 36 | [身份凭证统一链路（总卡）](<01-组织与权限/C-知识/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）.md>) | 17,149 |

### 02-Agent运行时

#### A 设计（design，5 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [决策型策略设计（策略引擎承载小脑场景）](<02-Agent运行时/A-设计/cerebellum_decision_strategy_design.md>) | 现行 | 24,529 |
| 2 | [Runtime Domain 设计](<02-Agent运行时/A-设计/runtime_design.md>) | 现行 | 141,847 |
| 3 | [思考生命周期设计（Think Lifecycle）](<02-Agent运行时/A-设计/think_lifecycle_design.md>) | 现行 | 43,584 |
| 4 | [思考图引擎改造设计（Awaken 主思考路径节点化）](<02-Agent运行时/A-设计/thinking_graph_engine_design.md>) | 落地中 | 10,207 |
| 5 | [Agent 思考运行时 + 策略引擎设计](<02-Agent运行时/A-设计/thinking_task_policy_engine_design.md>) | 现行 | 47,909 |

#### B 落地方案（plan，1 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [思考图引擎改造实施计划](<02-Agent运行时/B-落地方案/思考图引擎改造.md>) | 进行中 | 34,415 |

#### C 知识（wiki，19 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A Client + 外部 Agent Runtime（出站调用 + 内部桥接）](<02-Agent运行时/C-知识/A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送/A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送.md>) | 16,306 |
| 2 | [AI Agent 管理](<02-Agent运行时/C-知识/AI Agent 管理.md>) | 18,270 |
| 3 | [Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报](<02-Agent运行时/C-知识/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报.md>) | 8,818 |
| 4 | [Agent 搜索与推荐](<02-Agent运行时/C-知识/Agent 搜索与推荐.md>) | 13,238 |
| 5 | [Agent 生命周期管理](<02-Agent运行时/C-知识/Agent 生命周期管理.md>) | 28,012 |
| 6 | [Agent处理器](<02-Agent运行时/C-知识/Agent处理器.md>) | 24,451 |
| 7 | [Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2](<02-Agent运行时/C-知识/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联.md>) | 15,988 |
| 8 | [ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板](<02-Agent运行时/C-知识/ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板/ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板.md>) | 6,388 |
| 9 | [小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类](<02-Agent运行时/C-知识/小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类/小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类.md>) | 17,916 |
| 10 | [小脑快判断（System One）](<02-Agent运行时/C-知识/小脑快判断（System One）.md>) | 30,789 |
| 11 | [工具绑定管理](<02-Agent运行时/C-知识/工具绑定管理.md>) | 15,638 |
| 12 | [思考循环异常退出兜底总结与错误上报结构化](<02-Agent运行时/C-知识/思考循环异常退出兜底总结与错误上报结构化/思考循环异常退出兜底总结与错误上报结构化.md>) | 4,735 |
| 13 | [思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件](<02-Agent运行时/C-知识/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件.md>) | 13,082 |
| 14 | [技能包管理](<02-Agent运行时/C-知识/技能包管理.md>) | 30,153 |
| 15 | [策略引擎与 Shell 拦截层](<02-Agent运行时/C-知识/策略引擎与 Shell 拦截层.md>) | 31,774 |
| 16 | [策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层](<02-Agent运行时/C-知识/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层.md>) | 15,127 |
| 17 | [记忆系统管理](<02-Agent运行时/C-知识/记忆系统管理.md>) | 36,127 |
| 18 | [运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化](<02-Agent运行时/C-知识/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化.md>) | 13,270 |
| 19 | [运行时领域](<02-Agent运行时/C-知识/运行时领域.md>) | 40,301 |

### 03-记忆系统

#### C 知识（wiki，13 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Agent 记忆系统](<03-记忆系统/C-知识/Agent 记忆系统.md>) | 20,427 |
| 2 | [Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle](<03-记忆系统/C-知识/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md>) | 32,030 |
| 3 | [RESTful API](<03-记忆系统/C-知识/RESTful API.md>) | 35,544 |
| 4 | [原始记忆追踪](<03-记忆系统/C-知识/原始记忆追踪.md>) | 22,490 |
| 5 | [四层记忆系统](<03-记忆系统/C-知识/四层记忆系统.md>) | 26,215 |
| 6 | [工作记忆 (Working Memory)](<03-记忆系统/C-知识/工作记忆 (Working Memory).md>) | 20,269 |
| 7 | [核心记忆 (Core Memory)](<03-记忆系统/C-知识/核心记忆 (Core Memory).md>) | 17,586 |
| 8 | [短期记忆（Short-term Memory）](<03-记忆系统/C-知识/短期记忆 (Short-term Memory).md>) | 21,253 |
| 9 | [短期记忆索引](<03-记忆系统/C-知识/短期记忆索引.md>) | 16,653 |
| 10 | [记忆搜索机制](<03-记忆系统/C-知识/记忆搜索机制.md>) | 20,951 |
| 11 | [记忆沉淀机制](<03-记忆系统/C-知识/记忆沉淀机制.md>) | 28,039 |
| 12 | [记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开](<03-记忆系统/C-知识/记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开/记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开.md>) | 6,246 |
| 13 | [长期记忆（Long-term Memory）](<03-记忆系统/C-知识/长期记忆 (Long-term Memory).md>) | 21,875 |

### 04-对话与消息

#### A 设计（design，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [飞书（Lark）渠道集成设计 —— 长连接协议对齐版](<04-对话与消息/A-设计/lark_channel_integration_design.md>) | 现行 | 8,859 |
| 2 | [消息撤回与取消能力设计](<04-对话与消息/A-设计/message_recall_design.md>) | 现行 | 29,335 |
| 3 | [微信渠道接入设计（iLink ClawBot / 企微智能机器人）](<04-对话与消息/A-设计/wechat_channel_integration_design.md>) | 草案 | 59,535 |

#### B 落地方案（plan，4 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [微信 iLink 协议对齐重构方案](<04-对话与消息/B-落地方案/微信iLink协议对齐重构方案.md>) | 进行中 | 34,376 |
| 2 | [消息撤回与取消能力落地](<04-对话与消息/B-落地方案/消息撤回与取消能力.md>) | 已完成 | 19,991 |
| 3 | [飞书链路对齐审查](<04-对话与消息/B-落地方案/飞书链路对齐审查.md>) | 已完成 | 51,592 |
| 4 | [飞书链路对齐重构方案](<04-对话与消息/B-落地方案/飞书链路对齐重构方案.md>) | 进行中 | 26,561 |

#### C 知识（wiki，21 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [@提及功能（mention）](<04-对话与消息/C-知识/@提及功能（mention）.md>) | 23,363 |
| 2 | [@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染](<04-对话与消息/C-知识/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染.md>) | 6,024 |
| 3 | [AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容](<04-对话与消息/C-知识/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容.md>) | 11,731 |
| 4 | [Agent消息集成](<04-对话与消息/C-知识/Agent消息集成.md>) | 18,905 |
| 5 | [ChatMessage::System 消息角色：人设/指令/规则 与 对话内容 分层传递给 Chat Completions API](<04-对话与消息/C-知识/ChatMessage::System 消息角色：人设·指令·规则 与 对话内容分层传递给 Chat Completions API/ChatMessage::System 消息角色：人设·指令·规则 与 对话内容分层传递给 Chat Completions API.md>) | 4,549 |
| 6 | [InboundState 入站运行状态：动态游标 + 会话滚动刷新 + fail-open 游标损坏恢复](<04-对话与消息/C-知识/InboundState 入站运行状态：动态游标 + 会话滚动刷新/InboundState 入站运行状态：动态游标 + 会话滚动刷新.md>) | 8,474 |
| 7 | [WebSocket 与 SSE](<04-对话与消息/C-知识/WebSocket 与 SSE.md>) | 15,246 |
| 8 | [多渠道消息系统](<04-对话与消息/C-知识/多渠道消息系统.md>) | 25,168 |
| 9 | [实时推送](<04-对话与消息/C-知识/实时推送.md>) | 18,046 |
| 10 | [微信 iLink 专属渠道](<04-对话与消息/C-知识/微信 iLink 专属渠道.md>) | 38,858 |
| 11 | [微信 iLink 专属渠道闭环（wechat_dal + ilink_dao + inbound_state + 授权流程）](<04-对话与消息/C-知识/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程.md>) | 23,805 |
| 12 | [消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类](<04-对话与消息/C-知识/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类.md>) | 34,147 |
| 13 | [消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级](<04-对话与消息/C-知识/消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级/消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级.md>) | 8,671 |
| 14 | [消息处理核心](<04-对话与消息/C-知识/消息处理核心.md>) | 21,357 |
| 15 | [消息渠道入站适配中台（MessageInboundAdapter + Registry + 生命周期）](<04-对话与消息/C-知识/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期.md>) | 36,420 |
| 16 | [消息渠道管理（业务功能层）](<04-对话与消息/C-知识/消息渠道管理.md>) | 32,040 |
| 17 | [消息渠道适配器](<04-对话与消息/C-知识/消息渠道适配器.md>) | 22,785 |
| 18 | [消息管理](<04-对话与消息/C-知识/消息管理.md>) | 19,205 |
| 19 | [消息系统（业务功能层）](<04-对话与消息/C-知识/消息系统.md>) | 22,263 |
| 20 | [消息领域](<04-对话与消息/C-知识/消息领域.md>) | 25,572 |
| 21 | [邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型](<04-对话与消息/C-知识/邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型/邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型.md>) | 11,894 |

### 05-任务与项目

#### A 设计（design，1 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [聊天侧栏「项目内 Agent 列表」与「工具调用身份展示」设计说明](<05-任务与项目/A-设计/chat_project_agents_and_tool_call_identity_design.md>) | 现行 | 11,788 |

#### C 知识（wiki，53 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息](<05-任务与项目/C-知识/A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息/A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息.md>) | 9,422 |
| 2 | [Agent 全生命周期管理](<05-任务与项目/C-知识/Agent 全生命周期管理.md>) | 24,467 |
| 3 | [Agent 创建与配置](<05-任务与项目/C-知识/Agent 创建与配置.md>) | 16,595 |
| 4 | [Agent 搜索与查询](<05-任务与项目/C-知识/Agent 搜索与查询.md>) | 13,604 |
| 5 | [Agent 状态管理](<05-任务与项目/C-知识/Agent 状态管理.md>) | 18,472 |
| 6 | [Agent 维度统计](<05-任务与项目/C-知识/Agent 维度统计.md>) | 16,471 |
| 7 | [AgentRuntimeInfo 状态机 + BusyGuard RAII：Idle/Busy/Resting 三态转换 + task_id/project_id 业务上下文 + 前端 runtime-list 过滤](<05-任务与项目/C-知识/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视.md>) | 17,639 |
| 8 | [HTTP 请求工具 (http_fetch)](<05-任务与项目/C-知识/HTTP 请求工具 (http_fetch).md>) | 19,283 |
| 9 | [ModelProvider 维度统计](<05-任务与项目/C-知识/ModelProvider 维度统计.md>) | 14,709 |
| 10 | [Project 维度统计](<05-任务与项目/C-知识/Project 维度统计.md>) | 15,977 |
| 11 | [Project模块处理器](<05-任务与项目/C-知识/Project模块处理器.md>) | 19,537 |
| 12 | [Task 维度统计](<05-任务与项目/C-知识/Task 维度统计.md>) | 16,293 |
| 13 | [任务协作与执行计划](<05-任务与项目/C-知识/任务协作与执行计划.md>) | 23,708 |
| 14 | [任务处理器](<05-任务与项目/C-知识/任务处理器.md>) | 20,929 |
| 15 | [任务状态机 + 项目聚合进度追踪：TaskStatus 4 态流转 + execution_plan_result 结构化存储 + TaskGraph](<05-任务与项目/C-知识/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG.md>) | 14,020 |
| 16 | [任务生命周期管理](<05-任务与项目/C-知识/任务生命周期管理.md>) | 16,444 |
| 17 | [任务管理 API 参考](<05-任务与项目/C-知识/任务管理 API 参考.md>) | 16,929 |
| 18 | [任务管理（业务功能层）](<05-任务与项目/C-知识/任务管理.md>) | 22,672 |
| 19 | [任务管理接口](<05-任务与项目/C-知识/任务管理接口.md>) | 17,029 |
| 20 | [多 Agent 协作机制](<05-任务与项目/C-知识/多 Agent 协作机制.md>) | 18,310 |
| 21 | [多维统计系统](<05-任务与项目/C-知识/多维统计系统.md>) | 26,850 |
| 22 | [定时任务管理](<05-任务与项目/C-知识/定时任务管理.md>) | 21,226 |
| 23 | [定时任务调度（业务功能层）](<05-任务与项目/C-知识/定时任务调度.md>) | 17,163 |
| 24 | [工件处理器](<05-任务与项目/C-知识/工件处理器.md>) | 16,841 |
| 25 | [工件管理](<05-任务与项目/C-知识/工件管理.md>) | 18,210 |
| 26 | [工件管理接口](<05-任务与项目/C-知识/工件管理接口.md>) | 18,573 |
| 27 | [工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡](<05-任务与项目/C-知识/工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡/工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡.md>) | 9,378 |
| 28 | [工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询](<05-任务与项目/C-知识/工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询/工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询.md>) | 4,277 |
| 29 | [工具安全控制](<05-任务与项目/C-知识/工具安全控制.md>) | 20,028 |
| 30 | [工具调用模式](<05-任务与项目/C-知识/工具调用模式.md>) | 17,574 |
| 31 | [快速开始指南](<05-任务与项目/C-知识/快速开始指南.md>) | 15,052 |
| 32 | [执行计划与结果追踪](<05-任务与项目/C-知识/执行计划与结果追踪.md>) | 19,162 |
| 33 | [数据备份与恢复](<05-任务与项目/C-知识/数据备份与恢复.md>) | 16,894 |
| 34 | [文件系统写入工具 (fs_write)](<05-任务与项目/C-知识/文件系统写入工具 (fs_write).md>) | 15,115 |
| 35 | [文件系统读取工具 (fs_read)](<05-任务与项目/C-知识/文件系统读取工具 (fs_read).md>) | 12,138 |
| 36 | [日志管理系统（入门概览层）](<05-任务与项目/C-知识/日志管理系统.md>) | 19,426 |
| 37 | [核心功能特性](<05-任务与项目/C-知识/核心功能特性.md>) | 32,102 |
| 38 | [知识关系管理](<05-任务与项目/C-知识/知识关系管理.md>) | 25,224 |
| 39 | [知识节点管理](<05-任务与项目/C-知识/知识节点管理.md>) | 16,327 |
| 40 | [系统健康检查](<05-任务与项目/C-知识/系统健康检查.md>) | 24,038 |
| 41 | [系统管理功能](<05-任务与项目/C-知识/系统管理功能.md>) | 16,576 |
| 42 | [综合搜索能力](<05-任务与项目/C-知识/综合搜索能力.md>) | 22,725 |
| 43 | [聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文](<05-任务与项目/C-知识/聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文/聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文.md>) | 5,957 |
| 44 | [自定义工具开发](<05-任务与项目/C-知识/自定义工具开发.md>) | 16,513 |
| 45 | [通过 common DTO 补全 Project/Task execution_plan/result 读取链路](<05-任务与项目/C-知识/通过 common DTO 补全 Project_Task execution_plan_result 读取链路/通过 common DTO 补全 Project_Task execution_plan_result 读取链路.md>) | 1,666 |
| 46 | [项目 CRUD 接口](<05-任务与项目/C-知识/项目 CRUD 接口.md>) | 17,557 |
| 47 | [项目 CRUD 操作](<05-任务与项目/C-知识/项目 CRUD 操作.md>) | 20,431 |
| 48 | [项目和任务模型](<05-任务与项目/C-知识/项目和任务模型.md>) | 20,898 |
| 49 | [项目处理器](<05-任务与项目/C-知识/项目处理器.md>) | 22,202 |
| 50 | [项目管理（业务功能层）](<05-任务与项目/C-知识/项目管理.md>) | 18,195 |
| 51 | [项目管理模块 API](<05-任务与项目/C-知识/项目管理模块 API.md>) | 16,270 |
| 52 | [项目领域](<05-任务与项目/C-知识/项目领域.md>) | 14,952 |
| 53 | [项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合](<05-任务与项目/C-知识/项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合/项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合.md>) | 8,993 |

### 06-工具与技能

#### A 设计（design，4 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [技能导入方式扩展与统一导入框架设计](<06-工具与技能/A-设计/skill_import_methods_design.md>) | 草案 | 21,644 |
| 2 | [共享工具凭据增强器设计（MCP / HTTP 工具接入身份凭证体系）](<06-工具与技能/A-设计/tool_credential_requirement_design.md>) | 现行 | 55,893 |
| 3 | [工具输出三分边界设计（运行时输出 / 最终结果 / 产物）](<06-工具与技能/A-设计/tool_output_boundary_design.md>) | 现行 | 13,930 |
| 4 | [网络搜索与浏览器工具设计](<06-工具与技能/A-设计/web_search_and_browser_tools_design.md>) | 现行 | 16,852 |

#### B 落地方案（plan，5 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [共享工具凭据增强器落地](<06-工具与技能/B-落地方案/共享工具凭据增强器落地.md>) | 已完成 | 29,499 |
| 2 | [技能列表显示修复与技能同步过期处理](<06-工具与技能/B-落地方案/技能列表显示修复与技能同步过期处理.md>) | 已完成 | 10,812 |
| 3 | [技能导入方式扩展与统一导入框架落地](<06-工具与技能/B-落地方案/技能导入方式扩展与统一导入框架落地.md>) | 草案 | 22,299 |
| 4 | [浏览器工具与环境就绪提示（阶段二：browser + tool_readiness）](<06-工具与技能/B-落地方案/浏览器工具与环境就绪提示.md>) | 进行中 | 11,883 |
| 5 | [网络搜索工具（阶段一：tavily_search）](<06-工具与技能/B-落地方案/网络搜索工具.md>) | 草案 | 9,880 |

#### C 知识（wiki，52 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配](<06-工具与技能/C-知识/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配.md>) | 12,454 |
| 2 | [DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链](<06-工具与技能/C-知识/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链.md>) | 15,425 |
| 3 | [GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页](<06-工具与技能/C-知识/GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页/GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页.md>) | 16,088 |
| 4 | [HTTP 客户端工具](<06-工具与技能/C-知识/HTTP 客户端工具.md>) | 22,239 |
| 5 | [HTTP 工具（框架层）](<06-工具与技能/C-知识/HTTP 工具.md>) | 16,520 |
| 6 | [HTTP 工具创建表单限定 method 白名单为 GET/POST](<06-工具与技能/C-知识/HTTP 工具创建表单限定 method 白名单为 GET_POST/HTTP 工具创建表单限定 method 白名单为 GET_POST.md>) | 1,283 |
| 7 | [HTTP请求工具](<06-工具与技能/C-知识/HTTP请求工具.md>) | 18,481 |
| 8 | [MCP 协议工具](<06-工具与技能/C-知识/MCP 协议工具.md>) | 17,721 |
| 9 | [MCP 工具集成（代码落地层）](<06-工具与技能/C-知识/MCP 工具集成.md>) | 15,643 |
| 10 | [MCP 服务器集成](<06-工具与技能/C-知识/MCP 服务器集成.md>) | 19,624 |
| 11 | [Shell 工具全链路：shell_exec 内置执行 + shell_policy 策略拦截 + shell_tool 声明式注册](<06-工具与技能/C-知识/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行.md>) | 16,988 |
| 12 | [Shell 执行工具 (shell_exec)](<06-工具与技能/C-知识/Shell 执行工具 (shell_exec).md>) | 17,514 |
| 13 | [Shell 执行工具（业务功能层）](<06-工具与技能/C-知识/Shell 执行工具.md>) | 28,491 |
| 14 | [Shell执行工具](<06-工具与技能/C-知识/Shell执行工具.md>) | 20,046 |
| 15 | [§1 概述（一句话定位 + 解决什么问题）](<06-工具与技能/C-知识/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断.md>) | 43,310 |
| 16 | [Tool 维度统计](<06-工具与技能/C-知识/Tool 维度统计.md>) | 17,234 |
| 17 | [pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）](<06-工具与技能/C-知识/pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）/pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）.md>) | 13,782 |
| 18 | [pkg 基建统一](<06-工具与技能/C-知识/pkg 基建统一.md>) | 11,374 |
| 19 | [sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）](<06-工具与技能/C-知识/sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）/sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）.md>) | 8,975 |
| 20 | [共享工具凭据增强器](<06-工具与技能/C-知识/共享工具凭据增强器.md>) | 37,112 |
| 21 | [共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例](<06-工具与技能/C-知识/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md>) | 15,540 |
| 22 | [内置工具](<06-工具与技能/C-知识/内置工具.md>) | 22,252 |
| 23 | [内置工具系统（框架层）](<06-工具与技能/C-知识/内置工具系统.md>) | 18,955 |
| 24 | [内置工具集](<06-工具与技能/C-知识/内置工具集.md>) | 22,100 |
| 25 | [Shell 执行工具（框架层）](<06-工具与技能/C-知识/基础设施/工具注册表/Shell 执行工具.md>) | 22,917 |
| 26 | [工具安全控制](<06-工具与技能/C-知识/工具安全控制.md>) | 22,766 |
| 27 | [工具执行引擎](<06-工具与技能/C-知识/工具执行引擎.md>) | 21,548 |
| 28 | [工具注册与发现](<06-工具与技能/C-知识/工具注册与发现.md>) | 19,755 |
| 29 | [工具注册表（框架层）](<06-工具与技能/C-知识/工具注册表.md>) | 21,793 |
| 30 | [工具生态系统](<06-工具与技能/C-知识/工具生态系统.md>) | 22,916 |
| 31 | [工具管理 API（业务功能层）](<06-工具与技能/C-知识/工具管理 API.md>) | 18,614 |
| 32 | [工具系统三层调用架构：CoreTool trait + Builtin/HTTP/MCP 三协议路由 + register_handler_tool 宏](<06-工具与技能/C-知识/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md>) | 21,955 |
| 33 | [工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复](<06-工具与技能/C-知识/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复.md>) | 7,352 |
| 34 | [工具输出与安全治理](<06-工具与技能/C-知识/工具输出与安全治理/工具输出与安全治理.md>) | 14,551 |
| 35 | [工具输出与安全治理](<06-工具与技能/C-知识/工具输出与安全治理.md>) | 31,400 |
| 36 | [技能与工具绑定](<06-工具与技能/C-知识/技能与工具绑定.md>) | 17,426 |
| 37 | [技能和工具系统](<06-工具与技能/C-知识/技能和工具系统.md>) | 18,352 |
| 38 | [技能系统](<06-工具与技能/C-知识/技能系统.md>) | 29,511 |
| 39 | [文件写入工具](<06-工具与技能/C-知识/文件写入工具.md>) | 17,708 |
| 40 | [文件系统工具](<06-工具与技能/C-知识/文件系统工具.md>) | 20,154 |
| 41 | [文件读取工具](<06-工具与技能/C-知识/文件读取工具.md>) | 18,732 |
| 42 | [核心架构与注册机制（框架层）](<06-工具与技能/C-知识/核心架构与注册机制.md>) | 19,722 |
| 43 | [HTTP 工具（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/HTTP 工具.md>) | 19,346 |
| 44 | [Shell 执行工具（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/Shell 执行工具.md>) | 20,900 |
| 45 | [内置工具系统（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/内置工具系统.md>) | 27,038 |
| 46 | [工具安全控制](<06-工具与技能/C-知识/核心模块/工具注册表/工具安全控制.md>) | 26,759 |
| 47 | [工具注册表（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/工具注册表.md>) | 23,566 |
| 48 | [核心架构与注册机制（代码落地层）](<06-工具与技能/C-知识/核心模块/工具注册表/核心架构与注册机制.md>) | 20,834 |
| 49 | [用户维度工作区路径改造 + 工具调用安全边界检查（路径逃逸 / 跨用户访问 / 相对路径阻断）](<06-工具与技能/C-知识/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断.md>) | 19,995 |
| 50 | [运行时诊断工具组](<06-工具与技能/C-知识/运行时诊断工具组.md>) | 23,474 |
| 51 | [进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤](<06-工具与技能/C-知识/进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤/进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤.md>) | 1,985 |
| 52 | [MCP 工具集成（入门概览层）](<06-工具与技能/C-知识/项目概述/核心功能特性/统一工具调用架构/MCP 工具集成.md>) | 16,420 |

### 07-外部Agent(A2A)

#### C 知识（wiki，20 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A Client 模式](<07-外部Agent(A2A)/C-知识/A2A Client 模式.md>) | 19,118 |
| 2 | [A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建](<07-外部Agent(A2A)/C-知识/A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建/A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建.md>) | 10,672 |
| 3 | [A2A Server 模式](<07-外部Agent(A2A)/C-知识/A2A Server 模式.md>) | 16,789 |
| 4 | [A2A 协议](<07-外部Agent(A2A)/C-知识/A2A 协议.md>) | 16,938 |
| 5 | [A2A 协议支持](<07-外部Agent(A2A)/C-知识/A2A 协议支持.md>) | 19,396 |
| 6 | [A2A 协议规范](<07-外部Agent(A2A)/C-知识/A2A 协议规范.md>) | 15,553 |
| 7 | [A2A 集成指南](<07-外部Agent(A2A)/C-知识/A2A 集成指南.md>) | 15,582 |
| 8 | [A2A协议处理器](<07-外部Agent(A2A)/C-知识/A2A协议处理器.md>) | 19,684 |
| 9 | [Agent 发现机制](<07-外部Agent(A2A)/C-知识/Agent 发现机制.md>) | 16,140 |
| 10 | [Agent卡片发现](<07-外部Agent(A2A)/C-知识/Agent卡片发现.md>) | 14,279 |
| 11 | [JSON-RPC 通信协议](<07-外部Agent(A2A)/C-知识/JSON-RPC 通信协议.md>) | 19,586 |
| 12 | [JSON-RPC核心](<07-外部Agent(A2A)/C-知识/JSON-RPC核心.md>) | 17,948 |
| 13 | [任务创建](<07-外部Agent(A2A)/C-知识/任务创建.md>) | 14,418 |
| 14 | [任务取消](<07-外部Agent(A2A)/C-知识/任务取消.md>) | 13,855 |
| 15 | [任务查询](<07-外部Agent(A2A)/C-知识/任务查询.md>) | 14,884 |
| 16 | [任务生命周期](<07-外部Agent(A2A)/C-知识/任务生命周期.md>) | 16,027 |
| 17 | [任务管理](<07-外部Agent(A2A)/C-知识/任务管理.md>) | 14,774 |
| 18 | [回调机制](<07-外部Agent(A2A)/C-知识/回调机制.md>) | 16,195 |
| 19 | [回调系统](<07-外部Agent(A2A)/C-知识/回调系统.md>) | 16,477 |
| 20 | [任务管理（代码落地层）](<07-外部Agent(A2A)/C-知识/核心模块/处理器层/A2A协议处理器/任务管理.md>) | 19,172 |

### 08-搜索

#### A 设计（design，1 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [本体论驱动知识沉淀设计（Ontology-Driven Knowledge Sedimentation）](<08-搜索/A-设计/ontology_knowledge_sedimentation_design.md>) | 现行 | 37,337 |

#### B 落地方案（plan，2 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [向量搜索 Pre-Filter 改造：Payload 落库与谓词下推 实施计划](<08-搜索/B-落地方案/向量搜索Pre-Filter改造.md>) | 进行中 | 79,265 |
| 2 | [本体论知识沉淀落地](<08-搜索/B-落地方案/本体论知识沉淀落地.md>) | 草案 | 25,670 |

#### C 知识（wiki，21 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕](<08-搜索/C-知识/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕.md>) | 48,445 |
| 2 | [Embedding Provider 生命周期与重建触发条件](<08-搜索/C-知识/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵.md>) | 23,915 |
| 3 | [Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T](<08-搜索/C-知识/Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分/Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分.md>) | 8,433 |
| 4 | [FTS5 关键词搜索](<08-搜索/C-知识/FTS5 关键词搜索.md>) | 18,064 |
| 5 | [PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序](<08-搜索/C-知识/PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序/PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序.md>) | 17,734 |
| 6 | [RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配](<08-搜索/C-知识/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配.md>) | 12,962 |
| 7 | [SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离](<08-搜索/C-知识/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离.md>) | 7,416 |
| 8 | [全文搜索（框架层）](<08-搜索/C-知识/全文搜索.md>) | 16,308 |
| 9 | [向量存储抽象 + Vectorizable trait 统一索引](<08-搜索/C-知识/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity.md>) | 31,037 |
| 10 | [向量语义搜索](<08-搜索/C-知识/向量语义搜索.md>) | 15,894 |
| 11 | [本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查](<08-搜索/C-知识/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查.md>) | 18,477 |
| 12 | [本体属性管理](<08-搜索/C-知识/本体属性管理.md>) | 32,881 |
| 13 | [本体词表与漂移治理](<08-搜索/C-知识/本体词表与漂移治理.md>) | 30,670 |
| 14 | [本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账](<08-搜索/C-知识/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账.md>) | 22,057 |
| 15 | [混合搜索引擎](<08-搜索/C-知识/混合搜索引擎.md>) | 16,452 |
| 16 | [知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出](<08-搜索/C-知识/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md>) | 15,781 |
| 17 | [知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头](<08-搜索/C-知识/知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头/知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头.md>) | 23,435 |
| 18 | [知识图谱可视化](<08-搜索/C-知识/知识图谱可视化.md>) | 45,355 |
| 19 | [知识图谱搜索](<08-搜索/C-知识/知识图谱搜索.md>) | 27,998 |
| 20 | [知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导](<08-搜索/C-知识/知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导/知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导.md>) | 16,944 |
| 21 | [记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 度数排序推荐](<08-搜索/C-知识/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md>) | 14,261 |

### 09-统计与监控

#### A 设计（design，1 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [工作台顶栏聚合指标设计](<09-统计与监控/A-设计/workspace_topbar_metrics_design.md>) | 现行 | 8,651 |

#### B 落地方案（plan，1 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [Responses API 接入与推理统计扩展 实施计划](<09-统计与监控/B-落地方案/Responses API 接入与推理统计扩展.md>) | 进行中 | 23,905 |

#### C 知识（wiki，8 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表](<09-统计与监控/C-知识/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md>) | 9,494 |
| 2 | [Lark P2P WS 私信入站（凭证引用解析 + 多渠道按 app_id 聚合 + 用户身份自动映射 + 健康指标）](<09-统计与监控/C-知识/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标.md>) | 14,140 |
| 3 | [健康检查处理器](<09-统计与监控/C-知识/健康检查处理器.md>) | 14,076 |
| 4 | [底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（LanceDB 默认生产 → InMemory/Hnsw 测试备选）](<09-统计与监控/C-知识/底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（LanceDB 默认生产 _ InMemory_Hnsw 测试备选）/底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（InMemory_Hnsw 默认 _ LanceDB 大规模）.md>) | 12,623 |
| 5 | [故障排除与监控](<09-统计与监控/C-知识/故障排除与监控.md>) | 17,777 |
| 6 | [系统监控与健康检查](<09-统计与监控/C-知识/系统监控与健康检查.md>) | 15,875 |
| 7 | [统计与监控](<09-统计与监控/C-知识/统计与监控.md>) | 25,456 |
| 8 | [统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示](<09-统计与监控/C-知识/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示.md>) | 20,286 |

### 10-异步基建(AOP)

#### A 设计（design，1 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [AOP 生产者-消费者契约重构设计](<10-异步基建(AOP)/A-设计/aop_producer_consumer_contract_design.md>) | 现行 | 83,726 |

#### C 知识（wiki，32 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 事件系统（系统管理层）](<10-异步基建(AOP)/C-知识/AOP 事件系统.md>) | 22,493 |
| 2 | [AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册](<10-异步基建(AOP)/C-知识/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md>) | 15,769 |
| 3 | [Agent 循环消费者（代码落地层）](<10-异步基建(AOP)/C-知识/Agent 循环消费者.md>) | 18,847 |
| 4 | [Cron 任务调度与系统启动顺序：5 字段 cron 解析 + ensure_system_cron_triggers 2 条注入 + CronTriggerProducer](<10-异步基建(AOP)/C-知识/Cron 任务调度与系统启动顺序：5字段cron解析 + ensure_system_cron_triggers 2条注入 + CronTriggerConsumer 事件 + 两阶段init/aop严格分离/Cron 任务调度与系统启动顺序：5字段cron解析 + ensure_system_cron_triggers 2条注入 + CronTriggerConsumer 事件 + 两阶段init/aop严格分离.md>) | 20,202 |
| 5 | [Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅](<10-异步基建(AOP)/C-知识/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅.md>) | 12,345 |
| 6 | [事件持久化](<10-异步基建(AOP)/C-知识/事件持久化.md>) | 17,547 |
| 7 | [事件消费者](<10-异步基建(AOP)/C-知识/事件消费者.md>) | 19,188 |
| 8 | [事件生产者](<10-异步基建(AOP)/C-知识/事件生产者.md>) | 22,483 |
| 9 | [事件系统核心](<10-异步基建(AOP)/C-知识/事件系统核心.md>) | 19,652 |
| 10 | [事件调度器](<10-异步基建(AOP)/C-知识/事件调度器.md>) | 20,162 |
| 11 | [事件队列管理](<10-异步基建(AOP)/C-知识/事件队列管理.md>) | 20,149 |
| 12 | [事件队列系统](<10-异步基建(AOP)/C-知识/事件队列系统.md>) | 19,796 |
| 13 | [任务事件消费者（框架层）](<10-异步基建(AOP)/C-知识/任务事件消费者.md>) | 14,359 |
| 14 | [唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发](<10-异步基建(AOP)/C-知识/唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发/唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发.md>) | 10,184 |
| 15 | [AOP 事件系统（框架层）](<10-异步基建(AOP)/C-知识/基础设施/AOP 事件系统/AOP 事件系统.md>) | 22,947 |
| 16 | [定时任务和事件系统](<10-异步基建(AOP)/C-知识/定时任务和事件系统.md>) | 22,628 |
| 17 | [定时任务消费者](<10-异步基建(AOP)/C-知识/定时任务消费者.md>) | 17,084 |
| 18 | [定时触发生产者](<10-异步基建(AOP)/C-知识/定时触发生产者.md>) | 21,051 |
| 19 | [工具执行消费者](<10-异步基建(AOP)/C-知识/工具执行消费者.md>) | 18,629 |
| 20 | [思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路](<10-异步基建(AOP)/C-知识/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路.md>) | 7,340 |
| 21 | [核心概念与架构](<10-异步基建(AOP)/C-知识/核心概念与架构.md>) | 23,921 |
| 22 | [AOP 事件系统（代码落地层）](<10-异步基建(AOP)/C-知识/核心模块/AOP 事件系统/AOP 事件系统.md>) | 22,968 |
| 23 | [任务事件消费者（代码落地层）](<10-异步基建(AOP)/C-知识/核心模块/AOP 事件系统/消费者框架/任务事件消费者.md>) | 22,665 |
| 24 | [注册中心与调度器](<10-异步基建(AOP)/C-知识/注册中心与调度器.md>) | 21,035 |
| 25 | [消息消费者（代码落地层）](<10-异步基建(AOP)/C-知识/消息消费者.md>) | 24,489 |
| 26 | [消费者接口规范](<10-异步基建(AOP)/C-知识/消费者接口规范.md>) | 22,954 |
| 27 | [消费者框架](<10-异步基建(AOP)/C-知识/消费者框架.md>) | 18,485 |
| 28 | [生产者机制](<10-异步基建(AOP)/C-知识/生产者机制.md>) | 23,162 |
| 29 | [生产者消费者模式](<10-异步基建(AOP)/C-知识/生产者消费者模式.md>) | 23,783 |
| 30 | [监控与统计集成](<10-异步基建(AOP)/C-知识/监控与统计集成.md>) | 21,666 |
| 31 | [调度器消费者](<10-异步基建(AOP)/C-知识/调度器消费者.md>) | 18,154 |
| 32 | [队列实现](<10-异步基建(AOP)/C-知识/队列实现.md>) | 19,962 |

### 11-前端应用

#### A 设计（design，2 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [前端架构设计](<11-前端应用/A-设计/frontend_architecture.md>) | 现行 | 31,147 |
| 2 | [AI Orz HUD 设计系统](<11-前端应用/A-设计/ui_design_system.md>) | 现行 | 15,361 |

#### C 知识（wiki，49 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 监控面板](<11-前端应用/C-知识/AOP 监控面板.md>) | 19,675 |
| 2 | [API 客户端](<11-前端应用/C-知识/API 客户端.md>) | 27,449 |
| 3 | [Agent 管理功能](<11-前端应用/C-知识/Agent 管理功能.md>) | 26,969 |
| 4 | [Finance 管理页面](<11-前端应用/C-知识/Finance 管理页面.md>) | 21,204 |
| 5 | [HR 管理页面](<11-前端应用/C-知识/HR 管理页面.md>) | 16,133 |
| 6 | [HTTP工具创建界面](<11-前端应用/C-知识/HTTP工具创建界面.md>) | 17,852 |
| 7 | [MCP 服务器管理](<11-前端应用/C-知识/MCP 服务器管理.md>) | 19,137 |
| 8 | [Markdown渲染组件](<11-前端应用/C-知识/Markdown渲染组件.md>) | 20,753 |
| 9 | [Mermaid图表支持](<11-前端应用/C-知识/Mermaid图表支持.md>) | 16,179 |
| 10 | [Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线](<11-前端应用/C-知识/Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线/Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线.md>) | 16,397 |
| 11 | [Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格](<11-前端应用/C-知识/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格.md>) | 13,629 |
| 12 | [UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束](<11-前端应用/C-知识/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束.md>) | 23,129 |
| 13 | [UI 样式与主题](<11-前端应用/C-知识/UI 样式与主题.md>) | 31,958 |
| 14 | [recommend_seed_nodes 种子节点推荐：度数排序（入边+出边）Top N + published 度数持平决胜 + KnowledgeGraph 组件两端复用 + Agent 神经工具免绑定](<11-前端应用/C-知识/recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用/recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用.md>) | 17,103 |
| 15 | [业务组件](<11-前端应用/C-知识/业务组件.md>) | 24,297 |
| 16 | [任务管理功能](<11-前端应用/C-知识/任务管理功能.md>) | 23,183 |
| 17 | [前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发](<11-前端应用/C-知识/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发.md>) | 14,035 |
| 18 | [前端应用](<11-前端应用/C-知识/前端应用.md>) | 46,247 |
| 19 | [前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层](<11-前端应用/C-知识/前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层/前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层.md>) | 13,463 |
| 20 | [图表组件](<11-前端应用/C-知识/图表组件.md>) | 21,650 |
| 21 | [基础组件](<11-前端应用/C-知识/基础组件.md>) | 19,425 |
| 22 | [备份恢复管理（前端 UI 层）](<11-前端应用/C-知识/备份恢复管理.md>) | 14,975 |
| 23 | [定时任务调度（前端 UI 层）](<11-前端应用/C-知识/定时任务调度.md>) | 19,066 |
| 24 | [工件管理功能](<11-前端应用/C-知识/工件管理功能.md>) | 19,982 |
| 25 | [工具管理](<11-前端应用/C-知识/工具管理.md>) | 19,650 |
| 26 | [工具调用标签页](<11-前端应用/C-知识/工具调用标签页.md>) | 19,528 |
| 27 | [布局组件](<11-前端应用/C-知识/布局组件.md>) | 18,237 |
| 28 | [思考运行时面板观测接口](<11-前端应用/C-知识/思考运行时面板观测接口.md>) | 25,100 |
| 29 | [技能管理系统](<11-前端应用/C-知识/技能管理系统.md>) | 25,708 |
| 30 | [文档中心](<11-前端应用/C-知识/文档中心.md>) | 12,629 |
| 31 | [日志管理系统（前端 UI 层）](<11-前端应用/C-知识/日志管理系统.md>) | 17,970 |
| 32 | [模型提供商管理（前端 UI 层）](<11-前端应用/C-知识/模型提供商管理.md>) | 27,191 |
| 33 | [消息与工作区页面](<11-前端应用/C-知识/消息与工作区页面.md>) | 26,831 |
| 34 | [消息渠道管理（前端 UI 层）](<11-前端应用/C-知识/消息渠道管理.md>) | 16,884 |
| 35 | [用户与组织页面](<11-前端应用/C-知识/用户与组织页面.md>) | 18,217 |
| 36 | [种子数据管理（前端 UI 层）](<11-前端应用/C-知识/种子数据管理.md>) | 17,709 |
| 37 | [系统健康检查](<11-前端应用/C-知识/系统健康检查.md>) | 16,170 |
| 38 | [系统管理页面](<11-前端应用/C-知识/系统管理页面.md>) | 18,918 |
| 39 | [组件系统](<11-前端应用/C-知识/组件系统.md>) | 29,494 |
| 40 | [统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段](<11-前端应用/C-知识/统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段/统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段.md>) | 4,200 |
| 41 | [聊天侧面板](<11-前端应用/C-知识/聊天侧面板.md>) | 20,516 |
| 42 | [记忆搜索功能](<11-前端应用/C-知识/记忆搜索功能.md>) | 17,718 |
| 43 | [路由和中间件](<11-前端应用/C-知识/路由和中间件.md>) | 19,697 |
| 44 | [进程详情采用共享组件 + Modal 弹窗，不新建独立路由页](<11-前端应用/C-知识/进程详情采用共享组件 + Modal 弹窗，不新建独立路由页/进程详情采用共享组件 + Modal 弹窗，不新建独立路由页.md>) | 1,676 |
| 45 | [钩子系统](<11-前端应用/C-知识/钩子系统.md>) | 16,526 |
| 46 | [附件管理](<11-前端应用/C-知识/附件管理.md>) | 17,052 |
| 47 | [页面模块](<11-前端应用/C-知识/页面模块.md>) | 23,095 |
| 48 | [项目管理功能](<11-前端应用/C-知识/项目管理功能.md>) | 21,454 |
| 49 | [项目管理页面](<11-前端应用/C-知识/项目管理页面.md>) | 24,363 |

### 12-基础设施与部署

#### A 设计（design，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [日志系统设计文档](<12-基础设施与部署/A-设计/logging_design.md>) | 现行 | 14,363 |
| 2 | [模型访问模式（access_mode）UI 设计规范](<12-基础设施与部署/A-设计/model_provider_access_mode_design.md>) | 现行 | 9,281 |
| 3 | [SQLx 0.8 + SQLite 开发规范与最佳实践](<12-基础设施与部署/A-设计/sqlx_guide.md>) | 现行 | 15,095 |

#### B 落地方案（plan，2 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [容器化与网关部署方案](<12-基础设施与部署/B-落地方案/容器化与网关部署方案.md>) | 已完成 | 23,403 |
| 2 | [系统初始化模型配置策略调整](<12-基础设施与部署/B-落地方案/系统初始化模型配置策略调整.md>) | 已完成 | 10,808 |

#### C 知识（wiki，79 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A 轮询生产者](<12-基础设施与部署/C-知识/A2A 轮询生产者.md>) | 15,544 |
| 2 | [AOP 核心架构](<12-基础设施与部署/C-知识/AOP 核心架构.md>) | 23,455 |
| 3 | [AOP监控处理器](<12-基础设施与部署/C-知识/AOP监控处理器.md>) | 17,536 |
| 4 | [Agent 循环消费者（框架层）](<12-基础设施与部署/C-知识/Agent 循环消费者.md>) | 14,369 |
| 5 | [Doctor 依赖预检](<12-基础设施与部署/C-知识/Doctor 依赖预检/Doctor 依赖预检.md>) | 5,899 |
| 6 | [Doctor 依赖预检](<12-基础设施与部署/C-知识/Doctor 依赖预检.md>) | 5,419 |
| 7 | [Finance模块处理器](<12-基础设施与部署/C-知识/Finance模块处理器.md>) | 34,066 |
| 8 | [HNSW 索引存储后端](<12-基础设施与部署/C-知识/HNSW 索引存储后端.md>) | 15,771 |
| 9 | [LanceDB 存储后端](<12-基础设施与部署/C-知识/LanceDB 存储后端.md>) | 17,207 |
| 10 | [MCP 服务器管理 API](<12-基础设施与部署/C-知识/MCP 服务器管理 API.md>) | 15,775 |
| 11 | [MCP服务器管理处理器](<12-基础设施与部署/C-知识/MCP服务器管理处理器.md>) | 16,064 |
| 12 | [Rust Workspace + NPM 多语言依赖管理（Cargo.lock / package-lock.json / SQLx 离线构建）](<12-基础设施与部署/C-知识/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）.md>) | 9,616 |
| 13 | [SQLite VSS 存储后端](<12-基础设施与部署/C-知识/SQLite VSS 存储后端.md>) | 15,198 |
| 14 | [System模块处理器](<12-基础设施与部署/C-知识/System模块处理器.md>) | 26,161 |
| 15 | [AI Orz 文档编写与精简归档 Skill（规范全文版）](<12-基础设施与部署/C-知识/ai-orz-doc-maintainer.md>) | 29,966 |
| 16 | [AI Orz Wiki 维护 Skill（规范全文版）](<12-基础设施与部署/C-知识/ai-orz-wiki-maintainer.md>) | 26,949 |
| 17 | [pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语](<12-基础设施与部署/C-知识/pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语/pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语.md>) | 15,359 |
| 18 | [scripts/ 归置 + Makefile 统一入口 + 60+ 引用全量修正](<12-基础设施与部署/C-知识/scripts 归置 + Makefile 统一入口 + 60+ 引用全量修正/scripts 归置 + Makefile 统一入口 + 60+ 引用全量修正.md>) | 17,308 |
| 19 | [任务管理 API](<12-基础设施与部署/C-知识/任务管理 API.md>) | 16,158 |
| 20 | [任务管理处理器](<12-基础设施与部署/C-知识/任务管理处理器.md>) | 18,278 |
| 21 | [优雅关闭系统](<12-基础设施与部署/C-知识/优雅关闭系统.md>) | 11,944 |
| 22 | [内存向量存储后端](<12-基础设施与部署/C-知识/内存向量存储后端.md>) | 15,732 |
| 23 | [后台任务系统](<12-基础设施与部署/C-知识/后台任务系统.md>) | 20,726 |
| 24 | [后台进程管理系统](<12-基础设施与部署/C-知识/后台进程管理系统.md>) | 24,550 |
| 25 | [向量存储（框架层）](<12-基础设施与部署/C-知识/向量存储.md>) | 19,437 |
| 26 | [向量存储抽象接口（框架层）](<12-基础设施与部署/C-知识/向量存储抽象接口.md>) | 19,568 |
| 27 | [基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）](<12-基础设施与部署/C-知识/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）.md>) | 10,599 |
| 28 | [基础设施](<12-基础设施与部署/C-知识/基础设施.md>) | 18,627 |
| 29 | [备份恢复 API](<12-基础设施与部署/C-知识/备份恢复 API.md>) | 13,589 |
| 30 | [备份恢复管理（业务功能层）](<12-基础设施与部署/C-知识/备份恢复管理.md>) | 14,834 |
| 31 | [备份管理处理器](<12-基础设施与部署/C-知识/备份管理处理器.md>) | 14,226 |
| 32 | [存储系统（框架层）](<12-基础设施与部署/C-知识/存储系统.md>) | 22,490 |
| 33 | [存储配置](<12-基础设施与部署/C-知识/存储配置.md>) | 14,606 |
| 34 | [定时任务 API](<12-基础设施与部署/C-知识/定时任务 API.md>) | 22,093 |
| 35 | [定时任务处理器](<12-基础设施与部署/C-知识/定时任务处理器.md>) | 21,186 |
| 36 | [容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像](<12-基础设施与部署/C-知识/容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像/容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像.md>) | 9,573 |
| 37 | [工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆](<12-基础设施与部署/C-知识/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆.md>) | 9,509 |
| 38 | [工具执行日志消费者](<12-基础设施与部署/C-知识/工具执行日志消费者.md>) | 17,700 |
| 39 | [工具执行统计消费者](<12-基础设施与部署/C-知识/工具执行统计消费者.md>) | 13,444 |
| 40 | [工具管理 API（API 参考层）](<12-基础设施与部署/C-知识/工具管理 API.md>) | 19,986 |
| 41 | [工具管理处理器](<12-基础设施与部署/C-知识/工具管理处理器.md>) | 20,050 |
| 42 | [思考轮次统计消费者](<12-基础设施与部署/C-知识/思考轮次统计消费者.md>) | 17,852 |
| 43 | [持续集成与发布工作流](<12-基础设施与部署/C-知识/持续集成与发布工作流.md>) | 29,839 |
| 44 | [数据库迁移管理](<12-基础设施与部署/C-知识/数据库迁移管理/数据库迁移管理.md>) | 6,019 |
| 45 | [文档链接质量门禁](<12-基础设施与部署/C-知识/文档链接质量门禁.md>) | 24,115 |
| 46 | [日志查询 API](<12-基础设施与部署/C-知识/日志查询 API.md>) | 16,029 |
| 47 | [日志查询处理器](<12-基础设施与部署/C-知识/日志查询处理器.md>) | 16,914 |
| 48 | [日志管理系统（系统管理层）](<12-基础设施与部署/C-知识/日志管理系统.md>) | 19,200 |
| 49 | [日志系统](<12-基础设施与部署/C-知识/日志系统.md>) | 20,393 |
| 50 | [时间戳规范](<12-基础设施与部署/C-知识/时间戳规范/时间戳规范.md>) | 5,185 |
| 51 | [服务层](<12-基础设施与部署/C-知识/服务层.md>) | 18,070 |
| 52 | [本地文件安全与工作区隔离](<12-基础设施与部署/C-知识/本地文件安全与工作区隔离.md>) | 23,540 |
| 53 | [模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片](<12-基础设施与部署/C-知识/模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片/模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片.md>) | 7,888 |
| 54 | [模型提供商管理 API](<12-基础设施与部署/C-知识/模型提供商管理 API.md>) | 23,711 |
| 55 | [模型提供商管理（业务功能层）](<12-基础设施与部署/C-知识/模型提供商管理.md>) | 23,971 |
| 56 | [模型提供商管理处理器](<12-基础设施与部署/C-知识/模型提供商管理处理器.md>) | 21,479 |
| 57 | [模型提供商配置](<12-基础设施与部署/C-知识/模型提供商配置.md>) | 15,010 |
| 58 | [模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析](<12-基础设施与部署/C-知识/模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析/模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析.md>) | 11,455 |
| 59 | [消息处理 API](<12-基础设施与部署/C-知识/消息处理 API.md>) | 20,198 |
| 60 | [消息消费者（框架层）](<12-基础设施与部署/C-知识/消息消费者.md>) | 18,322 |
| 61 | [消息渠道管理 API](<12-基础设施与部署/C-知识/消息渠道管理 API.md>) | 20,338 |
| 62 | [消息渠道管理处理器](<12-基础设施与部署/C-知识/消息渠道管理处理器.md>) | 17,743 |
| 63 | [消息系统处理器](<12-基础设施与部署/C-知识/消息系统处理器.md>) | 19,774 |
| 64 | [消息通道生产者](<12-基础设施与部署/C-知识/消息通道生产者.md>) | 15,562 |
| 65 | [监控指标 API](<12-基础设施与部署/C-知识/监控指标 API.md>) | 18,506 |
| 66 | [种子数据处理器](<12-基础设施与部署/C-知识/种子数据处理器.md>) | 20,726 |
| 67 | [种子数据管理 API](<12-基础设施与部署/C-知识/种子数据管理 API.md>) | 18,856 |
| 68 | [种子数据管理（业务功能层）](<12-基础设施与部署/C-知识/种子数据管理.md>) | 32,555 |
| 69 | [种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离](<12-基础设施与部署/C-知识/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发.md>) | 30,345 |
| 70 | [系统初始化](<12-基础设施与部署/C-知识/系统初始化.md>) | 25,657 |
| 71 | [系统管理](<12-基础设施与部署/C-知识/系统管理.md>) | 20,151 |
| 72 | [系统管理模块 API](<12-基础设施与部署/C-知识/系统管理模块 API.md>) | 19,066 |
| 73 | [脱敏引擎下沉 common](<12-基础设施与部署/C-知识/脱敏引擎下沉 common.md>) | 25,444 |
| 74 | [脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏](<12-基础设施与部署/C-知识/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏.md>) | 5,884 |
| 75 | [财务管理模块 API](<12-基础设施与部署/C-知识/财务管理模块 API.md>) | 23,365 |
| 76 | [配置与部署](<12-基础设施与部署/C-知识/配置与部署.md>) | 32,105 |
| 77 | [配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入](<12-基础设施与部署/C-知识/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入.md>) | 14,871 |
| 78 | [附件管理 API](<12-基础设施与部署/C-知识/附件管理 API.md>) | 18,021 |
| 79 | [附件管理处理器](<12-基础设施与部署/C-知识/附件管理处理器.md>) | 17,390 |

### 13-数据模型

#### A 设计（design，3 篇）

| # | 页面 | 状态 | 字节 |
|---:|---|---|---:|
| 1 | [前后端 API 协议规范（common DTO 单一事实源）](<13-数据模型/A-设计/api_protocol_convention.md>) | 现行 | 6,432 |
| 2 | [查询接口分页与通用 count 规范](<13-数据模型/A-设计/pagination_and_count_convention.md>) | 现行 | 16,759 |
| 3 | [时间戳处理约定](<13-数据模型/A-设计/timestamp_convention.md>) | 现行 | 7,086 |

#### C 知识（wiki，39 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [API 参考](<13-数据模型/C-知识/API 参考.md>) | 19,264 |
| 2 | [Agent 和技能模型](<13-数据模型/C-知识/Agent 和技能模型.md>) | 18,999 |
| 3 | [Agent 实体](<13-数据模型/C-知识/Agent 实体.md>) | 16,167 |
| 4 | [DAO 层多态](<13-数据模型/C-知识/DAO 层多态.md>) | 16,330 |
| 5 | [HNSW向量存储](<13-数据模型/C-知识/HNSW向量存储.md>) | 15,553 |
| 6 | [Handler 宏元数据契约 + ToolPo config/parameters_schema 字段分离](<13-数据模型/C-知识/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位.md>) | 15,162 |
| 7 | [LanceDB向量存储](<13-数据模型/C-知识/LanceDB向量存储.md>) | 15,130 |
| 8 | [Mermaid 图表采用 vendor mermaid.js 的独立阶段实现](<13-数据模型/C-知识/Mermaid 图表采用 vendor mermaid.js 的独立阶段实现/Mermaid 图表采用 vendor mermaid.js 的独立阶段实现.md>) | 1,717 |
| 9 | [SQLite VSS向量存储](<13-数据模型/C-知识/SQLite VSS向量存储.md>) | 17,052 |
| 10 | [SQLite 数据访问对象](<13-数据模型/C-知识/SQLite 数据访问对象.md>) | 17,337 |
| 11 | [三位一体混合搜索（FTS5 + 向量 + 合并排序）](<13-数据模型/C-知识/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）.md>) | 14,569 |
| 12 | [全文搜索（代码落地层）](<13-数据模型/C-知识/全文搜索.md>) | 17,078 |
| 13 | [内存向量存储](<13-数据模型/C-知识/内存向量存储.md>) | 15,254 |
| 14 | [制品和附件](<13-数据模型/C-知识/制品和附件.md>) | 22,312 |
| 15 | [向量存储（代码落地层）](<13-数据模型/C-知识/向量存储.md>) | 17,975 |
| 16 | [向量存储抽象接口（代码落地层）](<13-数据模型/C-知识/向量存储抽象接口.md>) | 17,195 |
| 17 | [向量存储数据访问对象](<13-数据模型/C-知识/向量存储数据访问对象.md>) | 19,926 |
| 18 | [外部 API 数据访问对象](<13-数据模型/C-知识/外部 API 数据访问对象.md>) | 19,059 |
| 19 | [存储架构设计](<13-数据模型/C-知识/存储架构设计.md>) | 18,661 |
| 20 | [存储系统（代码落地层）](<13-数据模型/C-知识/存储系统.md>) | 21,790 |
| 21 | [数据对象层（DAO）](<13-数据模型/C-知识/数据对象层 (DAO).md>) | 19,645 |
| 22 | [数据模型](<13-数据模型/C-知识/数据模型.md>) | 28,067 |
| 23 | [数据访问层 (DAL)](<13-数据模型/C-知识/数据访问层 (DAL).md>) | 12,772 |
| 24 | [文件存储模型](<13-数据模型/C-知识/文件存储模型.md>) | 18,327 |
| 25 | [核心实体模型](<13-数据模型/C-知识/核心实体模型.md>) | 16,414 |
| 26 | [消息和记忆模型](<13-数据模型/C-知识/消息和记忆模型.md>) | 22,582 |
| 27 | [消息系统（数据模型层）](<13-数据模型/C-知识/消息系统.md>) | 22,483 |
| 28 | [消息系统数据访问对象](<13-数据模型/C-知识/消息系统数据访问对象.md>) | 15,260 |
| 29 | [系统模型](<13-数据模型/C-知识/系统模型.md>) | 19,796 |
| 30 | [统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系](<13-数据模型/C-知识/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系.md>) | 10,819 |
| 31 | [统计数据分析对象](<13-数据模型/C-知识/统计数据分析对象.md>) | 15,593 |
| 32 | [记忆和向量系统](<13-数据模型/C-知识/记忆和向量系统.md>) | 30,191 |
| 33 | [请求上下文](<13-数据模型/C-知识/请求上下文.md>) | 20,302 |
| 34 | [财务领域](<13-数据模型/C-知识/财务领域.md>) | 19,564 |
| 35 | [身份凭证管理（统一 Domain CRUD + AES-256-GCM 加密存储 + 生命周期联动）](<13-数据模型/C-知识/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md>) | 30,612 |
| 36 | [错误处理与状态码](<13-数据模型/C-知识/错误处理与状态码.md>) | 18,820 |
| 37 | [附件存储与DTO协议统一：Attachment Finance域统一资产 + PagedResult T map 全链路 + common::api 单一事实源 + count与query 复用 WHERE](<13-数据模型/C-知识/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE.md>) | 16,338 |
| 38 | [项目管理（数据模型层）](<13-数据模型/C-知识/项目管理.md>) | 21,355 |
| 39 | [飞书集成系统](<13-数据模型/C-知识/飞书集成系统.md>) | 24,125 |

### 14-测试

#### C 知识（wiki，4 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖](<14-测试/C-知识/Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖/Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖.md>) | 8,621 |
| 2 | [测试与质量工程：974测试100%通过 + 984后端82前端58common + 87集成测试19targets + cargo-llvm-cov PR38%/main45%门槛 + clippy -D warnings零容忍 + Playwright E2E](<14-测试/C-知识/测试与质量工程：1124测试100%通过 + 984后端82前端 + 87集成测试19targets + cargo-llvm-cov 38%/45%门槛 + clippy零容忍+Playwright E2E/测试与质量工程：1124测试100%通过 + 984后端82前端 + 87集成测试19targets + cargo-llvm-cov 38%/45%门槛 + clippy零容忍+Playwright E2E.md>) | 23,568 |
| 3 | [测试指南](<14-测试/C-知识/测试指南.md>) | 31,423 |
| 4 | [端到端测试基础设施](<14-测试/C-知识/端到端测试基础设施.md>) | 13,166 |
