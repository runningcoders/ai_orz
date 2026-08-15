# 浏览器操作验证用例设计

> 🎯 **本文档定位**：面向浏览器的可读/可执行/可复现的验证用例体系设计——脚本驱动 + Agent 驱动双模式共享同一份 Playbook
> 状态：v1.0（2026-08-15 整理）
> 查阅场景：新增浏览器操作 Playbook、排查脚本/Agent 双模式解析差异、理解 Playbook Markdown 契约格式时打开；具体解析器代码直接看 tests/playbook/
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [testing_guidelines.md](./testing_guidelines.md) — 项目单元/集成测试通用编写规范（Playbook 是 E2E 层补充）

## 📌 设计目标

为面向浏览器的用户操作流程提供一套可读、可执行、可复现的验证用例体系，支持**脚本驱动**和**Agent 驱动**两种执行模式，共享同一份 Playbook 定义。

---

## 核心概念

### Playbook（操作剧本）

以 Markdown 文件描述的用户操作流程，是**人读、解析器读、Agent 读**三者的共同契约。

```markdown
---
id: TOOLS-001
tags: [smoke, tools]
roles: [admin]
setup: bootstrap_admin_login
---

# 创建工具 SSRF 防护验证

## Steps

| Step | Action | Target | Data | Assert |
|------|--------|--------|------|--------|
| 1 | navigate | `/finance/tools` | - | URL matches `/finance/tools` |
| 2 | click | `[data-testid="create-tool-btn"]` | - | Modal visible |
| 3 | type | `[name="name"]` | `SsrfTestTool-{{uuid}}` | - |
| 4 | select | `[name="protocol"]` | `Http` | - |
| 5 | type | `[name="config.url"]` | `http://127.0.0.1/test` | - |
| 6 | toggle | `[name="config.allow_local_network"]` | `false` | - |
| 7 | click | `[data-testid="submit"]` | - | Toast contains "SSRF" |
```

### 三层结构

| 层级 | 说明 | 示例 |
|------|------|------|
| **Scenarios** | 一个 Playbook 文件包含多个场景 | `tools.md` → 创建工具 / 删除工具 / SSRF 防护 |
| **Steps** | 每个场景由有序步骤组成 | navigate → click → type → assert |
| **Assertions** | 每步可选断言，场景结束有总结断言 | URL 匹配 / Toast 包含文本 / 元素可见 |

---

## 两种执行模式

| 模式 | 驱动方式 | 适合场景 | 优点 | 缺点 |
|------|---------|---------|------|------|
| **A. 解析驱动** (Playwright) | Playbook → 结构化 AST → Playwright API 调用 | CI 冒烟、回归测试 | 100% 可复现、快、不依赖 LLM | 维护选择器成本 |
| **B. Agent 驱动** (LLM + Browser) | Playbook 原文当 prompt → Agent 拿 browser 工具自主操作 | 探索性 QA、复杂多步业务路径 | 容错强、能处理 UI 微变 | 慢、偶发漂移 |

**核心原则**：同一份 Playbook 可以被两种 runner 消费，不因模式不同而修改剧本。

---

## 目录结构

```
tests/
├── e2e/
│   ├── playbooks/               # Playbook 源文件（MD + YAML Front Matter）
│   │   ├── tools.md             # 工具管理场景集
│   │   ├── messages.md          # 消息发送场景集
│   │   └── agents.md            # Agent 创建绑工具场景集
│   ├── runners/
│   │   ├── playwright_runner.rs # 模式 A：解析 Playbook → Playwright API
│   │   └── agent_runner.rs      # 模式 B：Playbook 当 Prompt → LLM + Browser
│   └── fixtures/                # 登录态、Mock Server、数据预制
└── ...
```

---

## Action 类型规范

Playbook 中 Step 的 `Action` 字段统一使用以下枚举：

| Action | Target 格式 | 说明 |
|--------|------------|------|
| `navigate` | URL path | 导航到指定页面 |
| `click` | CSS selector / `[data-testid="..."]` | 点击元素 |
| `type` | CSS selector | 在输入框输入文本 |
| `select` | CSS selector | 选择下拉选项 |
| `toggle` | CSS selector | 切换开关/复选框 |
| `wait` | CSS selector / duration | 等待元素出现或固定时间 |
| `assert` | CSS selector / URL | 断言元素存在 / URL 匹配 |
| `screenshot` | - | 截图归档 |

**Target 优先使用 `data-testid`**，不用文字匹配或 XPath。

---

## 设计约束

### 1. `data-testid` 作为一等公民

所有交互点前端必须打 `data-testid`（如 `data-testid="tool-submit-btn"`），Playbook 里统一引用。这是人、解析器、Agent 三方的共同契约。

### 2. Precondition / Setup 独立

不要每个 Playbook 都写「注册→登录→创建组织」。抽成 setup 标签（`setup: bootstrap_admin_and_agent`），执行器在跑 Playbook 前先跑 fixture，把 jwt、agent_id、cookie 注入上下文。

### 3. 断言不是可选的

每个 Step 必须有期望，结束时 Playbook 要有一组「软断言」。模式 B 下 Agent 驱动的最后强制走一遍断言步骤，截图 + DOM 快照归档。

### 4. Playbook 当规范，不仅是测试

同一份 MD 可以当产品验收单 / PR QA 清单 / 回归 Case，三用。能被 Agent 读 = 新来的人或 Agent 想复现 bug，直接喂 Playbook 即可。

### 5. 模式 B 加冷却层

如果 Agent 连续 3 步 DOM 没变化（点错地方/元素不存在），自动切到模式 A 的兜底选择器，不要让 Agent 无限循环。

---

## 实施路径

1. **前端补 `data-testid`**：从 tools / messages 两个模块开始
2. **定义 Playbook 格式规范**：YAML Front Matter + Markdown 表格
3. **写第一个模式 A runner**：Playwright 解析驱动，跑最小场景（创建工具）
4. **格式稳定后写模式 B runner**：Agent 驱动，处理复杂探索性场景
