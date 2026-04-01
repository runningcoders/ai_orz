use dioxus::prelude::*;

#[derive(Clone, Debug)]
pub struct Agent {
    id: String,
    name: String,
    description: String,
    status: AgentStatus,
}

#[derive(Clone, Debug)]
pub enum AgentStatus {
    Running,
    Stopped,
    Error,
}

impl AgentStatus {
    fn to_str(&self) -> &str {
        match self {
            AgentStatus::Running => "运行中",
            AgentStatus::Stopped => "已停止",
            AgentStatus::Error => "异常",
        }
    }

    fn color(&self) -> &str {
        match self {
            AgentStatus::Running => "#2ecc71",
            AgentStatus::Stopped => "#95a5a6",
            AgentStatus::Error => "#e74c3c",
        }
    }
}

#[component]
pub fn AgentManagement() -> Element {
    let mut agents = use_signal(Vec::<Agent>::new);
    let mut loading = use_signal(|| true);
    let mut show_add_modal = use_signal(|| false);
    let mut new_agent_name = use_signal(String::new);
    let mut new_agent_desc = use_signal(String::new);

    // 模拟加载数据 - 后面会替换成真实 API 调用
    use_effect(move || {
        // 模拟从后端加载 Agent 列表
        let sample_agents = vec![
            Agent {
                id: "1".to_string(),
                name: "代码助手".to_string(),
                description: "帮你写代码和调试".to_string(),
                status: AgentStatus::Running,
            },
            Agent {
                id: "2".to_string(),
                name: "文案创作".to_string(),
                description: "文章、文案、广告创作".to_string(),
                status: AgentStatus::Running,
            },
            Agent {
                id: "3".to_string(),
                name: "数据分析".to_string(),
                description: "数据统计和可视化分析".to_string(),
                status: AgentStatus::Stopped,
            },
        ];

        agents.set(sample_agents);
        loading.set(false);
    });

    let agents_cloned = agents.read().clone();

    rsx! {
        div {
            style: "
                max-width: 900px;
                margin: 0 auto;
                background: white;
                border-radius: 12px;
                padding: 2rem;
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
            ",

            // 标题 + 添加按钮
            div {
                style: "
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    margin-bottom: 2rem;
                ",
                h2 {
                    style: "color: #2c3e50; margin: 0;",
                    "Agent 管理"
                }
                button {
                    style: "
                        background: #27ae60;
                        color: white;
                        border: none;
                        padding: 0.75rem 1.25rem;
                        border-radius: 6px;
                        cursor: pointer;
                        font-size: 1rem;
                        transition: background-color 0.2s;
                        &:hover {{
                            background-color: #219a52;
                        }}
                    ",
                    onclick: move |_| show_add_modal.set(true),
                    "+ 创建 Agent"
                }
            }

            // 加载状态
            if *loading.read() {
                div {
                    style: "text-align: center; padding: 3rem; color: #666;",
                    "加载中..."
                }
            } else if agents_cloned.is_empty() {
                div {
                    style: "
                        text-align: center;
                        padding: 4rem 2rem;
                        color: #666;
                    ",
                    "暂无 Agent，点击上方按钮创建第一个 Agent"
                }
            } else {
                // Agent 列表
                table {
                    style: "
                        width: 100%;
                        border-collapse: collapse;
                    ",
                    thead {
                        tr {
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "名称"
                            }
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "状态"
                            }
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "描述"
                            }
                            th {
                                style: "
                                    text-align: center;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "操作"
                            }
                        }
                    }
                    tbody {
                        {
                            agents_cloned.iter().map(|agent| rsx! {
                                tr {
                                    key: "{agent.id}",
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                            font-weight: 500;
                                            color: #2c3e50;
                                        ",
                                        "{agent.name}"
                                    }
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                        ",
                                        span {
                                            style: "
                                                display: inline-block;
                                                padding: 0.25rem 0.75rem;
                                                border-radius: 12px;
                                                color: white;
                                                font-size: 0.8rem;
                                                background-color: {agent.status.color()};
                                            ",
                                            "{agent.status.to_str()}"
                                        }
                                    }
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                            color: #666;
                                        ",
                                        "{agent.description}"
                                    }
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                            text-align: center;
                                        ",
                                        div {
                                            style: "display: flex; gap: 0.5rem; justify-content: center;",
                                            button {
                                                style: "
                                                    background: #3498db;
                                                    color: white;
                                                    border: none;
                                                    padding: 0.4rem 0.8rem;
                                                    border-radius: 4px;
                                                    cursor: pointer;
                                                    font-size: 0.85rem;
                                                ",
                                                "编辑"
                                            }
                                            button {
                                                style: "
                                                    background: #e74c3c;
                                                    color: white;
                                                    border: none;
                                                    padding: 0.4rem 0.8rem;
                                                    border-radius: 4px;
                                                    cursor: pointer;
                                                    font-size: 0.85rem;
                                                ",
                                                "删除"
                                            }
                                        }
                                    }
                                }
                            })
                        }
                    }
                }
            }

            // 创建 Agent 弹窗
            if *show_add_modal.read() {
                div {
                    style: "
                        position: fixed;
                        top: 0;
                        left: 0;
                        right: 0;
                        bottom: 0;
                        background-color: rgba(0, 0, 0, 0.5);
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        z-index: 200;
                    ",
                    onclick: move |_| show_add_modal.set(false),
                    div {
                        style: "
                            background: white;
                            border-radius: 12px;
                            padding: 2rem;
                            width: 500px;
                            max-width: 90vw;
                        ",
                        onclick: |e| e.stop_propagation(),
                        h3 {
                            style: "margin-top: 0; margin-bottom: 1.5rem; color: #2c3e50;",
                            "创建新 Agent"
                        }
                        div {
                            style: "margin-bottom: 1.5rem;",
                            label {
                                style: "
                                    display: block;
                                    margin-bottom: 0.5rem;
                                    color: #333;
                                    font-weight: 500;
                                ",
                                "Agent 名称"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_agent_name}",
                                oninput: move |e| new_agent_name.set(e.value()),
                                placeholder: "请输入 Agent 名称"
                            }
                        }
                        div {
                            style: "margin-bottom: 2rem;",
                            label {
                                style: "
                                    display: block;
                                    margin-bottom: 0.5rem;
                                    color: #333;
                                    font-weight: 500;
                                ",
                                "Agent 描述"
                            }
                            textarea {
                                style: "
                                    width: 100%;
                                    min-height: 100px;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    resize: vertical;
                                ",
                                value: "{new_agent_desc}",
                                oninput: move |e| new_agent_desc.set(e.value()),
                                placeholder: "描述一下这个 Agent 的功能"
                            }
                        }
                        div {
                            style: "
                                display: flex;
                                gap: 1rem;
                                justify-content: flex-end;
                            ",
                            button {
                                style: "
                                    background: #95a5a6;
                                    color: white;
                                    border: none;
                                    padding: 0.75rem 1.5rem;
                                    border-radius: 6px;
                                    cursor: pointer;
                                ",
                                onclick: move |_| {
                                    show_add_modal.set(false);
                                    new_agent_name.set(String::new());
                                    new_agent_desc.set(String::new());
                                },
                                "取消"
                            }
                            button {
                                style: "
                                    background: #27ae60;
                                    color: white;
                                    border: none;
                                    padding: 0.75rem 1.5rem;
                                    border-radius: 6px;
                                    cursor: pointer;
                                ",
                                onclick: move |_| {
                                    // TODO: 调用后端 API 创建 Agent，后端自动生成 id
                                    if !new_agent_name().is_empty() {
                                        let mut agents = agents.write();
                                        agents.push(Agent {
                                            id: String::new(), // 后端会分配正式 id
                                            name: new_agent_name(),
                                            description: new_agent_desc(),
                                            status: AgentStatus::Stopped,
                                        });
                                        show_add_modal.set(false);
                                        new_agent_name.set(String::new());
                                        new_agent_desc.set(String::new());
                                    }
                                },
                                "创建"
                            }
                        }
                    }
                }
            }
        }
    }
}
