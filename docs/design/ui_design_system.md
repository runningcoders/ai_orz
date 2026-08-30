# AI Orz HUD 设计系统

> 🎯 **本文档定位**：前端视觉设计系统规范（色彩 / 字体 / 组件 / HUD 皮肤 / 代码落地），是 `frontend/styles/input.css`、自定义 HUD 组件与 `utils/status.rs` 等实现的权威来源。
>
> 状态：v3.0（2026-08-30 更新，以 `hud_design_prototype.html` 为唯一视觉基准）
>
> 关联文档：
> - [frontend_architecture.md](./frontend_architecture.md) — 前端架构、组件层级与 DaisyUI 使用规范
> - [hud_design_prototype.html](./hud_design_prototype.html) — 未来感驾驶舱设计原型（可视化基准、色彩/字体/组件完整示例）
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构
> - [UI 样式与主题](../wiki/zh/content/前端应用/UI%20样式与主题.md) — 主题切换实现细节

---

## 1. 设计方向

AI Orz 前端采用 **未来感「任务指挥中心 / Mission-Control HUD」** 设计语言：

- **暖色航天驾驶舱**：以品牌橙 `#fa520f` 与信号黄 `#ffd900` 为锚，深暖墨底（HUD 深色）或暖象牙底（浅色）。
- **克制的科技感**：切角面板、1px 渐变发丝边、内网格、等宽数据标签、顶部/面板内信号流光。
- **刻意避开 AI Slop**：拒绝青+深色、紫蓝渐变、霓虹发光、默认泛光等常见 AI 产品套路。
- **双主题自适应**：同一套语义变量在 `orz-hud`（深色驾驶舱）与 `orz-light`（浅色办公）间切换。

> 权威可视化基准：`docs/design/hud_design_prototype.html` 可直接在浏览器打开，使用右下角切换器在 HUD 深色 / 浅色间预览。

---

## 2. 设计令牌（双主题）

所有设计令牌已在 `docs/design/hud_design_prototype.html` 中以原生 CSS 变量定义，并在 `frontend/styles/input.css` 中通过 DaisyUI v5 `--color-*` / Tailwind v4 主题机制落地。

### 2.1 色彩令牌

| 语义角色 | HUD 深色值 | 浅色值 | 说明 |
|----------|-------------|--------|------|
| `primary` | `#fa520f` | `#e8430a` | 品牌橙，CTA / 主按钮 / 高亮 |
| `accent` | `#ffd900` | `#c79100` | 信号黄，次强调 / 状态提示 / 品牌渐变终点 |
| `info` | `#5d8fd0` | `#2f6fb0` | 钢蓝（次色），运行时/数据次态 |
| `success` | `#4fb286` | `#2e8a5f` | 成功 / 在线 / 正常 |
| `warning` | `#e0a93b` | `#b9821f` | 警告 / 忙碌 / 待处理 |
| `error` | `#e0594f` | `#c43d33` | 错误 / 离线 / 已禁用 |
| `bg` | `#0a0c11` | `#f7f5f1` | 页面背景 |
| `bg-soft` | `#0d1016` | `#fbfaf7` | 次级背景 / 输入框背景 |
| `surface` | `#0f131a` | `#ffffff` | 面板/卡片表面 |
| `surface-2` | `#151b25` | `#f1efe9` | 面板内次级表面 |
| `content` | `#e7eaf2` | `#211d18` | 主要文本 |
| `muted` | `rgba(231,234,242,.58)` | `rgba(33,29,24,.60)` | 次级/辅助文本 |
| `faint` | `rgba(231,234,242,.34)` | `rgba(33,29,24,.38)` | 弱化文本（眉标、时间戳） |
| `line` | `rgba(231,234,242,.12)` | `rgba(33,29,24,.13)` | 边框 / 分割线 |
| `line-soft` | `rgba(231,234,242,.07)` | `rgba(33,29,24,.07)` | 悬停背景 / 弱化边 |

### 2.2 品牌渐变

```css
/* HUD 深色 */
linear-gradient(90deg, #ffd900 0%, #ffa110 50%, #fa520f 100%);

/* 浅色 */
linear-gradient(90deg, #c79100 0%, #e8430a 100%);
```

品牌渐变仅用于：品牌 Logo 文字、品牌 mark、顶部导航底部光条、关键进度条、信号流光。

### 2.3 阴影 / 光晕

