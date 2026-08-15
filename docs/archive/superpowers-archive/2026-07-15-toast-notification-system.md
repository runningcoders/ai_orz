# 全局通知系统 (Toast) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一套全局 Toast 通知系统，支持成功/错误/警告/信息四种类型，自动消失，堆叠展示，可手动关闭，通过全局状态管理在任何组件中调用。

**Architecture:** 基于 Dioxus 的 `use_context_provider` 实现全局通知状态管理，使用 `Signal<Vec<ToastItem>>` 存储通知列表。Toast 容器固定在页面右上角，支持滑入滑出动画、进度条倒计时、手动关闭按钮。子组件通过 `use_toast()` 获取全局句柄即可调用。

**Tech Stack:** Dioxus 0.7 (WebAssembly)、CSS 动画、Signal 全局状态管理、js_sys Promise 实现 sleep

---

## 文件结构

| 文件 | 职责 | 操作 |
|------|------|------|
| `frontend/src/store/toast.rs` | Toast 状态管理：类型定义、全局状态、API 函数 | 创建 |
| `frontend/src/store/mod.rs` | 导出 toast 模块 | 修改 |
| `frontend/src/components/toast.rs` | Toast 组件：容器 + 单条通知 + 动画 | 创建 |
| `frontend/src/components/mod.rs` | 导出 toast 组件 | 修改 |
| `frontend/src/main.rs` | 根组件注册全局 Toast 状态和容器 | 修改 |
| `frontend/index.html` | Toast 相关 CSS 样式（动画、定位、进度条等） | 修改 |
| `frontend/src/pages/hr/agent_detail.rs` | 示例页面：演示 Toast 用法 | 修改 |

---

## Task 1: Toast 状态管理核心

**Files:**
- Create: `frontend/src/store/toast.rs`
- Modify: `frontend/src/store/mod.rs`

### Step 1: 创建 toast.rs 核心类型

创建文件 `frontend/src/store/toast.rs`：

```rust
//! 全局 Toast 通知系统
//!
//! 使用方式：
//! ```
//! let toast = use_toast();
//! toast.success("操作成功");
//! toast.error("操作失败");
//! toast.warning("请注意");
//! toast.info("提示信息");
//! ```

use dioxus::prelude::*;

/// Toast 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastType {
    Success,
    Error,
    Warning,
    Info,
}

/// 单条 Toast 通知
#[derive(Debug, Clone)]
pub struct ToastItem {
    pub id: u64,
    pub message: String,
    pub toast_type: ToastType,
    pub duration_ms: u64,
}

/// 全局 Toast 状态
///
/// 内部两个 Signal 都是 Copy 类型，因此整个结构体也是 Copy
/// 可以安全地 move 到任意闭包中
#[derive(Clone, Copy)]
pub struct ToastState {
    pub toasts: Signal<Vec<ToastItem>>,
    next_id: Signal<u64>,
}

impl ToastState {
    /// 创建新的 Toast 状态
    pub fn new() -> Self {
        Self {
            toasts: Signal::new(Vec::new()),
            next_id: Signal::new(1),
        }
    }

    /// 显示一条 Toast
    pub fn show(&self, message: String, toast_type: ToastType, duration_ms: u64) {
        let id = self.next_id();
        self.next_id.set(id + 1);

        let item = ToastItem {
            id,
            message,
            toast_type,
            duration_ms,
        };

        self.toasts.write().push(item);
    }

    /// 关闭指定 id 的 Toast
    pub fn dismiss(&self, id: u64) {
        self.toasts.write().retain(|t| t.id != id);
    }

    /// 成功提示（默认 3 秒）
    pub fn success(&self, message: &str) {
        self.show(message.to_string(), ToastType::Success, 3000);
    }

    /// 错误提示（默认 5 秒）
    pub fn error(&self, message: &str) {
        self.show(message.to_string(), ToastType::Error, 5000);
    }

