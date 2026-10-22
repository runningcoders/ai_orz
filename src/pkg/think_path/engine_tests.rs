//! 图引擎行为测试：线性流转 / 回环预算封顶 / 顾问选路与降级 / 惰性加载

use super::arbiter::Arbiter;
use super::blueprint::ThinkPathBuilder;
use super::bus::ProviderSet;
use super::engine::ThinkPathEngine;
use super::run::RouteVia;
use super::testkit::{CountingProvider, ProbeNode, ScriptedAdvisor};
use super::types::{ContextKey, LoadDepth, NodeSignal, PhaseId, Stimulus};
use std::sync::Arc;

fn stimulus() -> Stimulus {
    Stimulus {
        source: "test".to_string(),
        text: "帮我看下这个任务".to_string(),
        message_id: None,
    }
}

#[tokio::test]
async fn linear_path_walks_all_nodes_then_terminates() {
    let bp = ThinkPathBuilder::new("t_linear")
        .entry("prepare")
        .node_with("prepare", PhaseId::Prepare, 0, "装配上下文")
        .node_with("deliberate", PhaseId::Deliberate, 0, "正式思考")
        .node_with("consolidate", PhaseId::Consolidate, 0, "收尾落档")
        .edge("prepare", "deliberate", Some("done"))
        .edge("deliberate", "consolidate", Some("done"))
        .build()
        .expect("blueprint");

    let engine = ThinkPathEngine::new(
        bp,
        vec![
            Arc::new(ProbeNode::new("prepare", NodeSignal::Done)),
            Arc::new(ProbeNode::new("deliberate", NodeSignal::Done)),
            Arc::new(ProbeNode::new("consolidate", NodeSignal::Done)),
        ],
        Arbiter::deterministic(),
        ProviderSet::new(),
    )
    .expect("engine");

    let run = engine.run("r1", stimulus()).await.expect("run");
    assert_eq!(run.trace.steps, 3);
    assert_eq!(
        run.trace.fingerprint(),
        "prepare#1>deliberate#1>consolidate#1"
    );
    assert_eq!(run.trace.advisor_calls, 0);
}

#[tokio::test]
async fn activation_budget_caps_the_loop() {
    let mut bp = ThinkPathBuilder::new("t_loop")
        .entry("prepare")
        .node_with("prepare", PhaseId::Prepare, 0, "")
        .node_with("deliberate", PhaseId::Deliberate, 0, "")
        .edge("prepare", "deliberate", Some("done"))
        .edge("deliberate", "prepare", Some("overflow"))
        .build()
        .expect("blueprint");
    // 两边都封顶 2 次：图 + 回环必须靠预算停下，否则 L0 合法化挡不住互踢皮球
    for node in bp.nodes.iter_mut() {
        node.budget.max_activations = Some(2);
    }

    let engine = ThinkPathEngine::new(
        bp,
        vec![
            Arc::new(ProbeNode::new("prepare", NodeSignal::Done)),
            Arc::new(ProbeNode::new(
                "deliberate",
                NodeSignal::Yield("overflow".into()),
            )),
        ],
        Arbiter::deterministic(),
        ProviderSet::new(),
    )
    .expect("engine");

    let run = engine.run("r2", stimulus()).await.expect("run");
    assert_eq!(run.trace.steps, 4);
    assert_eq!(
        run.trace.fingerprint(),
        "prepare#1>deliberate#1>prepare#2>deliberate#2"
    );
}

#[tokio::test]
async fn advisor_picks_within_legal_candidates() {
    let bp = ThinkPathBuilder::new("t_advisor")
        .entry("triage")
        .node_with("fast", PhaseId::Triage, 1, "走捷径")
        .node_with("deep", PhaseId::Deliberate, 5, "认真想")
        .node_with("triage", PhaseId::Triage, 0, "分诊")
        .edge("triage", "fast", Some("done"))
        .edge("triage", "deep", Some("done"))
        .build()
        .expect("blueprint");

    let advisor = Arc::new(ScriptedAdvisor::choose("fast"));
    let engine = ThinkPathEngine::new(
        bp,
        vec![
            Arc::new(ProbeNode::new("triage", NodeSignal::Done)),
            Arc::new(ProbeNode::new("fast", NodeSignal::Done)),
            Arc::new(ProbeNode::new("deep", NodeSignal::Done)),
        ],
        Arbiter::with_advisor(advisor.clone()),
        ProviderSet::new(),
    )
    .expect("engine");

    let run = engine.run("r3", stimulus()).await.expect("run");
    let route = run.trace.routes.first().expect("route recorded");
    assert_eq!(route.via, RouteVia::Advisor);
    assert_eq!(route.chosen.as_str(), "fast");
    assert_eq!(route.candidates.len(), 2);
    // 顾问看到的候选就是合法候选集 —— 它只能「选」，不能「定」
    let mut seen = advisor.candidate_ids();
    seen.sort();
    assert_eq!(seen, vec!["deep".to_string(), "fast".to_string()]);
}

