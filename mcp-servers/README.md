# MCP 通用子包代理框架（阶段 1）

MCP 通用代理框架——配置外置驱动子包加载（AMan 定调：「根据不同的配置文件加载不同的子包」），
腾讯 ima 为首个子系统实例。

> **本文档的定位**：阶段 2 SkillHub 集成（slug 联动安装 / 包内容审计 / 多包并发）明确暂不做，
> 候 AMan 明示需要时再启。在此之前，**新子包接入 = 按本指南手动四步**——本文档即「文档铺路」的承载。

- 唯一执行依据：评估工件 `01a157ce-e111`（ima-mcp-universal-proxy-framework-assessment.md，v2 终态）§2.3/§2.7
- 承阶段 0 单包封装（commit=`5bc379bd`）与阶段 1 通用化（commit=`f8dcfba8`）
- 形态：零依赖 Node stdio MCP server（单文件 `server.js` 通用框架，node >= 18）
- 测试：`node --test test/server.test.js`（零依赖 node:test，fixtures 受控 stub，零真实网络零真实凭证）

## 布局

```
mcp-servers/
├── server.js            # 通用框架：配置加载 + 协议处理 + 三类工具执行器（子包差异零硬编码）
├── configs/
│   └── ima.json         # ima 子包配置（六要素：subpkg/source/install_dir/runtime/entry/tools）
└── test/
    ├── server.test.js   # 配置层+单元层+协议层+纪律层测试（32 例）
    └── fixtures/pkg/    # 受控 stub（fake_mode=ok/error/update 复刻官方 ima_api.cjs 错误面）
```

**新子包接入 = 新增 `configs/<subpkg>.json` 一个配置文件，框架零改动**（流程见下文「新子包接入指南」）。

## 工作原理

### 配置加载链

`resolveConfigName`（argv[2] 合法 token 优先 → env `MCP_SUBPKG_CONFIG` → 默认 `ima`）
→ `loadConfig`（JSON 解析 + `validateConfig` 结构校验 fail-fast + installDir 解析）
→ 启动映射一致性校验（入口脚本 + 全部白名单文档存在性，缺件退出非 0——评估 §2.3
v1.1.9 缺 note-images.cjs 教训条款）→ stdio 主循环。

### tools 三类型

| type | 行为 | 关键字段 |
|------|------|----------|
| `api` | spawn 子进程执行 entry 脚本（argv=[apiPath, bodyJson, optionsJson]，stdout=响应 / stderr=结构化错误 JSON） | `apiPath` / `inputSchema` / `readonly` |
| `usage` | 能力面方法①：清单自动生成 + 使用说明书全文（不触子进程） | `moduleEnum`（可选，组装 module 参数 enum） |
| `detail` | 能力面方法②：按白名单读文档全文（doc enum 由 `docs` 单一事实源生成，防双份维护漂移） | — |

spawn 隔离为结构性必需：官方脚本在 -200（skill 需更新）时直接 `process.stdout.write`
更新上下文，require 进本进程会污染 stdio JSON-RPC 通道。

### 能力面两类方法（AMan 定调①②，勿翻案）

方法① `<subpkg>_usage` 工具 + MCP 原生 `resources/list`；方法② `<subpkg>_detail` 工具 +
MCP 原生 `resources/read`（`<baseUri><rel>` URI 白名单读取）。Agent 读后自行总结沉淀，
平台不代管学习质量；无 sync 字段、无主动同步（分发面明确不做）。

### 凭证链路

平台经 Stdio+Env binding 按配置 `credentials` 声明向本进程注入 env（键名仅存于配置文件，
框架源码零凭证键名字面量零凭证值），本进程按声明透传给子进程；argv 只携带 apiPath 与
body，凭证零 argv 暴露。ima 占位符口径：`<IMA_CLIENT_ID>` / `<IMA_API_KEY>`（见 configs/ima.json）。

### 安全边界

resources/能力面读取 = 文档白名单（配置 docs）+ URI 前缀 + 安装目录前缀双重校验防路径穿越；
配置结构校验 fail-fast；写操作工具 `readOnlyHint=false` 提示执行前确认。

## 配置字段参考（configs/<subpkg>.json）