    /// 警告提示（默认 4 秒）
    pub fn warning(&self, message: &str) {
        self.show(message.to_string(), ToastType::Warning, 4000);
    }

    /// 信息提示（默认 3 秒）
    pub fn info(&self, message: &str) {
        self.show(message.to_string(), ToastType::Info, 3000);
    }
}

impl Default for ToastState {
    fn default() -> Self {
        Self::new()
    }
}

/// 在根组件初始化全局 Toast 状态
pub fn use_provide_toast() -> ToastState {
    use_context_provider(ToastState::new)
}

/// 获取全局 Toast 状态（子组件中使用）
pub fn use_toast() -> ToastState {
    use_context()
}
```

### Step 2: 修改 store/mod.rs 导出模块

读取 `frontend/src/store/mod.rs`，如果不存在则创建，添加 `pub mod toast;`。

如果文件内容只有 `pub mod auth;`，修改为：

```rust
pub mod auth;
pub mod toast;
```

### Step 3: 验证编译通过

运行: `cd frontend && cargo check 2>&1 | grep "error" | wc -l`
预期输出: `0`（可能有 unused 警告，属正常）

### Step 4: 提交

```bash
git add frontend/src/store/toast.rs frontend/src/store/mod.rs
git commit -m "feat: add toast state management core"
```

---

## Task 2: Toast UI 组件

**Files:**
- Create: `frontend/src/components/toast.rs`
- Modify: `frontend/src/components/mod.rs`

### Step 1: 创建 Toast 组件

创建文件 `frontend/src/components/toast.rs`：

```rust
//! Toast 通知组件 - 全局容器 + 单条通知

use dioxus::prelude::*;
use crate::store::toast::{ToastState, ToastType, use_toast};

/// 全局 Toast 容器（放在根组件中）
#[component]
pub fn ToastContainer() -> Element {
    let toast_state = use_toast();
    let toasts = toast_state.toasts();

    rsx! {
        div { class: "toast-container",
            for item in toasts.into_iter() {
                ToastItemView {
                    key: "{item.id}",
                    id: item.id,
                    message: item.message,
                    toast_type: item.toast_type,
                    duration_ms: item.duration_ms,
                }
            }
        }
    }
}

/// 单条 Toast 通知
#[component]
fn ToastItemView(id: u64, message: String, toast_type: ToastType, duration_ms: u64) -> Element {
    let toast_state = use_toast();
    let mut visible = use_signal(|| false);
    let mut leaving = use_signal(|| false);

    // 进入动画延迟（下一帧触发，确保 transition 生效）
    use_effect(move || {
        spawn(async {
            sleep_ms(10).await;
            visible.set(true);
        });
    });

    // 自动关闭
    use_effect(move || {
        let toast = toast_state;
        spawn(async move {
            sleep_ms(duration_ms).await;
            leaving.set(true);
            sleep_ms(300).await;
            toast.dismiss(id);
        });
    });

    let type_class = match toast_type {
        ToastType::Success => "toast toast-success",
        ToastType::Error => "toast toast-error",
        ToastType::Warning => "toast toast-warning",
        ToastType::Info => "toast toast-info",
    };

    let icon = match toast_type {
        ToastType::Success => "✓",
        ToastType::Error => "✕",
        ToastType::Warning => "!",
        ToastType::Info => "i",
    };

    let animation_class = if leaving() {
        "toast-leaving"
    } else if visible() {
        "toast-visible"
    } else {
        ""
    };

    let handle_close = move |_| {
        leaving.set(true);
        let toast = toast_state;
        spawn(async move {
            sleep_ms(300).await;
            toast.dismiss(id);
        });
    };

    rsx! {
        div { class: "{type_class} {animation_class}",
            span { class: "toast-icon", "{icon}" }
            span { class: "toast-message", "{message}" }
            button { class: "toast-close", onclick: handle_close, "×" }
            div {
                class: "toast-progress",
                style: "animation-duration: {duration_ms}ms;",
            }
        }
    }
}

