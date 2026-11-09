//! 注入策略两 Tab 编辑器（工具详情页 · P3）
//!
//! ## 为什么是「两 Tab 过滤」而不是「两套数据」
//! `credential_requirements` 是**一个**数组（`Vec<CredentialRequirement>`）：P2 起
//! `CredentialKind` 新增 `user_info` / `agent_info` 两个非凭据变体，承载「运行时身份
//! 注入」。管理面把它拆成两个视图便于理解，但保存时必须回到**同一个数组且保持原
//! 声明序**——因此本模块的所有增删改都作用在**原始索引**上（[`filter_by_tab`] 返回
//! `(原始索引, 条目)`），从不重建顺序。
//!
//! ## 职责边界（规则单点在 common，本模块只做形态投影）
//! - platform：身份类 `requires_platform() == false` → 置灰并强制空；
//! - field：身份类必填且走白名单（`AGENT_INFO_FIELDS` / `USER_INFO_FIELDS`）；
//! - enhancer：身份类无增强语义 → 不渲染；
//! - 注入名唯一 / `AI_ORZ_` 前缀拒绝 / 三元组去重等规则由 [`prepare_for_save`]
//!   委托 common `validate_requirements`（与后端 handler 同一实现，双端零漂移）。

use dioxus::prelude::*;

use crate::components::credential_form::{
    all_credential_kinds, available_enhancers, binding_name, enhancer_display, enhancer_from_value,
    enhancer_to_value, has_any_enhancer_support, injection_value_preview, is_sensitive_name,
    kind_from_value, normalize_requirements, recommended_binding_name,
    validate_requirements_scoped,
};
use common::models::{
    AGENT_INFO_FIELDS, CredentialBinding, CredentialKind, CredentialRequirement,
    CredentialRequirementScope, USER_INFO_FIELDS, enhancer_supports,
};

/// 编辑器 Tab（纯视图标识，绝不落库）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionTab {
    /// 凭据：source = 真实 kind（lark_app / github_token / generic_token …）
    Credentials,
    /// 通用（身份）：kind ∈ {user_info, agent_info}
    Identity,
}

impl InjectionTab {
    pub const ALL: [InjectionTab; 2] = [InjectionTab::Credentials, InjectionTab::Identity];

    pub fn label(self) -> &'static str {
        match self {
            InjectionTab::Credentials => "凭据",
            InjectionTab::Identity => "通用（身份）",
        }
    }

    /// 该 Tab 是否收纳身份类需求（过滤判据）
    pub fn is_identity(self) -> bool {
        matches!(self, InjectionTab::Identity)
    }

    /// Tab 按钮样式（激活态高亮）
    pub fn button_class(self, active: InjectionTab) -> &'static str {
        if self == active {
            "btn hud-btn btn-primary btn-xs"
        } else {
            "btn hud-btn btn-ghost btn-xs"
        }
    }
}

/// 身份类 kind 固定清单（顺序即下拉顺序）
pub fn identity_kinds() -> [CredentialKind; 2] {
    [CredentialKind::AgentInfo, CredentialKind::UserInfo]
}

/// 身份 kind → 字段白名单（委托 common 常量单点；非身份 kind 返回空切片）
pub fn identity_fields(kind: CredentialKind) -> &'static [&'static str] {
    match kind {
        CredentialKind::AgentInfo => AGENT_INFO_FIELDS,
        CredentialKind::UserInfo => USER_INFO_FIELDS,
        _ => &[],
    }
}

/// Tab 过滤：返回 `(原始索引, 条目)`
///
/// 原始索引是「保序」的实现基础：增删改都按它写回同一个数组，保存时无需重排。
pub fn filter_by_tab(
    list: &[CredentialRequirement],
    tab: InjectionTab,
) -> Vec<(usize, CredentialRequirement)> {
    list.iter()
        .cloned()
        .enumerate()
        .filter(|(_, req)| req.kind.is_identity() == tab.is_identity())
        .collect()
}

/// 凭据 Tab 的 kind 选项：一维清单 + 列表内已出现的其它 kind
///
/// `all_credential_kinds()` 现状缺 `wechat_ilink` / `email_bot`（既有缺口，非本单范围，
/// 且本单不改其签名）；但存量条目必须能原样回显，故按需并入，避免「一打开就被改成别的
/// kind」——保存后回读必须与编辑态完全一致。
pub fn credential_kind_options(list: &[CredentialRequirement]) -> Vec<CredentialKind> {
    let mut options: Vec<CredentialKind> = all_credential_kinds().to_vec();
    for req in list {
        if !req.kind.is_identity() && !options.contains(&req.kind) {
            options.push(req.kind);
        }
    }
    options
}

