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
use crate::components::thinking_intensity_field::{
    ThinkingIntensityField, thinking_intensity_label,
};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    CallModelRequest, CreateModelProviderRequest, ListModelProvidersResponseItem,
    SwitchEmbeddingProviderRequest,
};
use common::enums::{
    ChatApiFlavor, ModelAccessMode, ModelCapability, ModelProviderStatus, ProviderType,
    ThinkingIntensity,
};
use dioxus_router::Link;

#[component]
pub fn FinanceModelProviders() -> Element {
    let mut providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_modal = use_signal(|| false);

    let mut name = use_signal(String::new);
    let mut provider_type = use_signal(|| ProviderType::OpenAI);
    let mut new_capability = use_signal(|| 0i32); // 0=Agent(对话) 1=Embedding(向量) 2=Decision(小脑决策)
    // 访问模式（T1 方案 §五 / T2 设计 §4.1）：默认 stream；Embedding 切换隐藏但不清除
    let mut new_access_mode = use_signal(|| ModelAccessMode::Stream);
    let mut new_supports_vision = use_signal(|| false); // 批5-P2：vision 能力开关（创建 Modal，默认不勾选）
    // 协议口味：默认 Responses（平台主线；与 ChatApiFlavor::default() 同语义）
    let mut new_api_flavor = use_signal(|| ChatApiFlavor::Responses);
    // 思考强度：默认不启用（None = 请求体不下发该字段，保持存量行为）；
    // 启用后平台统一 0–100 分，下游三档由 Provider 配置推导
    let mut new_thinking_intensity_enabled = use_signal(|| false);
    let mut new_thinking_intensity = use_signal(|| ThinkingIntensity::DEFAULT.score());
    let new_access_mode_str = match new_access_mode() {
        ModelAccessMode::Stream => "stream",
        ModelAccessMode::NonStream => "non_stream",
    };
    let new_api_flavor_str = match new_api_flavor() {
        ChatApiFlavor::Responses => "responses",
        ChatApiFlavor::ChatCompletions => "chat_completions",
    };
    let mut model_name = use_signal(String::new);
    let mut api_key = use_signal(String::new);
    let mut base_url = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut max_context_length = use_signal(String::new);
    let mut recommended_context_length = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // 能力派生标志：rsx 与提交逻辑共用，避免同一表达式散落多处后各自漂移
    let is_agent_cap = new_capability() == ModelCapability::Agent as i32;
    let is_embedding_cap = new_capability() == ModelCapability::Embedding as i32;
    let is_decision_cap = new_capability() == ModelCapability::Decision as i32;
    // Base URL 占位符随类型变化：方舟与 OpenAI 根地址不同，且都只填根路径
    let base_url_placeholder = match provider_type() {
        ProviderType::Doubao | ProviderType::DoubaoVision => {
            "https://ark.cn-beijing.volces.com/api/v3"
        }
        ProviderType::FastEmbed => "本地模型无需填写",
        _ => "https://api.openai.com/v1",
    };

    let mut show_test_modal = use_signal(|| false);
    let mut test_provider_id = use_signal(String::new);
    let mut test_capability = use_signal(|| ModelCapability::Agent);
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
            // 对话模型必填上下文长度：缺失 → 压缩阈值算不出来 → Agent 永不压缩（静默降级）
            let is_embedding = is_embedding_cap;
            if !is_embedding
                && max_context_length()
                    .trim()
                    .parse::<i32>()
                    .map_or(true, |v| v <= 0)
            {
                toast.error("请填写最大上下文长度（对话模型必填，正整数 token 数）");
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
                // 访问模式：Create 恒传（T2 §4.1/S3）；Embedding 隐藏时强制 stream 保持语义干净
                access_mode: Some(if is_embedding {
                    ModelAccessMode::Stream
                } else {
                    new_access_mode()
                }),
                // 批5-P2：vision 能力开关随创建上送；checkbox 仅 capability==Agent 显示，
                // 隐藏态（Embedding/Decision）保守上送 false（沿 access_mode 先例防隐藏态残留）
                supports_vision: Some(is_agent_cap && new_supports_vision()),
                // 协议口味只对对话模型有意义：Embedding 走 /embeddings、Decision 走小脑链路。
                // 非 Agent 下传 None（不落盘），避免把无关配置写进 config 后被误读——
                // 历史缺陷正是「Embedding 也写 api_flavor=Responses」导致向量化被路由错协议。
                api_flavor: is_agent_cap.then_some(new_api_flavor()),
                // 思考强度（仅对话模型有意义）：非 Agent 或未启用均传 None（不启用该能力开关）
                thinking_intensity: (is_agent_cap && new_thinking_intensity_enabled())
                    .then(|| ThinkingIntensity::new(new_thinking_intensity())),
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
                    new_access_mode.set(ModelAccessMode::Stream);
                    new_supports_vision.set(false); // 批5-P2：重置 vision checkbox
                    new_api_flavor.set(ChatApiFlavor::Responses); // 重置为主线协议
                    new_thinking_intensity_enabled.set(false); // 重置为不启用
                    new_thinking_intensity.set(ThinkingIntensity::DEFAULT.score());
                    toast.success("创建成功");
                    if resp.rebuild_task_id.is_some() {
                        toast.info("已触发向量索引全量重建，期间语义搜索可能不完整");
                    } else if resp.status == ModelProviderStatus::Disabled as i32 {
                        toast.info("已创建为未启用状态；在列表点「启用」并确认切换后生效");
                    }
                    // FastEmbed 是本地模型、无远程端点，创建后无法做连通性测试；
                    // 其余能力（Agent/Embedding/Decision）后端各有对应探针
                    let should_test_connection = provider_type() != ProviderType::FastEmbed;
                    new_capability.set(0);
                    // 类型随能力一起回到默认，避免下次创建继承「向量专用类型 + Agent」非法组合
                    provider_type.set(ProviderType::OpenAI);
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
                                        let p_capability = p.capability;
                                        let is_enabled = p.status == 1;
                                        let toggle_id = id.clone();
                                        let toggle_name = pname.clone();

                                        rsx! {
                                            tr { key: "{id}",
                                                td { class: "font-semibold",
                                                    Link { to: crate::pages::Route::FinanceModelProviderDetail { id: id_detail.clone() }, "{pname}" }
                                                }
                                                td {
                                                    div { class: "flex items-center gap-1",
                                                        span { class: "badge orz-tag badge-sm", "{ptype_str}" }
                                                        if matches!(p.access_mode, ModelAccessMode::NonStream) {
                                                            span { class: "badge hud-badge badge-ghost badge-sm", "非流式" }
                                                        }
                                                        if p.supports_vision {
                                                            span { class: "badge hud-badge badge-ghost badge-sm", "图像" }
                                                        }
                                                        // 主线协议（Responses）不显徽标，仅回退态显式标注
                                                        if matches!(p.api_flavor, ChatApiFlavor::ChatCompletions) {
                                                            span { class: "badge hud-badge badge-ghost badge-sm", "Chat API" }
                                                        }
                                                        // 思考强度仅在已配置（能力开关开启）时标注
                                                        if let Some(intensity) = p.thinking_intensity {
                                                            span { class: "badge hud-badge badge-ghost badge-sm",
                                                                {thinking_intensity_label(intensity)}
                                                            }
                                                        }
                                                    }
                                                }
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
                                                                        if let Ok(()) = toggle_model_provider(&id, ModelProviderStatus::Disabled as i32).await {}
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
                                                                            match toggle_model_provider(&id, 1).await {
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
                                                                            match toggle_model_provider(&id, 1).await {
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
                                                            test_capability.set(p_capability);
                                                            test_prompt.set(match p_capability {
                                                                ModelCapability::Embedding => {
                                                                    "connectivity test".to_string()
                                                                }
                                                                ModelCapability::Decision => {
                                                                    "这是一次连通性测试".to_string()
                                                                }
                                                                ModelCapability::Agent => {
                                                                    "你好，请介绍一下自己".to_string()
                                                                }
                                                            });
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
                            let pt = match e.value().as_str() {
                                "deepseek" => ProviderType::DeepSeek,
                                "qwen" => ProviderType::Qwen,
                                "doubao" => ProviderType::Doubao,
                                "ollama" => ProviderType::Ollama,
                                "custom" => ProviderType::Custom,
                                "fastembed" => ProviderType::FastEmbed,
                                "doubao_vision" => ProviderType::DoubaoVision,
                                "jev" => ProviderType::Jev,
                                _ => ProviderType::OpenAI,
                            };
                            provider_type.set(pt);
                            // 类型 → 能力联动：向量专用端点与决策模型只有唯一合法用途，
                            // 选类型即锁定能力，避免出现「DoubaoVision + Agent」这类非法组合
                            match pt {
                                ProviderType::DoubaoVision | ProviderType::FastEmbed => {
                                    new_capability.set(ModelCapability::Embedding as i32)
                                }
                                ProviderType::Jev => {
                                    new_capability.set(ModelCapability::Decision as i32)
                                }
                                _ => {}
                            }
                        },
                        // 类型选项按能力过滤：Jev 仅决策、向量专用端点仅 Embedding
                        if is_agent_cap {
                            option { value: "openai", "OpenAI" }
                            option { value: "custom", "OpenAI 兼容" }
                            option { value: "deepseek", "DeepSeek" }
                            option { value: "doubao", "豆包" }
                            option { value: "qwen", "通义千问" }
                            option { value: "ollama", "Ollama" }
                        }
                        if is_embedding_cap {
                            option { value: "openai", "OpenAI" }
                            option { value: "custom", "OpenAI 兼容" }
                            option { value: "deepseek", "DeepSeek" }
                            option { value: "doubao", "豆包" }
                            option { value: "doubao_vision", "豆包 Vision (多模态 Embedding)" }
                            option { value: "qwen", "通义千问" }
                            option { value: "ollama", "Ollama" }
                            option { value: "fastembed", "FastEmbed（本地向量模型）" }
                        }
                        if is_decision_cap {
                            option { value: "jev", "Jev（小脑决策模型）" }
                        }
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
                                // 能力 → 类型纠正：切到决策锁定 Jev；切出决策时 Jev 必须让位；
                                // 向量专用端点（DoubaoVision/FastEmbed）在 Agent 下非法
                                match ModelCapability::from_i32(v) {
                                    ModelCapability::Decision => provider_type.set(ProviderType::Jev),
                                    ModelCapability::Embedding => {
                                        if provider_type() == ProviderType::Jev {
                                            provider_type.set(ProviderType::OpenAI);
                                        }
                                    }
                                    ModelCapability::Agent => {
                                        if matches!(
                                            provider_type(),
                                            ProviderType::Jev | ProviderType::FastEmbed
                                        ) {
                                            provider_type.set(ProviderType::OpenAI);
                                        } else if provider_type() == ProviderType::DoubaoVision {
                                            provider_type.set(ProviderType::Doubao);
                                        }
                                    }
                                }
                            }
                        },
                        option { value: "0", "Agent（对话 / 思考）" }
                        option { value: "1", "Embedding（向量化 / 语义搜索）" }
                        option { value: "2", "小脑/决策" }
                    }
                }
                if !is_embedding_cap {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "访问模式" }
                        }
                        select {
                            class: "select select-bordered w-full",
                            "data-testid": "mp-create-access-mode",
                            value: "{new_access_mode_str}",
                            onchange: move |e| {
                                new_access_mode.set(match e.value().as_str() {
                                    "non_stream" => ModelAccessMode::NonStream,
                                    _ => ModelAccessMode::Stream,
                                });
                            },
                            option { value: "stream", "流式" }
                            option { value: "non_stream", "非流式" }
                        }
                        p { class: "text-xs text-base-content/60",
                            "仅当下游网关不支持 SSE 流式时选择非流式"
                        }
                    }
                }
                if is_agent_cap {
                    // 协议口味（仅对话模型有意义：Embedding 走 embed 端点、Decision 走小脑链路）
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "调用协议" }
                        }
                        select {
                            class: "select select-bordered w-full",
                            "data-testid": "mp-create-api-flavor",
                            value: "{new_api_flavor_str}",
                            onchange: move |e| {
                                new_api_flavor.set(match e.value().as_str() {
                                    "chat_completions" => ChatApiFlavor::ChatCompletions,
                                    _ => ChatApiFlavor::Responses,
                                });
                            },
                            option { value: "responses", "Responses（推荐）" }
                            option { value: "chat_completions", "Chat Completions（兼容回退）" }
                        }
                        p { class: "text-xs text-base-content/60",
                            "Responses 支持工具调用、推理项回传与上下文缓存；自建网关或老旧兼容层无 /responses 端点时改选 Chat Completions"
                        }
                    }
                    // 思考强度（Provider 默认值；不启用时请求体完全不下发该字段）
                    ThinkingIntensityField {
                        enabled: new_thinking_intensity_enabled(),
                        score: new_thinking_intensity(),
                        label: "思考强度".to_string(),
                        testid: "mp-create-thinking-intensity".to_string(),
                        on_toggle: move |v| new_thinking_intensity_enabled.set(v),
                        on_score: move |v| new_thinking_intensity.set(v),
                    }
                    // 批5-P2：vision 能力开关（仅对话模型显示；Embedding/Decision 隐藏，独立分支防 Decision 误显）
                    label { class: "flex cursor-pointer select-none items-center gap-2",
                        input {
                            class: "checkbox checkbox-sm checkbox-primary",
                            r#type: "checkbox",
                            checked: new_supports_vision(),
                            onchange: move |e| new_supports_vision.set(e.checked()),
                        }
                        span { class: "text-sm font-medium", "支持图像输入（vision）" }
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
                            if is_embedding_cap {
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
                        oninput: move |e| base_url.set(e.value()), placeholder: "{base_url_placeholder}" }
                    p { class: "text-xs text-base-content/60",
                        "只填 API 根地址，勿包含 /embeddings、/chat/completions 等端点路径（系统会自动拼接）"
                    }
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
                            span { class: "label-text font-medium",
                                if is_embedding_cap {
                                    "最大上下文长度"
                                } else {
                                    "最大上下文长度 *"
                                }
                            }
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
                            placeholder: "留空按最大值 60% 计算" }
                    }
                }
                p { class: "text-xs text-base-content/60",
                    "最大上下文长度为模型支持的 token 上限，对话模型必填——压缩触发阈值以它为基准；推荐上下文长度可留空，届时按最大值 60% 自动计算"
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
                    if test_loading() {
                        "测试中..."
                    } else if test_capability() == ModelCapability::Embedding {
                        "开始测试"
                    } else {
                        "发送"
                    }
                }
            },
            div { class: "space-y-4",
                if test_capability() == ModelCapability::Embedding {
                    // 向量模型没有对话端点：用固定短句探测，结果展示维度与耗时
                    p { class: "text-sm text-base-content/70",
                        "向量模型没有对话端点，将使用固定短句做向量化连通测试，返回向量维度与耗时。"
                    }
                } else {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium",
                                if test_capability() == ModelCapability::Decision {
                                    "待判定内容"
                                } else {
                                    "Prompt"
                                }
                            }
                        }
                        textarea { class: "textarea textarea-bordered w-full", rows: "4", value: "{test_prompt}",
                            oninput: move |e| test_prompt.set(e.value()) }
                    }
                }
                if !test_response().is_empty() {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium",
                                if test_capability() == ModelCapability::Agent {
                                    "响应"
                                } else {
                                    "测试结果"
                                }
                            }
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

