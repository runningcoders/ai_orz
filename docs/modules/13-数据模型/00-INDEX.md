<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 13-数据模型

> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。
>
> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。

本模块共 48 篇：A 设计 5 · B 落地方案 4 · C 知识 39。

## A 设计（design）

- [前后端 API 协议规范（common DTO 单一事实源）](<A-设计/api_protocol_convention.md>) — `现行`
- [产物与消息附件统一存储设计](<A-设计/attachment_storage.md>) — `已废弃`
- [查询接口分页与通用 count 规范](<A-设计/pagination_and_count_convention.md>) — `现行`
- [时间戳处理约定](<A-设计/timestamp_convention.md>) — `现行`
- [统一参数 IDL + 自动生成 HTTP handler 设计文档](<A-设计/unified_idl_http_handler.md>) — `已废弃`

## B 落地方案（plan）

- [Handler 管理面 API 补齐方案](<B-落地方案/Handler 管理面 API 补齐方案.md>) — `已完成`
- [Query 接口分页与 List 接口简化重构](<B-落地方案/Query接口分页与List接口简化重构.md>) — `已完成`
- [前端 API 协议结构体统一改造](<B-落地方案/前端API协议结构重构.md>) — `已完成`
- [批量查询与通用Query接口增强重构](<B-落地方案/批量查询与通用Query接口增强重构.md>) — `已完成`

## C 知识（wiki）

- [API 参考](<C-知识/API 参考.md>)
- [Agent 和技能模型](<C-知识/Agent 和技能模型.md>)
- [Agent 实体](<C-知识/Agent 实体.md>)
- [DAO 层多态](<C-知识/DAO 层多态.md>)
- [HNSW向量存储](<C-知识/HNSW向量存储.md>)
- [Handler 宏元数据契约 + ToolPo config/parameters_schema 字段分离](<C-知识/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位.md>)
- [LanceDB向量存储](<C-知识/LanceDB向量存储.md>)
- [Mermaid 图表采用 vendor mermaid.js 的独立阶段实现](<C-知识/Mermaid 图表采用 vendor mermaid.js 的独立阶段实现/Mermaid 图表采用 vendor mermaid.js 的独立阶段实现.md>)
- [SQLite VSS向量存储](<C-知识/SQLite VSS向量存储.md>)
- [SQLite 数据访问对象](<C-知识/SQLite 数据访问对象.md>)
- [三位一体混合搜索（FTS5 + 向量 + 合并排序）](<C-知识/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）.md>)
- [全文搜索（代码落地层）](<C-知识/全文搜索.md>)
- [内存向量存储](<C-知识/内存向量存储.md>)
- [制品和附件](<C-知识/制品和附件.md>)
- [向量存储（代码落地层）](<C-知识/向量存储.md>)
- [向量存储抽象接口（代码落地层）](<C-知识/向量存储抽象接口.md>)
- [向量存储数据访问对象](<C-知识/向量存储数据访问对象.md>)
- [外部 API 数据访问对象](<C-知识/外部 API 数据访问对象.md>)
- [存储架构设计](<C-知识/存储架构设计.md>)
- [存储系统（代码落地层）](<C-知识/存储系统.md>)
- [数据对象层（DAO）](<C-知识/数据对象层 (DAO).md>)
- [数据模型](<C-知识/数据模型.md>)
- [数据访问层 (DAL)](<C-知识/数据访问层 (DAL).md>)
- [文件存储模型](<C-知识/文件存储模型.md>)
- [核心实体模型](<C-知识/核心实体模型.md>)
- [消息和记忆模型](<C-知识/消息和记忆模型.md>)
- [消息系统（数据模型层）](<C-知识/消息系统.md>)
- [消息系统数据访问对象](<C-知识/消息系统数据访问对象.md>)
- [系统模型](<C-知识/系统模型.md>)
- [统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系](<C-知识/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系/统一错误模型：ErrorCode + ErrorType + ErrorField 的跨层错误处理体系.md>)
- [统计数据分析对象](<C-知识/统计数据分析对象.md>)
- [记忆和向量系统](<C-知识/记忆和向量系统.md>)
- [请求上下文](<C-知识/请求上下文.md>)
- [财务领域](<C-知识/财务领域.md>)
- [身份凭证管理（统一 Domain CRUD + AES-256-GCM 加密存储 + 生命周期联动）](<C-知识/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md>)
- [错误处理与状态码](<C-知识/错误处理与状态码.md>)
- [附件存储与DTO协议统一：Attachment Finance域统一资产 + PagedResult T map 全链路 + common::api 单一事实源 + count与query 复用 WHERE](<C-知识/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE.md>)
- [项目管理（数据模型层）](<C-知识/项目管理.md>)
- [飞书集成系统](<C-知识/飞书集成系统.md>)
