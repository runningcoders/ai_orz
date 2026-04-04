use dioxus::prelude::*;
use crate::api::agent::{
    list_agents, create_agent, delete_agent,
    AgentListItem, CreateAgentRequest,
};

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
    let mut agents = use_signal(Vec::<AgentListItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut show_add_modal = use_signal(|| false);
    let mut new_agent_name = use_signal(String::new);
    let mut new_agent_role = use_signal(String::new);
    let mut new_agent_model_provider = use_signal(String::new);

    // 加载 Agent 列表
    let mut load_agents = move || {
        loading.set(true);
        error.set(None);

        spawn(async move {
            match list_agents().await {
                Ok(list) => agents.set(list),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    // 初始加载
    use_effect(move || {
        load_agents();
    });

    // 创建 Agent
    let handle_create = move |_| {
        let name = new_agent_name.read().clone();
        let role = new_agent_role.read().clone();
        let model_provider_id = new_agent_model_provider.read().clone();

        if name.is_empty() || model_provider_id.is_empty() {
            return;
        }

        spawn(async move {
            let req = CreateAgentRequest {
                name,
                role: if role.is_empty() { None } else { Some(role) },
                capabilities: None,
                soul: None,
                model_provider_id,
            };

            match create_agent(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_agent_name.set(String::new());
                    new_agent_role.set(String::new());
                    new_agent_model_provider.set(String::new());
                    load_agents();
                }
                Err(e) => error.set(Some(format!("创建失败: {}", e))),
            }
        });
    };

    let agents_read = agents.read().clone();

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

            // 错误提示
            if let Some(err) = error.read().as_ref() {
                div {
                    style: "
                        background: #fee;
                        border: 1px solid #fcc;
                        color: #c33;
                        padding: 1rem;
                        border-radius: 6px;
                        margin-bottom: 1rem;
                    ",
                    "{err}"
                }
            }

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
            } else if agents_read.is_empty() {
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
                                "角色"
                            }
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "模型提供商"
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
                            agents_read.iter().map(|agent| {
                                let id = agent.id.clone();
                                rsx! {
                                    tr {
                                        key: "{id}",
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
                                                color: #666;
                                            ",
                                            {agent.role.clone().unwrap_or_else(|| "-".to_string())}
                                        }
                                        td {
                                            style: "
                                                padding: 1rem 0.75rem;
                                                border-bottom: 1px solid #dee2e6;
                                                color: #666;
                                                font-family: monospace;
                                                font-size: 0.85rem;
                                            ",
                                            "{agent.model_provider_id}"
                                        }
                                        td {
                                            style: "
                                                padding: 1rem 0.75rem;
                                                border-bottom: 1px solid #dee2e6;
                                                text-align: center;
                                            ",
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
                                                onclick: move |_| {
                                                    let id_cloned = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_agent(&id_cloned).await {
                                                            error.set(Some(format!("删除失败: {}", e)));
                                                        } else {
                                                            load_agents();
                                                        }
                                                    });
                                                },
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
                    onclick: move |_| {
                        show_add_modal.set(false);
                        new_agent_name.set(String::new());
                        new_agent_role.set(String::new());
                        new_agent_model_provider.set(String::new());
                    },
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
                            style: "margin-bottom: 1rem;",
                            label {
                                style: "
                                    display: block;
                                    margin-bottom: 0.5rem;
                                    color: #333;
                                    font-weight: 500;
                                ",
                                "Agent 名称 *"
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
                            style: "margin-bottom: 1rem;",
                            label {
                                style: "
                                    display: block;
                                    margin-bottom: 0.5rem;
                                    color: #333;
                                    font-weight: 500;
                                ",
                                "角色描述"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_agent_role}",
                                oninput: move |e| new_agent_role.set(e.value()),
                                placeholder: "描述这个 Agent 的角色，比如'代码助手'"
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
                                "模型提供商 ID *"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_agent_model_provider}",
                                oninput: move |e| new_agent_model_provider.set(e.value()),
                                placeholder: "请输入已配置的模型提供商 ID"
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
                                    new_agent_role.set(String::new());
                                    new_agent_model_provider.set(String::new());
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
                                onclick: handle_create,
                                "创建"
                            }
                        }
                    }
                }
            }
        }
    }
}