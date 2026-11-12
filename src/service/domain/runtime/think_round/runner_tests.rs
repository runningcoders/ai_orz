//! tests 单元测试（拆分自 runner.rs）
//!
//! 文件瘦身：原 569 行 → 313 行，测试体 257 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::resolve_call_options;
use crate::models::agent::AgentRuntimeConfig;
use crate::models::brain::Brain;
use crate::models::model_provider::ModelProviderPo;
use common::enums::{AgentKind, ThinkingIntensity, ThinkingScene};

fn provider(config: &str) -> ModelProviderPo {
    ModelProviderPo {
        id: "prov".to_string(),
        name: "prov".to_string(),
        provider_type: common::enums::ProviderType::OpenAI,
        model_name: "gpt".to_string(),
        capability: common::enums::ModelCapability::Agent,
        api_key: String::new(),
        base_url: None,
        description: None,
        config: config.to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "system".to_string(),
        modified_by: "system".to_string(),
        created_at: 0,
        updated_at: 0,
    }
}

fn brain_with(config: &str, agent_intensity: Option<ThinkingIntensity>) -> Brain {
    Brain::new_local(
        "agent".to_string(),
        "agent".to_string(),
        AgentRuntimeConfig {
            thinking_intensity: agent_intensity,
            ..Default::default()
        },
        provider(config),
        Vec::new(),
    )
}

/// 取本次调用的思考强度（None = 能力开关关闭）
fn intensity(brain: &Brain, scene: ThinkingScene, dynamic: i32) -> Option<ThinkingIntensity> {
    resolve_call_options(brain, scene, dynamic, None, None).thinking_intensity
}

