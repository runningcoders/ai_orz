# ima MCP 代理 server（阶段 0 单包封装）

ima OpenAPI 技能包（SkillHub `@tencent-adm/ima-skills`）的 MCP 代理封装——MCP 通用代理框架首个实例。

- 唯一执行依据：评估工件 `01a157ce-e111`（ima-mcp-universal-proxy-framework-assessment.md，v2 终态）§2.3
- 形态：零依赖 Node stdio MCP server（单文件 `server.js`，node >= 18）
- 测试：`node --test test/server.test.js`（零依赖 node:test，fixtures 受控 stub，零真实网络零真实凭证）

## 架构口径

1. **tools 业务面**：16 个 ima OpenAPI 接口的具名 schema 映射（镜像包内 api.md 字段表）。每次调用
   spawn 子进程执行官方 `ima_api.cjs`（stdout=响应 / stderr=结构化错误 JSON），零协议重写零函数级适配。
   spawn 隔离为结构性必需：官方脚本在 -200（skill 需更新）时直接 `process.stdout.write` 更新上下文，
   require 进本进程会污染 stdio JSON-RPC 通道。
2. **能力面两类方法**（AMan 定调口径）：
   - 方法① 获取使用说明+技能列表：`ima_usage` 工具（清单自动生成+说明书全文）+ MCP 原生 `resources/list`（协议标准路径）；
   - 方法② 获取技能详情：`ima_detail` 工具 + MCP 原生 `resources/read`（`ima://skill/<rel>` URI 白名单读取）。
   - Agent 读后自行总结沉淀，平台不代管学习质量；无 sync 字段、无主动同步（分发面明确不做）。
3. **凭证链路**：平台经 Stdio+Env binding 向本进程注入 `IMA_CLIENT_ID` / `IMA_API_KEY`
   （kind=generic_token，platform=ima；平台录入对应 `<IMA_CLIENT_ID>` / `<IMA_API_KEY>` 占位符），
   本进程 env 透传给子进程；argv 只携带 apiPath 与 body，凭证零 argv 暴露。本目录零凭证明文。
4. **安全边界**：resources 读取=文档白名单（v1.1.9 包内实锚 5 件）+ URI 前缀 + 安装目录前缀双重校验；
   启动映射一致性校验（入口脚本+白名单文档存在性）fail-fast；写操作工具 `readOnlyHint=false` 提示执行前确认。

## 平台注册示例（MCP server，Stdio 传输）

```json
{
  "name": "ima-mcp-proxy",
  "transport": "stdio",
  "command": "node",
  "args": ["/Users/aman/Technology/rust/ai_orz/mcp-servers/ima/server.js"],
  "credential_requirements": [
    { "kind": "generic_token", "platform": "ima", "binding": { "type": "env", "name": "IMA_CLIENT_ID" } },
    { "kind": "generic_token", "platform": "ima", "binding": { "type": "env", "name": "IMA_API_KEY" } }
  ]
}
```

技能包安装目录默认 `~/.skillhub/skills/@tencent-adm/ima-skills`（B 项 skillhub_cli 落位），
可用环境变量 `IMA_SKILL_INSTALL_DIR` 覆盖（测试即用此机制指向 fixtures）。

## 验证状态（如实标注）

- 协议面/能力面/校验面：24 例 node:test 全过（fixtures 受控 stub）。
- ima 真实直调冒烟：**未做**——需真实凭证 `<IMA_CLIENT_ID>` / `<IMA_API_KEY>`
  （候 AMan 自助获取 https://ima.qq.com/agent-interface，前置微信登录），框架建设不被凭证阻塞。
- 16 接口参数 schema 镜像自包内 api.md（v1.1.9 站内版）；官方接口字段演进时未知参数放行（容错口径）。

## 边界（不做清单）

- 分发面不做：无 sync 字段、无主动同步、无平台 skill 库导入链/版本检测/安全闸门建设（AMan 定调）。
- COS 上传工作流（preflight-check.cjs / cos-upload.cjs）不进 tools 面：属本地文件操作，
  由 Agent 经能力面说明书（knowledge-base/SKILL.md 安全门章节）自学执行；tools 面=API 代理非文件代理。
- 阶段 1 通用化（配置外置 config.json + loader）本单不做：单包内嵌常量，`CONFIG` 单点即抽起点。
