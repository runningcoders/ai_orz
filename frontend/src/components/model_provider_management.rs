use dioxus::prelude::*;
use common::constants::ProviderType;
use common::api::{
    ModelProviderListItem, CreateModelProviderRequest,
};
use crate::api::model_provider::{
    list_model_providers, create_model_provider, delete_model_provider, test_model_provider_connection,
};

#[component]
pub fn ModelProviderManagement() -> Element {
    let mut providers = use_signal(Vec::<ModelProviderListItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut show_add_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_model_name = use_signal(String::new);
    let mut new_api_key = use_signal(String::new);
    let mut new_base_url = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut selected_provider_type = use_signal(|| ProviderType::OpenAI);

    // 加载 Model Provider 列表
    let mut load_providers = move || {
        loading.set(true);
        error.set(None);

        spawn(async move {
            match list_model_providers().await {
                Ok(list) => {
                    providers.set(list);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    };

    // 初始加载
    use_effect(move || {
        load_providers();
    });

    // 创建 Model Provider
    let handle_create = move |_| {
        let name = new_name.read().clone();
        let model_name = new_model_name.read().clone();
        let api_key = new_api_key.read().clone();
        let base_url = if new_base_url.read().is_empty() { None } else { Some(new_base_url.read().clone()) };
        let description = if new_description.read().is_empty() { None } else { Some(new_description.read().clone()) };
        let provider_type = *selected_provider_type.read();

        if name.is_empty() || model_name.is_empty() || api_key.is_empty() {
            return;
        }

        spawn(async move {
            let req = CreateModelProviderRequest {
                name,
                provider_type,
                model_name,
                api_key,
                base_url,
                description,
            };

            match create_model_provider(req).await {
                Ok(resp) => {
                    // 创建成功 → 自动调用测试接口
                    match test_model_provider_connection(&resp.id).await {
                        Ok(test_resp) => {
                            show_add_modal.set(false);
                            new_name.set(String::new());
                            new_model_name.set(String::new());
                            new_api_key.set(String::new());
                            new_base_url.set(String::new());
                            new_description.set(String::new());
                            selected_provider_type.set(ProviderType::OpenAI);
                            load_providers();

                            // 如果测试失败，显示错误信息
                            if !test_resp.success {
                                error.set(Some(format!("创建成功，但连通性测试失败: {}", test_resp.message)));
                            }
                        }
                        Err(e) => {
                            error.set(Some(format!("连通性测试失败: {}", e)));
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(format!("创建失败: {}", e)));
                }
            }
        });
    };

    let providers_read = providers.read().clone();
    let selected_type_str = format!("{:?}", selected_provider_type.read());

    rsx! {
        div {
            style: "
                max-width: 1000px;
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
                    "模型提供商管理"
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
                    "+ 添加模型"
                }
            }

            // 加载状态
            if *loading.read() {
                div {
                    style: "text-align: center; padding: 3rem; color: #666;",
                    "加载中..."
                }
            } else if providers_read.is_empty() {
                div {
                    style: "
                        text-align: center;
                        padding: 4rem 2rem;
                        color: #666;
                    ",
                    "暂无模型提供商，点击上方按钮添加第一个模型"
                }
            } else {
                // Model Provider 列表
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
                                "类型"
                            }
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "模型名称"
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
                            providers_read.iter().map(|provider| {
                                let id = provider.id.clone();
                                rsx! {
                                    tr {
                                        key: "{provider.id}",
                                        td {
                                            style: "
                                                padding: 1rem 0.75rem;
                                                border-bottom: 1px solid #dee2e6;
                                                font-weight: 500;
                                                color: #2c3e50;
                                            ",
                                            "{provider.name}"
                                        }
                                        td {
                                            style: "
                                                padding: 1rem 0.75rem;
                                                border-bottom: 1px solid #dee2e6;
                                                color: #666;
                                            ",
                                            {format!("{:?}", provider.provider_type)}
                                        }
                                        td {
                                            style: "
                                                padding: 1rem 0.75rem;
                                                border-bottom: 1px solid #dee2e6;
                                                color: #666;
                                                font-family: monospace;
                                                font-size: 0.85rem;
                                            ",
                                            "{provider.model_name}"
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
                                                    let mut load_providers_cloned = load_providers;
                                                    spawn(async move {
                                                        if let Err(e) = delete_model_provider(&id_cloned).await {
                                                            error.set(Some(format!("删除失败: {}", e)));
                                                        } else {
                                                            load_providers_cloned();
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

            // 创建 Model Provider 弹窗
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
                        new_name.set(String::new());
                        new_model_name.set(String::new());
                        new_api_key.set(String::new());
                        new_base_url.set(String::new());
                        new_description.set(String::new());
                    },
                    div {
                        style: "
                            background: white;
                            border-radius: 12px;
                            padding: 2rem;
                            width: 550px;
                            max-width: 90vw;
                        ",
                        onclick: |e| e.stop_propagation(),
                        h3 {
                            style: "margin-top: 0; margin-bottom: 1.5rem; color: #2c3e50;",
                            "添加模型提供商"
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
                                "提供商名称 *"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_name}",
                                oninput: move |e| new_name.set(e.value()),
                                placeholder: "请输入提供商名称，比如'我的OpenAI'"
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
                                "提供商类型 *"
                            }
                            select {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{selected_type_str}",
                                onchange: move |e| {
                                    match e.value().as_str() {
                                        "OpenAI" => selected_provider_type.set(ProviderType::OpenAI),
                                        "OpenAICompatible" => selected_provider_type.set(ProviderType::OpenAICompatible),
                                        "DeepSeek" => selected_provider_type.set(ProviderType::DeepSeek),
                                        "Doubao" => selected_provider_type.set(ProviderType::Doubao),
                                        "Qwen" => selected_provider_type.set(ProviderType::Qwen),
                                        "Ollama" => selected_provider_type.set(ProviderType::Ollama),
                                        _ => selected_provider_type.set(ProviderType::OpenAI),
                                    }
                                },
                                option {
                                    value: "OpenAI",
                                    selected: matches!(*selected_provider_type.read(), ProviderType::OpenAI),
                                    "OpenAI"
                                }
                                option {
                                    value: "OpenAICompatible",
                                    selected: matches!(*selected_provider_type.read(), ProviderType::OpenAICompatible),
                                    "OpenAI 兼容"
                                }
                                option {
                                    value: "DeepSeek",
                                    selected: matches!(*selected_provider_type.read(), ProviderType::DeepSeek),
                                    "DeepSeek"
                                }
                                option {
                                    value: "Doubao",
                                    selected: matches!(*selected_provider_type.read(), ProviderType::Doubao),
                                    "豆包"
                                }
                                option {
                                    value: "Qwen",
                                    selected: matches!(*selected_provider_type.read(), ProviderType::Qwen),
                                    "通义千问"
                                }
                                option {
                                    value: "Ollama",
                                    selected: matches!(*selected_provider_type.read(), ProviderType::Ollama),
                                    "Ollama (本地)"
                                }
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
                                "模型名称 *"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_model_name}",
                                oninput: move |e| new_model_name.set(e.value()),
                                placeholder: "例如 gpt-3.5-turbo"
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
                                "API Key *"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_api_key}",
                                oninput: move |e| new_api_key.set(e.value()),
                                placeholder: "请输入 API Key"
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
                                "自定义 Base URL (可选)"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_base_url}",
                                oninput: move |e| new_base_url.set(e.value()),
                                placeholder: "例如 https://api.openai.com/v1"
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
                                "描述 (可选)"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_description}",
                                oninput: move |e| new_description.set(e.value()),
                                placeholder: "描述这个模型的用途"
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
                                    new_name.set(String::new());
                                    new_model_name.set(String::new());
                                    new_api_key.set(String::new());
                                    new_base_url.set(String::new());
                                    new_description.set(String::new());
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