/// 新凭据条目（`generic_token` 是唯一「平台 + 单 token」通用形态）
pub fn new_credential_requirement() -> CredentialRequirement {
    CredentialRequirement {
        kind: CredentialKind::GenericToken,
        platform: None,
        field: None,
        enhancer: None,
        binding: CredentialBinding::Env {
            name: String::new(),
        },
    }
}

/// 新身份条目（field 取白名单首项；注入点默认 Env——内置工具身份注入即环境变量）
pub fn new_identity_requirement(kind: CredentialKind) -> CredentialRequirement {
    CredentialRequirement {
        kind,
        platform: None,
        field: identity_fields(kind).first().map(|f| (*f).to_string()),
        enhancer: None,
        binding: CredentialBinding::Env {
            name: String::new(),
        },
    }
}

/// 切换 kind 时就地收敛互斥字段（避免留下必然被后端拒绝的残留组合）
pub fn retarget_kind(req: &mut CredentialRequirement, kind: CredentialKind) {
    req.kind = kind;
    if kind.is_identity() {
        // 身份类：platform 恒空、enhancer 无语义、field 必须落在白名单内
        req.platform = None;
        req.enhancer = None;
        let allowed = identity_fields(kind);
        let current_allowed = req
            .field
            .as_deref()
            .map(|field| allowed.contains(&field))
            .unwrap_or(false);
        if !current_allowed {
            req.field = allowed.first().map(|f| (*f).to_string());
        }
        return;
    }
    if !kind.requires_platform() {
        req.platform = None;
    }
    if let Some(enhancer) = req.enhancer
        && !enhancer_supports(kind, enhancer)
    {
        req.enhancer = None;
    }
}

/// Builtin 作用域的注入点形态（`binding_allowed(Env|Internal, Builtin) == true`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinBindingKind {
    /// 注入子进程环境变量（shell_exec 消费）
    Env,
    /// 存工具实例字段（其它内置工具的消费形态）
    Internal,
}

impl BuiltinBindingKind {
    pub const ALL: [BuiltinBindingKind; 2] =
        [BuiltinBindingKind::Env, BuiltinBindingKind::Internal];

    /// 从既有 binding 推导（Header/Query 属非法存量 → 展示为 Env 以便就地纠正）
    pub fn of(binding: &CredentialBinding) -> Self {
        match binding {
            CredentialBinding::Internal { .. } => BuiltinBindingKind::Internal,
            _ => BuiltinBindingKind::Env,
        }
    }

    pub fn to_value(self) -> &'static str {
        match self {
            BuiltinBindingKind::Env => "env",
            BuiltinBindingKind::Internal => "internal",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BuiltinBindingKind::Env => "Env（环境变量）",
            BuiltinBindingKind::Internal => "Internal（实例字段）",
        }
    }

    pub fn from_value(value: &str) -> Option<Self> {
        match value {
            "env" => Some(BuiltinBindingKind::Env),
            "internal" => Some(BuiltinBindingKind::Internal),
            _ => None,
        }
    }

    /// 切换注入点类型：保留已填名字（两种形态都只是「放在哪」的名字）
    pub fn rebuild(self, name: &str) -> CredentialBinding {
        let name = name.to_string();
        match self {
            BuiltinBindingKind::Env => CredentialBinding::Env { name },
            BuiltinBindingKind::Internal => CredentialBinding::Internal { field: name },
        }
    }
}

/// 保存前预处理：规范化（trim、空白 Option 归 None）+ common 单点校验
///
/// 与后端 `pkg::credential::validate_requirements` 同一实现，错误文案可直接显示。
pub fn prepare_for_save(
    list: Vec<CredentialRequirement>,
    scope: CredentialRequirementScope,
) -> Result<Vec<CredentialRequirement>, String> {
    let normalized = normalize_requirements(list);
    validate_requirements_scoped(&normalized, scope)?;
    Ok(normalized)
}