### 顶层字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `subpkg` | string | ✓ | 子包标识，即 configs/ 下文件名（去掉 .json），合法 token `[A-Za-z0-9_-]` |
| `serverName` | string | ✓ | MCP `initialize` 响应中 serverInfo.name |
| `source` | string | — | 来源记录（如 `skillhub:@org/slug`）；缺省由框架置 null |
| `installDir` | string | 二选一 | 技能包安装目录，支持 `~` 展开 |
| `installDirEnv` | string | 二选一 | 环境变量名优先于 installDir（测试/多环境注入点；无值时回退 installDir） |
| `runtime` | string | ✓ | 暂仅 `"node"`（其他执行器候第二子包出现时校准，不预先虚构抽象） |
| `entry` | string | ✓ | 入口脚本相对 installDir 的路径（如 `ima_api.cjs`）；api 工具 spawn 的执行对象 |
| `baseUri` | string | ✓ | resources URI 前缀（如 `ima://skill/`），读取时拼 `<baseUri><rel>` |
| `tools` | array | ✓ | 非空，工具映射表（见下） |
| `docs` | array | ✓ | 非空，文档白名单（resources 面）；`rel` 禁止 `..` 与 `/` 开头 |
| `credentials` | array | ✓ | 可为空数组；凭证声明（见下） |

### tools 项（按 type 三选一）

- 公共字段：`name`（全配置唯一）、`description`。
- `type: "api"`：业务工具。必填 `apiPath`（接口路径，透传给 entry 脚本）、`inputSchema`
  （JSON Schema，type 须为 object，镜像包内 api 文档逐字段）、`readonly`（布尔；写操作
  false，提示执行前用户确认）。
- `type: "usage"`：能力面方法①。可选 `moduleEnum`（字符串数组，组装 module 参数 enum）、
  `moduleDescription`；禁止声明 `apiPath`。
- `type: "detail"`：能力面方法②。无额外字段（doc enum 自动取自 docs）；禁止声明 `apiPath`。

### docs 项

`rel`（相对 installDir 的路径，即白名单与 URI 后缀）、`name`（展示名）、`desc`（一句话说明，
进 resources/list 与 usage 清单）。

### credentials 项

四字段全部必填：`env`（注入的进程环境变量名）、`platformKind`（平台凭证类型，如
`generic_token`）、`platform`（平台标识）、`placeholder`（占位符，如 `<IMA_CLIENT_ID>`）。
**仅声明键名与占位符，严禁出现任何凭证值**——纪律红线，测试有断言。

## 新子包接入指南（手动四步）

### 前置条件

- node >= 18（零第三方依赖）；
- 目标技能包已安装落位（下一步）；如需全新安装：`skillhub install @org/slug --dir ~/.skillhub/skills`
  （CLI 直调；平台侧等价走 skillhub_cli Builtin 工具，command=`install @org/slug`）；
- 通读目标包内 SKILL.md 与 api 文档，确定：入口脚本文件名、接口清单与参数、说明书文档清单。

### 第 1 步 · 装包并核对落位

```bash
# 核对 installDir 下「entry + 全部白名单文档」真实存在（缺件将在启动校验 fail-fast）
ls ~/.skillhub/skills/@org/<slug>/
```

### 第 2 步 · 编写 configs/<subpkg>.json

最小骨架（api 型子包，能力面工具框架自动组装）：

```json
{
  "subpkg": "demo",
  "serverName": "demo-mcp-proxy",
  "source": "skillhub:@org/demo-skills",
  "installDir": "~/.skillhub/skills/@org/demo-skills",
  "runtime": "node",
  "entry": "api.cjs",
  "baseUri": "demo://skill/",
  "credentials": [],
  "docs": [
    { "rel": "SKILL.md", "name": "demo 使用说明", "desc": "入口说明书" }
  ],
  "tools": [
    { "type": "usage", "name": "demo_usage", "description": "方法①·获取使用说明+技能列表" },
    { "type": "detail", "name": "demo_detail", "description": "方法②·读取白名单文档全文" }
  ]
}
```

业务接口按需追加 `api` 工具（schema 逐字段镜像包内 api 文档，未知参数放行由框架容错）：

```json
{ "type": "api", "name": "demo_foo", "apiPath": "v1/foo", "readonly": true,
  "description": "一句话说清能力与是否为读取操作",
  "inputSchema": { "type": "object", "properties": { "q": { "type": "string", "description": "..." } }, "required": ["q"] } }
```