- **玻璃质感**：`backdrop-filter: blur(4px~12px)` 配合半透明表面。
- **品牌辉光**：`box-shadow: 0 0 14px -4px <primary 50%>`，用于主按钮聚焦、悬停状态。
- **全局底纹**：极低存在感（`opacity: .05`）的橙色网格 + 角落径向暖光，避免喧宾夺主。

---

## 3. 字体系统

| 字体 | 用途 | 代码变量 / 类 |
|------|------|---------------|
| **Chakra Petch** | 展示 / 标题 / 品牌文字 | `--font-display` / `.font-display` |
| **JetBrains Mono** | 数据 / 标签 / 数值 / 代码 | `--font-family-mono` |
| **Sora** | 正文 / 按钮 / 表单 | 默认 sans-serif 回退 |

### 3.1 层级规范

| 角色 | 字体 | 字号 | 字重 | 特征 |
|------|------|------|------|------|
| 品牌标题 | Chakra Petch | `1.15rem`~`2rem` | 700 | 品牌渐变填充、负间距微调 |
| 页面眉标（eyebrow） | JetBrains Mono | `11px` | 500 | 大写、字间距 `.18em`、弱化色 |
| 统计读数 | Chakra Petch | `2rem`+ | 700 | 等宽数字 `tabular-nums` |
| 徽章 / 标签 | JetBrains Mono | `10px`~`11px` | 600 | 胶囊形、等宽、小字间距 |
| 正文 | Sora | `.85rem`~`1rem` | 400~600 | 正常行高 1.5~1.7 |
| 按钮 | Sora | `.8rem`~`.85rem` | 600 | 紧凑内边距 |

---

## 4. 组件规范

### 4.1 切角 HUD 面板 `.panel` / `.hud-panel`

**设计原型**（HTML）：
```css
.panel {
  background:
    linear-gradient(var(--c-surface), var(--c-surface)) padding-box,
    linear-gradient(140deg, color-mix(in srgb, var(--c-primary) 45%, var(--c-line)),
      var(--c-line-soft) 35%, var(--c-line-soft) 65%,
      color-mix(in srgb, var(--c-accent) 35%, var(--c-line))) border-box;
  border: 1px solid transparent;
  border-radius: 6px;
  padding: 1.1rem 1.25rem;
  clip-path: polygon(0 11px, 11px 0, calc(100% - 11px) 0, 100% 11px,
                     100% calc(100% - 11px), calc(100% - 11px) 100%, 11px 100%, 0 calc(100% - 11px));
}
```

**代码落地**：`frontend/styles/input.css` 中 `.hud-panel` 系列 + `components/hud.rs` 的 `HudPanel`。

- 统一 6px 大圆角 + 11px 切角（clip-path）。
- 1px 渐变发丝边：从 primary 过渡到 accent，经 line-soft。
- 可选顶部 `.signal-bar`（2px 流光动画）。
- 面板内顶部常配 eyebrow 眉标。

### 4.2 顶部导航 `.topbar` / `.nav-link`

**设计原型**（HTML）：
```css
.topbar {
  height: 58px;
  background: color-mix(in srgb, var(--c-neutral) 92%, transparent);
  backdrop-filter: blur(12px);
  border-bottom: 1px solid var(--c-line);
}
.topbar::after {
  /* 底部品牌渐变光条 */
  background: linear-gradient(90deg, transparent, var(--c-primary), var(--c-accent), transparent);
  opacity: .55;
}
.nav-link {
  font-size: .85rem;
  color: var(--c-muted);
  padding: .35rem .7rem;
  border-radius: 6px;
  border: 1px solid transparent;
}
.nav-link:hover { color: var(--c-content); background: var(--c-line-soft); }
.nav-link.active { color: var(--c-content); border-color: var(--c-line); background: var(--c-line-soft); }
```

**代码落地**：`frontend/src/layouts/navbar.rs` + `frontend/styles/input.css` 的 `.navbar-link`。

- 扁平文本链接，**禁用圆角按钮/胶囊按钮**作为主导航。
- 下拉触发器使用文本 + 小三角，不加按钮背景。
- 右侧在线状态使用胶囊徽章（`.online-badge`），用户菜单仅头像。

### 4.3 按钮 `.btn` / `.hud-btn`

**设计原型**（HTML）：
```css
.btn {
  display: inline-flex; align-items: center; gap: .45rem;
  font-size: .85rem; font-weight: 600;
  padding: .5rem .95rem; border-radius: 6px;
  border: 1px solid transparent;
}
.btn-primary { color: #fff; background: var(--c-primary); box-shadow: 0 6px 20px -10px var(--c-primary); }
.btn-ghost { color: var(--c-content); border-color: var(--c-line); background: transparent; }
.btn-sm { padding: .32rem .7rem; font-size: .8rem; }
```