/// 简易 sleep 实现（基于 js_sys Promise + setTimeout）
async fn sleep_ms(ms: u64) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}
```

### Step 2: 修改 components/mod.rs

修改 `frontend/src/components/mod.rs`，添加 `pub mod toast;`：

```rust
//! 基础 UI 组件库

pub mod button;
pub mod graph;
pub mod modal;
pub mod state;
pub mod toast;
```

### Step 3: 验证编译通过

运行: `cd frontend && cargo check 2>&1 | grep "error" | wc -l`
预期输出: `0`（可能有 dead_code 警告，未使用时正常）

### Step 4: 提交

```bash
git add frontend/src/components/toast.rs frontend/src/components/mod.rs
git commit -m "feat: add toast UI component with enter/leave animations"
```

---

## Task 3: 根组件集成 + CSS 样式

**Files:**
- Modify: `frontend/src/main.rs`
- Modify: `frontend/index.html`

### Step 1: 修改 main.rs 注册全局 Toast

读取 `frontend/src/main.rs`，修改为以下内容：

```rust
mod api;
mod components;
mod config;
mod layouts;
mod pages;
mod store;

// Include compile-time generated configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/compiled_config.rs"));

use dioxus::prelude::*;

#[allow(unused_imports)]
use store::auth::AuthState;
use store::toast::use_provide_toast;

use crate::components::toast::ToastContainer;
use crate::pages::Route;

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    // 初始化全局认证状态
    use_context_provider(|| Signal::new(AuthState::restore()));
    // 初始化全局 Toast 状态
    let _toast = use_provide_toast();

    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        Router::<Route> {}
        ToastContainer {}
    }
}
```

### Step 2: 添加 CSS 样式

读取 `frontend/index.html`，在 `</style>` 结束标签之前，添加以下样式：

```css
      /* ===== Toast Notifications ===== */
      .toast-container {
        position: fixed;
        top: 20px;
        right: 20px;
        z-index: 9999;
        display: flex;
        flex-direction: column;
        gap: 10px;
        pointer-events: none;
      }

      .toast {
        display: flex;
        align-items: center;
        gap: 12px;
        min-width: 280px;
        max-width: 400px;
        padding: 12px 16px;
        border-radius: var(--radius-md);
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        font-size: 14px;
        position: relative;
        overflow: hidden;
        pointer-events: auto;
        opacity: 0;
        transform: translateX(120%);
        transition: opacity 0.3s ease, transform 0.3s ease;
      }

      .toast-visible {
        opacity: 1;
        transform: translateX(0);
      }

      .toast-leaving {
        opacity: 0;
        transform: translateX(120%);
      }

      .toast-icon {
        width: 20px;
        height: 20px;
        border-radius: 50%;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 12px;
        font-weight: bold;
        flex-shrink: 0;
        color: white;
      }

      .toast-message {
        flex: 1;
        line-height: 1.5;
        word-break: break-word;
      }

      .toast-close {
        background: transparent;
        border: none;
        font-size: 18px;
        cursor: pointer;
        color: inherit;
        opacity: 0.6;
        padding: 0 4px;
        flex-shrink: 0;
        line-height: 1;
      }

      .toast-close:hover {
        opacity: 1;
      }

      .toast-progress {
        position: absolute;
        bottom: 0;
        left: 0;
        height: 3px;
        background: currentColor;
        opacity: 0.3;
        animation-name: toast-progress-shrink;
        animation-timing-function: linear;
        animation-fill-mode: forwards;
      }

      @keyframes toast-progress-shrink {
        from { width: 100%; }
        to { width: 0%; }
      }

      .toast-success {
        background-color: var(--color-success-bg);
        border: 1px solid var(--color-success);
        color: var(--color-success);
      }
      .toast-success .toast-icon { background-color: var(--color-success); }

      .toast-error {
        background-color: var(--color-error-bg);
        border: 1px solid var(--color-error);
        color: var(--color-error);
      }
      .toast-error .toast-icon { background-color: var(--color-error); }

      .toast-warning {
        background-color: var(--color-warning-bg);
        border: 1px solid var(--color-warning);
        color: var(--color-warning);
      }
      .toast-warning .toast-icon { background-color: var(--color-warning); }

      .toast-info {
        background-color: var(--color-info-bg);
        border: 1px solid var(--color-info);
        color: var(--color-info);
      }
      .toast-info .toast-icon { background-color: var(--color-info); }
