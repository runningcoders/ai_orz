//! 模型提供商管理

use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use dioxus::prelude::*;

use crate::api::finance::{
    call_model_provider, create_model_provider, delete_model_provider, list_model_providers,
    switch_embedding_provider, test_model_provider_connection, toggle_model_provider,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    CallModelRequest, CreateModelProviderRequest, ListModelProvidersResponseItem,
    SwitchEmbeddingProviderRequest, UpdateModelProviderStatusRequest,
};
use common::enums::{ModelCapability, ModelProviderStatus, ProviderType};
use dioxus_router::Link;

#[component]
pub fn FinanceModelProviders() -> Element {
    let mut providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_modal = use_signal(|| false);

    let mut name = use_signal(String::new);
    let mut provider_type = use_signal(|| ProviderType::OpenAI);
    let mut new_capability = use_signal(|| 0i32); // 0=Agent(对话) 1=Embedding(向量)
    let mut model_name = use_signal(String::new);
    let mut api_key = use_signal(String::new);
    let mut base_url = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut max_context_length = use_signal(String::new);
    let mut recommended_context_length = use_signal(String::new);
    let mut creating = use_signal(|| false);

    let mut show_test_modal = use_signal(|| false);
    let mut test_provider_id = use_signal(String::new);
    let mut test_prompt = use_signal(|| "你好，请介绍一下自己".to_string());
    let mut test_response = use_signal(String::new);
    let mut test_loading = use_signal(|| false);

    let mut show_switch_modal = use_signal(|| false);
    let mut switch_provider_id = use_signal(String::new);
    let mut switch_provider_name = use_signal(String::new);
    let mut switch_loading = use_signal(|| false);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_model_providers().await {
                Ok(list) => providers.set(list.providers),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if name().is_empty() || model_name().is_empty() {
                toast.error("名称和模型名称不能为空");
                return;
            }
            creating.set(true);
            let req = CreateModelProviderRequest {
                name: name(),
                provider_type: provider_type(),
                capability: ModelCapability::from_i32(new_capability()),
                model_name: model_name(),
                api_key: api_key(),
                base_url: if base_url().is_empty() {
                    None
                } else {
                    Some(base_url())
                },
                description: if description().is_empty() {
                    None
                } else {
                    Some(description())
                },
                max_context_length: max_context_length().trim().parse::<i32>().ok(),
                recommended_context_length: recommended_context_length().trim().parse::<i32>().ok(),
            };
            match create_model_provider(req).await {
                Ok(resp) => {
                    show_modal.set(false);
                    name.set(String::new());
                    model_name.set(String::new());
                    api_key.set(String::new());
                    base_url.set(String::new());
                    description.set(String::new());
                    max_context_length.set(String::new());
                    recommended_context_length.set(String::new());
                    toast.success("创建成功");
                    if resp.rebuild_task_id.is_some() {
                        toast.info("已触发向量索引全量重建，期间语义搜索可能不完整");
                    } else if resp.status == ModelProviderStatus::Disabled as i32 {
                        toast.info("已创建为未启用状态；在列表点「启用」并确认切换后生效");
                    }
                    // Embedding（FastEmbed 等向量模型）无对话端点，连接测试必失败，跳过
                    let should_test_connection =
                        new_capability() != ModelCapability::Embedding as i32;
                    new_capability.set(0);
                    match list_model_providers().await {
                        Ok(list) => providers.set(list.providers),
                        Err(e) => toast.error(&e),
                    }
                    if should_test_connection {
                        spawn(async move {
                            match test_model_provider_connection(&resp.id).await {
                                Ok(_) => toast.success("创建成功，连接测试通过"),
                                Err(e) => toast.error(format!("创建成功但测试失败: {}", e)),
                            }
                        });
                    }
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let handle_test_send = move |_| {
        spawn(async move {
            test_loading.set(true);
            match call_model_provider(CallModelRequest {
                id: test_provider_id(),
                prompt: test_prompt(),
            })
            .await
            {
                Ok(resp) => test_response.set(resp.result),
                Err(e) => toast.error(format!("调用测试失败: {}", e)),
            }
            test_loading.set(false);
        });
    };

    let handle_switch_confirm = move |_| {
        spawn(async move {
            switch_loading.set(true);
            let id = switch_provider_id();
            match switch_embedding_provider(SwitchEmbeddingProviderRequest { id, confirm: true })
                .await
            {
                Ok(resp) => {
                    show_switch_modal.set(false);
                    toast.success(format!(
                        "Embedding Provider 已切换为 {}，向量索引重建完成",
                        resp.name
                    ));
                    match list_model_providers().await {
                        Ok(list) => providers.set(list.providers),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => {
                    show_switch_modal.set(false);
                    toast.error(format!("切换失败: {}", e));
                }
            }
            switch_loading.set(false);
        });
    };

    let providers_list = providers.read().clone();

    let provider_type_str = provider_type().to_string();

    rsx! {
        AppLayout {
            HudPanel { signal: Some(true),
                div { class: "card-body",
                    PageHeader {
                        eyebrow: Some("FINANCE".to_string()),
                        title: "模型提供商管理".to_string(),
                        actions: Some(rsx!{
                        button { class: "btn hud-btn btn-primary", onclick: move |_| show_modal.set(true), "+ 添加提供商" }
                        }),
                    },

                if loading() {
                    Loading {}
                } else if providers_list.is_empty() {
                    EmptyState { icon: "🧠".to_string(), message: "暂无模型提供商".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table hud-table table-zebra table-pin-rows",
                            thead { tr {
                                th { "名称" }
                                th { "类型" }
                                th { "能力" }
                                th { "模型" }
                                th { "状态" }
                                th { "操作" }
                            }}
                            tbody {
                                for p in providers_list.iter() {
                                    {
                                        let id = p.id.clone();
                                        let pname = p.name.clone();
                                        let pmodel = p.model_name.clone();
                                        let ptype_str = p.provider_type.to_string();
                                        let id_delete = id.clone();
                                        let id_test = id.clone();
                                        let id_detail = id.clone();
                                        let id_toggle = id.clone();
                                        let is_embedding = p.capability.is_embedding();
                                        let is_enabled = p.status == 1;
                                        let toggle_id = id.clone();
                                        let toggle_name = pname.clone();

                                        rsx! {
                                            tr { key: "{id}",
                                                td { class: "font-semibold",
                                                    Link { to: crate::pages::Route::FinanceModelProviderDetail { id: id_detail.clone() }, "{pname}" }
                                                }
                                                td { span { class: "badge orz-tag badge-sm", "{ptype_str}" } }
                                                td {
                                                    if is_embedding {
                                                        span { class: "badge orz-tag badge-sm", "embedding" }
                                                    } else {
                                                        span { class: "badge orz-tag badge-sm", "agent" }
                                                    }
                                                }
                                                td { class: "font-mono", "{pmodel}" }
                                                td {
                                                    if is_enabled {
                                                        span { class: "badge hud-badge badge-success", "启用" }
                                                    } else {
                                                        span { class: "badge hud-badge badge-neutral", "禁用" }
                                                    }
                                                }
                                                td { class: "flex gap-2 items-center flex-wrap",
                                                    if is_enabled {
                                                        button { class: "btn hud-btn btn-outline btn-sm",
                                                            onclick: {
                                                                let id = id_toggle.clone();
                                                                move |_| {
                                                                    let id = id.clone();
                                                                    spawn(async move {
                                                                        // Disabled=2：真禁用（条目保留可再启用）；软删除走「删除」按钮
                                                                        if let Ok(()) = toggle_model_provider(UpdateModelProviderStatusRequest { id, status: ModelProviderStatus::Disabled as i32 }).await {}
                                                                        match list_model_providers().await {
                                                                            Ok(list) => providers.set(list.providers),
                                                                            Err(e) => toast.error(&e),
                                                                        }
                                                                    });
                                                                }
                                                            },
                                                            "禁用"
                                                        }
                                                    } else {
                                                        button {
                                                            class: "btn hud-btn btn-sm btn-primary",
                                                            onclick: {
                                                                let id = toggle_id.clone();
                                                                let name = toggle_name.clone();
                                                                move |_| {
                                                                    let id = id.clone();
                                                                    let name = name.clone();
                                                                    let is_emb = is_embedding;
                                                                    spawn(async move {
                                                                        if is_emb {
                                                                            match toggle_model_provider(UpdateModelProviderStatusRequest { id: id.clone(), status: 1 }).await {
                                                                                Ok(()) => {
                                                                                    toast.success("已启用");
                                                                                    match list_model_providers().await {
                                                                                        Ok(list) => providers.set(list.providers),
                                                                                        Err(e) => toast.error(&e),
                                                                                    }
                                                                                }
                                                                                Err(e) => {
                                                                                    if e.error_code.as_deref() == Some("embedding_provider_switch_required") {
                                                                                        switch_provider_id.set(id);
                                                                                        switch_provider_name.set(name);
                                                                                        show_switch_modal.set(true);
                                                                                    } else {
                                                                                        toast.error(format!("启用失败: {}", e));
                                                                                    }
                                                                                }
                                                                            }
                                                                        } else {
                                                                            match toggle_model_provider(UpdateModelProviderStatusRequest { id, status: 1 }).await {
                                                                                Ok(()) => {
                                                                                    toast.success("已启用");
                                                                                    match list_model_providers().await {
                                                                                        Ok(list) => providers.set(list.providers),
                                                                                        Err(e) => toast.error(&e),
                                                                                    }
                                                                                }
                                                                                Err(e) => {
                                                                                    toast.error(format!("启用失败: {}", e));
                                                                                }
                                                                            }
                                                                        }
                                                                    });
                                                                }
                                                            },
                                                            "启用"
                                                        }
                                                    }
                                                    button { class: "btn hud-btn btn-outline btn-sm",
                                                        onclick: move |_| {
                                                            test_provider_id.set(id_test.clone());
                                                            test_prompt.set("你好，请介绍一下自己".to_string());
                                                            test_response.set(String::new());
                                                            show_test_modal.set(true);
                                                        },
                                                        "调用测试"
                                                    }
                                                    button { class: "btn hud-btn btn-error btn-sm",
                                                        onclick: move |_| {
                                                            pending_delete_id.set(id_delete.clone());
                                                            show_delete_confirm.set(true);
                                                        },
                                                        "删除"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Modal {
            title: "添加模型提供商".to_string(),
            show: show_modal(),
            on_close: move |_| show_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{name}",
                        oninput: move |e| name.set(e.value()), placeholder: "如：OpenAI 主账号" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "类型" }
                    }
                    select { class: "select select-bordered w-full", value: "{provider_type_str}",
                        onchange: move |e| {
                            provider_type.set(match e.value().as_str() {
                                "deepseek" => ProviderType::DeepSeek,
                                "qwen" => ProviderType::Qwen,
                                "doubao" => ProviderType::Doubao,
                                "ollama" => ProviderType::Ollama,
                                "custom" => ProviderType::Custom,
                                "fastembed" => ProviderType::FastEmbed,
                                "doubao_vision" => ProviderType::DoubaoVision,
                                _ => ProviderType::OpenAI,
                            });
                        },
                        option { value: "openai", "OpenAI" }
                        option { value: "custom", "OpenAI 兼容" }
                        option { value: "deepseek", "DeepSeek" }
                        option { value: "doubao", "豆包" }
                        option { value: "doubao_vision", "豆包 Vision (多模态 Embedding)" }
                        option { value: "qwen", "通义千问" }
                        option { value: "ollama", "Ollama" }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "能力类型 *" }
                    }
                    select {
                        class: "select select-bordered w-full",
                        "data-testid": "mp-create-capability",
                        value: "{new_capability}",
                        onchange: move |e| {
                            if let Ok(v) = e.value().parse::<i32>() {
                                new_capability.set(v);
                            }
                        },
                        option { value: "0", "Agent（对话 / 思考）" }
                        option { value: "1", "Embedding（向量化 / 语义搜索）" }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "模型名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{model_name}",
                        oninput: move |e| model_name.set(e.value()), placeholder: "如：gpt-4o" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium",
                            if new_capability() == ModelCapability::Embedding as i32 {
                                "API Key（FastEmbed 无需填写）"
                            } else {
                                "API Key *"
                            }
                        }
                    }
                    input { class: "input input-bordered w-full", r#type: "password", value: "{api_key}",
                        oninput: move |e| api_key.set(e.value()), placeholder: "sk-..." }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "Base URL" }
                    }
                    input { class: "input input-bordered w-full", value: "{base_url}",
                        oninput: move |e| base_url.set(e.value()), placeholder: "https://api.openai.com/v1" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "描述" }
                    }
                    input { class: "input input-bordered w-full", value: "{description}",
                        oninput: move |e| description.set(e.value()), placeholder: "可选" }
                }
                div { class: "grid grid-cols-2 gap-4",
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "最大上下文长度" }
                        }
                        input { class: "input input-bordered w-full", r#type: "number",
                            value: "{max_context_length}",
                            oninput: move |e| max_context_length.set(e.value()),
                            placeholder: "如：128000" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "推荐上下文长度" }
                        }
                        input { class: "input input-bordered w-full", r#type: "number",
                            value: "{recommended_context_length}",
                            oninput: move |e| recommended_context_length.set(e.value()),
                            placeholder: "留空自动计算" }
                    }
                }
                p { class: "text-xs text-base-content/60",
                    "最大上下文长度为模型支持的 token 上限；推荐上下文长度作为压缩触发阈值，留空时按最大值 60% 自动计算"
                }
            }
        }

        Modal {
            title: "调用测试".to_string(),
            show: show_test_modal(),
            on_close: move |_| show_test_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_test_modal.set(false), "关闭" }
                button { class: "btn hud-btn btn-primary", disabled: test_loading(), onclick: handle_test_send,
                    if test_loading() { "发送中..." } else { "发送" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "Prompt" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", rows: "4", value: "{test_prompt}",
                        oninput: move |e| test_prompt.set(e.value()) }
                }
                if !test_response().is_empty() {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "响应" }
                        }
                        textarea { class: "textarea textarea-bordered w-full", rows: "6", readonly: true, value: "{test_response}" }
                    }
                }
            }
        }

        Modal {
            title: "切换 Embedding Provider".to_string(),
            show: show_switch_modal(),
            on_close: move |_| show_switch_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_switch_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-warning", disabled: switch_loading(), onclick: handle_switch_confirm,
                    if switch_loading() { "切换中..." } else { "确认切换" }
                }
            },
            div {
                p { class: "mb-3 leading-relaxed",
                    "当前已有启用的 Embedding Provider，切换到 "
                    strong { "{switch_provider_name}" }
                    " 将会："
                }
                ul { class: "pl-5 leading-loose text-base-content/70 list-disc",
                    li { "禁用当前的 Embedding Provider" }
                    li { "启用新的 Embedding Provider" }
                    li { "使用新的 Embedding 模型重建所有向量索引" }
                }
                p { class: "mt-3 text-warning font-medium",
                    "⚠️ 重建向量索引可能需要较长时间，期间搜索功能将受影响"
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除".to_string(),
            message: {
                let id = pending_delete_id();
                let is_embedding = providers.read().iter()
                    .any(|p| p.id == id && p.capability.is_embedding());
                if is_embedding {
                    "确定删除此模型提供商？若它是使用中的向量模型，删除后语义搜索将失效，需重新配置并全量重建向量索引。".to_string()
                } else {
                    "确定删除此模型提供商？此操作不可撤销。".to_string()
                }
            },
            on_confirm: move |_| {
                let id = pending_delete_id();
                show_delete_confirm.set(false);
                spawn(async move {
                    if let Err(e) = delete_model_provider(&id).await {
                        toast.error(format!("删除失败: {}", e));
                    } else {
                        toast.success("已删除");
                        match list_model_providers().await {
                            Ok(list) => providers.set(list.providers),
                            Err(e) => toast.error(&e),
                        }
                    }
                });
            },
            on_cancel: move |_| {
                show_delete_confirm.set(false);
            }
        }
        }
    }
}
