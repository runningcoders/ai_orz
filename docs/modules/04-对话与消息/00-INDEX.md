<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 04-对话与消息

> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。
>
> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。

本模块共 37 篇：A 设计 7 · B 落地方案 9 · C 知识 21。

## A 设计（design）

- [Agent 间协作压力（链深度 + 信息疲劳度）可复用组件设计](<A-设计/collaboration_pressure_design.md>) — `现行`
- [飞书（Lark）渠道集成设计 —— 长连接协议对齐版](<A-设计/lark_channel_integration_design.md>) — `现行`
- [lark-cli 集成与飞书渠道多应用化设计](<A-设计/lark_cli_integration.md>) — `已废弃`
- [消息渠道设计文档](<A-设计/message_channel_design.md>) — `已废弃`
- [用户-Agent 消息交互设计](<A-设计/message_interaction_design.md>) — `已废弃`
- [消息撤回与取消能力设计](<A-设计/message_recall_design.md>) — `现行`
- [微信渠道接入设计（iLink ClawBot / 企微智能机器人）](<A-设计/wechat_channel_integration_design.md>) — `草案`

## B 落地方案（plan）

- [SSE 消息推送实现计划](<B-落地方案/SSE 消息推送实现计划（执行蓝图）.md>) — `已完成`
- [lark-cli 集成二期：绑定体验 + 用户身份 + WS 稳定性](<B-落地方案/lark-cli_集成二期.md>) — `已完成`
- [微信 iLink 协议对齐重构方案](<B-落地方案/微信iLink协议对齐重构方案.md>) — `进行中`
- [消息撤回与取消能力落地](<B-落地方案/消息撤回与取消能力.md>) — `已完成`
- [聊天MVP](<B-落地方案/聊天MVP.md>) — `已完成`
- [飞书P2P消息集成](<B-落地方案/飞书P2P消息集成.md>) — `已完成`
- [飞书出站引用回复 Implementation Plan](<B-落地方案/飞书出站引用回复方案.md>) — `已完成`
- [飞书链路对齐审查](<B-落地方案/飞书链路对齐审查.md>) — `已完成`
- [飞书链路对齐重构方案](<B-落地方案/飞书链路对齐重构方案.md>) — `进行中`

## C 知识（wiki）

- [@提及功能（mention）](<C-知识/@提及功能（mention）.md>)
- [@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染](<C-知识/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染.md>)
- [AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容](<C-知识/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容.md>)
- [Agent消息集成](<C-知识/Agent消息集成.md>)
- [ChatMessage::System 消息角色：人设/指令/规则 与 对话内容 分层传递给 Chat Completions API](<C-知识/ChatMessage::System 消息角色：人设·指令·规则 与 对话内容分层传递给 Chat Completions API/ChatMessage::System 消息角色：人设·指令·规则 与 对话内容分层传递给 Chat Completions API.md>)
- [InboundState 入站运行状态：动态游标 + 会话滚动刷新 + fail-open 游标损坏恢复](<C-知识/InboundState 入站运行状态：动态游标 + 会话滚动刷新/InboundState 入站运行状态：动态游标 + 会话滚动刷新.md>)
- [WebSocket 与 SSE](<C-知识/WebSocket 与 SSE.md>)
- [多渠道消息系统](<C-知识/多渠道消息系统.md>)
- [实时推送](<C-知识/实时推送.md>)
- [微信 iLink 专属渠道](<C-知识/微信 iLink 专属渠道.md>)
- [微信 iLink 专属渠道闭环（wechat_dal + ilink_dao + inbound_state + 授权流程）](<C-知识/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程.md>)
- [消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类](<C-知识/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类.md>)
- [消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级](<C-知识/消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级/消息出站引用回复：thread_id 继承 + ReplyTarget 反查父消息 + 飞书 reply 端点 + 静默降级.md>)
- [消息处理核心](<C-知识/消息处理核心.md>)
- [消息渠道入站适配中台（MessageInboundAdapter + Registry + 生命周期）](<C-知识/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期.md>)
- [消息渠道管理（业务功能层）](<C-知识/消息渠道管理.md>)
- [消息渠道适配器](<C-知识/消息渠道适配器.md>)
- [消息管理](<C-知识/消息管理.md>)
- [消息系统（业务功能层）](<C-知识/消息系统.md>)
- [消息领域](<C-知识/消息领域.md>)
- [邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型](<C-知识/邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型/邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型.md>)