```

### Step 3: 验证编译通过

运行: `cd frontend && cargo check 2>&1 | grep "error" | wc -l`
预期输出: `0`

### Step 4: 提交

```bash
git add frontend/src/main.rs frontend/index.html
git commit -m "feat: integrate toast system into root component with CSS styles"
```

---

## Task 4: 在 Agent 详情页演示 Toast 用法

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`

### Step 1: 添加 import 和获取 toast

在 `frontend/src/pages/hr/agent_detail.rs` 顶部添加：

```rust
use crate::store::toast::use_toast;
```

在 `HrAgentDetail` 组件函数开头（其他 `let mut xxx = use_signal...` 附近）添加：

```rust
let toast = use_toast();
```

### Step 2: 替换状态切换的成功/失败提示

找到 Agent 状态切换的 `onclick` 处理逻辑（大概在 `STATUS_OPTIONS` 的 for 循环里），将：

```rust
success.set(format!("状态已更新为：{}", label_clone));
error.set(String::new());
```

替换为：

```rust
toast.success(&format!("状态已更新为：{}", label_clone));
```

将错误分支：
```rust
Err(e) => error.set(format!("状态更新失败: {}", e)),
```

替换为：
```rust
Err(e) => toast.error(&format!("状态更新失败: {}", e)),
```

### Step 3: 替换安装工具包的成功/失败提示

找到安装工具包的逻辑，将成功提示从 `success.set(...)` 改为 `toast.success(...)`，失败提示从 `error.set(...)` 改为 `toast.error(...)`。

注意：页面顶部的 `ErrorAlert { message: error() }` 和 `SuccessAlert { message: success() }` 保持不变，本任务仅替换 2-3 处作为演示。

### Step 4: 验证编译通过

运行: `cd frontend && cargo check 2>&1 | grep "error" | wc -l`
预期输出: `0`

### Step 5: 提交

```bash
git add frontend/src/pages/hr/agent_detail.rs
git commit -m "feat: use toast notifications in agent detail page as example"
```

---

## 自检清单

### 1. Spec 覆盖检查
- ✅ 四种 Toast 类型（成功/错误/警告/信息）→ Task 1 `ToastType` 枚举 + 4 个方法
- ✅ 自动消失 → Task 2 `use_effect` + `sleep_ms` 倒计时
- ✅ 堆叠展示 → Task 2 `ToastContainer` + `for` 循环
- ✅ 可手动关闭 → Task 2 `toast-close` 按钮 + `dismiss`
- ✅ 全局状态管理 → Task 1 `use_context_provider` + `use_toast()`
- ✅ 滑入滑出动画 → Task 3 CSS transition + opacity/transform
- ✅ 进度条倒计时 → Task 3 CSS `@keyframes toast-progress-shrink`

### 2. Placeholder 扫描
- 无 TBD / TODO / "后续补充"
- 所有代码步骤都有完整代码
- 文件路径都具体明确

### 3. 类型一致性
- `ToastType` 枚举在 `store/toast.rs` 定义，组件中使用一致
- `ToastItem` 字段一致：id / message / toast_type / duration_ms
- API 命名一致：`success` / `error` / `warning` / `info` / `show` / `dismiss`
- `ToastState` 是 `Copy` 类型（内部全是 Signal），可安全 move 到闭包