/// 小脑选定值优先于 Agent 覆盖，作为基准再叠加场景偏移
#[test]
fn cerebellum_strength_overrides_agent_then_applies_scene_offset() {
    let brain = brain_with(r#"{"thinking_intensity":50}"#, Some(ThinkingIntensity::MAX));
    // Awaken 场景偏移为正：小脑给 20 → 20 + 场景偏移（clamp 100）
    let got = resolve_call_options(
        &brain,
        ThinkingScene::Awaken,
        0,
        Some(ThinkingIntensity::new(20)),
        None,
    )
    .thinking_intensity
    .expect("provider configured");
    assert_eq!(
        got,
        ThinkingIntensity::new(20).offset(ThinkingScene::Awaken.thinking_intensity_offset())
    );
    // 小脑缺席 → 仍用 Agent 覆盖
    let fallback = resolve_call_options(&brain, ThinkingScene::Awaken, 0, None, None)
        .thinking_intensity
        .expect("provider configured");
    assert_eq!(
        fallback,
        ThinkingIntensity::MAX.offset(ThinkingScene::Awaken.thinking_intensity_offset())
    );
}

/// Provider 未配置 = 能力开关关闭：小脑选定值也不下发（轨迹里留 applied=false）
#[test]
fn cerebellum_strength_still_gated_by_provider_capability() {
    let brain = brain_with("{}", None);
    assert_eq!(
        resolve_call_options(
            &brain,
            ThinkingScene::Awaken,
            0,
            Some(ThinkingIntensity::new(90)),
            None
        )
        .thinking_intensity,
        None
    );
}

/// Provider 未配置 = 能力开关关闭：Agent 覆盖与场景偏移都不生效
#[test]
fn provider_unconfigured_disables_whole_chain() {
    let brain = brain_with("{}", Some(ThinkingIntensity::MAX));
    assert_eq!(intensity(&brain, ThinkingScene::Awaken, 0), None);
}

/// 外部 Agent 无 provider → 同样关闭
#[test]
fn external_agent_without_provider_returns_none() {
    let brain = Brain::new_external(
        AgentKind::Cli,
        "agent".to_string(),
        "agent".to_string(),
        AgentRuntimeConfig::default(),
        Vec::new(),
    );
    assert_eq!(intensity(&brain, ThinkingScene::Awaken, 0), None);
}

/// Agent 显式覆盖优先于 Provider 基准
#[test]
fn agent_override_wins_over_provider_default() {
    let brain = brain_with(r#"{"thinking_intensity":30}"#, Some(ThinkingIntensity::MAX));
    assert_eq!(
        intensity(&brain, ThinkingScene::Awaken, 0),
        Some(ThinkingIntensity::MAX)
    );
}

/// 场景偏移 + 意图动态偏移在基准分数上叠加，并饱和钳制到 [0, 100]
#[test]
fn offsets_stack_and_clamp_at_bounds() {
    // 基准 10 + Awaken(0) + 动态 -20 → 钳制到 0
    assert_eq!(
        intensity(
            &brain_with(r#"{"thinking_intensity":10}"#, None),
            ThinkingScene::Awaken,
            -20
        ),
        Some(ThinkingIntensity::MIN)
    );
    // 基准 80 + Awaken(0) + 动态 +20 → 钳制到 100
    assert_eq!(
        intensity(
            &brain_with(r#"{"thinking_intensity":80}"#, None),
            ThinkingScene::Awaken,
            20
        ),
        Some(ThinkingIntensity::MAX)
    );
    // 基准 50 + Awaken(0) + 动态 -10 → 40
    assert_eq!(
        intensity(
            &brain_with(r#"{"thinking_intensity":50}"#, None),
            ThinkingScene::Awaken,
            -10
        ),
        Some(ThinkingIntensity::new(40))
    );
    // 默认基准 50 + Awaken(0) + 动态 0 → 50
    assert_eq!(
        intensity(
            &brain_with(r#"{"thinking_intensity":50}"#, None),
            ThinkingScene::Awaken,
            0
        ),
        Some(ThinkingIntensity::DEFAULT)
    );
}

// ---------- P3-b：绝对强度通道（唤醒门判浅答 → 强度真 0） ----------
//
// 为什么必须是独立通道：思考强度是 `base + 场景偏移 + 动态偏移`（**加减**，
// 不是赋值）。`base=70` 时靠偏移减不到 0，用偏移表达「浅答强度=0」是假语义；
// 复用 `cerebellum_strength` 也不行（它只替换基准、之后仍叠加偏移）。

/// A 成对（force=None 侧）：整条 base + 场景偏移 + 动态偏移链**仍然生效**。
///
/// base=70 + Awaken(0) + 动态 20 → 90（不是 70，也不是 0）。
#[test]
fn force_none_keeps_base_and_offsets() {
    let brain = brain_with(r#"{"thinking_intensity":70}"#, None);
    let got = resolve_call_options(&brain, ThinkingScene::Awaken, 20, None, None)
        .thinking_intensity
        .expect("provider configured");
    assert_eq!(got, ThinkingIntensity::new(90));
}

/// A 成对（force=Some(MIN) 侧）：**真 0** —— 不是「小于 70」，也不是「归零到 70」。
#[test]
fn force_min_is_absolute_zero() {
    let brain = brain_with(r#"{"thinking_intensity":70}"#, None);
    let got = resolve_call_options(
        &brain,
        ThinkingScene::Awaken,
        20,
        None,
        Some(ThinkingIntensity::MIN),
    )
    .thinking_intensity
    .expect("provider configured");
    assert_eq!(got, ThinkingIntensity::MIN);
    assert_eq!(got.score(), 0, "浅答强度必须真为 0");
}

/// B 成对（force=None 侧）：借 `Settle` 的 +30 构造最坏组合 —— base 100 饱和到 100。
///
/// `wake_gate` 实际跑在 `Awaken` 场景（偏移 0）；这里取最坏组合只为证明
/// 「偏移再大也压不动 force」。
#[test]
fn force_none_saturates_with_scene_offset() {
    let brain = brain_with(r#"{"thinking_intensity":100}"#, None);
    let got = resolve_call_options(&brain, ThinkingScene::Settle, 0, None, None)
        .thinking_intensity
        .expect("provider configured");
    assert_eq!(got, ThinkingIntensity::MAX);
}

/// B 成对（force=Some(MIN) 侧）：base=100 + Settle(+30) + 动态 25 + 小脑基准 MAX，
/// 结果仍必须**真 0** —— 绝对通道先于整条 base/offset 链。
#[test]
fn force_min_beats_worst_case_base_and_offsets() {
    let brain = brain_with(r#"{"thinking_intensity":100}"#, None);
    let got = resolve_call_options(
        &brain,
        ThinkingScene::Settle,
        25,
        Some(ThinkingIntensity::MAX),
        Some(ThinkingIntensity::MIN),
    )
    .thinking_intensity
    .expect("provider configured");
    assert_eq!(got, ThinkingIntensity::MIN);
    assert_eq!(got.score(), 0);
}

/// Provider 未配 = 能力开关关闭：绝对通道也不得绕过（`None`，不是 `Some(0)`）。
#[test]
fn force_min_still_gated_by_provider_capability() {
    let brain = brain_with("{}", None);
    assert_eq!(
        resolve_call_options(
            &brain,
            ThinkingScene::Awaken,
            0,
            None,
            Some(ThinkingIntensity::MIN)
        )
        .thinking_intensity,
        None
    );
}

/// force=None 时偏移确实改变结果：不能只断言「等于旧值」，
/// 否则「偏移恒为 0」的实现会空转通过。
#[test]
fn offset_still_changes_result_when_force_absent() {
    let brain = brain_with(r#"{"thinking_intensity":50}"#, None);
    let with_offset = resolve_call_options(&brain, ThinkingScene::Awaken, 20, None, None)
        .thinking_intensity
        .expect("provider configured");
    let without_offset = resolve_call_options(&brain, ThinkingScene::Awaken, 0, None, None)
        .thinking_intensity
        .expect("provider configured");
    assert_eq!(with_offset, ThinkingIntensity::new(70));
    assert_eq!(without_offset, ThinkingIntensity::DEFAULT);
    assert_ne!(with_offset, without_offset, "偏移必须真的改变结果");
}
