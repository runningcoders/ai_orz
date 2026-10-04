//! 协议组件与注册表（决策点①落点：机制组件化注册匹配）
//!
//! [`ProtocolComponent`] 是协议层对「一种资源 scheme 如何被理解」的抽象：
//! 每个 scheme 对应一个组件实现，向注册表登记后，`markdown_protocol`
//! 的解析 / 渲染链路即自动获得对该 scheme 的拦截能力——
//! **扩展 = 一组件 + 一行注册**，解析器本体零改动。
//!
//! ## 内置组件
//!
//! - [`MentionComponent`]：agent / task / project 三实例，委托
//!   [`crate::mention`] 既有 API（单一事实源地位不变，本模块零协议逻辑复制）；
//! - [`AttachmentComponent`] / [`ArtifactComponent`]：批2 资源占位，
//!   批1 仅登记 scheme 与降级文本形态，`extract` 快照通道待 DAL resolver 注入。
//!
//! ## 降级安全
//!
//! 注册表未命中（scheme 未注册）时，`transform` / `parse_protocol` 一律按
//! 普通链接原样透传——与 mention「未识别 dest 降级普通链接」同一哲学，
//! 协议 id 不会裸奔给渲染层。

use std::collections::HashMap;
use std::sync::Arc;

use crate::mention::{MentionKind, MentionRef};

/// 组件解析产物：可读名 + 可选一行摘要（批2 起 DAL-backed 组件填充）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPayload {
    /// 可读展示名（实时解析命中或快照回退）
    pub name: String,
    /// 一行上下文摘要（任务状态、附件大小等），可空
    pub summary: Option<String>,
}

/// 一种 scheme 的协议组件：声明拦截资格与降级行为
///
/// 实现要求：`scheme` 返回值全局唯一（重复注册以最后登记者覆盖）。
/// 线程安全：注册表按 `Arc<dyn ProtocolComponent>` 持有，双端共享。
pub trait ProtocolComponent: Send + Sync {
    /// 本组件负责的 scheme（dest 中 `scheme:id[@org]` 的 scheme 段）
    fn scheme(&self) -> &'static str;

    /// 是否参与通知提取链（mention 族 = true；资源引用 = false，
    /// 通知正文不展开附件 / 产物列表）
    fn participates_in_notify(&self) -> bool;

    /// 解析引用为可读载荷（快照通道）
    ///
    /// 批1 默认 `None`（协议层不触存储）；批2 起 DAL-backed 组件
    /// 经 Registry 注入 resolver 实现。
    fn extract(&self, r: &ProtocolRef) -> Option<ResolvedPayload> {
        let _ = r;
        None
    }

    /// 降级文本渲染（prompt 注入 / 出站降级场景用，双端一致）
    fn render_text(&self, r: &ProtocolRef, snapshot: &str) -> String;
}

/// 统一资源引用形态（决策点②落点）：`scheme:id[@org]`
///
/// 协议层的统一引用表示；mention 族经 [`ProtocolRef::to_mention`] /
/// [`From<&MentionRef>`] 与 [`crate::mention::MentionRef`] 保形互转。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolRef {
    /// scheme 段（已注册才具备拦截资格）
    pub scheme: String,
    /// 实体 id
    pub id: String,
    /// 跨组织限定（仅 agent 提及使用），None = 组织内
    pub org: Option<String>,
}

impl ProtocolRef {
    /// 转 mention 引用：仅 agent / task / project 三种 scheme 可转，其余 `None`
    pub fn to_mention(&self) -> Option<MentionRef> {
        let kind = match self.scheme.as_str() {
            "agent" => MentionKind::Agent,
            "task" => MentionKind::Task,
            "project" => MentionKind::Project,
            _ => return None,
        };
        Some(MentionRef {
            kind,
            id: self.id.clone(),
            org: self.org.clone(),
        })
    }
}

impl From<&MentionRef> for ProtocolRef {
    fn from(m: &MentionRef) -> Self {
        Self {
            scheme: m.kind.as_str().to_string(),
            id: m.id.clone(),
            org: m.org.clone(),
        }
    }
}

/// scheme → 组件 的运行时注册表（拦截资格的唯一事实源）
#[derive(Default)]
pub struct ProtocolRegistry {
    components: HashMap<&'static str, Arc<dyn ProtocolComponent>>,
}