#[cfg(test)]
mod batch5_p2_tests {
    /// 批5-P2 结构守卫：创建 Modal vision checkbox 与列表徽标锚点存在且唯一。
    /// 源码结构断言（include_str!，零运行时依赖），任一渲染面回退即失败。
    #[test]
    fn batch5_p2_vision_ui_anchors_present() {
        let src = include_str!("model_providers.rs");

        // 断言仅作用于首个测试模块之前的代码区（切出测试模块自身，防测试字面量自匹配——批3 同款教训规避）
        let code = src.split("#[cfg(test)]").next().expect("code region");
        // checkbox 事件绑定各恰 1
        assert_eq!(code.matches("checked: new_supports_vision(),").count(), 1);
        assert_eq!(
            code.matches("onchange: move |e| new_supports_vision.set(e.checked()),")
                .count(),
            1
        );
        // SSE hint 先于 checkbox（checkbox 位于访问模式块之后）
        let hint_pos = code
            .find("仅当下游网关不支持 SSE 流式时选择非流式")
            .expect("access_mode hint");
        let cb_pos = code
            .find("checked: new_supports_vision(),")
            .expect("vision checkbox");
        assert!(hint_pos < cb_pos, "vision checkbox 应位于访问模式块之后");
        // Agent 分支恰 2（类型选项分组 + 表单独立分支，防 Decision 误显）
        assert_eq!(code.matches("if is_agent_cap {").count(), 2);
        // 列表徽标：图像 badge 恰 1（true 显 false 隐）
        assert_eq!(code.matches("if p.supports_vision {").count(), 1);
        assert_eq!(code.matches("\"图像\"").count(), 1);
        // 提交上送恰 1 且带 Agent 守卫
        assert_eq!(code.matches("supports_vision: Some(").count(), 1);
        assert!(code.contains("is_agent_cap && new_supports_vision()"));
        // P1 机械接线注释已由 P2 接管
        assert!(!code.contains("批5-P1 机械接线"));
    }

