//! `resolve_entities` 护栏：范围解析必须「全能收窄、错必报错」。
//! 另含注入式单测：任务只调注进来的目标，不再摸 DAL 全局单例。

use super::{
    ALL_ENTITIES, RebuildTargets, RebuildVectorsTask, VectorRebuildTarget, resolve_entities,
};
use crate::pkg::RequestContext;
use crate::pkg::background_task::TaskProgressCounter;
use async_trait::async_trait;
use common::enums::VectorRebuildMode;
use common::error::Result;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 记录调用次数；`fail=true` 时模拟 DAL 抛错。
struct CountingTarget {
    calls: AtomicUsize,
    fail: bool,
}

#[async_trait]
impl VectorRebuildTarget for CountingTarget {
    async fn rebuild(
        &self,
        _ctx: RequestContext,
        _progress: &TaskProgressCounter,
        _mode: VectorRebuildMode,
    ) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(common::error::Error::internal("boom"));
        }
        Ok(())
    }
}

/// 不传 `entities` 时保持语义：全部 8 类。
#[test]
fn missing_entities_means_all() {
    let all = resolve_entities(None).expect("None 应解析为全量");
    assert_eq!(all.len(), ALL_ENTITIES.len());
    assert_eq!(all, ALL_ENTITIES.to_vec());
}

/// 空数组与不传等价。
///
/// 反例：把空数组当成「什么都不重建」，前端一个 `entities: []` 就会让任务
/// 「成功完成、处理 0 条」，而用户以为自己点了个全量重建。
#[test]
fn empty_entities_means_all() {
    let all = resolve_entities(Some(&[])).expect("空数组应解析为全量");
    assert_eq!(all.len(), ALL_ENTITIES.len());
}

/// 只传技能时必须只返回技能，且步骤数为 1（任务进度不能还显示 1/7）。
#[test]
fn skill_only_is_narrowed_to_one_step() {
    let picked = resolve_entities(Some(&["skill".to_string()])).expect("skill 是合法实体");
    assert_eq!(picked, vec![("skill", "Skill")]);
}

/// 未知实体名必须报错，不能静默忽略。
///
/// 反例：忽略未知值 ⇒ 用户把 `skill` 拼成 `skils`，任务照常启动、0 条重建、
/// 状态 Completed，前端只能看到「重建完成」。
#[test]
fn unknown_entity_is_rejected_with_legal_values() {
    let err = resolve_entities(Some(&["skils".to_string()])).expect_err("未知实体必须报错");
    let msg = err.to_string();
    assert!(msg.contains("skils"), "错误信息应带上非法值：{msg}");
    assert!(msg.contains("skill"), "错误信息应列出合法值：{msg}");
}

/// 大小写与首尾空白归一化，重复项去重（重复不会多跑一轮 clear + 全量重算）。
#[test]
fn names_are_normalized_and_deduplicated() {
    let picked = resolve_entities(Some(&[
        " SKILL ".to_string(),
        "skill".to_string(),
        "Tool".to_string(),
    ]))
    .expect("归一化后均合法");
    assert_eq!(picked, vec![("skill", "Skill"), ("tool", "Tool")]);
}

/// 解析结果保持 `ALL_ENTITIES` 的内部顺序仍是「按请求顺序」，
/// 这样用户可以自定义执行顺序（例如先补 skill 再补 memory）。
#[test]
fn order_follows_request() {
    let picked = resolve_entities(Some(&["tool".to_string(), "skill".to_string()]))
        .expect("两个都是合法实体");
    assert_eq!(picked, vec![("tool", "Tool"), ("skill", "Skill")]);
}

/// 只执行注入且被选中的实体，不再依赖全局 DAL。
#[tokio::test]
async fn run_rebuilds_dispatches_only_injected_targets() {
    crate::pkg::storage::test_support::init_for_test().await;
    let agent = Arc::new(CountingTarget {
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let memory = Arc::new(CountingTarget {
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let mut targets: RebuildTargets = BTreeMap::new();
    targets.insert("agent", agent.clone());
    targets.insert("memory", memory.clone());

    let task = RebuildVectorsTask::with_targets(
        RequestContext::new_system(),
        vec![("agent", "Agent")],
        VectorRebuildMode::Force,
        targets,
    );
    task.run_rebuilds().await.expect("fake 目标不应失败");

    assert_eq!(agent.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        memory.calls.load(Ordering::SeqCst),
        0,
        "未选中的实体不应执行"
    );
}

/// 任一实体失败即中断后续实体，错误向上抛（保持原「失败即停」语义）。
#[tokio::test]
async fn run_rebuilds_stops_on_first_error() {
    crate::pkg::storage::test_support::init_for_test().await;
    let agent = Arc::new(CountingTarget {
        calls: AtomicUsize::new(0),
        fail: true,
    });
    let memory = Arc::new(CountingTarget {
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let mut targets: RebuildTargets = BTreeMap::new();
    targets.insert("agent", agent.clone());
    targets.insert("memory", memory.clone());

    let task = RebuildVectorsTask::with_targets(
        RequestContext::new_system(),
        vec![("agent", "Agent"), ("memory", "Memory")],
        VectorRebuildMode::Auto,
        targets,
    );
    let err = task.run_rebuilds().await.expect_err("首个实体失败应上抛");
    assert!(err.to_string().contains("boom"));

    assert_eq!(agent.calls.load(Ordering::SeqCst), 1);
    assert_eq!(memory.calls.load(Ordering::SeqCst), 0, "失败后不应继续执行");
}
