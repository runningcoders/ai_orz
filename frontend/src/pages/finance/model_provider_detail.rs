//! 模型提供商详情页

use crate::api::finance::{
    call_model_provider, delete_model_provider, get_model_provider, switch_embedding_provider,
    test_model_provider_connection, toggle_model_provider, update_model_provider,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::ModelProviderStatsPanel;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    CallModelRequest, GetModelProviderRequest, SwitchEmbeddingProviderRequest,
    UpdateModelProviderRequest, UpdateModelProviderStatusRequest,
};
use common::enums::ProviderType;
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

/// 构造带模型调用统计的 GetModelProviderRequest
fn build_provider_stats_request(id: String) -> GetModelProviderRequest {
    GetModelProviderRequest {
        id,
        with_model_call_stats: Some(true),
        stats_interval: Some("daily".to_string()),
        ..Default::default()
    }
}

#[component]
pub fn FinanceModelProviderDetail(id: String) -> Element {
    // 方案 B：响应式 rid + use_resource，拉取仅在 :id 变化时触发
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(|| String::new());
    if let crate::pages::Route::FinanceModelProviderDetail { id: route_id } = &route {
        if *rid.peek() != *route_id {
            rid.set(route_id.clone());
        }
    }
    let mut provider_res = use_resource(move || {
        let id = rid();
        async move { get_model_provider(build_provider_stats_request(id)).await }
    });
    let toast = use_toast();
    let navigator = use_navigator();

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    let mut show_test_modal = use_signal(|| false);
    let mut test_prompt = use_signal(|| "你好，请介绍一下自己".to_string());
    let mut test_response = use_signal(String::new);
    let mut test_loading = use_signal(|| false);

    let mut show_switch_modal = use_signal(|| false);
    let mut switch_provider_name = use_signal(String::new);
    let mut switch_loading = use_signal(|| false);

    // ===== 编辑 Modal =====
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_name = use_signal(String::new);
    let mut edit_provider_type = use_signal(String::new);
    let mut edit_model_name = use_signal(String::new);
    let mut edit_api_key = use_signal(String::new);
    let mut edit_base_url = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_max_context_length = use_signal(String::new);
    let mut edit_recommended_context_length = use_signal(String::new);
    let mut saving_meta = use_signal(|| false);

    // 加载由上方 use_resource（rid 响应式）负责，无需 use_effect

    let handle_test_send = move |_| {
        spawn(async move {
            test_loading.set(true);
            if let Some(p) = provider_res.read().as_ref().and_then(|r| r.as_ref().ok()) {
                match call_model_provider(CallModelRequest {
                    id: p.id.clone(),
                    prompt: test_prompt(),
                })
                .await
                {
                    Ok(resp) => test_response.set(resp.result),
                    Err(e) => toast.error(format!("调用测试失败: {}", e)),
                }
            }
            test_loading.set(false);
        });
    };

    let switch_id = id.clone();
    let handle_switch_confirm = move |_| {
        let reload_id = switch_id.clone();
        let provider_id = provider_res
            .read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|p| p.id.clone());
        spawn(async move {
            switch_loading.set(true);
            if let Some(pid) = provider_id {
                match switch_embedding_provider(SwitchEmbeddingProviderRequest {
                    id: pid.clone(),
                    confirm: true,
                })
                .await
                {
                    Ok(resp) => {
                        show_switch_modal.set(false);
                        toast.success(format!(
                            "Embedding Provider 已切换为 {}，向量索引重建完成",
                            resp.name
                        ));
                        match get_model_provider(build_provider_stats_request(reload_id)).await {
                            Ok(provider) => provider_res.set(Some(Ok(provider))),
                            Err(e) => toast.error(&e),
                        }
                    }
                    Err(e) => {
                        show_switch_modal.set(false);
                        toast.error(format!("切换失败: {}", e));
                    }
                }
            }
            switch_loading.set(false);
        });
    };

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                div {
                    h1 { class: "text-2xl font-bold", "模型提供商详情" }
                }
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceModelProviders {},
                    "← 返回列表"
                }
            }

            match provider_res.read().as_ref() {
                None => rsx! { Loading {} },
                Some(Ok(p)) => {
                    let p = p.clone();
                    rsx! {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{p.name}" }
                            div { class: "flex gap-2",
                                {
                                    let is_embedding = p.capability.is_embedding();
                                    let is_enabled = p.status == 1;
                                    let provider_id = p.id.clone();
                                    let provider_name = p.name.clone();
                                    let reload_id = id.clone();
                                    let edit_name_init = p.name.clone();
                                    let edit_provider_type_init = format!("{:?}", p.provider_type);
                                    let edit_model_name_init = p.model_name.clone();
                                    let edit_base_url_init = p.base_url.clone().unwrap_or_default();
                                    let edit_description_init = p.description.clone().unwrap_or_default();
                                    let edit_max_ctx_init = p
                                        .max_context_length
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    let edit_rec_ctx_init = p
                                        .recommended_context_length
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    rsx! {
                                        if is_enabled {
                                            button { class: "btn btn-outline btn-sm",
                                                onclick: {
                                                    let pid = provider_id.clone();
                                                    let rid = reload_id.clone();
                                                    move |_| {
                                                        let pid = pid.clone();
                                                        let rid = rid.clone();
                                                        spawn(async move {
                                                            match toggle_model_provider(UpdateModelProviderStatusRequest { id: pid, status: 0 }).await {
                                                                Ok(()) => {
                                                                    toast.success("已禁用");
                                                                    match get_model_provider(build_provider_stats_request(rid)).await {
                                                                        Ok(provider) => provider_res.set(Some(Ok(provider))),
                                                                        Err(e) => toast.error(&e),
                                                                    }
                                                                }
                                                                Err(e) => toast.error(format!("禁用失败: {}", e)),
                                                            }
                                                        });
                                                    }
                                                },
                                                "禁用"
                                            }
                                        } else {
                                            button { class: "btn btn-primary btn-sm",
                                                onclick: {
                                                    let pid = provider_id.clone();
                                                    let pname = provider_name.clone();
                                                    let rid = reload_id.clone();
                                                    move |_| {
                                                        let pid = pid.clone();
                                                        let pname = pname.clone();
                                                        let rid = rid.clone();
                                                        let is_emb = is_embedding;
                                                        spawn(async move {
                                                            if is_emb {
                                                                match toggle_model_provider(UpdateModelProviderStatusRequest { id: pid, status: 1 }).await {
                                                                    Ok(()) => {
                                                                        toast.success("已启用");
                                                                        match get_model_provider(build_provider_stats_request(rid)).await {
                                                                            Ok(provider) => provider_res.set(Some(Ok(provider))),
                                                                            Err(e) => toast.error(&e),
                                                                        }
                                                                    }
                                                                    Err(e) => {
                                                                        if e.error_code.as_deref() == Some("embedding_provider_switch_required") {
                                                                            switch_provider_name.set(pname);
                                                                            show_switch_modal.set(true);
                                                                        } else {
                                                                            toast.error(format!("启用失败: {}", e));
                                                                        }
                                                                    }
                                                                }
                                                            } else {
                                                                match toggle_model_provider(UpdateModelProviderStatusRequest { id: pid, status: 1 }).await {
                                                                    Ok(()) => {
                                                                        toast.success("已启用");
                                                                        match get_model_provider(build_provider_stats_request(rid)).await {
                                                                            Ok(provider) => provider_res.set(Some(Ok(provider))),
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
                                        button {
                                            class: "btn btn-ghost btn-sm",
                                            onclick: move |_| {
                                                edit_name.set(edit_name_init.clone());
                                                edit_provider_type.set(edit_provider_type_init.clone());
                                                edit_model_name.set(edit_model_name_init.clone());
                                                edit_api_key.set(String::new());
                                                edit_base_url.set(edit_base_url_init.clone());
                                                edit_description.set(edit_description_init.clone());
                                                edit_max_context_length.set(edit_max_ctx_init.clone());
                                                edit_recommended_context_length.set(edit_rec_ctx_init.clone());
                                                show_edit_modal.set(true);
                                            },
                                            "✏️ 编辑"
                                        }
                                        button { class: "btn btn-outline btn-sm",
                                            onclick: {
                                                let pid = provider_id.clone();
                                                move |_| {
                                                    let pid = pid.clone();
                                                    spawn(async move {
                                                        match test_model_provider_connection(&pid).await {
                                                            Ok(_) => toast.success("连接测试通过"),
                                                            Err(e) => toast.error(format!("连接测试失败: {}", e)),
                                                        }
                                                    });
                                                }
                                            },
                                            "测试连接"
                                        }
                                        button { class: "btn btn-outline btn-sm",
                                            onclick: move |_| {
                                                test_prompt.set("你好，请介绍一下自己".to_string());
                                                test_response.set(String::new());
                                                show_test_modal.set(true);
                                            },
                                            "调用测试"
                                        }
                                        button { class: "btn btn-error btn-sm",
                                            onclick: {
                                                let pid = provider_id.clone();
                                                move |_| {
                                                    pending_delete_id.set(pid.clone());
                                                    show_delete_confirm.set(true);
                                                }
                                            },
                                            "删除"
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "模型名称" }
                                }
                                div { class: "font-mono", "{p.model_name}" }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "类型" }
                                }
                                div { span { class: "badge badge-info", "{p.provider_type}" } }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "能力" }
                                }
                                div {
                                    if p.capability.is_embedding() {
                                        span { class: "badge badge-warning", "embedding" }
                                    } else {
                                        span { class: "badge badge-success", "agent" }
                                    }
                                }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "状态" }
                                }
                                div {
                                    if p.status == 1 {
                                        span { class: "badge badge-success", "启用" }
                                    } else {
                                        span { class: "badge badge-neutral", "禁用" }
                                    }
                                }
                            }
                            if let Some(base_url) = &p.base_url {
                                div {
                                    label { class: "label",
                                        span { class: "label-text font-medium", "Base URL" }
                                    }
                                    div { class: "font-mono", "{base_url}" }
                                }
                            }
                            if let Some(description) = &p.description {
                                div { class: "md:col-span-2",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "描述" }
                                    }
                                    MarkdownRenderer { content: description.clone(), compact: true }
                                }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "提供商 ID" }
                                }
                                div { class: "font-mono text-sm", "{p.id}" }
                            }
                            if let Some(max_ctx) = p.max_context_length {
                                div {
                                    label { class: "label",
                                        span { class: "label-text font-medium", "最大上下文长度" }
                                    }
                                    div { class: "font-mono", "{max_ctx} tokens" }
                                }
                            }
                            if let Some(rec_ctx) = p.recommended_context_length {
                                div {
                                    label { class: "label",
                                        span { class: "label-text font-medium", "推荐上下文长度" }
                                    }
                                    div { class: "font-mono", "{rec_ctx} tokens" }
                                }
                            }
                        }
                    }
                }

                if p.stats.is_some() {
                    ModelProviderStatsPanel { stats: p.stats.clone() }
                }

                Modal {
                    title: "调用测试".to_string(),
                    show: show_test_modal(),
                    on_close: move |_| show_test_modal.set(false),
                    footer: rsx! {
                        button { class: "btn btn-ghost", onclick: move |_| show_test_modal.set(false), "关闭" }
                        button { class: "btn btn-primary", disabled: test_loading(), onclick: handle_test_send,
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
                        button { class: "btn btn-ghost", onclick: move |_| show_switch_modal.set(false), "取消" }
                        button { class: "btn btn-warning", disabled: switch_loading(), onclick: handle_switch_confirm,
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
                    }
                }
                Some(Err(e)) => rsx! {
                    EmptyState { icon: "❓".to_string(), message: format!("加载失败: {}", e) }
                },
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此模型提供商？此操作不可撤销。".to_string(),
                on_confirm: move |_| {
                    let id = pending_delete_id();
                    show_delete_confirm.set(false);
                    spawn(async move {
                        if let Err(e) = delete_model_provider(&id).await {
                            toast.error(format!("删除失败: {}", e));
                        } else {
                            toast.success("已删除");
                            let _ = navigator.push("/finance/model-providers".to_string());
                        }
                    });
                },
                on_cancel: move |_| {
                    show_delete_confirm.set(false);
                }
            }

            Modal {
                title: "编辑模型提供商".to_string(),
                show: show_edit_modal(),
                on_close: move |_| show_edit_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                    button {
                        class: "btn btn-primary",
                        disabled: saving_meta(),
                        onclick: {
                            let id_for_submit = id.clone();
                            move |_| {
                                let name = edit_name().trim().to_string();
                                if name.is_empty() {
                                    toast.error("名称不能为空");
                                    return;
                                }
                                let provider_type = match edit_provider_type().as_str() {
                                    "OpenAI" => ProviderType::OpenAI,
                                    "DeepSeek" => ProviderType::DeepSeek,
                                    "Qwen" => ProviderType::Qwen,
                                    "Doubao" => ProviderType::Doubao,
                                    "Ollama" => ProviderType::Ollama,
                                    "Custom" => ProviderType::Custom,
                                    "FastEmbed" => ProviderType::FastEmbed,
                                    "DoubaoVision" => ProviderType::DoubaoVision,
                                    _ => ProviderType::OpenAI,
                                };
                                let api_key = if edit_api_key().is_empty() { None } else { Some(edit_api_key()) };
                                let base_url = if edit_base_url().trim().is_empty() { None } else { Some(edit_base_url()) };
                                let description = if edit_description().trim().is_empty() { None } else { Some(edit_description()) };
                                // 空字符串 → 0（清除配置）；有值 → 解析为数字
                                let max_ctx = edit_max_context_length()
                                    .trim()
                                    .parse::<i32>()
                                    .unwrap_or(0);
                                let rec_ctx = edit_recommended_context_length()
                                    .trim()
                                    .parse::<i32>()
                                    .unwrap_or(0);
                                let req = UpdateModelProviderRequest {
                                    id: id_for_submit.clone(),
                                    name: Some(name),
                                    provider_type: Some(provider_type),
                                    model_name: Some(edit_model_name()),
                                    api_key,
                                    base_url,
                                    description,
                                    status: None,
                                    max_context_length: Some(max_ctx),
                                    recommended_context_length: Some(rec_ctx),
                                };
                                saving_meta.set(true);
                                let reload_id = id_for_submit.clone();
                                spawn(async move {
                                    match update_model_provider(req).await {
                                        Ok(_) => {
                                            toast.success("已更新");
                                            show_edit_modal.set(false);
                                            match get_model_provider(build_provider_stats_request(reload_id)).await {
                                                Ok(p) => provider_res.set(Some(Ok(p))),
                                                Err(e) => toast.error(format!("重新加载失败: {}", e)),
                                            }
                                        }
                                        Err(e) => toast.error(format!("更新失败: {}", e)),
                                    }
                                    saving_meta.set(false);
                                });
                            }
                        },
                        if saving_meta() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "名称 *" } }
                        input { class: "input input-bordered w-full", value: "{edit_name}",
                            oninput: move |e| edit_name.set(e.value()) }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "提供商类型" } }
                        select {
                            class: "select select-bordered w-full",
                            value: "{edit_provider_type}",
                            onchange: move |e| edit_provider_type.set(e.value()),
                            option { value: "OpenAI", "OpenAI" }
                            option { value: "DeepSeek", "DeepSeek" }
                            option { value: "Qwen", "通义千问" }
                            option { value: "Doubao", "豆包" }
                            option { value: "Ollama", "Ollama" }
                            option { value: "Custom", "自定义" }
                            option { value: "FastEmbed", "FastEmbed" }
                            option { value: "DoubaoVision", "豆包 Vision (多模态)" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "模型名称" } }
                        input { class: "input input-bordered w-full", value: "{edit_model_name}",
                            oninput: move |e| edit_model_name.set(e.value()) }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "API Key（留空不修改）" } }
                        input { class: "input input-bordered w-full", r#type: "password", value: "{edit_api_key}",
                            oninput: move |e| edit_api_key.set(e.value()), placeholder: "输入新 Key 或留空保持不变" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "Base URL" } }
                        input { class: "input input-bordered w-full", value: "{edit_base_url}",
                            oninput: move |e| edit_base_url.set(e.value()), placeholder: "https://api.openai.com/v1" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "描述" } }
                        textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                            oninput: move |e| edit_description.set(e.value()) }
                    }
                    div { class: "grid grid-cols-2 gap-4",
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "最大上下文长度" } }
                            input { class: "input input-bordered w-full", r#type: "number",
                                value: "{edit_max_context_length}",
                                oninput: move |e| edit_max_context_length.set(e.value()),
                                placeholder: "如：128000" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "推荐上下文长度" } }
                            input { class: "input input-bordered w-full", r#type: "number",
                                value: "{edit_recommended_context_length}",
                                oninput: move |e| edit_recommended_context_length.set(e.value()),
                                placeholder: "留空自动计算" }
                        }
                    }
                    p { class: "text-xs text-base-content/60",
                        "清空输入框将清除对应配置（回退为自动计算）；推荐上下文长度留空时按最大值 60% 自动计算"
                    }
                }
            }
        }
    }
}
