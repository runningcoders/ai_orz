//! 通用搜索下拉框组件
//!
//! 支持两种数据源模式：
//! - Static: 传入完整候选列表，前端 filter
//! - Dynamic: 传入搜索函数，输入时实时调接口

use dioxus::prelude::*;

/// 搜索下拉框 Props
#[derive(Props, Clone, PartialEq)]
pub struct SearchableSelectProps {
    /// 输入框 placeholder
    pub placeholder: String,
    /// 当前选中的值
    pub selected: Option<String>,
    /// 候选列表（静态模式）
    pub options: Vec<String>,
    /// 选中值时的回调
    pub on_select: EventHandler<String>,
    /// 输入文本变化时的回调（动态搜索模式，可选）
    pub on_search: Option<EventHandler<String>>,
    /// 是否正在搜索（动态模式显示 loading）
    #[props(default = false)]
    pub loading: bool,
}

#[component]
pub fn SearchableSelect(props: SearchableSelectProps) -> Element {
    let mut input_value = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);
    let mut focused_index = use_signal(|| 0usize);

    // 根据输入文本过滤候选（静态模式），用 memo 让闭包可读取
    let filtered_options = use_memo(move || {
        let input = input_value.read();
        let keyword = input.to_lowercase();
        props
            .options
            .iter()
            .filter(|opt| input.is_empty() || opt.to_lowercase().contains(&keyword))
            .cloned()
            .collect::<Vec<String>>()
    });
    // 克隆当前过滤结果用于 rsx 渲染（读取 memo 建立响应式订阅）
    let opts = filtered_options.read().clone();

    rsx! {
        div { class: "relative w-full",
            // 输入框
            input {
                class: "input input-bordered input-sm w-full",
                r#type: "text",
                placeholder: "{props.placeholder}",
                value: "{input_value}",
                onfocus: move |_| show_dropdown.set(true),
                oninput: move |e| {
                    input_value.set(e.value().clone());
                    focused_index.set(0);
                    if let Some(handler) = &props.on_search {
                        handler.call(e.value().clone());
                    }
                },
                onkeydown: move |e| {
                    let opts = filtered_options.read();
                    if e.key() == Key::ArrowDown {
                        if focused_index() + 1 < opts.len() {
                            focused_index.set(focused_index() + 1);
                        }
                    } else if e.key() == Key::ArrowUp {
                        if focused_index() > 0 {
                            focused_index.set(focused_index() - 1);
                        }
                    } else if e.key() == Key::Enter {
                        if let Some(opt) = opts.get(focused_index()) {
                            props.on_select.call(opt.clone());
                            input_value.set(String::new());
                            show_dropdown.set(false);
                        }
                    } else if e.key() == Key::Escape {
                        show_dropdown.set(false);
                    }
                },
            }

            // 下拉列表
            if show_dropdown() && !opts.is_empty() {
                div {
                    class: "absolute z-50 mt-1 w-full max-h-60 overflow-auto bg-base-100 border border-base-300 rounded-lg shadow-lg",
                    onmouseleave: move |_| show_dropdown.set(false),

                    for (i, opt) in opts.iter().enumerate() {
                        {
                            let opt_owned = opt.clone();
                            rsx! {
                                div {
                                    class: if i == focused_index() {
                                        "px-3 py-2 cursor-pointer bg-primary text-primary-content text-sm"
                                    } else {
                                        "px-3 py-2 cursor-pointer hover:bg-base-200 text-sm"
                                    },
                                    onclick: move |_| {
                                        props.on_select.call(opt_owned.clone());
                                        input_value.set(String::new());
                                        show_dropdown.set(false);
                                    },
                                    "{opt_owned}"
                                }
                            }
                        }
                    }
                }
            }

            // Loading 指示器
            if props.loading {
                div {
                    class: "absolute right-2 top-1/2 -translate-y-1/2 loading loading-spinner loading-sm"
                }
            }
        }
    }
}