**代码落地**：`frontend/src/components/button.rs` 已封装 `Button` 组件，所有 variant 默认带 `hud-btn`：
- `Primary` → `btn hud-btn btn-primary`
- `Accent` → `btn hud-btn btn-accent`
- `Secondary` → `btn hud-btn btn-secondary`
- `Danger` → `btn hud-btn btn-error`
- `Ghost` → `btn hud-btn btn-ghost`

**强制规则**：搜索按钮、列表操作按钮、Tab 切换器必须经由 `Button` 组件或显式带 `hud-btn`，禁止裸用 DaisyUI `btn btn-primary` / `btn btn-outline`（会出现厚重白边）。

### 4.4 徽章 `.badge`（状态 vs 属性）

设计原型定义了统一视觉的胶囊徽章族：

```css
.badge {
  display: inline-flex; align-items: center; gap: .3rem;
  font-size: 11px; font-weight: 600;
  padding: .12rem .5rem;
  border-radius: 999px;
  font-family: 'JetBrains Mono', monospace;
  letter-spacing: .02em;
}
.badge-primary  { color: var(--c-primary);  background: var(--c-primary-soft); border: 1px solid color-mix(in srgb, var(--c-primary) 35%, transparent); }
.badge-success  { color: var(--c-success);  background: color-mix(in srgb, var(--c-success) 14%, transparent); border: ...; }
.badge-warning  { color: var(--c-warning);  background: color-mix(in srgb, var(--c-warning) 14%, transparent); border: ...; }
.badge-error    { color: var(--c-error);    background: color-mix(in srgb, var(--c-error)   14%, transparent); border: ...; }
.badge-info     { color: var(--c-info);     background: color-mix(in srgb, var(--c-info)    14%, transparent); border: ...; }
.badge-neutral  { color: var(--c-muted);    background: var(--c-line-soft); border: 1px solid var(--c-line); }
.badge-tag      { color: var(--c-content); background: var(--c-line-soft); border: 1px solid var(--c-line); }
```

**代码落地**：
- 状态徽章（成功/警告/错误/信息/主要/中性）单一事实源在 `frontend/src/utils/status.rs`，统一返回 `badge hud-badge badge-sm badge-*`。
- 属性标签（角色、类型、能力 tag、项目标签等）单一事实源为 `tag_chip()`，返回 `badge orz-tag badge-sm`。
- **状态徽章语义必须与 `status.rs` 一一对应**，禁止页面散写 `badge badge-success` 等。

> 注意：状态徽章与属性标签在视觉上是同一种胶囊 chip（等宽字体、相同圆角、相同内边距），区别仅在于颜色语义：状态走语义色，属性走中性 tag 色。不要一个是实底、一个是透明 outline，造成视觉重量不一致。

### 4.5 表格 `.hud-row` / `.hud-table`

- 行左侧 2px 数据条（`border-left`）。
- 悬停时背景微亮 + 品牌色发丝边 + 品牌辉光。
- 表头 eyebrow 化（大写等宽弱化）。
- 数值列使用 `tabular-nums`。

代码落地：`frontend/styles/input.css` 的 `.hud-row`、`.hud-table`，以及 `components/hud.rs` 的 `HudTable`。

### 4.6 进度条 / 信号条

- `.signal-bar`：2px 高，品牌渐变流光，位于面板顶部。
- `.bar`：6px 高，圆角 99px，默认品牌渐变；可覆盖 `.info` / `.success`。
- `HudProgress`：与 `progress_tone()` 配套，按 0-25/26-50/51-75/76-100 四段映射 warning/primary/accent/success。

---

## 5. 代码落地位置

| 规范项 | 落地文件 | 说明 |
|--------|----------|------|
| 主题变量 / HUD 皮肤 CSS | `frontend/styles/input.css` | DaisyUI v5 `--color-*` + 自定义 `.hud-*`、`.orz-tag`、`.navbar-link`、`.online-badge` |
| 状态徽章单一事实源 | `frontend/src/utils/status.rs` | `agent_lifecycle_badge` / `task_status_badge` / `project_status_badge` / `auth_state_badge` / `priority_badge` / `agent_runtime_badge` |
| 属性标签单一事实源 | `frontend/src/utils/status.rs` | `tag_chip()` |
| 按钮组件 | `frontend/src/components/button.rs` | 封装 `Button`，强制带 `hud-btn` |
| HUD 面板 / 表格 / Tab 原语 | `frontend/src/components/hud.rs` | `HudPanel` / `HudTable` / `HudTabs` / `HudCallout` 等 |
| 顶部导航 | `frontend/src/layouts/navbar.rs` | 扁平 `.navbar-link` + `.online-badge` |
| 可用主题列表 | `frontend/src/hooks/mod.rs` | `AVAILABLE_THEMES`（当前 9 个主题，首项 `orz-hud` 为默认） |

