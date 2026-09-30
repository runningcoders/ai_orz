//! 「停止思考」按钮 —— 取消目标 Agent 当前正在进行的思考轮次
//!
//! 与 Agent 详情页 `RuntimePanel` 里的「取消思考」同源（同一个接口
//! `POST /api/v1/hr/agents/{id}/cancel-thinking`），区别只在入口位置：这里是
//! **对话内**入口，用户不必离开聊天窗口去详情页找人。
//!
//! 两个落点共用本组件，避免在 `pages/message/chat.rs` 的两个 rsx 分支
//! （项目对话 / 默认对话）里各写一遍，也避免再抄一份请求 + 二次确认逻辑：
//! - `CancelThinkingStyle::Inline`：置底状态气泡内的小号按钮，紧挨「正在思考中…」
//! - `CancelThinkingStyle::Block`：输入区发送键位，Busy 时顶替被禁用的「处理中」
//!
//! 语义：`cancel_thinking` 是**信号语义**（Agent 在当前轮次边界退出），不是状态语义——
//! 队列里排着、还没轮到处理的消息不受影响，那些要用 `recall_message` 作废。
//!
//! 职责边界：调用方决定「何时渲染」（仅 `runtime_state == Busy(2)` 时）与「乐观回写」
//! （通过 `on_cancelled`）；本组件只管按钮外观 + 二次确认 + 发请求。

use crate::api::hr::cancel_thinking;
use crate::components::confirm_dialog::ConfirmDialog;
use crate::store::toast::use_toast;
use common::api::CancelThinkingRequest;
use dioxus::prelude::*;

/// 按钮外观变体
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum CancelThinkingStyle {
    /// 置底状态气泡内：小号，与气泡内其它元素同排
    #[default]
    Inline,
    /// 输入区发送键位：常规尺寸，承接原本「发送」按钮的位置
    Block,
}

/// 停止思考按钮 Props
#[derive(Props, Clone, PartialEq)]
pub struct CancelThinkingButtonProps {
    /// 目标 Agent ID（对话内 = 项目 owner / 前台 Agent）
    pub agent_id: String,
    /// 外观变体
    #[props(default)]
    pub style: CancelThinkingStyle,
    /// 取消信号投递成功后的回调：调用方据此乐观回写运行时状态，
    /// 不必等 3s 周期轮询才把按钮 / 气泡收敛掉
    pub on_cancelled: EventHandler<()>,
}

/// 停止思考按钮（含二次确认弹窗）
#[component]
pub fn CancelThinkingButton(props: CancelThinkingButtonProps) -> Element {
    let toast = use_toast();
    let mut show_confirm = use_signal(|| false);
    let mut cancelling = use_signal(|| false);

    let style = props.style;
    let agent_id = props.agent_id.clone();
    let on_cancelled = props.on_cancelled;

    let on_confirm = move |_| {
        show_confirm.set(false);
        let aid = agent_id.clone();
        cancelling.set(true);
        spawn(async move {
            match cancel_thinking(CancelThinkingRequest { id: aid }).await {
                // success=false 表示 Agent 已自行跑完这一轮（不是错误）：同样收起按钮——
                // 它此刻已不在思考，只是如实告知，别让用户以为操作失败了
                Ok(resp) => {
                    // 先复位禁用态，再触发 on_cancelled：后者会让父级把 runtime_state
                    // 归零，本组件随即卸载（scope 一 drop，再写自身 signal 就没意义了）
                    cancelling.set(false);
                    if resp.success {
                        toast.success(&resp.message);
                    } else {
                        toast.info(&resp.message);
                    }
                    on_cancelled.call(());
                }
                Err(e) => {
                    toast.error(format!("取消失败: {e}"));
                    cancelling.set(false);
                }
            }
        });
    };

    let busy = *cancelling.read();
    let (class, label) = match style {
        CancelThinkingStyle::Inline => (
            "btn hud-btn btn-xs btn-warning shrink-0",
            if busy { "取消中…" } else { "停止" },
        ),
        CancelThinkingStyle::Block => (
            "btn hud-btn btn-warning",
            if busy { "取消中…" } else { "停止思考" },
        ),
    };

    rsx! {
        // display:contents —— 本层不生成盒子，按钮直接作为父级 flex 的子项参与布局。
        // ConfirmDialog 底层走原生 <dialog>.showModal()（浏览器 top layer），
        // 不受此处祖先的堆叠上下文影响，放哪都恒定在最上层。
        div { class: "contents",
            button {
                class: "{class}",
                r#type: "button",
                disabled: busy,
                title: "让 Agent 停下手里的思考（当前轮次结束后退出）",
                onclick: move |_| show_confirm.set(true),
                "{label}"
            }
            ConfirmDialog {
                show: *show_confirm.read(),
                title: "确认停止思考".to_string(),
                message: "Agent 将在当前轮次完成后退出思考，已消耗的 token 不会回退；队列中尚未处理的消息不受影响。".to_string(),
                confirm_text: Some("停止思考".to_string()),
                confirm_class: Some("btn hud-btn btn-warning".to_string()),
                on_confirm,
                on_cancel: move |_| show_confirm.set(false),
            }
        }
    }
}
