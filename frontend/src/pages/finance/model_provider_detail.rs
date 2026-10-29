//! 模型提供商详情页

use crate::api::finance::{
    call_model_provider, delete_model_provider, get_model_provider, switch_embedding_provider,
    test_model_provider_connection, toggle_model_provider, update_model_provider,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudPanel, PageHeader};
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::ModelProviderStatsPanel;
use crate::components::thinking_intensity_field::{
    ThinkingIntensityField, thinking_intensity_label,
};
use crate::components::time_range_picker::{TimeRange, TimeRangePicker};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    CallModelRequest, GetModelProviderRequest, SwitchEmbeddingProviderRequest,
    UpdateModelProviderRequest,
};
use common::enums::{
    ChatApiFlavor, ModelAccessMode, ModelCapability, ProviderType, ThinkingIntensity,
};
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

/// 构造带模型调用统计的 GetModelProviderRequest
///
/// `range` 为统计时间窗口（详情页时间筛选器产出，毫秒闭区间）。注意本接口字段命名为
/// `stats_start_time` / `stats_end_time`（与其它实体的 `stats_time_start` 不同），
/// 且后端用 `zip` 配对，必须成对提供才生效。
fn build_provider_stats_request(id: String, range: TimeRange) -> GetModelProviderRequest {
    GetModelProviderRequest {
        id,
        with_model_call_stats: Some(true),
        stats_start_time: Some(range.start_ms),
        stats_end_time: Some(range.end_ms),
        stats_interval: Some(range.suggested_interval().to_string()),
    }
}

/// access_mode → select 绑定键（编辑 Modal 回显；T2 设计 §4.2）
fn access_mode_key(m: &ModelAccessMode) -> &'static str {
    match m {
        ModelAccessMode::Stream => "stream",
        ModelAccessMode::NonStream => "non_stream",
    }
}

/// access_mode → 展示文案（详情信息格徽标；T2 设计 §4.4/§5）
fn access_mode_label(m: &ModelAccessMode) -> &'static str {
    match m {
        ModelAccessMode::Stream => "流式",
        ModelAccessMode::NonStream => "非流式",
    }
}

/// api_flavor → select 绑定键（编辑 Modal 回显；沿 access_mode_key 先例）
fn api_flavor_key(f: &ChatApiFlavor) -> &'static str {
    match f {
        ChatApiFlavor::Responses => "responses",
        ChatApiFlavor::ChatCompletions => "chat_completions",
    }
}

/// api_flavor → 展示文案（详情信息格徽标）
fn api_flavor_label(f: &ChatApiFlavor) -> &'static str {
    match f {
        ChatApiFlavor::Responses => "Responses",
        ChatApiFlavor::ChatCompletions => "Chat Completions",
    }
}