impl ProtocolRegistry {
    /// 进入 builder 形态（`let reg = ProtocolRegistry::builder().register(..).build()`）
    pub fn builder() -> ProtocolRegistryBuilder {
        ProtocolRegistryBuilder {
            registry: ProtocolRegistry::default(),
        }
    }

    /// 注册一个组件（扩展点：一行注册；同 scheme 后注册者覆盖）
    pub fn register(&mut self, c: Arc<dyn ProtocolComponent>) {
        self.components.insert(c.scheme(), c);
    }

    /// 按 scheme 查组件；未注册返回 `None`（调用方降级普通链接）
    pub fn get(&self, scheme: &str) -> Option<Arc<dyn ProtocolComponent>> {
        self.components.get(scheme).cloned()
    }

    /// scheme 是否已注册
    pub fn contains(&self, scheme: &str) -> bool {
        self.components.contains_key(scheme)
    }

    /// 双端默认组装：mention 族 3 组件 + attachment / artifact 占位 2 组件
    pub fn default_registry() -> ProtocolRegistry {
        let mut reg = ProtocolRegistry::default();
        for kind in [MentionKind::Agent, MentionKind::Task, MentionKind::Project] {
            reg.register(Arc::new(MentionComponent { kind }));
        }
        reg.register(Arc::new(AttachmentComponent::default()));
        reg.register(Arc::new(ArtifactComponent::default()));
        reg
    }
}

/// [`ProtocolRegistry`] 的链式 builder
pub struct ProtocolRegistryBuilder {
    registry: ProtocolRegistry,
}

impl ProtocolRegistryBuilder {
    /// 注册一个组件（链式）
    pub fn register(mut self, c: Arc<dyn ProtocolComponent>) -> Self {
        self.registry.register(c);
        self
    }

    /// 完成构建
    pub fn build(self) -> ProtocolRegistry {
        self.registry
    }
}

/// mention 族组件（agent / task / project 三实例）
///
/// 委托 [`crate::mention`] 既有 API，自身零协议逻辑：
/// `render_text` 的实时名覆盖语义与 [`crate::mention::resolve_display_name`] 一致。
pub struct MentionComponent {
    /// 本实例对应的提及类型（决定 scheme）
    pub kind: MentionKind,
}

impl ProtocolComponent for MentionComponent {
    fn scheme(&self) -> &'static str {
        self.kind.as_str()
    }

    fn participates_in_notify(&self) -> bool {
        true
    }

    fn render_text(&self, _r: &ProtocolRef, snapshot: &str) -> String {
        format!("@{snapshot}")
    }
}

/// 附件引用组件（批2 数据化：domain resolver 预解析注入，协议层零 DAL 依赖）
///
/// - [`AttachmentComponent::default`]：批1 占位形态（空解析表，`extract` = None、
///   不参与通知），`default_registry` 维持向后兼容、批1 行为零回归；
/// - [`AttachmentComponent::with_resolved`]：批2 实装形态——domain 层 async 预解析
///   attachment id → [`ResolvedPayload`] 后注入，命中实名降级、未命中快照兜底。
pub struct AttachmentComponent {
    /// 预解析表：attachment id → 资源载荷（domain 层注入）
    resolved: HashMap<String, ResolvedPayload>,
    /// 是否参与通知提取链（数据化实例 = true；批1 占位 = false）
    notify_participant: bool,
}

impl Default for AttachmentComponent {
    fn default() -> Self {
        Self {
            resolved: HashMap::new(),
            notify_participant: false,
        }
    }
}

impl AttachmentComponent {
    /// 数据化构造：注入预解析载荷表（经 Registry 注册即覆盖批1 占位组件）
    pub fn with_resolved(resolved: HashMap<String, ResolvedPayload>) -> Self {
        Self {
            resolved,
            notify_participant: true,
        }
    }
}

impl ProtocolComponent for AttachmentComponent {
    fn scheme(&self) -> &'static str {
        "attachment"
    }

    fn participates_in_notify(&self) -> bool {
        self.notify_participant
    }

    fn extract(&self, r: &ProtocolRef) -> Option<ResolvedPayload> {
        self.resolved.get(&r.id).cloned()
    }

    fn render_text(&self, r: &ProtocolRef, snapshot: &str) -> String {
        match self.resolved.get(&r.id) {
            // 已解析：实名占位（资源详情参与出站降级）
            Some(p) => format!("[附件：{}]", p.name),
            // 未命中（id 查无 / 已删）：快照兜底，防悬空
            None => format!("[附件：{snapshot}]"),
        }
    }
}

