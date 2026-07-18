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
#[allow(dead_code)]
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
        let id = *self.next_id.read();
        let mut next_id = self.next_id;
        next_id.set(id + 1);

        let item = ToastItem {
            id,
            message,
            toast_type,
            duration_ms,
        };

        let mut toasts = self.toasts;
        toasts.write().push(item);
    }

    /// 关闭指定 id 的 Toast
    pub fn dismiss(&self, id: u64) {
        let mut toasts = self.toasts;
        toasts.write().retain(|t| t.id != id);
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
    #[allow(dead_code)]
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
