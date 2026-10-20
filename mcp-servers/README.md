# MCP 通用子包代理框架（阶段 1）

MCP 通用代理框架——配置外置驱动子包加载（AMan 定调：「根据不同的配置文件加载不同的子包」），
腾讯 ima 为首个子系统实例。

- 唯一执行依据：评估工件 `01a157ce-e111`（ima-mcp-universal-proxy-framework-assessment.md，v2 终态）§2.3/§2.7
- 承阶段 0 单包封装（commit=`5bc379bd`，mcp-servers/ima/ 已整体迁移为框架布局）
- 形态：零依赖 Node stdio MCP server（单文件 `server.js` 通用框架，node >= 18）
- 测试：`node --test test/server.test.js`（零依赖 node:test，fixtures 受控 stub，零真实网络零真实凭证）

## 布局

```
mcp-servers/
├── server.js            # 通用框架：配置加载 + 协议处理 + 三类工具执行器（子包差异零硬编码）
├── configs/
│   └── ima.json         # ima 子包配置（六要素：subpkg/source/install_dir/runtime/entry/tools）
└── test/
    ├── server.test.js   # 配置层+单元层+协议层+纪律层测试
    └── fixtures/pkg/    # 受控 stub（fake_mode=ok/error/update 复刻官方 ima_api.cjs 错误面）
```

**新子包接入 = 新增 `configs/<subpkg>.json` 一个配置文件，框架零改动。**

## 配置六要素（评估 v2 §2.3）

`subpkg`（子包标识）/ `source`（来源 slug 或 URL）/ `install_dir`（安装落位，支持 `~` 展开；
`installDirEnv` 环境变量优先）/ `runtime`（暂仅 node）/ `entry`（入口脚本）/
`tools`（映射表：`api` 业务工具=apiPath+schema+readonly；`usage` 能力面方法①；`detail` 能力面方法②）+
`docs`（文档白名单=resources 面）+ `credentials`（env 键名声明，仅键名与占位符，零凭证值）。

配置加载链：`resolveConfigName`（argv[2] 合法 token 优先 → env `MCP_SUBPKG_CONFIG` → 默认 `ima`）
→ `loadConfig`（结构校验 fail-fast + installDir 解析）→ 启动映射一致性校验（入口脚本+白名单文档
存在性，缺件退出非 0，评估 §2.3 v1.1.9 缺 note-images.cjs 教训条款）→ 主循环。

## 架构口径

1. **tools 三类型**：`api`=spawn 子进程执行 entry 脚本（argv=[apiPath, bodyJson, optionsJson]，
   stdout=响应/stderr=结构化错误 JSON）；`usage`=清单自动生成+说明书全文（方法①）；`detail`=白名单
   文档全文读取（方法②）。detail 的 doc enum 由 docs 单一事实源生成，防双份维护漂移。
   spawn 隔离为结构性必需：官方脚本在 -200（skill 需更新）时直接 `process.stdout.write` 更新上下文，
   require 进本进程会污染 stdio JSON-RPC 通道。
2. **能力面两类方法**（AMan 定调①②，勿翻案）：方法① `ima_usage` + MCP 原生 `resources/list`；
   方法② `ima_detail` + MCP 原生 `resources/read`（`<baseUri><rel>` URI 白名单读取）。
   Agent 读后自行总结沉淀，平台不代管学习质量；无 sync 字段、无主动同步（分发面明确不做）。
3. **凭证链路**：平台经 Stdio+Env binding 按配置 `credentials` 声明向本进程注入 env（键名仅存于
   配置文件，框架源码零凭证键名字面量零凭证值），本进程按声明透传给子进程；argv 只携带 apiPath
   与 body，凭证零 argv 暴露。ima 占位符口径：`<IMA_CLIENT_ID>` / `<IMA_API_KEY>`（见 configs/ima.json）。
4. **安全边界**：resources/能力面读取=文档白名单（配置 docs）+ URI 前缀 + 安装目录前缀双重校验
   防路径穿越；配置结构校验 fail-fast；写操作工具 `readOnlyHint=false` 提示执行前确认。

## 平台注册示例（MCP server，Stdio 传输）

```json
{
  "name": "ima-mcp-proxy",
  "transport": "stdio",
  "command": "node",
  "args": ["/Users/aman/Technology/rust/ai_orz/mcp-servers/server.js"],
  "credential_requirements": [
    { "kind": "generic_token", "platform": "ima", "binding": { "type": "env", "name": "IMA_CLIENT_ID" } },
    { "kind": "generic_token", "platform": "ima", "binding": { "type": "env", "name": "IMA_API_KEY" } }
  ]
}
```

技能包安装目录默认 `~/.skillhub/skills/@tencent-adm/ima-skills`（B 项 skillhub_cli 落位），
可用环境变量 `IMA_SKILL_INSTALL_DIR` 覆盖（测试即用此机制指向 fixtures）。
第二子包接入时：新配置文件中 `installDirEnv` 声明各自的环境变量名，框架零改动。

## 验证状态（如实标注）

- 配置面/协议面/能力面/校验面/纪律面：33 例 node:test 全过（fixtures 受控 stub）。
- ima 真实直调冒烟：**未做**——需真实凭证 `<IMA_CLIENT_ID>` / `<IMA_API_KEY>`
  （候 AMan 自助获取 https://ima.qq.com/agent-interface，前置微信登录），框架建设不被凭证阻塞。
- 16 接口参数 schema 镜像自包内 api.md（v1.1.9 站内版）；官方接口字段演进时未知参数放行（容错口径）。

## 边界（不做清单）

- 分发面不做：无 sync 字段、无主动同步、无平台 skill 库导入链/版本检测/安全闸门建设（AMan 定调）。
- COS 上传工作流（preflight-check.cjs / cos-upload.cjs）不进 tools 面：属本地文件操作，
  由 Agent 经能力面说明书（knowledge-base/SKILL.md 安全门章节）自学执行；tools 面=API 代理非文件代理。
- 第二子包未出现前不预先虚构抽象（评估 v2 演进路径）：options 第三参形态、非 node runtime 等
  差异候第二子包出现时再配置化；阶段 2 SkillHub 集成（1~2 人日可选）候 AMan 表态。