/// 注入名敏感警示（None = 无需提示）
///
/// 身份类惯例名（`GIT_AUTHOR_EMAIL` / `GIT_COMMITTER_NAME` 等）不应命中：判据只有
/// authorization/cookie/set-cookie 精确匹配 + api-key/token/secret/password 子串匹配。
pub fn sensitive_name_warning(req: &CredentialRequirement) -> Option<String> {
    let name = binding_name(&req.binding).trim();
    if name.is_empty() || !is_sensitive_name(name) {
        return None;
    }
    Some(format!(
        "「{name}」命名含敏感语义（token/secret/password/api-key 等）：该值以明文写进工具配置，请勿在此声明密钥"
    ))
}

/// 配置层是否已有 `credential_requirements` 声明
///
/// 详情接口的 `config` 会经 redact 把该键打成 `[REDACTED]` 字符串——键仍在即说明
/// 「配置层已覆盖」；键缺失说明当前展示的是**工厂静态声明兜底**（tavily / gh_cli /
/// lark_cli 等），管理面需提示「保存即固化」的行为变化。
pub fn config_layer_declared(config: Option<&serde_json::Value>) -> bool {
    config
        .and_then(|value| value.as_object())
        .map(|object| object.contains_key("credential_requirements"))
        .unwrap_or(false)
}

/// 注入策略卡片是否可见（Builtin 即使零声明也必须给入口；其余保持「有声明/有继承才显示」）
pub fn injection_panel_visible(
    protocol: common::enums::ToolProtocol,
    has_requirements: bool,
    has_source: bool,
) -> bool {
    has_requirements || has_source || matches!(protocol, common::enums::ToolProtocol::Builtin)
}