    /// 协议口味 UI 守卫：创建 Modal 下拉存在且唯一、默认主线 Responses、
    /// 列表仅回退态显徽标，且下拉位于「仅对话模型」分支内。
    #[test]
    fn api_flavor_ui_anchors_present_and_defaults_to_responses() {
        let src = include_str!("model_providers.rs");
        let code = src.split("#[cfg(test)]").next().expect("code region");

        // 下拉与两个 option 各恰 1（主线在前，回退在后）
        assert_eq!(
            code.matches("\"data-testid\": \"mp-create-api-flavor\",")
                .count(),
            1
        );
        assert_eq!(
            code.matches("option { value: \"responses\", \"Responses（推荐）\" }")
                .count(),
            1
        );
        assert_eq!(
            code.matches(
                "option { value: \"chat_completions\", \"Chat Completions（兼容回退）\" }"
            )
            .count(),
            1
        );
        // 默认主线协议：初值与重置值均为 Responses（与 ChatApiFlavor::default() 同语义）
        assert!(code.contains("use_signal(|| ChatApiFlavor::Responses)"));
        assert!(code.contains("new_api_flavor.set(ChatApiFlavor::Responses);"));
        // 创建上送恰 1：仅 Agent 能力下传，非 Agent 传 None（不落盘）
        assert_eq!(
            code.matches("api_flavor: is_agent_cap.then_some(new_api_flavor()),")
                .count(),
            1
        );
        // 列表仅回退态显徽标（主线不显，避免每行噪音）
        assert_eq!(
            code.matches("if matches!(p.api_flavor, ChatApiFlavor::ChatCompletions) {")
                .count(),
            1
        );
        // 下拉必须仍在唯一的「仅对话模型」分支内：Embedding 走 embed 端点、
        // Decision 走小脑链路，二者都无 chat 协议语义（防分支复制导致误显）
        // 2 = 类型选项分组 + 表单 Agent 分支
        assert_eq!(code.matches("if is_agent_cap {").count(), 2);
    }

