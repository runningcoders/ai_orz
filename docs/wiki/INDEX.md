<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# Wiki 索引

> **自动生成**：生成器 `tools/wiki_index.py` · 门禁 `make wiki-lint` · 重建 `make wiki-index`
>
> 覆盖 `docs/wiki/zh/content/`（代码 Wiki 长文）与 `docs/wiki/knowledge/zh/`（RAG 原子知识卡）。
> 语义入口与维护办法见 [README.md](README.md)；四象限定位见 [../README.md](../README.md)。

**合计 497 篇 / 9,116,841 B** —— 长文 377 篇 / 7,632,340 B，知识卡 120 张 / 1,484,501 B。

## 分类概览

| 分类 | 篇数 | 字节 | 章节 |
|---|---:|---:|---|
| 核心模块 | 89 | 1,775,413 | [跳转](#核心模块) |
| 项目概述 | 62 | 1,202,238 | [跳转](#项目概述) |
| 功能模块 | 50 | 1,145,091 | [跳转](#功能模块) |
| 基础设施 | 50 | 959,727 | [跳转](#基础设施) |
| 前端应用 | 42 | 943,799 | [跳转](#前端应用) |
| API 参考 | 39 | 721,474 | [跳转](#API-参考) |
| 架构设计 | 25 | 458,884 | [跳转](#架构设计) |
| 数据模型 | 15 | 313,970 | [跳转](#数据模型) |
| 总纲与运维 | 3 | 66,778 | [跳转](#总纲与运维) |
| 测试指南 | 2 | 44,966 | [跳转](#测试指南) |
| RAG 原子知识卡 | 120 | 1,484,501 | [跳转](#rag-原子知识卡) |

## 代码 Wiki 长文

### 核心模块（89 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 事件系统（代码落地层）](<zh/content/核心模块/AOP 事件系统/AOP 事件系统.md>) | 23,038 |
| 2 | [事件持久化](<zh/content/核心模块/AOP 事件系统/事件队列系统/事件持久化.md>) | 17,547 |
| 3 | [事件调度器](<zh/content/核心模块/AOP 事件系统/事件队列系统/事件调度器.md>) | 20,162 |
| 4 | [事件队列系统](<zh/content/核心模块/AOP 事件系统/事件队列系统/事件队列系统.md>) | 19,796 |
| 5 | [队列实现](<zh/content/核心模块/AOP 事件系统/事件队列系统/队列实现.md>) | 19,962 |
| 6 | [核心概念与架构](<zh/content/核心模块/AOP 事件系统/核心概念与架构.md>) | 24,313 |
| 7 | [Agent 循环消费者（代码落地层）](<zh/content/核心模块/AOP 事件系统/消费者框架/Agent 循环消费者.md>) | 18,866 |
| 8 | [任务事件消费者（代码落地层）](<zh/content/核心模块/AOP 事件系统/消费者框架/任务事件消费者.md>) | 22,757 |
| 9 | [定时任务消费者](<zh/content/核心模块/AOP 事件系统/消费者框架/定时任务消费者.md>) | 17,063 |
| 10 | [工具执行消费者](<zh/content/核心模块/AOP 事件系统/消费者框架/工具执行消费者.md>) | 18,628 |
| 11 | [消息消费者（代码落地层）](<zh/content/核心模块/AOP 事件系统/消费者框架/消息消费者.md>) | 24,508 |
| 12 | [消费者接口规范](<zh/content/核心模块/AOP 事件系统/消费者框架/消费者接口规范.md>) | 22,954 |
| 13 | [消费者框架](<zh/content/核心模块/AOP 事件系统/消费者框架/消费者框架.md>) | 18,485 |
| 14 | [生产者机制](<zh/content/核心模块/AOP 事件系统/生产者机制.md>) | 23,162 |
| 15 | [监控与统计集成](<zh/content/核心模块/AOP 事件系统/监控与统计集成.md>) | 21,665 |
| 16 | [A2A协议处理器](<zh/content/核心模块/处理器层/A2A协议处理器/A2A协议处理器.md>) | 19,683 |
| 17 | [Agent卡片发现](<zh/content/核心模块/处理器层/A2A协议处理器/Agent卡片发现.md>) | 14,241 |
| 18 | [JSON-RPC核心](<zh/content/核心模块/处理器层/A2A协议处理器/JSON-RPC核心.md>) | 17,947 |
| 19 | [任务管理（代码落地层）](<zh/content/核心模块/处理器层/A2A协议处理器/任务管理.md>) | 19,177 |
| 20 | [回调系统](<zh/content/核心模块/处理器层/A2A协议处理器/回调系统.md>) | 16,476 |
| 21 | [Finance模块处理器](<zh/content/核心模块/处理器层/Finance模块处理器/Finance模块处理器.md>) | 34,028 |
| 22 | [MCP服务器管理处理器](<zh/content/核心模块/处理器层/Finance模块处理器/MCP服务器管理处理器.md>) | 16,063 |
| 23 | [工具管理处理器](<zh/content/核心模块/处理器层/Finance模块处理器/工具管理处理器.md>) | 20,049 |
| 24 | [模型提供商管理处理器](<zh/content/核心模块/处理器层/Finance模块处理器/模型提供商管理处理器.md>) | 21,441 |
| 25 | [消息渠道管理处理器](<zh/content/核心模块/处理器层/Finance模块处理器/消息渠道管理处理器.md>) | 17,742 |
| 26 | [消息系统处理器](<zh/content/核心模块/处理器层/Finance模块处理器/消息系统处理器.md>) | 19,773 |
| 27 | [附件管理处理器](<zh/content/核心模块/处理器层/Finance模块处理器/附件管理处理器.md>) | 17,389 |
| 28 | [Agent处理器](<zh/content/核心模块/处理器层/HR模块处理器/Agent处理器.md>) | 24,436 |
| 29 | [HR模块处理器](<zh/content/核心模块/处理器层/HR模块处理器/HR模块处理器.md>) | 19,320 |
| 30 | [Skill处理器](<zh/content/核心模块/处理器层/HR模块处理器/Skill处理器.md>) | 21,833 |
| 31 | [Organization模块处理器](<zh/content/核心模块/处理器层/Organization模块处理器/Organization模块处理器.md>) | 19,638 |
| 32 | [当前组织信息处理器](<zh/content/核心模块/处理器层/Organization模块处理器/当前组织信息处理器.md>) | 19,656 |
| 33 | [用户管理处理器](<zh/content/核心模块/处理器层/Organization模块处理器/用户管理处理器.md>) | 21,027 |
| 34 | [系统初始化处理器](<zh/content/核心模块/处理器层/Organization模块处理器/系统初始化处理器.md>) | 16,803 |
| 35 | [组织管理处理器](<zh/content/核心模块/处理器层/Organization模块处理器/组织管理处理器.md>) | 23,558 |
| 36 | [认证处理器](<zh/content/核心模块/处理器层/Organization模块处理器/认证处理器.md>) | 15,043 |
| 37 | [Project模块处理器](<zh/content/核心模块/处理器层/Project模块处理器/Project模块处理器.md>) | 19,536 |
| 38 | [任务处理器](<zh/content/核心模块/处理器层/Project模块处理器/任务处理器.md>) | 20,910 |
| 39 | [工件处理器](<zh/content/核心模块/处理器层/Project模块处理器/工件处理器.md>) | 16,840 |
| 40 | [项目处理器](<zh/content/核心模块/处理器层/Project模块处理器/项目处理器.md>) | 22,201 |
| 41 | [AOP监控处理器](<zh/content/核心模块/处理器层/System模块处理器/AOP监控处理器.md>) | 17,535 |
| 42 | [System模块处理器](<zh/content/核心模块/处理器层/System模块处理器/System模块处理器.md>) | 26,137 |
| 43 | [任务管理处理器](<zh/content/核心模块/处理器层/System模块处理器/任务管理处理器.md>) | 18,277 |
| 44 | [备份管理处理器](<zh/content/核心模块/处理器层/System模块处理器/备份管理处理器.md>) | 14,225 |
| 45 | [定时任务处理器](<zh/content/核心模块/处理器层/System模块处理器/定时任务处理器.md>) | 21,185 |
| 46 | [日志查询处理器](<zh/content/核心模块/处理器层/System模块处理器/日志查询处理器.md>) | 16,913 |
| 47 | [种子数据处理器](<zh/content/核心模块/处理器层/System模块处理器/种子数据处理器.md>) | 20,761 |
| 48 | [User模块处理器](<zh/content/核心模块/处理器层/User模块处理器.md>) | 14,976 |
| 49 | [健康检查处理器](<zh/content/核心模块/处理器层/健康检查处理器.md>) | 14,075 |
| 50 | [处理器层](<zh/content/核心模块/处理器层/处理器层.md>) | 15,226 |
| 51 | [全文搜索（代码落地层）](<zh/content/核心模块/存储系统/全文搜索.md>) | 17,093 |
| 52 | [HNSW向量存储](<zh/content/核心模块/存储系统/向量存储/HNSW向量存储.md>) | 15,553 |
| 53 | [LanceDB向量存储](<zh/content/核心模块/存储系统/向量存储/LanceDB向量存储.md>) | 15,130 |
| 54 | [SQLite VSS向量存储](<zh/content/核心模块/存储系统/向量存储/SQLite VSS向量存储.md>) | 17,052 |
| 55 | [内存向量存储](<zh/content/核心模块/存储系统/向量存储/内存向量存储.md>) | 15,254 |
| 56 | [向量存储（代码落地层）](<zh/content/核心模块/存储系统/向量存储/向量存储.md>) | 17,988 |
| 57 | [向量存储抽象接口（代码落地层）](<zh/content/核心模块/存储系统/向量存储/向量存储抽象接口.md>) | 17,208 |
| 58 | [存储架构设计](<zh/content/核心模块/存储系统/存储架构设计.md>) | 19,000 |
| 59 | [存储系统（代码落地层）](<zh/content/核心模块/存储系统/存储系统.md>) | 21,478 |
| 60 | [工具输出与安全治理](<zh/content/核心模块/工具与技能/工具系统/工具输出与安全治理.md>) | 31,328 |
| 61 | [脱敏引擎下沉 common](<zh/content/核心模块/工具与技能/工具系统/脱敏引擎下沉 common.md>) | 25,444 |
| 62 | [HTTP 工具（代码落地层）](<zh/content/核心模块/工具注册表/HTTP 工具.md>) | 19,354 |
| 63 | [MCP 工具集成（代码落地层）](<zh/content/核心模块/工具注册表/MCP 工具集成.md>) | 15,622 |
| 64 | [Shell 执行工具（代码落地层）](<zh/content/核心模块/工具注册表/Shell 执行工具.md>) | 20,907 |
| 65 | [内置工具系统（代码落地层）](<zh/content/核心模块/工具注册表/内置工具系统.md>) | 27,065 |
| 66 | [工具安全控制](<zh/content/核心模块/工具注册表/工具安全控制.md>) | 26,758 |
| 67 | [工具注册表（代码落地层）](<zh/content/核心模块/工具注册表/工具注册表.md>) | 23,550 |
| 68 | [核心架构与注册机制（代码落地层）](<zh/content/核心模块/工具注册表/核心架构与注册机制.md>) | 20,842 |
| 69 | [SQLite 数据访问对象](<zh/content/核心模块/服务层/数据对象层 (DAO)/SQLite 数据访问对象.md>) | 17,018 |
| 70 | [向量存储数据访问对象](<zh/content/核心模块/服务层/数据对象层 (DAO)/向量存储数据访问对象.md>) | 19,926 |
| 71 | [外部 API 数据访问对象](<zh/content/核心模块/服务层/数据对象层 (DAO)/外部 API 数据访问对象.md>) | 19,058 |
| 72 | [数据对象层（DAO）](<zh/content/核心模块/服务层/数据对象层 (DAO)/数据对象层 (DAO).md>) | 20,029 |
| 73 | [消息系统数据访问对象](<zh/content/核心模块/服务层/数据对象层 (DAO)/消息系统数据访问对象.md>) | 15,259 |
| 74 | [统计数据分析对象](<zh/content/核心模块/服务层/数据对象层 (DAO)/统计数据分析对象.md>) | 15,592 |
| 75 | [数据访问层 (DAL)](<zh/content/核心模块/服务层/数据访问层 (DAL).md>) | 12,771 |
| 76 | [服务层](<zh/content/核心模块/服务层/服务层.md>) | 18,069 |
| 77 | [HR领域](<zh/content/核心模块/服务层/领域层/HR领域.md>) | 20,261 |
| 78 | [消息领域](<zh/content/核心模块/服务层/领域层/消息领域.md>) | 25,553 |
| 79 | [策略引擎与 Shell 拦截层](<zh/content/核心模块/服务层/领域层/策略引擎与 Shell 拦截层.md>) | 25,080 |
| 80 | [系统领域](<zh/content/核心模块/服务层/领域层/系统领域.md>) | 14,006 |
| 81 | [组织领域](<zh/content/核心模块/服务层/领域层/组织领域.md>) | 17,546 |
| 82 | [财务领域](<zh/content/核心模块/服务层/领域层/财务领域/财务领域.md>) | 19,563 |
| 83 | [身份凭证管理（统一 Domain CRUD + AES-256-GCM 加密存储 + 生命周期联动）](<zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md>) | 30,165 |
| 84 | [飞书集成系统](<zh/content/核心模块/服务层/领域层/财务领域/飞书集成系统.md>) | 24,330 |
| 85 | [运行时领域](<zh/content/核心模块/服务层/领域层/运行时领域.md>) | 40,507 |
| 86 | [项目领域](<zh/content/核心模块/服务层/领域层/项目领域.md>) | 14,951 |
| 87 | [领域层](<zh/content/核心模块/服务层/领域层/领域层.md>) | 18,300 |
| 88 | [核心模块](<zh/content/核心模块/核心模块.md>) | 14,085 |
| 89 | [路由和中间件](<zh/content/核心模块/路由和中间件.md>) | 19,692 |

### 项目概述（62 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [快速开始指南](<zh/content/项目概述/快速开始指南.md>) | 14,520 |
| 2 | [技术栈概览](<zh/content/项目概述/技术栈概览.md>) | 19,061 |
| 3 | [A2A Client 模式](<zh/content/项目概述/核心功能特性/A2A 协议支持/A2A Client 模式.md>) | 19,117 |
| 4 | [A2A Server 模式](<zh/content/项目概述/核心功能特性/A2A 协议支持/A2A Server 模式.md>) | 16,784 |
| 5 | [A2A 协议支持](<zh/content/项目概述/核心功能特性/A2A 协议支持/A2A 协议支持.md>) | 19,395 |
| 6 | [A2A 协议规范](<zh/content/项目概述/核心功能特性/A2A 协议支持/A2A 协议规范.md>) | 15,552 |
| 7 | [A2A 集成指南](<zh/content/项目概述/核心功能特性/A2A 协议支持/A2A 集成指南.md>) | 15,581 |
| 8 | [Agent 全生命周期管理](<zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 全生命周期管理.md>) | 24,447 |
| 9 | [Agent 创建与配置](<zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 创建与配置.md>) | 16,594 |
| 10 | [Agent 搜索与查询](<zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 搜索与查询.md>) | 13,603 |
| 11 | [Agent 状态管理](<zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 状态管理.md>) | 18,469 |
| 12 | [Agent 记忆系统](<zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 记忆系统.md>) | 20,471 |
| 13 | [技能与工具绑定](<zh/content/项目概述/核心功能特性/Agent 全生命周期管理/技能与工具绑定.md>) | 17,407 |
| 14 | [任务协作与执行计划](<zh/content/项目概述/核心功能特性/任务协作与执行计划/任务协作与执行计划.md>) | 23,385 |
| 15 | [任务生命周期管理](<zh/content/项目概述/核心功能特性/任务协作与执行计划/任务生命周期管理.md>) | 16,425 |
| 16 | [任务管理 API 参考](<zh/content/项目概述/核心功能特性/任务协作与执行计划/任务管理 API 参考.md>) | 16,928 |
| 17 | [多 Agent 协作机制](<zh/content/项目概述/核心功能特性/任务协作与执行计划/多 Agent 协作机制.md>) | 18,661 |
| 18 | [执行计划与结果追踪](<zh/content/项目概述/核心功能特性/任务协作与执行计划/执行计划与结果追踪.md>) | 18,803 |
| 19 | [四层记忆系统](<zh/content/项目概述/核心功能特性/四层记忆系统/四层记忆系统.md>) | 26,527 |
| 20 | [工作记忆 (Working Memory)](<zh/content/项目概述/核心功能特性/四层记忆系统/工作记忆 (Working Memory).md>) | 20,312 |
| 21 | [核心记忆 (Core Memory)](<zh/content/项目概述/核心功能特性/四层记忆系统/核心记忆 (Core Memory).md>) | 17,581 |
| 22 | [原始记忆追踪](<zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆 (Short-term Memory)/原始记忆追踪.md>) | 22,569 |
| 23 | [短期记忆（Short-term Memory）](<zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆 (Short-term Memory)/短期记忆 (Short-term Memory).md>) | 21,253 |
| 24 | [短期记忆索引](<zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆 (Short-term Memory)/短期记忆索引.md>) | 16,653 |
| 25 | [记忆搜索机制](<zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆 (Short-term Memory)/记忆搜索机制.md>) | 21,216 |
| 26 | [记忆沉淀机制](<zh/content/项目概述/核心功能特性/四层记忆系统/记忆沉淀机制.md>) | 29,414 |
| 27 | [知识关系管理](<zh/content/项目概述/核心功能特性/四层记忆系统/长期记忆 (Long-term Memory)/知识关系管理.md>) | 25,294 |
| 28 | [知识节点管理](<zh/content/项目概述/核心功能特性/四层记忆系统/长期记忆 (Long-term Memory)/知识节点管理.md>) | 16,326 |
| 29 | [长期记忆（Long-term Memory）](<zh/content/项目概述/核心功能特性/四层记忆系统/长期记忆 (Long-term Memory)/长期记忆 (Long-term Memory).md>) | 21,874 |
| 30 | [多渠道消息系统](<zh/content/项目概述/核心功能特性/多渠道消息系统/多渠道消息系统.md>) | 25,235 |
| 31 | [消息处理核心](<zh/content/项目概述/核心功能特性/多渠道消息系统/消息处理核心.md>) | 21,376 |
| 32 | [消息渠道适配器](<zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md>) | 22,923 |
| 33 | [Agent 维度统计](<zh/content/项目概述/核心功能特性/多维统计系统/Agent 维度统计.md>) | 16,452 |
| 34 | [ModelProvider 维度统计](<zh/content/项目概述/核心功能特性/多维统计系统/ModelProvider 维度统计.md>) | 14,708 |
| 35 | [Project 维度统计](<zh/content/项目概述/核心功能特性/多维统计系统/Project 维度统计.md>) | 15,976 |
| 36 | [Task 维度统计](<zh/content/项目概述/核心功能特性/多维统计系统/Task 维度统计.md>) | 16,292 |
| 37 | [Tool 维度统计](<zh/content/项目概述/核心功能特性/多维统计系统/Tool 维度统计.md>) | 17,215 |
| 38 | [多维统计系统](<zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md>) | 26,900 |
| 39 | [核心功能特性](<zh/content/项目概述/核心功能特性/核心功能特性.md>) | 32,101 |
| 40 | [定时任务管理](<zh/content/项目概述/核心功能特性/系统管理功能/定时任务管理.md>) | 21,557 |
| 41 | [数据备份与恢复](<zh/content/项目概述/核心功能特性/系统管理功能/数据备份与恢复.md>) | 16,893 |
| 42 | [日志管理系统（入门概览层）](<zh/content/项目概述/核心功能特性/系统管理功能/日志管理系统.md>) | 19,313 |
| 43 | [系统健康检查](<zh/content/项目概述/核心功能特性/系统管理功能/系统健康检查.md>) | 24,037 |
| 44 | [系统管理功能](<zh/content/项目概述/核心功能特性/系统管理功能/系统管理功能.md>) | 16,575 |
| 45 | [MCP 工具集成（入门概览层）](<zh/content/项目概述/核心功能特性/统一工具调用架构/MCP 工具集成.md>) | 16,428 |
| 46 | [HTTP 请求工具 (http_fetch)](<zh/content/项目概述/核心功能特性/统一工具调用架构/内置工具/HTTP 请求工具 (http_fetch).md>) | 19,282 |
| 47 | [Shell 执行工具 (shell_exec)](<zh/content/项目概述/核心功能特性/统一工具调用架构/内置工具/Shell 执行工具 (shell_exec).md>) | 17,509 |
| 48 | [内置工具](<zh/content/项目概述/核心功能特性/统一工具调用架构/内置工具/内置工具.md>) | 22,251 |
| 49 | [文件系统写入工具 (fs_write)](<zh/content/项目概述/核心功能特性/统一工具调用架构/内置工具/文件系统写入工具 (fs_write).md>) | 15,114 |
| 50 | [文件系统读取工具 (fs_read)](<zh/content/项目概述/核心功能特性/统一工具调用架构/内置工具/文件系统读取工具 (fs_read).md>) | 12,137 |
| 51 | [工具安全控制](<zh/content/项目概述/核心功能特性/统一工具调用架构/工具安全控制.md>) | 20,027 |
| 52 | [工具调用模式](<zh/content/项目概述/核心功能特性/统一工具调用架构/工具调用模式.md>) | 17,573 |
| 53 | [统一工具调用架构](<zh/content/项目概述/核心功能特性/统一工具调用架构/统一工具调用架构.md>) | 23,832 |
| 54 | [自定义工具开发](<zh/content/项目概述/核心功能特性/统一工具调用架构/自定义工具开发.md>) | 16,512 |
| 55 | [FTS5 关键词搜索](<zh/content/项目概述/核心功能特性/综合搜索能力/FTS5 关键词搜索.md>) | 18,064 |
| 56 | [向量语义搜索](<zh/content/项目概述/核心功能特性/综合搜索能力/向量语义搜索.md>) | 15,894 |
| 57 | [混合搜索引擎](<zh/content/项目概述/核心功能特性/综合搜索能力/混合搜索引擎.md>) | 16,452 |
| 58 | [知识图谱搜索](<zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md>) | 28,594 |
| 59 | [综合搜索能力](<zh/content/项目概述/核心功能特性/综合搜索能力/综合搜索能力.md>) | 22,816 |
| 60 | [项目介绍与目标](<zh/content/项目概述/项目介绍与目标.md>) | 13,872 |
| 61 | [项目概述](<zh/content/项目概述/项目概述.md>) | 17,985 |
| 62 | [项目结构说明](<zh/content/项目概述/项目结构说明.md>) | 16,121 |

### 功能模块（50 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AI Agent 管理](<zh/content/功能模块/AI Agent 管理/AI Agent 管理.md>) | 18,234 |
| 2 | [Agent 搜索与推荐](<zh/content/功能模块/AI Agent 管理/Agent 搜索与推荐.md>) | 13,237 |
| 3 | [Agent 生命周期管理](<zh/content/功能模块/AI Agent 管理/Agent 生命周期管理.md>) | 28,036 |
| 4 | [小脑快判断（System One）](<zh/content/功能模块/AI Agent 管理/小脑快判断（System One）.md>) | 27,908 |
| 5 | [工具绑定管理](<zh/content/功能模块/AI Agent 管理/工具绑定管理.md>) | 15,637 |
| 6 | [技能包管理](<zh/content/功能模块/AI Agent 管理/技能包管理.md>) | 30,198 |
| 7 | [记忆系统管理](<zh/content/功能模块/AI Agent 管理/记忆系统管理.md>) | 36,445 |
| 8 | [功能模块](<zh/content/功能模块/功能模块.md>) | 15,674 |
| 9 | [GitHub 集成](<zh/content/功能模块/外部集成/GitHub 集成.md>) | 24,077 |
| 10 | [MCP 服务器集成](<zh/content/功能模块/工具生态系统/MCP 服务器集成.md>) | 19,623 |
| 11 | [HTTP 客户端工具](<zh/content/功能模块/工具生态系统/内置工具集/HTTP 客户端工具.md>) | 22,214 |
| 12 | [Shell 执行工具（业务功能层）](<zh/content/功能模块/工具生态系统/内置工具集/Shell 执行工具.md>) | 28,430 |
| 13 | [内置工具集](<zh/content/功能模块/工具生态系统/内置工具集/内置工具集.md>) | 22,075 |
| 14 | [文件系统工具](<zh/content/功能模块/工具生态系统/内置工具集/文件系统工具.md>) | 20,129 |
| 15 | [工具执行引擎](<zh/content/功能模块/工具生态系统/工具执行引擎.md>) | 21,547 |
| 16 | [工具注册与发现](<zh/content/功能模块/工具生态系统/工具注册与发现.md>) | 19,712 |
| 17 | [工具生态系统](<zh/content/功能模块/工具生态系统/工具生态系统.md>) | 22,923 |
| 18 | [工具管理 API（业务功能层）](<zh/content/功能模块/工具生态系统/工具管理 API.md>) | 18,633 |
| 19 | [运行时诊断工具组](<zh/content/功能模块/工具生态系统/运行时诊断工具组.md>) | 22,963 |
| 20 | [技能系统](<zh/content/功能模块/技能系统.md>) | 29,615 |
| 21 | [模型提供商管理（业务功能层）](<zh/content/功能模块/模型提供商管理.md>) | 23,677 |
| 22 | [@提及功能（mention）](<zh/content/功能模块/消息系统/@提及功能（mention）.md>) | 23,364 |
| 23 | [Agent消息集成](<zh/content/功能模块/消息系统/Agent消息集成.md>) | 18,904 |
| 24 | [实时推送](<zh/content/功能模块/消息系统/实时推送.md>) | 18,027 |
| 25 | [微信 iLink 专属渠道](<zh/content/功能模块/消息系统/微信 iLink 专属渠道.md>) | 38,567 |
| 26 | [消息渠道管理（业务功能层）](<zh/content/功能模块/消息系统/消息渠道管理.md>) | 32,265 |
| 27 | [消息管理](<zh/content/功能模块/消息系统/消息管理.md>) | 19,186 |
| 28 | [消息系统（业务功能层）](<zh/content/功能模块/消息系统/消息系统.md>) | 22,278 |
| 29 | [用户与组织管理](<zh/content/功能模块/用户与组织管理/用户与组织管理.md>) | 20,659 |
| 30 | [用户管理](<zh/content/功能模块/用户与组织管理/用户管理.md>) | 19,427 |
| 31 | [用户认证与授权](<zh/content/功能模块/用户与组织管理/用户认证与授权.md>) | 19,288 |
| 32 | [系统初始化](<zh/content/功能模块/用户与组织管理/系统初始化.md>) | 26,026 |
| 33 | [组织管理](<zh/content/功能模块/用户与组织管理/组织管理.md>) | 20,351 |
| 34 | [组织组网与联邦](<zh/content/功能模块/用户与组织管理/组织组网与联邦.md>) | 23,563 |
| 35 | [跨组织业务调用鉴权](<zh/content/功能模块/用户与组织管理/跨组织业务调用鉴权.md>) | 20,970 |
| 36 | [身份凭证与授权流程](<zh/content/功能模块/用户与组织管理/身份凭证与授权流程.md>) | 29,380 |
| 37 | [本体属性管理](<zh/content/功能模块/知识图谱管理/本体属性管理.md>) | 32,950 |
| 38 | [本体词表与漂移治理](<zh/content/功能模块/知识图谱管理/本体词表与漂移治理.md>) | 31,144 |
| 39 | [AOP 事件系统（系统管理层）](<zh/content/功能模块/系统管理/AOP 事件系统.md>) | 22,804 |
| 40 | [后台进程管理系统](<zh/content/功能模块/系统管理/后台进程管理系统.md>) | 24,516 |
| 41 | [备份恢复管理（业务功能层）](<zh/content/功能模块/系统管理/备份恢复管理.md>) | 14,861 |
| 42 | [定时任务调度（业务功能层）](<zh/content/功能模块/系统管理/定时任务调度.md>) | 17,522 |
| 43 | [日志管理系统（系统管理层）](<zh/content/功能模块/系统管理/日志管理系统.md>) | 19,153 |
| 44 | [种子数据管理（业务功能层）](<zh/content/功能模块/系统管理/种子数据管理.md>) | 32,595 |
| 45 | [系统监控与健康检查](<zh/content/功能模块/系统管理/系统监控与健康检查.md>) | 16,177 |
| 46 | [系统管理](<zh/content/功能模块/系统管理/系统管理.md>) | 20,150 |
| 47 | [任务管理（业务功能层）](<zh/content/功能模块/项目管理/任务管理.md>) | 22,774 |
| 48 | [工件管理](<zh/content/功能模块/项目管理/工件管理.md>) | 18,209 |
| 49 | [项目 CRUD 操作](<zh/content/功能模块/项目管理/项目 CRUD 操作.md>) | 20,430 |
| 50 | [项目管理（业务功能层）](<zh/content/功能模块/项目管理/项目管理.md>) | 18,564 |

### 基础设施（50 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 事件系统（框架层）](<zh/content/基础设施/AOP 事件系统/AOP 事件系统.md>) | 22,910 |
| 2 | [AOP 核心架构](<zh/content/基础设施/AOP 事件系统/AOP 核心架构/AOP 核心架构.md>) | 23,455 |
| 3 | [事件系统核心](<zh/content/基础设施/AOP 事件系统/AOP 核心架构/事件系统核心.md>) | 19,652 |
| 4 | [注册中心与调度器](<zh/content/基础设施/AOP 事件系统/AOP 核心架构/注册中心与调度器.md>) | 21,036 |
| 5 | [生产者消费者模式](<zh/content/基础设施/AOP 事件系统/AOP 核心架构/生产者消费者模式.md>) | 23,783 |
| 6 | [Agent 循环消费者（框架层）](<zh/content/基础设施/AOP 事件系统/事件消费者/Agent 循环消费者.md>) | 14,375 |
| 7 | [事件消费者](<zh/content/基础设施/AOP 事件系统/事件消费者/事件消费者.md>) | 19,212 |
| 8 | [任务事件消费者（框架层）](<zh/content/基础设施/AOP 事件系统/事件消费者/任务事件消费者.md>) | 14,336 |
| 9 | [工具执行日志消费者](<zh/content/基础设施/AOP 事件系统/事件消费者/工具执行日志消费者.md>) | 17,699 |
| 10 | [工具执行统计消费者](<zh/content/基础设施/AOP 事件系统/事件消费者/工具执行统计消费者.md>) | 13,443 |
| 11 | [思考轮次统计消费者](<zh/content/基础设施/AOP 事件系统/事件消费者/思考轮次统计消费者.md>) | 17,856 |
| 12 | [消息消费者（框架层）](<zh/content/基础设施/AOP 事件系统/事件消费者/消息消费者.md>) | 18,363 |
| 13 | [调度器消费者](<zh/content/基础设施/AOP 事件系统/事件消费者/调度器消费者.md>) | 18,174 |
| 14 | [A2A 轮询生产者](<zh/content/基础设施/AOP 事件系统/事件生产者/A2A 轮询生产者.md>) | 15,544 |
| 15 | [事件生产者](<zh/content/基础设施/AOP 事件系统/事件生产者/事件生产者.md>) | 22,483 |
| 16 | [定时触发生产者](<zh/content/基础设施/AOP 事件系统/事件生产者/定时触发生产者.md>) | 21,030 |
| 17 | [消息通道生产者](<zh/content/基础设施/AOP 事件系统/事件生产者/消息通道生产者.md>) | 15,557 |
| 18 | [事件队列管理](<zh/content/基础设施/AOP 事件系统/事件队列管理.md>) | 20,118 |
| 19 | [统计与监控](<zh/content/基础设施/AOP 事件系统/统计与监控.md>) | 25,802 |
| 20 | [Doctor 依赖预检](<zh/content/基础设施/Doctor 依赖预检.md>) | 5,360 |
| 21 | [优雅关闭系统](<zh/content/基础设施/优雅关闭系统.md>) | 11,943 |
| 22 | [后台任务系统](<zh/content/基础设施/后台任务系统.md>) | 21,078 |
| 23 | [基础设施](<zh/content/基础设施/基础设施.md>) | 18,626 |
| 24 | [全文搜索（框架层）](<zh/content/基础设施/存储系统/全文搜索.md>) | 16,317 |
| 25 | [HNSW 索引存储后端](<zh/content/基础设施/存储系统/向量存储/HNSW 索引存储后端.md>) | 15,771 |
| 26 | [LanceDB 存储后端](<zh/content/基础设施/存储系统/向量存储/LanceDB 存储后端.md>) | 17,207 |
| 27 | [SQLite VSS 存储后端](<zh/content/基础设施/存储系统/向量存储/SQLite VSS 存储后端.md>) | 15,198 |
| 28 | [内存向量存储后端](<zh/content/基础设施/存储系统/向量存储/内存向量存储后端.md>) | 15,732 |
| 29 | [向量存储（框架层）](<zh/content/基础设施/存储系统/向量存储/向量存储.md>) | 19,459 |
| 30 | [向量存储抽象接口（框架层）](<zh/content/基础设施/存储系统/向量存储/向量存储抽象接口.md>) | 19,590 |
| 31 | [存储系统（框架层）](<zh/content/基础设施/存储系统/存储系统.md>) | 22,527 |
| 32 | [存储配置](<zh/content/基础设施/存储系统/存储配置.md>) | 14,606 |
| 33 | [本地文件安全与工作区隔离](<zh/content/基础设施/存储系统/本地文件安全与工作区隔离.md>) | 23,495 |
| 34 | [HTTP 工具（框架层）](<zh/content/基础设施/工具注册表/HTTP 工具.md>) | 16,499 |
| 35 | [MCP 协议工具](<zh/content/基础设施/工具注册表/MCP 协议工具.md>) | 17,720 |
| 36 | [Shell 执行工具（框架层）](<zh/content/基础设施/工具注册表/Shell 执行工具.md>) | 22,928 |
| 37 | [pkg 基建统一](<zh/content/基础设施/工具注册表/pkg 基建统一.md>) | 11,417 |
| 38 | [共享工具凭据增强器](<zh/content/基础设施/工具注册表/共享工具凭据增强器.md>) | 36,559 |
| 39 | [HTTP请求工具](<zh/content/基础设施/工具注册表/内置工具系统/HTTP请求工具.md>) | 18,480 |
| 40 | [Shell执行工具](<zh/content/基础设施/工具注册表/内置工具系统/Shell执行工具.md>) | 20,094 |
| 41 | [内置工具系统（框架层）](<zh/content/基础设施/工具注册表/内置工具系统/内置工具系统.md>) | 18,934 |
| 42 | [文件写入工具](<zh/content/基础设施/工具注册表/内置工具系统/文件写入工具.md>) | 17,707 |
| 43 | [文件读取工具](<zh/content/基础设施/工具注册表/内置工具系统/文件读取工具.md>) | 18,722 |
| 44 | [工具安全控制](<zh/content/基础设施/工具注册表/工具安全控制.md>) | 22,730 |
| 45 | [工具注册表（框架层）](<zh/content/基础设施/工具注册表/工具注册表.md>) | 21,798 |
| 46 | [核心架构与注册机制（框架层）](<zh/content/基础设施/工具注册表/核心架构与注册机制.md>) | 19,701 |
| 47 | [持续集成与发布工作流](<zh/content/基础设施/持续集成与发布工作流.md>) | 30,168 |
| 48 | [文档链接质量门禁](<zh/content/基础设施/持续集成与发布工作流/文档链接质量门禁.md>) | 24,058 |
| 49 | [日志系统](<zh/content/基础设施/日志系统.md>) | 20,193 |
| 50 | [请求上下文](<zh/content/基础设施/请求上下文.md>) | 20,282 |

### 前端应用（42 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [API 客户端](<zh/content/前端应用/API 客户端.md>) | 27,643 |
| 2 | [UI 样式与主题](<zh/content/前端应用/UI 样式与主题.md>) | 32,313 |
| 3 | [前端应用](<zh/content/前端应用/前端应用.md>) | 46,453 |
| 4 | [前端架构设计](<zh/content/前端应用/前端架构设计.md>) | 39,090 |
| 5 | [业务组件](<zh/content/前端应用/组件系统/业务组件.md>) | 24,392 |
| 6 | [思考运行时面板观测接口](<zh/content/前端应用/组件系统/业务组件/思考运行时面板观测接口.md>) | 25,073 |
| 7 | [Mermaid图表支持](<zh/content/前端应用/组件系统/图表组件/Mermaid图表支持.md>) | 16,163 |
| 8 | [图表组件](<zh/content/前端应用/组件系统/图表组件/图表组件.md>) | 21,928 |
| 9 | [Markdown渲染组件](<zh/content/前端应用/组件系统/基础组件/Markdown渲染组件.md>) | 20,678 |
| 10 | [基础组件](<zh/content/前端应用/组件系统/基础组件/基础组件.md>) | 19,424 |
| 11 | [布局组件](<zh/content/前端应用/组件系统/布局组件.md>) | 18,487 |
| 12 | [组件系统](<zh/content/前端应用/组件系统/组件系统.md>) | 29,681 |
| 13 | [钩子系统](<zh/content/前端应用/组件系统/钩子系统.md>) | 16,510 |
| 14 | [Finance 管理页面](<zh/content/前端应用/页面模块/Finance 管理页面/Finance 管理页面.md>) | 21,262 |
| 15 | [MCP 服务器管理](<zh/content/前端应用/页面模块/Finance 管理页面/MCP 服务器管理.md>) | 19,136 |
| 16 | [HTTP工具创建界面](<zh/content/前端应用/页面模块/Finance 管理页面/工具管理/HTTP工具创建界面.md>) | 17,791 |
| 17 | [工具管理](<zh/content/前端应用/页面模块/Finance 管理页面/工具管理/工具管理.md>) | 19,589 |
| 18 | [模型提供商管理（前端 UI 层）](<zh/content/前端应用/页面模块/Finance 管理页面/模型提供商管理.md>) | 27,155 |
| 19 | [消息渠道管理（前端 UI 层）](<zh/content/前端应用/页面模块/Finance 管理页面/消息渠道管理.md>) | 16,889 |
| 20 | [附件管理](<zh/content/前端应用/页面模块/Finance 管理页面/附件管理.md>) | 17,051 |
| 21 | [Agent 管理功能](<zh/content/前端应用/页面模块/HR 管理页面/Agent 管理功能.md>) | 27,014 |
| 22 | [HR 管理页面](<zh/content/前端应用/页面模块/HR 管理页面/HR 管理页面.md>) | 16,117 |
| 23 | [技能管理系统](<zh/content/前端应用/页面模块/HR 管理页面/技能管理系统.md>) | 25,653 |
| 24 | [知识图谱可视化](<zh/content/前端应用/页面模块/HR 管理页面/知识图谱可视化.md>) | 46,032 |
| 25 | [记忆搜索功能](<zh/content/前端应用/页面模块/HR 管理页面/记忆搜索功能.md>) | 17,718 |
| 26 | [消息与工作区页面](<zh/content/前端应用/页面模块/消息与工作区页面/消息与工作区页面.md>) | 26,831 |
| 27 | [工具调用标签页](<zh/content/前端应用/页面模块/消息与工作区页面/聊天侧面板/工具调用标签页.md>) | 19,527 |
| 28 | [聊天侧面板](<zh/content/前端应用/页面模块/消息与工作区页面/聊天侧面板/聊天侧面板.md>) | 20,774 |
| 29 | [用户与组织页面](<zh/content/前端应用/页面模块/用户与组织页面.md>) | 18,216 |
| 30 | [AOP 监控面板](<zh/content/前端应用/页面模块/系统管理页面/AOP 监控面板.md>) | 19,636 |
| 31 | [备份恢复管理（前端 UI 层）](<zh/content/前端应用/页面模块/系统管理页面/备份恢复管理.md>) | 14,974 |
| 32 | [定时任务调度（前端 UI 层）](<zh/content/前端应用/页面模块/系统管理页面/定时任务调度.md>) | 19,071 |
| 33 | [文档中心](<zh/content/前端应用/页面模块/系统管理页面/文档中心.md>) | 12,628 |
| 34 | [日志管理系统（前端 UI 层）](<zh/content/前端应用/页面模块/系统管理页面/日志管理系统.md>) | 18,000 |
| 35 | [种子数据管理（前端 UI 层）](<zh/content/前端应用/页面模块/系统管理页面/种子数据管理.md>) | 17,708 |
| 36 | [系统健康检查](<zh/content/前端应用/页面模块/系统管理页面/系统健康检查.md>) | 16,169 |
| 37 | [系统管理页面](<zh/content/前端应用/页面模块/系统管理页面/系统管理页面.md>) | 18,918 |
| 38 | [页面模块](<zh/content/前端应用/页面模块/页面模块.md>) | 23,095 |
| 39 | [任务管理功能](<zh/content/前端应用/页面模块/项目管理页面/任务管理功能.md>) | 23,191 |
| 40 | [工件管理功能](<zh/content/前端应用/页面模块/项目管理页面/工件管理功能.md>) | 19,977 |
| 41 | [项目管理功能](<zh/content/前端应用/页面模块/项目管理页面/项目管理功能.md>) | 21,480 |
| 42 | [项目管理页面](<zh/content/前端应用/页面模块/项目管理页面/项目管理页面.md>) | 24,362 |

### API 参考（39 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [A2A 协议](<zh/content/API 参考/A2A 协议/A2A 协议.md>) | 16,937 |
| 2 | [Agent 发现机制](<zh/content/API 参考/A2A 协议/Agent 发现机制.md>) | 16,121 |
| 3 | [JSON-RPC 通信协议](<zh/content/API 参考/A2A 协议/JSON-RPC 通信协议.md>) | 19,585 |
| 4 | [任务创建](<zh/content/API 参考/A2A 协议/任务管理/任务创建.md>) | 14,417 |
| 5 | [任务取消](<zh/content/API 参考/A2A 协议/任务管理/任务取消.md>) | 13,854 |
| 6 | [任务查询](<zh/content/API 参考/A2A 协议/任务管理/任务查询.md>) | 14,883 |
| 7 | [任务生命周期](<zh/content/API 参考/A2A 协议/任务管理/任务生命周期.md>) | 16,026 |
| 8 | [任务管理](<zh/content/API 参考/A2A 协议/任务管理/任务管理.md>) | 14,773 |
| 9 | [回调机制](<zh/content/API 参考/A2A 协议/回调机制.md>) | 16,194 |
| 10 | [API 参考](<zh/content/API 参考/API 参考.md>) | 19,226 |
| 11 | [RESTful API](<zh/content/API 参考/RESTful API/RESTful API.md>) | 35,462 |
| 12 | [Agent 管理 API](<zh/content/API 参考/RESTful API/人力资源模块 API/Agent 管理 API.md>) | 18,849 |
| 13 | [人力资源模块 API](<zh/content/API 参考/RESTful API/人力资源模块 API/人力资源模块 API.md>) | 28,314 |
| 14 | [技能包管理 API](<zh/content/API 参考/RESTful API/人力资源模块 API/技能包管理 API.md>) | 23,375 |
| 15 | [记忆管理 API](<zh/content/API 参考/RESTful API/人力资源模块 API/记忆管理 API.md>) | 16,523 |
| 16 | [用户管理模块 API](<zh/content/API 参考/RESTful API/用户管理模块 API.md>) | 19,246 |
| 17 | [任务管理 API](<zh/content/API 参考/RESTful API/系统管理模块 API/任务管理 API.md>) | 16,157 |
| 18 | [备份恢复 API](<zh/content/API 参考/RESTful API/系统管理模块 API/备份恢复 API.md>) | 13,588 |
| 19 | [定时任务 API](<zh/content/API 参考/RESTful API/系统管理模块 API/定时任务 API.md>) | 22,092 |
| 20 | [日志查询 API](<zh/content/API 参考/RESTful API/系统管理模块 API/日志查询 API.md>) | 15,923 |
| 21 | [监控指标 API](<zh/content/API 参考/RESTful API/系统管理模块 API/监控指标 API.md>) | 18,505 |
| 22 | [种子数据管理 API](<zh/content/API 参考/RESTful API/系统管理模块 API/种子数据管理 API.md>) | 18,855 |
| 23 | [系统管理模块 API](<zh/content/API 参考/RESTful API/系统管理模块 API/系统管理模块 API.md>) | 19,043 |
| 24 | [组织管理模块 API](<zh/content/API 参考/RESTful API/组织管理模块 API.md>) | 17,919 |
| 25 | [认证与授权 API](<zh/content/API 参考/RESTful API/认证与授权 API.md>) | 15,386 |
| 26 | [MCP 服务器管理 API](<zh/content/API 参考/RESTful API/财务管理模块 API/MCP 服务器管理 API.md>) | 15,774 |
| 27 | [工具管理 API（API 参考层）](<zh/content/API 参考/RESTful API/财务管理模块 API/工具管理 API.md>) | 19,997 |
| 28 | [模型提供商管理 API](<zh/content/API 参考/RESTful API/财务管理模块 API/模型提供商管理 API.md>) | 23,674 |
| 29 | [消息处理 API](<zh/content/API 参考/RESTful API/财务管理模块 API/消息处理 API.md>) | 20,197 |
| 30 | [消息渠道管理 API](<zh/content/API 参考/RESTful API/财务管理模块 API/消息渠道管理 API.md>) | 20,390 |
| 31 | [财务管理模块 API](<zh/content/API 参考/RESTful API/财务管理模块 API/财务管理模块 API.md>) | 23,364 |
| 32 | [附件管理 API](<zh/content/API 参考/RESTful API/财务管理模块 API/附件管理 API.md>) | 18,020 |
| 33 | [任务管理接口](<zh/content/API 参考/RESTful API/项目管理模块 API/任务管理接口.md>) | 17,028 |
| 34 | [工件管理接口](<zh/content/API 参考/RESTful API/项目管理模块 API/工件管理接口.md>) | 18,572 |
| 35 | [项目 CRUD 接口](<zh/content/API 参考/RESTful API/项目管理模块 API/项目 CRUD 接口.md>) | 17,556 |
| 36 | [项目管理模块 API](<zh/content/API 参考/RESTful API/项目管理模块 API/项目管理模块 API.md>) | 16,269 |
| 37 | [WebSocket 与 SSE](<zh/content/API 参考/WebSocket 与 SSE.md>) | 15,245 |
| 38 | [认证与授权](<zh/content/API 参考/认证与授权.md>) | 15,346 |
| 39 | [错误处理与状态码](<zh/content/API 参考/错误处理与状态码.md>) | 18,789 |

### 架构设计（25 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [AOP 事件系统架构](<zh/content/架构设计/AOP 事件系统架构.md>) | 20,889 |
| 2 | [API协议规范](<zh/content/架构设计/API协议规范/API协议规范.md>) | 16,779 |
| 3 | [分页与计数规范](<zh/content/架构设计/API协议规范/分页与计数规范.md>) | 12,115 |
| 4 | [DAL 层组合](<zh/content/架构设计/分层架构设计/DAL 层组合.md>) | 19,944 |
| 5 | [DAO 层多态](<zh/content/架构设计/分层架构设计/DAO 层多态.md>) | 16,329 |
| 6 | [Domain 层编排](<zh/content/架构设计/分层架构设计/Domain 层编排/Domain 层编排.md>) | 18,364 |
| 7 | [Finance 领域编排](<zh/content/架构设计/分层架构设计/Domain 层编排/Finance 领域编排.md>) | 19,260 |
| 8 | [HR 领域编排](<zh/content/架构设计/分层架构设计/Domain 层编排/HR 领域编排.md>) | 19,952 |
| 9 | [Message 领域编排](<zh/content/架构设计/分层架构设计/Domain 层编排/Message 领域编排.md>) | 15,301 |
| 10 | [Organization 领域编排](<zh/content/架构设计/分层架构设计/Domain 层编排/Organization 领域编排.md>) | 19,291 |
| 11 | [Project 领域编排](<zh/content/架构设计/分层架构设计/Domain 层编排/Project 领域编排.md>) | 13,243 |
| 12 | [Runtime 领域编排](<zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md>) | 39,609 |
| 13 | [System 领域编排](<zh/content/架构设计/分层架构设计/Domain 层编排/System 领域编排.md>) | 16,353 |
| 14 | [Handler 层设计](<zh/content/架构设计/分层架构设计/Handler 层设计.md>) | 14,148 |
| 15 | [PO 与业务实体分层](<zh/content/架构设计/分层架构设计/PO 与业务实体分层.md>) | 14,820 |
| 16 | [分层架构设计](<zh/content/架构设计/分层架构设计/分层架构设计.md>) | 15,490 |
| 17 | [安全架构设计](<zh/content/架构设计/安全架构设计.md>) | 13,906 |
| 18 | [思维轨迹架构](<zh/content/架构设计/思维轨迹架构.md>) | 5,377 |
| 19 | [数据存储架构](<zh/content/架构设计/数据存储架构.md>) | 28,806 |
| 20 | [数据库迁移管理](<zh/content/架构设计/数据存储架构/数据库迁移管理.md>) | 33,433 |
| 21 | [时间戳处理规范](<zh/content/架构设计/数据存储架构/时间戳处理规范.md>) | 19,627 |
| 22 | [整体架构概览](<zh/content/架构设计/整体架构概览.md>) | 13,800 |
| 23 | [本体子系统架构](<zh/content/架构设计/本体子系统架构.md>) | 6,645 |
| 24 | [架构设计](<zh/content/架构设计/架构设计.md>) | 15,500 |
| 25 | [记忆系统架构](<zh/content/架构设计/记忆系统架构.md>) | 29,903 |

### 数据模型（15 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [Agent 和技能模型](<zh/content/数据模型/Agent 和技能模型/Agent 和技能模型.md>) | 18,980 |
| 2 | [Agent 实体](<zh/content/数据模型/Agent 和技能模型/Agent 实体.md>) | 16,149 |
| 3 | [技能和工具系统](<zh/content/数据模型/Agent 和技能模型/技能和工具系统.md>) | 18,351 |
| 4 | [数据模型](<zh/content/数据模型/数据模型.md>) | 28,066 |
| 5 | [核心实体模型](<zh/content/数据模型/核心实体模型.md>) | 16,409 |
| 6 | [消息和记忆模型](<zh/content/数据模型/消息和记忆模型/消息和记忆模型.md>) | 22,578 |
| 7 | [消息系统（数据模型层）](<zh/content/数据模型/消息和记忆模型/消息系统.md>) | 22,158 |
| 8 | [记忆和向量系统](<zh/content/数据模型/消息和记忆模型/记忆和向量系统.md>) | 30,295 |
| 9 | [定时任务和事件系统](<zh/content/数据模型/系统模型/定时任务和事件系统.md>) | 22,972 |
| 10 | [文件存储模型](<zh/content/数据模型/系统模型/文件存储模型.md>) | 18,326 |
| 11 | [模型提供商配置](<zh/content/数据模型/系统模型/模型提供商配置.md>) | 15,009 |
| 12 | [系统模型](<zh/content/数据模型/系统模型/系统模型.md>) | 19,788 |
| 13 | [制品和附件](<zh/content/数据模型/项目和任务模型/制品和附件.md>) | 22,632 |
| 14 | [项目和任务模型](<zh/content/数据模型/项目和任务模型/项目和任务模型.md>) | 20,897 |
| 15 | [项目管理（数据模型层）](<zh/content/数据模型/项目和任务模型/项目管理.md>) | 21,360 |

### 总纲与运维（3 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [开发指南](<zh/content/开发指南.md>) | 17,007 |
| 2 | [故障排除与监控](<zh/content/故障排除与监控.md>) | 17,776 |
| 3 | [配置与部署](<zh/content/配置与部署.md>) | 31,995 |

### 测试指南（2 篇）

| # | 页面 | 字节 |
|---:|---|---:|
| 1 | [测试指南](<zh/content/测试指南/测试指南.md>) | 31,801 |
| 2 | [端到端测试基础设施](<zh/content/测试指南/端到端测试基础设施.md>) | 13,165 |

## RAG 原子知识卡

共 120 张，按路径排序。标题取 frontmatter `name` 字段（无 H1 的卡同样可索引）。

| # | 卡片 | 字节 |
|---:|---|---:|
| 1 | [@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染](<knowledge/zh/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染.md>) | 5,855 |
| 2 | [A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 +](<knowledge/zh/A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送/A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送.md>) | 15,870 |
| 3 | [A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建](<knowledge/zh/A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建/A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建.md>) | 10,442 |
| 4 | [A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息](<knowledge/zh/A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息/A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息.md>) | 9,436 |
| 5 | [AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容](<knowledge/zh/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容.md>) | 10,953 |
| 6 | [技术栈](<knowledge/zh/AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/技术栈.md>) | 337 |
| 7 | [架构设计](<knowledge/zh/AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/架构设计.md>) | 1,691 |
| 8 | [概述](<knowledge/zh/AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/概述.md>) | 184 |
| 9 | [特殊配置与命令](<knowledge/zh/AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/特殊配置与命令.md>) | 400 |
| 10 | [编码规范](<knowledge/zh/AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/编码规范.md>) | 1,058 |
| 11 | [技术栈](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/AI Orz 端到端测试（Playwright E2E）/技术栈.md>) | 163 |
| 12 | [架构设计](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/AI Orz 端到端测试（Playwright E2E）/架构设计.md>) | 964 |
| 13 | [概述](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/AI Orz 端到端测试（Playwright E2E）/概述.md>) | 144 |
| 14 | [特殊配置与命令](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/AI Orz 端到端测试（Playwright E2E）/特殊配置与命令.md>) | 247 |
| 15 | [编码规范](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/AI Orz 端到端测试（Playwright E2E）/编码规范.md>) | 837 |
| 16 | [技术栈](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/技术栈.md>) | 161 |
| 17 | [架构设计](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/架构设计.md>) | 899 |
| 18 | [概述](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/概述.md>) | 179 |
| 19 | [特殊配置与命令](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/特殊配置与命令.md>) | 213 |
| 20 | [编码规范](<knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/编码规范.md>) | 703 |
| 21 | [AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册](<knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md>) | 14,768 |
| 22 | [Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配](<knowledge/zh/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配.md>) | 12,446 |
| 23 | [Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报](<knowledge/zh/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报.md>) | 8,522 |
| 24 | [AgentRuntimeInfo 状态机 + BusyGuard RAII：Idle/Busy/Resting 三态转换 + task_id/project_id 业务上下文 + 前端 runtime-list 过滤](<knowledge/zh/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视.md>) | 17,714 |
| 25 | [Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景 Line/Donut + 仪表盘 Gauge 双版 + HudPalette 橙光光晕](<knowledge/zh/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕.md>) | 48,393 |
| 26 | [ChatMessage::System 消息角色：人设/指令/规则 与 对话内容 分层传递给 Chat Completions API](<knowledge/zh/ChatMessage::System 消息角色：人设·指令·规则 与 对话内容分层传递给 Chat Completions API/ChatMessage::System 消息角色：人设·指令·规则 与 对话内容分层传递给 Chat Completions API.md>) | 4,569 |
| 27 | [Cron 任务调度与系统启动顺序：5 字段 cron 解析 + ensure_system_cron_triggers 2 条注入 + CronTriggerProducer](<knowledge/zh/Cron 任务调度与系统启动顺序：5字段cron解析 + ensure_system_cron_triggers 2条注入 + CronTriggerConsumer 事件 + 两阶段init/aop严格分离/Cron 任务调度与系统启动顺序：5字段cron解析 + ensure_system_cron_triggers 2条注入 + CronTriggerConsumer 事件 + 两阶段init/aop严格分离.md>) | 19,838 |
| 28 | [DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链](<knowledge/zh/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链/DocLinkClassifier 通用分类器 + docs_lint 二进制门禁 + docs_migrate 迁移脚本工具链.md>) | 14,892 |
| 29 | [Doctor 依赖预检](<knowledge/zh/Doctor 依赖预检/Doctor 依赖预检.md>) | 5,849 |
| 30 | [Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅](<knowledge/zh/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅.md>) | 12,294 |
| 31 | [DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表](<knowledge/zh/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表/DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表.md>) | 9,212 |
| 32 | [Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵](<knowledge/zh/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵.md>) | 23,914 |
| 33 | [Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T](<knowledge/zh/Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分/Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分.md>) | 8,485 |
| 34 | [GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页](<knowledge/zh/GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页/GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页.md>) | 15,725 |
| 35 | [HTTP 工具创建表单限定 method 白名单为 GET/POST](<knowledge/zh/HTTP 工具创建表单限定 method 白名单为 GET_POST/HTTP 工具创建表单限定 method 白名单为 GET_POST.md>) | 1,325 |
| 36 | [Handler 宏元数据契约 + ToolPo config/parameters_schema 字段分离](<knowledge/zh/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位.md>) | 14,561 |
| 37 | [IdentityCredentials 身份凭证扩展：WechatIlink 扫码授权 + 凭证整组轮换 + 与通用 CRUD 框架的集成点](<knowledge/zh/IdentityCredentials 身份凭证扩展：授权流程 + QR登录 + 多渠道凭证管理/IdentityCredentials 身份凭证扩展：授权流程 + QR登录 + 多渠道凭证管理.md>) | 11,747 |
| 38 | [InboundState 入站运行状态：动态游标 + 会话滚动刷新 + fail-open 游标损坏恢复](<knowledge/zh/InboundState 入站运行状态：动态游标 + 会话滚动刷新/InboundState 入站运行状态：动态游标 + 会话滚动刷新.md>) | 8,474 |
| 39 | [Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2](<knowledge/zh/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联.md>) | 16,541 |
| 40 | [Lark P2P WS 私信入站（凭证引用解析 + 多渠道按 app_id 聚合 + 用户身份自动映射 + 健康指标）](<knowledge/zh/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标.md>) | 14,140 |
| 41 | [Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle](<knowledge/zh/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md>) | 32,876 |
| 42 | [Mermaid 图表采用 vendor mermaid.js 的独立阶段实现](<knowledge/zh/Mermaid 图表采用 vendor mermaid.js 的独立阶段实现/Mermaid 图表采用 vendor mermaid.js 的独立阶段实现.md>) | 1,738 |
| 43 | [Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖](<knowledge/zh/Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖/Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖.md>) | 8,585 |
| 44 | [PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序](<knowledge/zh/PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序/PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序.md>) | 17,360 |
| 45 | [RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配](<knowledge/zh/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配.md>) | 12,259 |
| 46 | [Rust Workspace + Dioxus WASM 全栈构建与 CI/CD 流水线](<knowledge/zh/Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线/Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线.md>) | 16,346 |
| 47 | [Rust Workspace + NPM 多语言依赖管理（Cargo.lock / package-lock.json / SQLx 离线构建）](<knowledge/zh/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）.md>) | 9,603 |
| 48 | [SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离](<knowledge/zh/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离/SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离.md>) | 7,335 |
| 49 | [Shell 工具全链路：shell_exec 内置执行 + shell_policy 策略拦截 + shell_tool 声明式注册](<knowledge/zh/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行.md>) | 16,836 |
| 50 | [Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt](<knowledge/zh/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断.md>) | 43,427 |
| 51 | [Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格](<knowledge/zh/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格.md>) | 13,494 |
| 52 | [ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板](<knowledge/zh/ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板/ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板.md>) | 6,446 |
| 53 | [UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束](<knowledge/zh/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束/UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束.md>) | 23,072 |
| 54 | [pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）](<knowledge/zh/pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）/pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）.md>) | 13,041 |
| 55 | [pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语](<knowledge/zh/pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语/pkg 基建统一：pkg/http + pkg/process/exec + pkg/ws 单一原语.md>) | 15,222 |
| 56 | [recommend_seed_nodes 种子节点推荐：度数排序（入边+出边）Top N + published 度数持平决胜 + KnowledgeGraph 组件两端复用 + Agent 神经工具免绑定](<knowledge/zh/recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用/recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用.md>) | 16,401 |
| 57 | [scripts/ 归置 + Makefile 统一入口 + 60+ 引用全量修正](<knowledge/zh/scripts 归置 + Makefile 统一入口 + 60+ 引用全量修正/scripts 归置 + Makefile 统一入口 + 60+ 引用全量修正.md>) | 16,712 |
| 58 | [sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）](<knowledge/zh/sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）/sync_seed_skill 预置技能同步工具（运行期数据目录增量导入）.md>) | 8,392 |
| 59 | [三位一体混合搜索（FTS5 + 向量 + 合并排序）](<knowledge/zh/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）.md>) | 14,569 |
| 60 | [任务状态机 + 项目聚合进度追踪：TaskStatus 4 态流转 + execution_plan_result 结构化存储 + TaskGraph](<knowledge/zh/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG.md>) | 13,524 |
| 61 | [共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例](<knowledge/zh/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md>) | 15,427 |
| 62 | [前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发](<knowledge/zh/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发/前端 MarkdownRenderer 接入 DocLinkClassifier JS 桥接：data-repo-href 标注 + 点击拦截站内分发.md>) | 14,054 |
| 63 | [前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层](<knowledge/zh/前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层/前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层.md>) | 13,122 |
| 64 | [向量存储抽象 + Vectorizable trait 统一索引](<knowledge/zh/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity.md>) | 31,037 |
| 65 | [唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发](<knowledge/zh/唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发/唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发.md>) | 9,425 |
| 66 | [基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）](<knowledge/zh/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）/基于 tracing 的结构化日志系统（宏 + 文件滚动 + JSONL 查询）.md>) | 10,550 |
| 67 | [容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像](<knowledge/zh/容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像/容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像.md>) | 9,448 |
| 68 | [小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类](<knowledge/zh/小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类/小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类.md>) | 17,886 |
| 69 | [工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡](<knowledge/zh/工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡/工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡.md>) | 9,387 |
| 70 | [工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询](<knowledge/zh/工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询/工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询.md>) | 4,402 |
| 71 | [工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆](<knowledge/zh/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆.md>) | 9,002 |
| 72 | [工具系统三层调用架构](<knowledge/zh/工具系统三层调用架构/工具系统三层调用架构.md>) | 5,202 |
| 73 | [工具系统三层调用架构：CoreTool trait + Builtin/HTTP/MCP 三协议路由 + register_handler_tool 宏](<knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md>) | 21,237 |
| 74 | [工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复](<knowledge/zh/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复.md>) | 7,159 |
| 75 | [工具输出与安全治理](<knowledge/zh/工具输出与安全治理/工具输出与安全治理.md>) | 14,371 |
| 76 | [底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（LanceDB 默认生产 → InMemory/Hnsw 测试备选）](<knowledge/zh/底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（LanceDB 默认生产 _ InMemory_Hnsw 测试备选）/底层数据库选型架构决策：SQLite 业务 + DuckDB 统计 + FTS5 关键词 + 向量可插拔（InMemory_Hnsw 默认 _ LanceDB 大规模）.md>) | 11,146 |
| 77 | [微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程](<knowledge/zh/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程.md>) | 23,306 |
| 78 | [思考循环异常退出兜底总结与错误上报结构化](<knowledge/zh/思考循环异常退出兜底总结与错误上报结构化/思考循环异常退出兜底总结与错误上报结构化.md>) | 4,585 |
| 79 | [思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件](<knowledge/zh/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件.md>) | 13,042 |
| 80 | [思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路](<knowledge/zh/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路.md>) | 7,173 |
| 81 | [数据库迁移管理](<knowledge/zh/数据库迁移管理/数据库迁移管理.md>) | 5,960 |
| 82 | [时间戳规范](<knowledge/zh/时间戳规范/时间戳规范.md>) | 5,156 |
| 83 | [本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查](<knowledge/zh/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查/本体属性层：ontology_properties TBox 第四张词表 + knowledge_property EAV 版本化 ABox + 四类型列软门禁写入 + property_filters 结构化反查.md>) | 18,232 |
| 84 | [本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账](<knowledge/zh/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账.md>) | 21,504 |
| 85 | [模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片](<knowledge/zh/模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片/模型提供商信息气泡：浮层定位工具复用 + ModelProviderBubble 懒加载卡片.md>) | 7,480 |
| 86 | [模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析](<knowledge/zh/模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析/模型访问模式 access_mode：Stream 默认 + NonStream 兼容 + resolve_access_mode 兜底 + 非流式单 JSON 解析.md>) | 11,152 |
| 87 | [测试与质量工程：974测试100%通过 + 984后端82前端58common + 87集成测试19targets + cargo-llvm-cov PR38%/main45%门槛 + clippy -D warnings零容忍 + Playwright E2E](<knowledge/zh/测试与质量工程：1124测试100%通过 + 984后端82前端 + 87集成测试19targets + cargo-llvm-cov 38%/45%门槛 + clippy零容忍+Playwright E2E/测试与质量工程：1124测试100%通过 + 984后端82前端 + 87集成测试19targets + cargo-llvm-cov 38%/45%门槛 + clippy零容忍+Playwright E2E.md>) | 23,140 |
| 88 | [消息交互与SSE推送：MessageDomain delivery+management 双能力 + AgentLoopConsumer 循环驱动 +](<knowledge/zh/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类.md>) | 33,259 |
| 89 | [消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级](<knowledge/zh/消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级/消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级.md>) | 8,422 |
| 90 | [消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期](<knowledge/zh/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期.md>) | 36,420 |
| 91 | [用户维度工作区路径改造 + 工具调用安全边界检查（路径逃逸 / 跨用户访问 / 相对路径阻断）](<knowledge/zh/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断.md>) | 19,818 |
| 92 | [知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出](<knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md>) | 14,989 |
| 93 | [知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头](<knowledge/zh/知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头/知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头.md>) | 23,413 |
| 94 | [知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导](<knowledge/zh/知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导/知识图谱联想与关系边补写：update_memory relations 唯一建边入口 + 正文引用对遍历不可达 + 沉淀探索引导.md>) | 16,205 |
| 95 | [种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离](<knowledge/zh/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发/种子配置与系统两阶段初始化：5 套 TEMPLATE_SKILL 编译期嵌入 + seed diff 增量导入 + 两阶段 init aop 严格分离 + init_all_base_data 域派发.md>) | 30,395 |
| 96 | [策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层](<knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层.md>) | 14,809 |
| 97 | [组织权限与用户偏好：Organization 多级组织 + UserRole 并查集继承 + JWT Cookie+Bearer 双模式 + 偏好双源沉淀 + Agent入职流程](<knowledge/zh/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步.md>) | 27,487 |
| 98 | [统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段](<knowledge/zh/统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段/统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段.md>) | 4,221 |
| 99 | [统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系](<knowledge/zh/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系.md>) | 10,812 |
| 100 | [统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示](<knowledge/zh/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示/统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示.md>) | 20,364 |
| 101 | [聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成](<knowledge/zh/聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成/聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成.md>) | 8,226 |
| 102 | [聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文](<knowledge/zh/聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文/聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文.md>) | 5,988 |
| 103 | [联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断](<knowledge/zh/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断.md>) | 13,456 |
| 104 | [联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接](<knowledge/zh/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接.md>) | 31,616 |
| 105 | [脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏](<knowledge/zh/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏.md>) | 5,721 |
| 106 | [记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 度数排序推荐](<knowledge/zh/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md>) | 14,374 |
| 107 | [记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开](<knowledge/zh/记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开/记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开.md>) | 6,321 |
| 108 | [跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌](<knowledge/zh/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities/跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities.md>) | 20,988 |
| 109 | [身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用](<knowledge/zh/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用.md>) | 13,030 |
| 110 | [身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式](<knowledge/zh/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式.md>) | 11,404 |
| 111 | [身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立](<knowledge/zh/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立.md>) | 11,729 |
| 112 | [身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail](<knowledge/zh/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）.md>) | 16,369 |
| 113 | [运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化](<knowledge/zh/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化.md>) | 13,100 |
| 114 | [进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤](<knowledge/zh/进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤/进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤.md>) | 1,984 |
| 115 | [进程详情采用共享组件 + Modal 弹窗，不新建独立路由页](<knowledge/zh/进程详情采用共享组件 + Modal 弹窗，不新建独立路由页/进程详情采用共享组件 + Modal 弹窗，不新建独立路由页.md>) | 1,675 |
| 116 | [通过 common DTO 补全 Project/Task execution_plan/result 读取链路](<knowledge/zh/通过 common DTO 补全 Project_Task execution_plan_result 读取链路/通过 common DTO 补全 Project_Task execution_plan_result 读取链路.md>) | 1,705 |
| 117 | [邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型](<knowledge/zh/邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型/邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型.md>) | 11,451 |
| 118 | [配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入](<knowledge/zh/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入.md>) | 14,845 |
| 119 | [附件存储与DTO协议统一：Attachment Finance域统一资产 + PagedResult T map 全链路 + common::api 单一事实源 + count与query 复用 WHERE](<knowledge/zh/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE.md>) | 15,851 |
| 120 | [项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合](<knowledge/zh/项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合/项目领域与制品聚合：ProjectService 六能力 + TaskGraph DAG 依赖编排 + Artifact 制品双关联 + 对话上下文聚合.md>) | 9,086 |