---

## 6. 与 DaisyUI / Tailwind 的关系

- **DaisyUI** 提供基础骨架类：`btn`、`badge`、`card`、`input`、`tabs`、`modal` 等。
- **Tailwind v4** 提供工具类与 `@theme` 主题变量机制。
- **`.hud-*` / `.orz-tag` 是 HUD 皮肤覆盖层**：在 DaisyUI 语义类之上追加发丝边、玻璃质感、等宽字体、暖色辉光。
- **禁止裸用 DaisyUI 默认按钮/徽章**：直接写 `btn btn-primary` / `badge badge-success` 会呈现 DaisyUI 默认白边或实底，破坏 HUD 一致性。

---

## 7. 主题切换

- 当前前端提供 **9 个可选主题**（`AVAILABLE_THEMES` 于 `frontend/src/hooks/mod.rs`）：
  `orz-hud`（HUD 深色，默认）、`orz-light`、`light`、`dark`、`cupcake`、`emerald`、`corporate`、`nord`、`synthwave`。
- **默认主题为 `orz-hud`**（`get_saved_theme` 回退值），且已作为首项加入 `AVAILABLE_THEMES`，设置页可一键切回 HUD 深色。
- 切换机制：修改 `<html data-theme="xxx">` + `localStorage` 持久化。

---

## 8. 常见错误与红线

| 错误 | 后果 | 正确做法 |
|------|------|----------|
| 裸用 `btn btn-primary` | 按钮出现厚重白边 | 使用 `Button` 组件或显式 `btn hud-btn btn-primary` |
| 裸用 `badge badge-success` | 徽章风格与设计稿不一致 | 使用 `status.rs` 的 helper 或 `tag_chip()` |
| 属性标签用 `hud-badge` 彩色 | 视觉重量与状态徽章冲突 | 属性用 `tag_chip()` → `badge orz-tag badge-sm` |
| 导航项用 `btn btn-ghost` 圆角按钮 | 与设计稿扁平导航不符 | 使用 `.navbar-link` 扁平文本链接 |
| 面板用 DaisyUI 原生 `card` 无切角 | 缺少 HUD 面板特征 | 使用 `HudPanel` / `.hud-panel` |
| 状态语义与 `status.rs` 不一致 | 同一状态在不同页面颜色不同 | 统一走 `status.rs` helper |

---

## 9. 更新记录

### 2026-08-30 v3.0 重构为新 HUD 设计系统

- 以 `docs/design/hud_design_prototype.html` 为唯一视觉基准，替换原 Mistral AI 风格描述。
- 明确双主题（orz-hud / orz-light）色彩令牌、字体系统、面板/导航/按钮/徽章/表格规范。
- 明确代码落地位置：`.hud-*` / `.orz-tag` 皮肤、`utils/status.rs` 状态语义单一事实源、`Button` 组件、`HudPanel`/`HudTable`/`HudTabs` 组件。
- 新增「主题切换」「常见错误与红线」章节。

### 2026-08-30 v3.1 标签样式与主题收口

- **属性标签 `.orz-tag` 重设计为中性 HUD pill**：对齐原型 `.badge-tag`——圆角 `999px`、JetBrains Mono `11px/600`、中性表面（`base-content 7%` 底 + `14%` 边），并补足 `line-height:1.4` + `white-space:nowrap` + 充足内边距，解决「边框粗细不一 / 文字贴边 / 文字被裁切 / 颜色与主体不搭」。
- **状态徽章 `.hud-badge` 保留玻璃质感版本**（blur + 顶部高光 + inset 阴影），并统一为 pill 圆角，与 `.orz-tag` 形状一致。
- **修复默认主题缺口**：`("orz-hud", "HUD 深色")` 已加入 `AVAILABLE_THEMES` 首项，设置页可切回默认 HUD 深色（原「已知不一致」已消除）。
- 状态（生命周期/启用禁用）走 `hud-badge` 彩色玻璃；属性（角色/类型/标签 cloud）走 `orz-tag` 中性 pill；二者均为同形状 capsule，仅颜色语义不同。