#[component]
pub fn FinanceModelProviderDetail(id: String) -> Element {
    // 方案 B：响应式 rid + use_resource，拉取仅在 :id 变化时触发
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::FinanceModelProviderDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    // 统计时间窗口（时间筛选器产出，默认最近 7 天）：变化时 use_resource 重拉
    let mut stats_range = use_signal(TimeRange::default);
    // 稳定回调（hook 必须在组件顶层无条件调用，不能写在 rsx 条件分支里）
    let on_stats_range = use_callback(move |r: TimeRange| stats_range.set(r));
    let mut provider_res = use_resource(move || {
        let id = rid();
        let range = stats_range();
        async move { get_model_provider(build_provider_stats_request(id, range)).await }
    });
    let toast = use_toast();
    let navigator = use_navigator();

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    let mut show_test_modal = use_signal(|| false);
    let mut test_capability = use_signal(|| ModelCapability::Agent);
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
    let mut edit_access_mode = use_signal(|| "stream".to_string());
    let mut edit_supports_vision = use_signal(|| false); // 批5-P2：vision 能力开关（编辑 Modal）
    // 协议口味（编辑 Modal 回显；初值 Responses 与 ChatApiFlavor::default() 同语义）
    let mut edit_api_flavor = use_signal(|| "responses".to_string());
    // 思考强度（编辑 Modal 回显；默认不启用 = 请求体不下发该字段）
    let mut edit_thinking_intensity_enabled = use_signal(|| false);
    let mut edit_thinking_intensity = use_signal(|| ThinkingIntensity::DEFAULT.score());
    let mut saving_meta = use_signal(|| false);

    // 当前提供商是否 Embedding：编辑表单的上下文长度必填校验与标签用它。
    // Embedding 无对话上下文概念，不参与压缩阈值计算，跳过必填。
    let editing_is_embedding = provider_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .is_some_and(|p| p.capability == ModelCapability::Embedding);

    // 当前提供商是否对话模型：编辑 Modal 的 vision 开关显示条件用它（Modal 不在 p 作用域，沿 editing_is_embedding 派生先例）。
    let editing_is_agent = provider_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .is_some_and(|p| p.capability == ModelCapability::Agent);

    // 当前提供商是否决策模型：编辑 Modal 的类型选项按能力过滤用它
    let editing_is_decision = provider_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .is_some_and(|p| p.capability == ModelCapability::Decision);

    // Base URL 占位符随类型变化：方舟与 OpenAI 根地址不同，且都只填根路径
    let edit_base_url_placeholder = match edit_provider_type().as_str() {
        "Doubao" | "DoubaoVision" => "https://ark.cn-beijing.volces.com/api/v3",
        "FastEmbed" => "本地模型无需填写",
        _ => "https://api.openai.com/v1",
    };

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
                        match get_model_provider(build_provider_stats_request(
                            reload_id,
                            stats_range(),
                        ))
                        .await
                        {
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
            PageHeader {
                eyebrow: "FINANCE".to_string(),
                title: "模型提供商详情".to_string(),
                actions: Some(rsx! {
                    Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::FinanceModelProviders {},
                        "← 返回列表"
                    }
                }),
            }

            match provider_res.read().as_ref() {
                None => rsx! { Loading {} },
                Some(Ok(p)) => {
                    let p = p.clone();
                    rsx! {
                HudPanel {
                    title: p.name.clone(),
                    eyebrow: "PROVIDER".to_string(),
                    signal: true,
                    div { class: "card-body",
                        div { class: "flex justify-end mb-4",
                            div { class: "flex gap-2",
                                {
                                    let is_embedding = p.capability.is_embedding();
                                    let provider_capability = p.capability;
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
                                    let edit_access_mode_init = access_mode_key(&p.access_mode).to_string();
                                    let edit_supports_vision_init = p.supports_vision; // 批5-P2：回显（Get 恒返回 bool 无 None 态）
                                    let edit_api_flavor_init = api_flavor_key(&p.api_flavor).to_string();
                                    let edit_thinking_intensity_init = p.thinking_intensity;
                                    rsx! {
                                        if is_enabled {
                                            button { class: "btn hud-btn btn-outline btn-sm",
                                                onclick: {
                                                    let pid = provider_id.clone();
                                                    let rid = reload_id.clone();
                                                    move |_| {
                                                        let pid = pid.clone();
                                                        let rid = rid.clone();
                                                        spawn(async move {
                                                            match toggle_model_provider(&pid, 0).await {
                                                                Ok(()) => {
                                                                    toast.success("已禁用");
                                                                    match get_model_provider(build_provider_stats_request(rid, stats_range())).await {
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
                                            button { class: "btn hud-btn btn-primary btn-sm",
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
                                                                match toggle_model_provider(&pid, 1).await {
                                                                    Ok(()) => {
                                                                        toast.success("已启用");
                                                                        match get_model_provider(build_provider_stats_request(rid, stats_range())).await {
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
                                                                match toggle_model_provider(&pid, 1).await {
                                                                    Ok(()) => {
                                                                        toast.success("已启用");
                                                                        match get_model_provider(build_provider_stats_request(rid, stats_range())).await {
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
                                            class: "btn hud-btn btn-ghost btn-sm",
                                            onclick: move |_| {
                                                edit_name.set(edit_name_init.clone());
                                                edit_provider_type.set(edit_provider_type_init.clone());
                                                edit_model_name.set(edit_model_name_init.clone());
                                                edit_api_key.set(String::new());
                                                edit_base_url.set(edit_base_url_init.clone());
                                                edit_description.set(edit_description_init.clone());
                                                edit_max_context_length.set(edit_max_ctx_init.clone());
                                                edit_recommended_context_length.set(edit_rec_ctx_init.clone());
                                                edit_access_mode.set(edit_access_mode_init.clone());
                                                edit_supports_vision.set(edit_supports_vision_init); // 批5-P2：回显 vision 开关
                                                edit_api_flavor.set(edit_api_flavor_init.clone()); // 回显协议口味
                                                // 回显思考强度：未配置 = 开关关闭，滑块落在平台默认分
                                                edit_thinking_intensity_enabled
                                                    .set(edit_thinking_intensity_init.is_some());
                                                edit_thinking_intensity.set(
                                                    edit_thinking_intensity_init
                                                        .map_or(ThinkingIntensity::DEFAULT.score(), |v| v.score()),
                                                );
                                                show_edit_modal.set(true);
                                            },
                                            "✏️ 编辑"
                                        }
                                        button { class: "btn hud-btn btn-outline btn-sm",
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
                                        // 向量模型没有对话端点：「测试连接」已是完整的向量化探针，
                                        // 不再重复提供语义等价的「调用测试」入口
                                        if !is_embedding {
                                            button { class: "btn hud-btn btn-outline btn-sm",
                                                onclick: move |_| {
                                                    test_capability.set(provider_capability);
                                                    test_prompt.set(match provider_capability {
                                                        ModelCapability::Decision => {
                                                            "这是一次连通性测试".to_string()
                                                        }
                                                        _ => "你好，请介绍一下自己".to_string(),
                                                    });
                                                    test_response.set(String::new());
                                                    show_test_modal.set(true);
                                                },
                                                "调用测试"
                                            }
                                        }
                                        button { class: "btn hud-btn btn-error btn-sm",
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
                                div { span { class: "badge orz-tag badge-sm", "{p.provider_type}" } }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "能力" }
                                }
                                div {
                                    if p.capability.is_embedding() {
                                        span { class: "badge orz-tag badge-sm", "embedding" }
                                    } else {
                                        span { class: "badge orz-tag badge-sm", "agent" }
                                    }
                                }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "状态" }
                                }
                                div {
                                    if p.status == 1 {
                                        span { class: "badge hud-badge badge-success", "启用" }
                                    } else {
                                        span { class: "badge hud-badge badge-neutral", "禁用" }
                                    }
                                }
                            }
                            if !p.capability.is_embedding() {
                                div {
                                    label { class: "label",
                                        span { class: "label-text font-medium", "访问模式" }
                                    }
                                    div {
                                        span { class: "badge hud-badge badge-ghost badge-sm",
                                            "{access_mode_label(&p.access_mode)}"
                                        }
                                    }
                                }
                            }
                            if !p.capability.is_embedding() {
                                div {
                                    label { class: "label",
                                        span { class: "label-text font-medium", "调用协议" }
                                    }
                                    div {
                                        span { class: "badge hud-badge badge-ghost badge-sm",
                                            "{api_flavor_label(&p.api_flavor)}"
                                        }
                                    }
                                }
                            }
                            // 思考强度仅对话模型展示；未配置（能力开关关闭）不显示
                            if !p.capability.is_embedding() {
                                if let Some(intensity) = p.thinking_intensity {
                                    div {
                                        label { class: "label",
                                            span { class: "label-text font-medium", "思考强度" }
                                        }
                                        div {
                                            span { class: "badge hud-badge badge-ghost badge-sm",
                                                "{thinking_intensity_label(intensity)}"
                                            }
                                        }
                                    }
                                }
                            }
                            // 向量模型不参与视觉输入，徽标即便存量 supports_vision=true 也不展示
                            if !p.capability.is_embedding() && p.supports_vision {
                                div {
                                    label { class: "label",
                                        span { class: "label-text font-medium", "图像输入" }
                                    }
                                    div {
                                        span { class: "badge hud-badge badge-ghost badge-sm", "支持图像" }
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
                    // 时间筛选：切换即重拉（use_resource 订阅 stats_range）
                    div { class: "mb-3",
                        TimeRangePicker { value: stats_range(), on_change: on_stats_range }
                    }
                    ModelProviderStatsPanel { stats: p.stats.clone() }
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
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                    button {
                        class: "btn hud-btn btn-primary",
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
    "Jev" => ProviderType::Jev,
                                    _ => ProviderType::OpenAI,
                                };
                                let access_mode = match edit_access_mode().as_str() {
                                    "non_stream" => ModelAccessMode::NonStream,
                                    _ => ModelAccessMode::Stream,
                                };
                                let api_flavor = match edit_api_flavor().as_str() {
                                    "chat_completions" => ChatApiFlavor::ChatCompletions,
                                    _ => ChatApiFlavor::Responses,
                                };
                                let api_key = if edit_api_key().is_empty() { None } else { Some(edit_api_key()) };
                                let base_url = if edit_base_url().trim().is_empty() { None } else { Some(edit_base_url()) };
                                let description = if edit_description().trim().is_empty() { None } else { Some(edit_description()) };
                                // 空字符串 → 0（清除配置）；有值 → 解析为数字
                                let max_ctx = edit_max_context_length()
                                    .trim()
                                    .parse::<i32>()
                                    .unwrap_or(0);
                                // 对话模型不允许清空最大上下文长度：清掉后压缩阈值无从计算，
                                // Agent 会静默地永不压缩（后端同样拒绝，这里前置提示）
                                if !editing_is_embedding && max_ctx <= 0 {
                                    toast.error("最大上下文长度不能为空（对话模型必填）");
                                    return;
                                }
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
                                    // 访问模式：Update 恒传当前选值（T2 §4.2/S3，无 None 分支）
                                    access_mode: Some(access_mode),
                                    // 批5-P2：vision 开关随保存上送，仅对话模型生效；
                                    // 非对话模型（Embedding/Decision）隐藏该开关，保守上送 false 防隐藏态残留
                                    supports_vision: Some(
                                        editing_is_agent && edit_supports_vision(),
                                    ),
                                    // 协议口味只对对话模型有意义：Embedding 走 /embeddings、Decision 走小脑链路，
                                    // 非 Agent 传 None（不落盘），避免把无关配置写进 config 后被误读
                                    api_flavor: editing_is_agent.then_some(api_flavor),
                                    // 思考强度整体替换（None = 不启用）；非对话模型不落盘
                                    thinking_intensity: (editing_is_agent
                                        && edit_thinking_intensity_enabled())
                                    .then(|| ThinkingIntensity::new(edit_thinking_intensity())),
                                };
                                saving_meta.set(true);
                                let reload_id = id_for_submit.clone();
                                spawn(async move {
                                    match update_model_provider(req).await {
                                        Ok(_) => {
                                            toast.success("已更新");
                                            show_edit_modal.set(false);
                                            match get_model_provider(build_provider_stats_request(reload_id, stats_range())).await {
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
                            // 类型选项按能力过滤：Jev 仅决策、向量专用端点仅 Embedding
                            if editing_is_agent {
                                option { value: "OpenAI", "OpenAI" }
                                option { value: "DeepSeek", "DeepSeek" }
                                option { value: "Qwen", "通义千问" }
                                option { value: "Doubao", "豆包" }
                                option { value: "Ollama", "Ollama" }
                                option { value: "Custom", "自定义" }
                            }
                            if editing_is_embedding {
                                option { value: "OpenAI", "OpenAI" }
                                option { value: "DeepSeek", "DeepSeek" }
                                option { value: "Qwen", "通义千问" }
                                option { value: "Doubao", "豆包" }
                                option { value: "DoubaoVision", "豆包 Vision (多模态 Embedding)" }
                                option { value: "Ollama", "Ollama" }
                                option { value: "Custom", "自定义" }
                                option { value: "FastEmbed", "FastEmbed（本地向量模型）" }
                            }
                            if editing_is_decision {
                                option { value: "Jev", "Jev（小脑决策模型）" }
                            }
                        }
                    }
                    if !editing_is_embedding {
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "访问模式" } }
                            select {
                                class: "select select-bordered w-full",
                                "data-testid": "mp-edit-access-mode",
                                value: "{edit_access_mode}",
                                onchange: move |e| edit_access_mode.set(e.value()),
                                option { value: "stream", "流式" }
                                option { value: "non_stream", "非流式" }
                            }
                            p { class: "text-xs text-base-content/60",
                                "仅当下游网关不支持 SSE 流式时选择非流式"
                            }
                        }
                    }
                    if editing_is_agent {
                        // 协议口味（仅对话模型有意义：Embedding 走 embed 端点、Decision 走小脑链路）
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "调用协议" } }
                            select {
                                class: "select select-bordered w-full",
                                "data-testid": "mp-edit-api-flavor",
                                value: "{edit_api_flavor}",
                                onchange: move |e| edit_api_flavor.set(e.value()),
                                option { value: "responses", "Responses（推荐）" }
                                option { value: "chat_completions", "Chat Completions（兼容回退）" }
                            }
                            p { class: "text-xs text-base-content/60",
                                "Responses 支持工具调用、推理项回传与上下文缓存；自建网关或老旧兼容层无 /responses 端点时改选 Chat Completions"
                            }
                        }
                        // 思考强度（Provider 默认值；不启用时请求体完全不下发该字段）
                        ThinkingIntensityField {
                            enabled: edit_thinking_intensity_enabled(),
                            score: edit_thinking_intensity(),
                            label: "思考强度".to_string(),
                            testid: "mp-edit-thinking-intensity".to_string(),
                            on_toggle: move |v| edit_thinking_intensity_enabled.set(v),
                            on_score: move |v| edit_thinking_intensity.set(v),
                        }
                        // 批5-P2：vision 能力开关（仅对话模型显示；Embedding/Decision 隐藏；
                        // 显示条件沿 editing_is_agent 派生——编辑 Modal 不在 p 作用域）
                        label { class: "flex cursor-pointer select-none items-center gap-2",
                            input {
                                class: "checkbox checkbox-sm checkbox-primary",
                                r#type: "checkbox",
                                checked: edit_supports_vision(),
                                onchange: move |e| edit_supports_vision.set(e.checked()),
                            }
                            span { class: "text-sm font-medium", "支持图像输入（vision）" }
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
                            oninput: move |e| edit_base_url.set(e.value()), placeholder: "{edit_base_url_placeholder}" }
                        p { class: "text-xs text-base-content/60",
                            "只填 API 根地址，勿包含 /embeddings、/chat/completions 等端点路径（系统会自动拼接）"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "描述" } }
                        textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                            oninput: move |e| edit_description.set(e.value()) }
                    }
                    div { class: "grid grid-cols-2 gap-4",
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium",
                                    if editing_is_embedding {
                                        "最大上下文长度"
                                    } else {
                                        "最大上下文长度 *"
                                    }
                                }
                            }
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
                                placeholder: "留空按最大值 60% 计算" }
                        }
                    }
                    p { class: "text-xs text-base-content/60",
                        if editing_is_embedding {
                            "Embedding 模型不参与上下文压缩，两项均可留空"
                        } else {
                            "最大上下文长度必填——压缩触发阈值以它为基准（推荐值缺省时取 60%），清空会让 Agent 永不压缩；推荐上下文长度可留空自动计算"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod access_mode_tests {
    use super::*;

    #[test]
    fn access_mode_key_maps_both_values() {
        assert_eq!(access_mode_key(&ModelAccessMode::Stream), "stream");
        assert_eq!(access_mode_key(&ModelAccessMode::NonStream), "non_stream");
    }

    #[test]
    fn access_mode_label_maps_both_values() {
        assert_eq!(access_mode_label(&ModelAccessMode::Stream), "流式");
        assert_eq!(access_mode_label(&ModelAccessMode::NonStream), "非流式");
    }

    /// 协议口味映射锁定：绑定键必须与后端 serde snake_case 契约逐字一致
    #[test]
    fn api_flavor_key_maps_both_values() {
        assert_eq!(api_flavor_key(&ChatApiFlavor::Responses), "responses");
        assert_eq!(
            api_flavor_key(&ChatApiFlavor::ChatCompletions),
            "chat_completions"
        );
    }

    #[test]
    fn api_flavor_label_maps_both_values() {
        assert_eq!(api_flavor_label(&ChatApiFlavor::Responses), "Responses");
        assert_eq!(
            api_flavor_label(&ChatApiFlavor::ChatCompletions),
            "Chat Completions"
        );
    }
}

#[cfg(test)]
mod batch5_p2_tests {
    /// 批5-P2 结构守卫：编辑 Modal vision checkbox 回显/恒传与信息格徽标锚点存在且唯一。
    /// 源码结构断言（include_str!，零运行时依赖），任一渲染面回退即失败。
    #[test]
    fn batch5_p2_vision_edit_ui_anchors_present() {
        let src = include_str!("model_provider_detail.rs");

        // 断言仅作用于首个测试模块之前的代码区（切出测试模块自身，防测试字面量自匹配——批3 同款教训规避）
        let code = src.split("#[cfg(test)]").next().expect("code region");
        // checkbox 事件绑定各恰 1
        assert_eq!(code.matches("checked: edit_supports_vision(),").count(), 1);
        assert_eq!(
            code.matches("onchange: move |e| edit_supports_vision.set(e.checked()),")
                .count(),
            1
        );
        // 回显链：init 与 set 各恰 1
        assert_eq!(
            code.matches("let edit_supports_vision_init = p.supports_vision;")
                .count(),
            1
        );
        assert_eq!(
            code.matches("edit_supports_vision.set(edit_supports_vision_init);")
                .count(),
            1
        );
        // 保存仅对话模型落盘（非 Agent → None）
        assert_eq!(
            code.matches("editing_is_agent && edit_supports_vision()")
                .count(),
            1
        );
        // 信息格徽标恰 1（true 显 false 隐，且向量模型不展示）
        assert_eq!(
            code.matches("if !p.capability.is_embedding() && p.supports_vision {")
                .count(),
            1
        );
        assert_eq!(code.matches("\"支持图像\"").count(), 1);
        // P1 机械接线注释已由 P2 接管
        assert!(!code.contains("批5-P1 机械接线"));
    }

    /// 协议口味 UI 守卫：编辑 Modal 下拉 + 回显链 + 恒传 + 信息格徽标，各锚点唯一。
    #[test]
    fn api_flavor_edit_ui_anchors_present() {
        let src = include_str!("model_provider_detail.rs");
        let code = src.split("#[cfg(test)]").next().expect("code region");

        // 下拉与两个 option 各恰 1
        assert_eq!(
            code.matches("\"data-testid\": \"mp-edit-api-flavor\",")
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
        // 回显链：init 与 set 各恰 1（防「打开编辑不回显 → 保存覆盖成默认值」）
        assert_eq!(
            code.matches("let edit_api_flavor_init = api_flavor_key(&p.api_flavor).to_string();")
                .count(),
            1
        );
        assert_eq!(
            code.matches("edit_api_flavor.set(edit_api_flavor_init.clone());")
                .count(),
            1
        );
        // 保存仅对话模型落盘恰 1（非 Agent → None）
        assert_eq!(
            code.matches("api_flavor: editing_is_agent.then_some(api_flavor),")
                .count(),
            1
        );
        // 信息格徽标恰 1（恒显，与列表页「仅回退态显徽标」不同：详情页要能看出主线）
        assert_eq!(
            code.matches("\"{api_flavor_label(&p.api_flavor)}\"")
                .count(),
            1
        );
        // 初值主线协议
        assert!(code.contains("use_signal(|| \"responses\".to_string())"));
    }
}