/// 单行编辑（按原始索引就地写回，保序）
#[component]
fn InjectionRow(
    requirements: Signal<Vec<CredentialRequirement>>,
    index: usize,
    requirement: CredentialRequirement,
    tab: InjectionTab,
    credential_options: Vec<CredentialKind>,
) -> Element {
    let is_identity = requirement.kind.is_identity();
    let kind_value = requirement.kind.as_str().to_string();
    let platform_enabled = requirement.kind.requires_platform();
    let platform_value = if platform_enabled {
        requirement.platform.clone().unwrap_or_default()
    } else {
        String::new()
    };
    let platform_placeholder = if platform_enabled {
        "如 github / tavily"
    } else {
        "身份/专用类不适用"
    };
    let field_value = requirement.field.clone().unwrap_or_default();
    let field_options = identity_fields(requirement.kind);
    let field_label = if is_identity {
        "提取字段 *（白名单）"
    } else {
        "提取字段（可选）"
    };
    let enhancer_supported = has_any_enhancer_support(requirement.kind);
    let enhancer_value = requirement
        .enhancer
        .map(enhancer_to_value)
        .unwrap_or("none")
        .to_string();
    let enhancer_options = available_enhancers(requirement.kind);
    let binding_kind = BuiltinBindingKind::of(&requirement.binding);
    let binding_value = binding_kind.to_value().to_string();
    let binding_name_value = binding_name(&requirement.binding).to_string();
    let binding_placeholder = if is_identity {
        "如 GIT_AUTHOR_EMAIL"
    } else {
        "如 GITHUB_TOKEN"
    };
    let suggestion = recommended_binding_name(&requirement).map(str::to_string);
    let warning = sensitive_name_warning(&requirement);
    let preview = injection_value_preview(&requirement);

    rsx! {
        div { class: "border border-base-300 rounded-box p-3 space-y-2", key: "{index}",
            div { class: "flex justify-between items-center",
                span { class: "text-xs font-semibold text-base-content/60",
                    "需求 #{index + 1} · {requirement.kind.as_str()}"
                }
                button { class: "btn hud-btn btn-ghost btn-xs text-error",
                    onclick: move |_| {
                        let mut guard = requirements.write();
                        if index < guard.len() {
                            guard.remove(index);
                        }
                    },
                    "✕ 移除"
                }
            }
            div { class: "flex gap-2 flex-wrap",
                div { class: "form-control flex-1 min-w-[10rem]",
                    label { class: "label", span { class: "label-text text-xs", "类型 *" } }
                    select { class: "select select-bordered hud-input select-sm w-full", value: "{kind_value}",
                        onchange: move |e| {
                            if let Some(kind) = kind_from_value(&e.value()) {
                                let mut guard = requirements.write();
                                if let Some(req) = guard.get_mut(index) {
                                    retarget_kind(req, kind);
                                }
                            }
                        },
                        if tab.is_identity() {
                            for kind in identity_kinds() {
                                option {
                                    value: "{kind.as_str()}",
                                    selected: kind.as_str() == kind_value.as_str(),
                                    "{kind.as_str()}"
                                }
                            }
                        } else {
                            for kind in credential_options.iter().copied() {
                                option {
                                    value: "{kind.as_str()}",
                                    selected: kind.as_str() == kind_value.as_str(),
                                    "{kind.as_str()}"
                                }
                            }
                        }
                    }
                }
                div { class: "form-control flex-1 min-w-[8rem]",
                    label { class: "label", span { class: "label-text text-xs", "平台标识" } }
                    input {
                        class: "input input-bordered hud-input input-sm w-full",
                        disabled: !platform_enabled,
                        value: "{platform_value}",
                        placeholder: "{platform_placeholder}",
                        oninput: move |e| {
                            let value = e.value();
                            let mut guard = requirements.write();
                            if let Some(req) = guard.get_mut(index) {
                                req.platform = if value.trim().is_empty() { None } else { Some(value) };
                            }
                        },
                    }
                }
            }
            div { class: "flex gap-2 flex-wrap",
                div { class: "form-control flex-1 min-w-[8rem]",
                    label { class: "label", span { class: "label-text text-xs", "{field_label}" } }
                    if is_identity {
                        select { class: "select select-bordered hud-input select-sm w-full", value: "{field_value}",
                            onchange: move |e| {
                                let value = e.value();
                                let mut guard = requirements.write();
                                if let Some(req) = guard.get_mut(index) {
                                    req.field = if value.trim().is_empty() { None } else { Some(value) };
                                }
                            },
                            for field in field_options.iter().copied() {
                                option {
                                    value: "{field}",
                                    selected: field == field_value.as_str(),
                                    "{field}"
                                }
                            }
                        }
                    } else {
                        input {
                            class: "input input-bordered hud-input input-sm w-full",
                            value: "{field_value}",
                            placeholder: "如 token / api_key",
                            oninput: move |e| {
                                let value = e.value();
                                let mut guard = requirements.write();
                                if let Some(req) = guard.get_mut(index) {
                                    req.field = if value.trim().is_empty() { None } else { Some(value) };
                                }
                            },
                        }
                    }
                }
                if !is_identity {
                    div { class: "form-control flex-1 min-w-[8rem]",
                        label { class: "label", span { class: "label-text text-xs", "增强器（与字段互斥）" } }
                        select { class: "select select-bordered hud-input select-sm w-full",
                            value: "{enhancer_value}",
                            disabled: !enhancer_supported && enhancer_options.is_empty(),
                            onchange: move |e| {
                                let mut guard = requirements.write();
                                if let Some(req) = guard.get_mut(index) {
                                    req.enhancer = enhancer_from_value(&e.value());
                                }
                            },
                            option {
                                value: "none",
                                selected: enhancer_value.as_str() == "none",
                                "默认（无）"
                            }
                            for enhancer in enhancer_options.iter().copied() {
                                option {
                                    value: "{enhancer_to_value(enhancer)}",
                                    selected: enhancer_value.as_str() == enhancer_to_value(enhancer),
                                    "{enhancer_display(enhancer)}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "flex gap-2 flex-wrap",
                div { class: "form-control min-w-[10rem]",
                    label { class: "label", span { class: "label-text text-xs", "注入点类型 *" } }
                    select { class: "select select-bordered hud-input select-sm w-full", value: "{binding_value}",
                        onchange: move |e| {
                            let Some(next) = BuiltinBindingKind::from_value(&e.value()) else {
                                return;
                            };
                            let mut guard = requirements.write();
                            if let Some(req) = guard.get_mut(index) {
                                let current = binding_name(&req.binding).to_string();
                                req.binding = next.rebuild(&current);
                            }
                        },
                        for kind in BuiltinBindingKind::ALL {
                            option {
                                value: "{kind.to_value()}",
                                selected: kind.to_value() == binding_value.as_str(),
                                "{kind.label()}"
                            }
                        }
                    }
                }
                div { class: "form-control flex-1 min-w-[10rem]",
                    label { class: "label", span { class: "label-text text-xs", "注入名 *" } }
                    input {
                        class: "input input-bordered hud-input input-sm w-full font-mono",
                        value: "{binding_name_value}",
                        placeholder: "{binding_placeholder}",
                        oninput: move |e| {
                            let value = e.value();
                            let mut guard = requirements.write();
                            if let Some(req) = guard.get_mut(index) {
                                let kind = BuiltinBindingKind::of(&req.binding);
                                req.binding = kind.rebuild(&value);
                            }
                        },
                    }
                }
            }
            div { class: "flex flex-wrap items-center gap-2 text-xs",
                span { class: "text-base-content/50", "注入值形态：" }
                code { class: "bg-base-200 rounded px-1", "{preview}" }
                if let Some(suggestion) = suggestion {
                    span { class: "text-info", "建议注入名：{suggestion}" }
                }
            }
            if let Some(warning) = warning {
                div { class: "text-xs text-warning", "{warning}" }
            }
        }
    }
}

/// 注入策略两 Tab 编辑器（父组件持有 `requirements` signal：完整数组，保序的唯一真相源）
#[component]
pub fn InjectionStrategyEditor(requirements: Signal<Vec<CredentialRequirement>>) -> Element {
    let mut tab = use_signal(|| InjectionTab::Credentials);
    let active = tab();
    let list = requirements.read().clone();
    let rows = filter_by_tab(&list, active);
    let credential_options = credential_kind_options(&list);
    let tabs: Vec<(InjectionTab, usize)> = InjectionTab::ALL
        .into_iter()
        .map(|candidate| (candidate, filter_by_tab(&list, candidate).len()))
        .collect();

    rsx! {
        div { class: "space-y-3",
            div { class: "flex flex-wrap items-center gap-2",
                for (candidate, count) in tabs {
                    button { class: "{candidate.button_class(active)}",
                        onclick: move |_| tab.set(candidate),
                        "{candidate.label()}（{count}）"
                    }
                }
                div { class: "flex-1" }
                button { class: "btn hud-btn btn-outline btn-xs",
                    onclick: move |_| {
                        let mut guard = requirements.write();
                        if active.is_identity() {
                            guard.push(new_identity_requirement(CredentialKind::AgentInfo));
                        } else {
                            guard.push(new_credential_requirement());
                        }
                    },
                    "+ 添加"
                }
            }
            if rows.is_empty() {
                p { class: "text-xs text-base-content/40",
                    if active.is_identity() {
                        "尚未声明身份注入（「+ 添加」后可选择 user_info / agent_info 与字段）"
                    } else {
                        "尚未声明凭据需求"
                    }
                }
            } else {
                div { class: "space-y-2",
                    for (index, requirement) in rows.iter().cloned() {
                        InjectionRow {
                            requirements,
                            index,
                            requirement,
                            tab: active,
                            credential_options: credential_options.clone(),
                        }
                    }
                }
            }
            p { class: "text-xs opacity-60",
                "两 Tab 是同一份声明的视图过滤：保存时按原声明序合并回一个数组，不重排、不丢项。"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::enums::ToolProtocol;
    use common::models::CredentialEnhancerKind;

    fn env(name: &str) -> CredentialBinding {
        CredentialBinding::Env {
            name: name.to_string(),
        }
    }

    fn credential(
        kind: CredentialKind,
        platform: Option<&str>,
        name: &str,
    ) -> CredentialRequirement {
        CredentialRequirement {
            kind,
            platform: platform.map(str::to_string),
            field: None,
            enhancer: None,
            binding: env(name),
        }
    }

    fn identity(kind: CredentialKind, field: &str, name: &str) -> CredentialRequirement {
        CredentialRequirement {
            kind,
            platform: None,
            field: Some(field.to_string()),
            enhancer: None,
            binding: env(name),
        }
    }

    #[test]
    fn filter_by_tab_is_mutually_exclusive_and_keeps_original_index() {
        let list = vec![
            credential(CredentialKind::GithubToken, None, "GITHUB_TOKEN"),
            identity(CredentialKind::AgentInfo, "email", "GIT_AUTHOR_EMAIL"),
            credential(
                CredentialKind::GenericToken,
                Some("tavily"),
                "TAVILY_API_KEY",
            ),
            identity(
                CredentialKind::UserInfo,
                "display_name",
                "GIT_COMMITTER_NAME",
            ),
        ];
        let credentials = filter_by_tab(&list, InjectionTab::Credentials);
        assert_eq!(
            credentials.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 2]
        );
        let identities = filter_by_tab(&list, InjectionTab::Identity);
        assert_eq!(
            identities.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![1, 3]
        );
        // 互斥且完备：两个 Tab 的索引集合无交集，并集 = 全集
        let mut union: Vec<usize> = credentials
            .iter()
            .chain(identities.iter())
            .map(|(i, _)| *i)
            .collect();
        union.sort_unstable();
        assert_eq!(union, vec![0, 1, 2, 3]);
    }

    #[test]
    fn identity_fields_follow_common_whitelists() {
        assert_eq!(
            identity_fields(CredentialKind::AgentInfo),
            AGENT_INFO_FIELDS
        );
        assert_eq!(identity_fields(CredentialKind::UserInfo), USER_INFO_FIELDS);
        assert!(identity_fields(CredentialKind::AgentInfo).contains(&"git_name"));
        assert!(identity_fields(CredentialKind::UserInfo).contains(&"username"));
        assert!(identity_fields(CredentialKind::GithubToken).is_empty());
    }

    #[test]
    fn new_identity_requirement_defaults_to_first_field_and_env() {
        for kind in identity_kinds() {
            let req = new_identity_requirement(kind);
            assert_eq!(req.kind, kind);
            assert_eq!(req.platform, None, "身份类 platform 恒空");
            assert_eq!(req.enhancer, None, "身份类无增强器");
            assert_eq!(
                req.field.as_deref(),
                identity_fields(kind).first().copied(),
                "field 默认白名单首项"
            );
            assert!(matches!(req.binding, CredentialBinding::Env { .. }));
        }
    }

    #[test]
    fn retarget_kind_drops_identity_incompatible_residue() {
        // 凭据（带 platform + 增强器）切身份 → platform/enhancer 收敛，field 落白名单
        let mut req = credential(CredentialKind::GenericToken, Some("tavily"), "X");
        req.enhancer = Some(CredentialEnhancerKind::BearerToken);
        retarget_kind(&mut req, CredentialKind::AgentInfo);
        assert_eq!(req.platform, None);
        assert_eq!(req.enhancer, None);
        assert_eq!(
            req.field.as_deref(),
            identity_fields(CredentialKind::AgentInfo).first().copied()
        );

        // 需要 platform 的 kind 切到不需要 platform 的专用 kind → platform 清空
        let mut req = credential(CredentialKind::GenericToken, Some("tavily"), "X");
        retarget_kind(&mut req, CredentialKind::GithubToken);
        assert_eq!(req.platform, None);

        // 身份切到另一身份：白名单不含旧 field 时回落首项
        let mut req = identity(CredentialKind::AgentInfo, "git_name", "GIT_AUTHOR_NAME");
        retarget_kind(&mut req, CredentialKind::UserInfo);
        assert_eq!(
            req.field.as_deref(),
            identity_fields(CredentialKind::UserInfo).first().copied()
        );
    }

    #[test]
    fn binding_kind_roundtrip_and_rebuild_keeps_name() {
        for kind in BuiltinBindingKind::ALL {
            assert_eq!(BuiltinBindingKind::from_value(kind.to_value()), Some(kind));
            let binding = kind.rebuild("GIT_AUTHOR_EMAIL");
            assert_eq!(binding_name(&binding), "GIT_AUTHOR_EMAIL");
            assert_eq!(BuiltinBindingKind::of(&binding), kind);
        }
        assert_eq!(BuiltinBindingKind::from_value("header"), None);
    }

    #[test]
    fn prepare_for_save_rejects_identity_platform_field_and_enhancer_violations() {
        let scope = CredentialRequirementScope::Builtin;
        // 身份带 platform → 拒
        let mut req = identity(CredentialKind::AgentInfo, "email", "GIT_AUTHOR_EMAIL");
        req.platform = Some("github".to_string());
        assert!(prepare_for_save(vec![req], scope).is_err());
        // field 越界（user_info 不含 username 之外的 agent 字段）→ 拒
        let req = identity(CredentialKind::AgentInfo, "username", "X");
        assert!(prepare_for_save(vec![req], scope).is_err());
        // 身份带增强器 → 拒
        let mut req = identity(CredentialKind::UserInfo, "email", "GIT_COMMITTER_EMAIL");
        req.enhancer = Some(CredentialEnhancerKind::BearerToken);
        assert!(prepare_for_save(vec![req], scope).is_err());
    }

    #[test]
    fn prepare_for_save_rejects_duplicate_and_reserved_prefix() {
        let scope = CredentialRequirementScope::Builtin;
        // 身份注入名重复 → 拒
        let list = vec![
            identity(CredentialKind::AgentInfo, "name", "GIT_AUTHOR_NAME"),
            identity(CredentialKind::UserInfo, "display_name", "GIT_AUTHOR_NAME"),
        ];
        assert!(prepare_for_save(list, scope).is_err());
        // AI_ORZ_ 前缀 → 拒（平台结构身份变量恒硬覆盖）
        let list = vec![identity(CredentialKind::AgentInfo, "id", "AI_ORZ_AGENT_ID")];
        assert!(prepare_for_save(list, scope).is_err());
        // 合法身份 + 合法凭据混合 → 通过，且顺序原样保留
        let list = vec![
            credential(CredentialKind::GithubToken, None, "GITHUB_TOKEN"),
            identity(CredentialKind::AgentInfo, "email", "GIT_AUTHOR_EMAIL"),
            identity(
                CredentialKind::UserInfo,
                "display_name",
                "GIT_COMMITTER_NAME",
            ),
        ];
        let saved = prepare_for_save(list.clone(), scope).expect("合法声明应通过");
        assert_eq!(saved, list, "保存结果必须与编辑态逐项同序");
    }

    #[test]
    fn sensitive_warning_skips_git_identity_names() {
        // 身份类惯例名不含 token/secret/password/api-key 子串 → 不应被判敏感
        for name in [
            "GIT_AUTHOR_EMAIL",
            "GIT_AUTHOR_NAME",
            "GIT_COMMITTER_EMAIL",
            "GIT_COMMITTER_NAME",
            "AGENT_ID",
            "USER_DISPLAY_NAME",
        ] {
            let req = identity(CredentialKind::AgentInfo, "email", name);
            assert!(sensitive_name_warning(&req).is_none(), "不应判敏感: {name}");
        }
        // 真敏感名仍要提示
        for name in ["X-API-KEY", "GITHUB_TOKEN", "client_secret"] {
            let req = credential(CredentialKind::GithubToken, None, name);
            assert!(sensitive_name_warning(&req).is_some(), "应判敏感: {name}");
        }
        // 空注入名不提示（未填时不打扰）
        let req = credential(CredentialKind::GithubToken, None, "   ");
        assert!(sensitive_name_warning(&req).is_none());
    }

    #[test]
    fn config_layer_detection_distinguishes_factory_default() {
        assert!(!config_layer_declared(None));
        assert!(!config_layer_declared(Some(
            &serde_json::json!({ "command": "gh" })
        )));
        // redact 后键仍在 → 配置层已覆盖（前端只需判「键是否存在」）
        assert!(config_layer_declared(Some(
            &serde_json::json!({ "credential_requirements": "[REDACTED]" })
        )));
        assert!(config_layer_declared(Some(
            &serde_json::json!({ "credential_requirements": [] })
        )));
    }

    #[test]
    fn injection_panel_visible_for_builtin_even_without_declarations() {
        // Builtin 即使零声明也必须给编辑入口（shell_exec 现状）
        assert!(injection_panel_visible(ToolProtocol::Builtin, false, false));
        // 非 Builtin：无声明无继承 → 不渲染（保持既有形态零变化）
        assert!(!injection_panel_visible(ToolProtocol::Http, false, false));
        assert!(!injection_panel_visible(ToolProtocol::Shell, false, false));
        assert!(injection_panel_visible(ToolProtocol::Http, true, false));
        assert!(injection_panel_visible(ToolProtocol::Mcp, false, true));
    }

    #[test]
    fn credential_kind_options_keep_existing_kinds_selectable() {
        // wechat_ilink / email_bot 不在 all_credential_kinds() 内，但存量条目必须可回显
        let list = vec![
            credential(CredentialKind::WechatIlink, None, "WECHAT_TOKEN"),
            credential(CredentialKind::EmailBot, Some("qq"), "EMAIL_PASSWORD"),
            identity(CredentialKind::AgentInfo, "email", "GIT_AUTHOR_EMAIL"),
        ];
        let options = credential_kind_options(&list);
        assert!(options.contains(&CredentialKind::WechatIlink));
        assert!(options.contains(&CredentialKind::EmailBot));
        // 身份类不进入凭据 Tab 的 kind 选项
        assert!(!options.contains(&CredentialKind::AgentInfo));
        // 默认一维清单仍在
        for kind in all_credential_kinds() {
            assert!(options.contains(&kind));
        }
    }
}