    /// 类型 ↔ 能力联动守卫：选向量专用类型自动切 Embedding、选 Jev 自动切 Decision；
    /// 类型下拉按能力过滤且补回 FastEmbed option（历史缺陷：onchange 分支有、option 缺失）。
    #[test]
    fn provider_type_capability_linkage_anchors_present() {
        let src = include_str!("model_providers.rs");
        let code = src.split("#[cfg(test)]").next().expect("code region");

        // 类型 → 能力联动分支存在
        assert!(code.contains("ProviderType::DoubaoVision | ProviderType::FastEmbed => {"));
        assert!(code.contains("ProviderType::Jev => {"));
        assert!(
            code.contains("new_capability.set(ModelCapability::Embedding as i32)"),
            "向量专用类型必须自动切 Embedding"
        );
        assert!(
            code.contains("new_capability.set(ModelCapability::Decision as i32)"),
            "Jev 必须自动切 Decision"
        );
        // FastEmbed 选项存在（此前只有 onchange 分支，没有 option）
        assert_eq!(
            code.matches("option { value: \"fastembed\", \"FastEmbed（本地向量模型）\" }")
                .count(),
            1
        );
        // 类型选项按能力分组过滤：决策组恰 1，Embedding 组至少 1
        assert_eq!(code.matches("if is_decision_cap {").count(), 1);
    }
}
