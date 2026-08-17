# E2E 测试工程（Playwright，仅本地运行）

> 定位：**本地回归工具**，不进 CI（rust.yml 无 e2e job）。
> 前后端协议以 `common` crate 为单一事实源（见 `docs/design/api_protocol_convention.md`），
> 协议变更时应同步检查本目录用例的断言契约。

## 运行

```bash
cd tests/e2e
npm install                # 首次
npx playwright install chromium
npx playwright test        # 全量：setup（初始化+登录冒烟）+ 15 条路由导航巡检
npx playwright test -g "可访问：/$"   # 按名称过滤单跑
npx playwright show-report            # 查看最近一次 HTML 报告
```

前置条件：`make build` 已产出 `dist/`（前端构建产物）。
后端二进制优先级：`AI_ORZ_E2E_BINARY` > `target/debug/ai_orz` > `target/release/ai_orz`
（**debug 优先**：本地 release 常是陈旧构建，E2E 必须与当前代码一致）。

## 组成

| 文件 | 职责 |
|------|------|
| `playwright.config.ts` | webServer 编排（独立端口 3310 + 隔离数据目录）、串行执行、失败保留 trace |
| `scripts/start-server.mjs` | 创建干净 `.e2e-runtime/` 数据目录 + 定位二进制 + 启动服务 |
| `tests/auth.setup.ts` | 浏览器全流程：系统初始化 → 错误密码 → 登录成功，保存 storageState |
| `tests/navigation.spec.ts` | 登录态下巡检 15 条主要路由：布局渲染 + 关键文案 + 无错误提示；失败时归档 pageerror/console |
| `scripts/diagnose-page.mjs` | 白屏/崩溃诊断：全新隔离服务器 + API 直调初始化登录 + 串行巡检路由，打印请求生命周期与崩溃事件 |

## 已知约束与排查经验

- **每个用例都是新 browser context**，wasm 冷编译耗时，断言超时已放宽（导航栏 45s / 文案 30s）。
- `/`（MessageChat）已接入统一 Navbar（h-screen 布局），与其他页面一样断言导航栏，无特例。
- **白屏排查**：先跑 `node scripts/diagnose-page.mjs <路由>` 区分三类问题——
  资源 404（构建产物缺失）、API 挂起（后端阻塞）、`PAGE CRASHED`（前端渲染死循环等）。
- 历史战果：E2E 曾发现两个生产缺陷——SPA 深链 404（router 回退缺失）、
  KnowledgeGraph 渲染期无条件 `Signal::set` 死循环导致渲染器崩溃白屏。
- 后端已实现优雅退出（SIGTERM/SIGINT/SIGQUIT → 排空在途请求 → 渠道/AOP 停服 → stats 落盘 → DB 关闭），
  诊断脚本可直接发 SIGTERM；手动排查后若有残留进程，`pkill -f target/debug/ai_orz` 清理，
  残留进程会占住数据库（表现为 `attempt to write a readonly database`）。