#[tokio::test]
async fn advisor_out_of_range_or_failure_falls_back() {
    let bp = ThinkPathBuilder::new("t_fallback")
        .entry("triage")
        .node_with("fast", PhaseId::Triage, 1, "")
        .node_with("deep", PhaseId::Deliberate, 5, "")
        .node_with("triage", PhaseId::Triage, 0, "")
        .edge("triage", "fast", Some("done"))
        .edge("triage", "deep", Some("done"))
        .build()
        .expect("blueprint");

    let make_nodes = || {
        vec![
            Arc::new(ProbeNode::new("triage", NodeSignal::Done)) as Arc<dyn super::node::ThinkNode>,
            Arc::new(ProbeNode::new("fast", NodeSignal::Done)),
            Arc::new(ProbeNode::new("deep", NodeSignal::Done)),
        ]
    };

    // 越界：顾问返回一个不在候选集里的节点
    let engine = ThinkPathEngine::new(
        bp.clone(),
        make_nodes(),
        Arbiter::with_advisor(Arc::new(ScriptedAdvisor::choose("nonexistent"))),
        ProviderSet::new(),
    )
    .expect("engine");
    let run = engine.run("r4", stimulus()).await.expect("run");
    let route = run.trace.routes.first().expect("route recorded");
    assert_eq!(route.via, RouteVia::Fallback);
    assert_eq!(route.chosen.as_str(), "deep"); // 降级为确定性优先级最高者

    // 失败：顾问报错，同样降级
    let engine = ThinkPathEngine::new(
        bp,
        make_nodes(),
        Arbiter::with_advisor(Arc::new(ScriptedAdvisor::failing())),
        ProviderSet::new(),
    )
    .expect("engine");
    let run = engine.run("r5", stimulus()).await.expect("run");
    let route = run.trace.routes.first().expect("route recorded");
    assert_eq!(route.via, RouteVia::Fallback);
    assert_eq!(route.chosen.as_str(), "deep");
}

#[tokio::test]
async fn context_loads_lazily_and_only_deepens() {
    let (provider, calls) = CountingProvider::new(ContextKey::Memory);
    let bp = ThinkPathBuilder::new("t_lazy")
        .entry("shallow_one")
        .node_with("shallow_one", PhaseId::Prepare, 0, "")
        .node_with("shallow_two", PhaseId::Prepare, 0, "")
        .node_with("full_one", PhaseId::Deliberate, 0, "")
        .edge("shallow_one", "shallow_two", Some("done"))
        .edge("shallow_two", "full_one", Some("done"))
        .build()
        .expect("blueprint");

    let engine = ThinkPathEngine::new(
        bp,
        vec![
            Arc::new(
                ProbeNode::new("shallow_one", NodeSignal::Done)
                    .with_require(ContextKey::Memory, LoadDepth::Shallow),
            ),
            Arc::new(
                ProbeNode::new("shallow_two", NodeSignal::Done)
                    .with_require(ContextKey::Memory, LoadDepth::Shallow),
            ),
            Arc::new(
                ProbeNode::new("full_one", NodeSignal::Done)
                    .with_require(ContextKey::Memory, LoadDepth::Full),
            ),
        ],
        Arbiter::deterministic(),
        ProviderSet::new().register(provider),
    )
    .expect("engine");

    let run = engine.run("r6", stimulus()).await.expect("run");
    assert_eq!(run.trace.steps, 3);
    // 两次 Shallow 只取一次；Full 是更深档，才真正重新加载
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}