/// 产物引用组件（批2 数据化，与 [`AttachmentComponent`] 同构）
///
/// - [`ArtifactComponent::default`]：批1 占位形态（向后兼容）；
/// - [`ArtifactComponent::with_resolved`]：批2 实装形态（产物 id → 载荷注入）。
pub struct ArtifactComponent {
    /// 预解析表：artifact id → 资源载荷（domain 层注入）
    resolved: HashMap<String, ResolvedPayload>,
    /// 是否参与通知提取链（数据化实例 = true；批1 占位 = false）
    notify_participant: bool,
}

impl Default for ArtifactComponent {
    fn default() -> Self {
        Self {
            resolved: HashMap::new(),
            notify_participant: false,
        }
    }
}

impl ArtifactComponent {
    /// 数据化构造：注入预解析载荷表（经 Registry 注册即覆盖批1 占位组件）
    pub fn with_resolved(resolved: HashMap<String, ResolvedPayload>) -> Self {
        Self {
            resolved,
            notify_participant: true,
        }
    }
}

impl ProtocolComponent for ArtifactComponent {
    fn scheme(&self) -> &'static str {
        "artifact"
    }

    fn participates_in_notify(&self) -> bool {
        self.notify_participant
    }

    fn extract(&self, r: &ProtocolRef) -> Option<ResolvedPayload> {
        self.resolved.get(&r.id).cloned()
    }

    fn render_text(&self, r: &ProtocolRef, snapshot: &str) -> String {
        match self.resolved.get(&r.id) {
            Some(p) => format!("[产物：{}]", p.name),
            None => format!("[产物：{snapshot}]"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_ref_mention_roundtrip() {
        // agent / task / project 保形互转（含 org 段）
        for dest in ["agent:agt_9@org-B12", "task:tsk_a91", "project:prj_2c8"] {
            let m = crate::mention::parse_mention_dest(dest).expect("合法提及 dest");
            let r = ProtocolRef::from(&m);
            assert_eq!(r.to_mention(), Some(m));
        }
    }

    #[test]
    fn protocol_ref_non_mention_scheme_to_mention_none() {
        let r = ProtocolRef {
            scheme: "attachment".into(),
            id: "att_1".into(),
            org: None,
        };
        assert_eq!(r.to_mention(), None);
    }

    #[test]
    fn mention_components_delegate_fact_source() {
        let reg = ProtocolRegistry::default_registry();
        for (scheme, kind) in [
            ("agent", MentionKind::Agent),
            ("task", MentionKind::Task),
            ("project", MentionKind::Project),
        ] {
            let c = reg.get(scheme).expect("mention 三组件已注册");
            assert_eq!(c.scheme(), scheme);
            assert!(c.participates_in_notify());
            let m = MentionRef {
                kind,
                id: "x1".into(),
                org: None,
            };
            assert_eq!(c.render_text(&ProtocolRef::from(&m), "张伟"), "@张伟");
        }
    }

    #[test]
    fn resource_components_are_notify_silent() {
        let reg = ProtocolRegistry::default_registry();
        for scheme in ["attachment", "artifact"] {
            let c = reg.get(scheme).expect("资源占位组件已注册");
            assert!(!c.participates_in_notify());
            // extract 批1 快照通道默认 None（批2 DAL resolver 注入）
            let r = ProtocolRef {
                scheme: scheme.into(),
                id: "r1".into(),
                org: None,
            };
            assert_eq!(c.extract(&r), None);
        }
        assert_eq!(
            reg.get("attachment").unwrap().render_text(
                &ProtocolRef {
                    scheme: "attachment".into(),
                    id: "att_1".into(),
                    org: None
                },
                "设计稿.png"
            ),
            "[附件：设计稿.png]"
        );
        assert_eq!(
            reg.get("artifact").unwrap().render_text(
                &ProtocolRef {
                    scheme: "artifact".into(),
                    id: "art_1".into(),
                    org: None
                },
                "调研报告"
            ),
            "[产物：调研报告]"
        );
    }

    #[test]
    fn registry_extension_demo_one_component_one_line() {
        // 「一组件 + 一行注册」演示：新 scheme 注册后立即具备拦截资格
        struct TestComponent;
        impl ProtocolComponent for TestComponent {
            fn scheme(&self) -> &'static str {
                "test"
            }
            fn participates_in_notify(&self) -> bool {
                false
            }
            fn render_text(&self, _r: &ProtocolRef, snapshot: &str) -> String {
                format!("[测试：{snapshot}]")
            }
        }
        let mut reg = ProtocolRegistry::default_registry();
        assert!(!reg.contains("test"));
        reg.register(Arc::new(TestComponent));
        assert!(reg.contains("test"));
        let r = ProtocolRef {
            scheme: "test".into(),
            id: "1".into(),
            org: None,
        };
        assert_eq!(reg.get("test").unwrap().render_text(&r, "x"), "[测试：x]");
    }

    #[test]
    fn builder_chain_registers() {
        struct Another;
        impl ProtocolComponent for Another {
            fn scheme(&self) -> &'static str {
                "another"
            }
            fn participates_in_notify(&self) -> bool {
                false
            }
            fn render_text(&self, _r: &ProtocolRef, s: &str) -> String {
                s.to_string()
            }
        }
        let reg = ProtocolRegistry::builder()
            .register(Arc::new(MentionComponent {
                kind: MentionKind::Agent,
            }))
            .register(Arc::new(Another))
            .build();
        assert!(reg.contains("another"));
        assert!(reg.contains("agent"));
    }

    #[test]
    fn resolved_attachment_component_extracts_and_renders() {
        // 批2 数据化：命中实名载荷 / 未命中快照兜底 / 参与通知
        let mut map = HashMap::new();
        map.insert(
            "att_1".to_string(),
            ResolvedPayload {
                name: "设计稿.png".to_string(),
                summary: Some("image/png · 125952".to_string()),
            },
        );
        let c = AttachmentComponent::with_resolved(map);
        let r = ProtocolRef {
            scheme: "attachment".into(),
            id: "att_1".into(),
            org: None,
        };
        assert_eq!(
            c.extract(&r),
            Some(ResolvedPayload {
                name: "设计稿.png".into(),
                summary: Some("image/png · 125952".into()),
            })
        );
        assert_eq!(c.render_text(&r, "快照名"), "[附件：设计稿.png]");
        assert!(c.participates_in_notify());

        // 未命中 id：快照兜底防悬空
        let miss = ProtocolRef {
            scheme: "attachment".into(),
            id: "att_x".into(),
            org: None,
        };
        assert_eq!(c.extract(&miss), None);
        assert_eq!(c.render_text(&miss, "快照名"), "[附件：快照名]");
    }

    #[test]
    fn default_resource_components_stay_placeholder_compatible() {
        // 批1 占位行为零回归：Default 构造 extract=None / 不参与通知 / 快照 render
        let c = AttachmentComponent::default();
        let r = ProtocolRef {
            scheme: "attachment".into(),
            id: "att_1".into(),
            org: None,
        };
        assert_eq!(c.extract(&r), None);
        assert!(!c.participates_in_notify());
        assert_eq!(c.render_text(&r, "设计稿"), "[附件：设计稿]");

        let a = ArtifactComponent::default();
        let ar = ProtocolRef {
            scheme: "artifact".into(),
            id: "art_1".into(),
            org: None,
        };
        assert_eq!(a.extract(&ar), None);
        assert!(!a.participates_in_notify());
        assert_eq!(a.render_text(&ar, "报告"), "[产物：报告]");
    }

    #[test]
    fn late_registration_overrides_placeholder() {
        // 「同 scheme 后注册者覆盖」：数据化实例覆盖 default_registry 占位
        let mut map = HashMap::new();
        map.insert(
            "att_2".to_string(),
            ResolvedPayload {
                name: "doc.pdf".to_string(),
                summary: None,
            },
        );
        let mut reg = ProtocolRegistry::default_registry();
        assert!(!reg.get("attachment").unwrap().participates_in_notify());
        reg.register(Arc::new(AttachmentComponent::with_resolved(map)));
        let c = reg.get("attachment").unwrap();
        assert!(c.participates_in_notify());
        assert_eq!(
            c.extract(&ProtocolRef {
                scheme: "attachment".into(),
                id: "att_2".into(),
                org: None,
            }),
            Some(ResolvedPayload {
                name: "doc.pdf".into(),
                summary: None,
            })
        );
    }
}