完整活例：`configs/ima.json`（18 tools = 16 api + usage + detail，5 docs，2 credentials）。
凭证接入时在 `credentials` 声明四字段（仅键名+占位符），并在下方「平台注册」中一一对应。

### 第 3 步 · 配置校验与启动冒烟

```bash
cd mcp-servers
# ① 结构校验：非法字段/缺件立即报错（fail-fast），合法输出解析结果
node -e 'const { loadConfig } = require("./server.js"); const c = loadConfig("demo"); console.log("OK", c.subpkg, c.installDir, c.tools.length, "tools");'
# ② 启动冒烟：映射一致性校验（缺入口脚本/白名单文档退出非 0）+ initialize 握手
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | node server.js demo
# 期望 stdout 输出 serverInfo.name = "<serverName>" 的 initialize 响应
```

### 第 4 步 · 回归验证

```bash
node --test test/server.test.js   # 期望 32/32（既有基准零回归）
```

测试以 ima 配置与 fixtures 为既有基准，不要求新子包已安装；新配置文件不影响测试
（resolveConfigName 三级回退默认 ima）。

### （可选）平台注册（MCP server，Stdio 传输）

以 ima 为例，`credential_requirements` 与配置 `credentials` 一一对应：

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

第二子包注册同理：`args` 追加子包名（或用 env `MCP_SUBPKG_CONFIG`），installDirEnv
声明各自环境变量名，框架零改动。

### 接入 checklist

- [ ] node >= 18；技能包已落位 installDir 且 entry + docs 全部存在
- [ ] `configs/<subpkg>.json` 顶层字段齐备（installDir 与 installDirEnv 至少其一）
- [ ] api 工具 schema 逐字段镜像包内 api 文档；写操作 `readonly=false`
- [ ] credentials 仅 env 键名 + 占位符，零凭证值（红线）
- [ ] loadConfig 校验 OK + initialize 冒烟 serverInfo.name 正确
- [ ] `node --test` 32/32 零回归
- [ ] （平台接入）credential_requirements 与配置 credentials 一一对应
- [ ] 恰整一笔 commit，产物挂任务

## ima 子包速览（首个实例）

- 安装落位：`~/.skillhub/skills/@tencent-adm/ima-skills`（B 项 skillhub_cli 落位），
  可用环境变量 `IMA_SKILL_INSTALL_DIR` 覆盖（测试即用此机制指向 fixtures）。
- 工具面：18 tools = 16 api（openapi/note/v1 六接口 + openapi/wiki/v1 十接口，写操作五件
  `readOnlyHint=false`）+ `ima_usage`（方法①）+ `ima_detail`（方法②）。
- 资源面：`resources/list|read`，5 件白名单文档（`ima://skill/` 前缀）。
- 凭证：`<IMA_CLIENT_ID>` / `<IMA_API_KEY>` 占位符口径，env 透传零明文。

## 验证状态（如实标注）

- 配置面/协议面/能力面/校验面/纪律面：32 例 node:test 全过（fixtures 受控 stub）。
- 启动冒烟：initialize 握手实测 PASS（serverInfo=ima-mcp-proxy v0.2.0）。
- ima 真实直调冒烟：**未做**——需真实凭证 `<IMA_CLIENT_ID>` / `<IMA_API_KEY>`
  （候 AMan 自助获取 https://ima.qq.com/agent-interface，前置微信登录），框架建设不被凭证阻塞。
- 16 接口参数 schema 镜像自包内 api.md（v1.1.9 站内版）；官方接口字段演进时未知参数放行（容错口径）。

## 边界（不做清单）

- 阶段 2 SkillHub 集成（slug 联动安装/包内容审计/多包并发）暂不做，候 AMan 需要时明示再启；
  新子包在此之前一律走本指南手动四步。
- 分发面不做：无 sync 字段、无主动同步、无平台 skill 库导入链/版本检测/安全闸门建设（AMan 定调）。
- 「自动更新联动」不纳入。
- COS 上传工作流（preflight-check.cjs / cos-upload.cjs）不进 tools 面：属本地文件操作，
  由 Agent 经能力面说明书（knowledge-base/SKILL.md 安全门章节）自学执行；tools 面=API 代理非文件代理。
- 第二子包未出现前不预先虚构抽象（评估 v2 演进路径）：options 第三参形态、非 node runtime 等
  差异候第二子包出现时再配置化。
