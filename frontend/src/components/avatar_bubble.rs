//! 通用头像信息气泡（AvatarBubble）
//!
//! 点击头像弹出基础信息卡，Agent / 用户两用：
//! - Agent 头像：点击时按需拉取 `get_agent`（组件内缓存，重复点击不重复请求），
//!   卡片内容与聊天侧栏 Agent Tab 共用 [`crate::components::agent_summary`] 的分组实现。
//! - 用户头像：静态展示显示名与短 ID。
//!
//! 交互采用 DaisyUI dropdown 范式（tabindex + focus-within），点外自动收起，零额外 JS；
//! 浮层位置在点击 / 键盘聚焦时按头像实时视口坐标计算（见 [`resolve_anchor`]）。
//! `status` 传入时按 [`avatar_status_ring`] 渲染生命周期警示环（待离职=黄 / 已离职=红）。
//!
//! ⚠️ **宿主契约**：调用方只需给一个定位容器（聊天页是 `.chat-image`，负责 grid 定位），
//! **不要**再叠加 `.avatar`——`.avatar` 由本组件贴在触发层上，紧邻圆形，
//! 否则 DaisyUI 的 `.avatar > div`（aspect-ratio + overflow:hidden）会落到中间的
//! dropdown 包装层，导致头像变椭圆 + 浮层被裁（详见组件内结构约束注释）。

use dioxus::prelude::*;
use dioxus_router::Link;
use wasm_bindgen::JsCast;

use crate::api::hr::get_agent;
use crate::components::agent_summary::{agent_badge_row, agent_identity_row};
use crate::components::state::Loading;
use crate::utils::{avatar_initials, avatar_status_ring};
use common::api::{GetAgentRequest, GetAgentResponse};

/// 头像配色基调：Agent=secondary / User=primary（与聊天页原头像配色一致）
#[derive(Clone, Copy, PartialEq)]
pub enum AvatarTone {
    Agent,
    User,
}

impl AvatarTone {
    fn classes(self) -> &'static str {
        match self {
            Self::Agent => "bg-secondary text-secondary-content",
            Self::User => "bg-primary text-primary-content",
        }
    }
}

/// 浮层水平对齐（CODE_STANDARDS §5：枚举类型安全，替代散落的方位字符串）。
///
/// 与 DaisyUI dropdown 方位类一一对应，仍保留该类以复用其 `:focus-within` 展开机制；
/// 真正决定位置的是本组件算出的 `position: fixed` 锚点。
#[derive(Clone, Copy, PartialEq, Default)]
pub enum BubbleAlign {
    /// 左对齐（`dropdown-start`）—— Agent 头像在左，气泡向右展开
    #[default]
    Start,
    /// 右对齐（`dropdown-end`）—— 用户头像在右，气泡向左展开
    End,
}

impl BubbleAlign {
    fn dropdown_class(self) -> &'static str {
        match self {
            Self::Start => "dropdown-start",
            Self::End => "dropdown-end",
        }
    }

    /// 锚点用 `right`（右对齐）还是 `left`（左对齐）
    fn is_end(self) -> bool {
        matches!(self, Self::End)
    }
}

/// 头像盒边长（Tailwind `w-10` = 2.5rem = 40px）：水平锚点与翻转判定都要用
const AVATAR_SIZE: f64 = 40.0;
/// 浮层与头像之间的间距（对应原 DaisyUI 的 `mb-2`）
const BUBBLE_GAP: f64 = 8.0;
/// 浮层高度判定的经验上限：内容最满时（头像行 + 徽章行 + 3 行简介 + 能力 chips + 按钮）
/// 无头实测约 168px，留余量按 220px 判定是否还有空间向上弹。
const BUBBLE_MAX_H: f64 = 220.0;

/// 浮层锚点（视口坐标）。定位改用 `position: fixed` 而非 DaisyUI 的
/// `bottom: 100%`，见 [`AvatarBubble`] 的定位注释。
#[derive(Clone, Copy, PartialEq)]
struct BubbleAnchor {
    /// `false` 用 `left`（start 左对齐）；`true` 用 `right`（end 右对齐）
    end: bool,
    /// 水平值：`left` 或 `right` 的 px
    horizontal: f64,
    /// 垂直值：向上弹时为 `bottom`，向下翻转时为 `top`
    vertical: f64,
    /// 向上弹（优先）；头像上方放不下整张卡时翻到下方
    upward: bool,
    /// 所选方向的可用空间（px）：卡片高度超过估算时用它限高，避免越出可用区
    available: f64,
}

impl BubbleAnchor {
    /// 拼 inline style：四个方向里**必须成对写 `auto`**，
    /// 否则 DaisyUI `.dropdown-top .dropdown-content{bottom:100%}` 会与 inline `top` 同时生效，
    /// 把卡片按两个方向一起约束 → 高度被压扁。
    fn style(&self) -> String {
        let h = if self.end {
            format!("right:{}px;left:auto", self.horizontal)
        } else {
            format!("left:{}px;right:auto", self.horizontal)
        };
        let v = if self.upward {
            format!("bottom:{}px;top:auto", self.vertical)
        } else {
            format!("top:{}px;bottom:auto", self.vertical)
        };
        let origin = if self.upward { "bottom" } else { "top" };
        // 内容超出所选方向的空间时限高 + 内部滚动，宁可滚动也不越出可用区
        let max_h = (self.available - BUBBLE_GAP).max(120.0);
        format!(
            "position:fixed;{h};{v};margin:0;transform-origin:{origin};\
             max-height:{max_h}px;overflow-y:auto"
        )
    }
}

/// 视口尺寸 `(宽, 高)`；取不到或为 0 时返回 `None`。
fn viewport_size() -> Option<(f64, f64)> {
    let win = web_sys::window()?;
    let vw = win.inner_width().ok().and_then(|v| v.as_f64())?;
    let vh = win.inner_height().ok().and_then(|v| v.as_f64())?;
    (vw > 0.0 && vh > 0.0).then_some((vw, vh))
}

/// 当前焦点元素（展开瞬间即触发层本体），用于向上定位滚动祖先。
fn focused_element() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.active_element())
}

/// 向上找最近的「可滚动祖先」的视口矩形，作为浮层的可用区边界。
///
/// 消息区是 `overflow-y: auto`：浮层虽用 `position: fixed` 脱离了它的裁剪，
/// **上边界仍应受其约束** —— 否则按视口算出的「上方空间」会把 `.navbar`
/// （`sticky top-0 z-50`）的高度也算进去，卡片一路弹到导航条下面被切掉一截。
///
/// 判定用「内容高度 > 可视高度」，不依赖具体 id、也不需要 computed style
/// （本项目 web-sys 未开 `CssStyleDeclaration` feature）。
/// 找不到时返回 `None`，调用方退化为按整个视口计算。
fn scroll_ancestor_rect(from: &web_sys::Element) -> Option<web_sys::DomRect> {
    let mut cur = from.parent_element();
    while let Some(el) = cur {
        if let Some(h) = el.dyn_ref::<web_sys::HtmlElement>()
            && h.scroll_height() > h.client_height() + 1
        {
            return Some(el.get_bounding_client_rect());
        }
        cur = el.parent_element();
    }
    None
}

/// 由头像盒的视口左上角推导浮层锚点。
///
/// 点击与键盘聚焦两条路径共用同一套「向上优先 / 不够就翻向下 / 再不够就限高」规则，
/// 避免其中一条路径退回 DaisyUI 默认定位后在消息区被裁。
/// 可用的上/下边界取自 `bounds`（滚动祖先），取不到则退化为整个视口。
fn resolve_anchor(
    box_left: f64,
    box_top: f64,
    vw: f64,
    vh: f64,
    align: BubbleAlign,
    bounds: Option<web_sys::DomRect>,
) -> BubbleAnchor {
    // 边界与视口取交集：滚动祖先矩形同样可能带部分越界的边（如消息区底部被输入区覆盖）
    let top_edge = bounds.as_ref().map_or(0.0, |b| b.top().max(0.0));
    let bottom_edge = bounds.as_ref().map_or(vh, |b| b.bottom().min(vh));
    let space_above = (box_top - top_edge).max(0.0);
    let space_below = (bottom_edge - box_top - AVATAR_SIZE).max(0.0);
    // 优先向上（既有视觉）；上方放不下整张卡且下方不更宽裕时才翻转
    let upward = space_above >= BUBBLE_MAX_H + BUBBLE_GAP || space_above >= space_below;
    let end = align.is_end();
    let horizontal = if end {
        (vw - box_left - AVATAR_SIZE).max(0.0)
    } else {
        box_left.max(0.0)
    };
    BubbleAnchor {
        end,
        horizontal,
        vertical: if upward {
            vh - box_top + BUBBLE_GAP
        } else {
            box_top + AVATAR_SIZE + BUBBLE_GAP
        },
        upward,
        available: if upward { space_above } else { space_below },
    }
}

/// 点击头像弹出基础信息气泡。
///
/// - `agent_id` 有值 → Agent 卡（点击时懒加载 get_agent）
/// - `user_id` 有值 → 用户卡（展示短 ID）
/// - `status` 有值 → 按 [`avatar_status_ring`] 叠加生命周期警示环
/// - `align` → 浮层水平对齐（见 [`BubbleAlign`]）
#[component]
pub fn AvatarBubble(
    name: String,
    tone: AvatarTone,
    #[props(default)] agent_id: Option<String>,
    #[props(default)] status: Option<i32>,
    #[props(default)] user_id: Option<String>,
    #[props(default)] align: BubbleAlign,
) -> Element {
    // None=未加载 / Some(Err)=加载失败 / Some(Ok)=已加载；仅点击时拉取并缓存
    let mut agent_card = use_signal(|| None::<Result<GetAgentResponse, ()>>);
    // 浮层锚点：点击 / 聚焦时按头像实时位置算出，配合 `position: fixed` 渲染
    let mut anchor = use_signal(|| None::<BubbleAnchor>);
    let ring = status.map(avatar_status_ring).unwrap_or("");
    let tone_classes = tone.classes();
    let align_class = align.dropdown_class();
    let has_agent_card = agent_id.is_some();
    let open_agent_id = agent_id.clone();
    // 浮层用 `position: fixed` + 视口坐标锚定，而非 DaisyUI 的 `bottom: 100%`：
    // 消息区是 `overflow-y-auto`，absolute 浮层一旦越出它的 padding box 就被整块裁掉。
    // 无头实测（720px 消息区，卡片高 168px）：头像距消息区顶不足卡片高度时，
    // 卡片 computed style 仍是 `display:block; opacity:1` 却完全不可见 ——
    // 症状与「点击无反应」一模一样。fixed + 方向自适应（向上优先、上方不够就翻向下）
    // 同时绕开「消息区裁剪」和「视口顶部裁剪」两个限制。
    // 代价：锚点是展开瞬间的快照 → 滚动必须收起浮层（见聊天页 scroll 处理）。
    let card_style = anchor().map_or_else(String::new, |a| a.style());
    rsx! {
        // ⚠️ 根节点与 `.avatar` 之间**不能再插包装层**：DaisyUI 的
        // `.avatar > div { aspect-ratio: 1; display: block; overflow: hidden }`
        // 会命中 `.avatar` 的直接子 div。此前把 `.dropdown` 放在中间，
        // 该规则落到包装层上，同时踩两个坑（2026-09-14 无头实测）：
        //   ① 圆只剩 `w-10`（无 h-10）、正方形由 `.avatar > div` 的 aspect-ratio 提供，
        //      规则被包装层吃掉后圆盒实测 40 × 24 ——「头像被挤压」；
        //   ② `overflow: hidden` 把 `bottom: 100%` 的浮层裁进 40 × 40 的包装盒里，
        //      卡片 computed style 是 `display:block; opacity:1`（focus-within 正常工作）
        //      却一个像素都看不见 ——「点击无反应」。
        // 故 `.avatar` 只贴在触发层、紧邻圆形；宿主（`.chat-image`）只负责 grid 定位。
        div { class: "dropdown dropdown-top {align_class}",
            div {
                tabindex: 0,
                role: "button",
                class: "avatar avatar-bubble-trigger cursor-pointer outline-none",
                // 两条展开路径都必须先算锚点：漏掉的那条会退回 DaisyUI 的
                // `position: absolute; bottom: 100%`，在 `overflow-y-auto` 的消息区里
                // 被整块裁掉（键盘 Tab 聚焦即命中此坑，实测卡片 176px 仅露出 5px）。
                // 键盘路径没有鼠标坐标，改用 activeElement（此刻即本触发层）的视口矩形。
                onfocus: move |_| {
                    let Some(el) = focused_element() else {
                        return;
                    };
                    let Some((vw, vh)) = viewport_size() else {
                        return;
                    };
                    let r = el.get_bounding_client_rect();
                    let bounds = scroll_ancestor_rect(&el);
                    anchor.set(Some(resolve_anchor(r.left(), r.top(), vw, vh, align, bounds)));
                },
                onclick: move |evt: MouseEvent| {
                    if let Some((vw, vh)) = viewport_size() {
                        // 头像盒（= 事件 target）的视口左上角 = 鼠标视口坐标 − 鼠标相对盒的偏移
                        let c = evt.client_coordinates();
                        let o = evt.element_coordinates();
                        // 焦点此刻已落在触发层上（tabindex=0 → mousedown 先于 click 完成聚焦），
                        // 借它向上找滚动祖先，只用于取边界、不参与坐标计算
                        let bounds = focused_element().and_then(|el| scroll_ancestor_rect(&el));
                        anchor.set(Some(resolve_anchor(c.x - o.x, c.y - o.y, vw, vh, align, bounds)));
                    }
                    let Some(aid) = open_agent_id.clone() else {
                        return;
                    };
                    if agent_card().is_some() {
                        return;
                    }
                    spawn(async move {
                        let result = match get_agent(GetAgentRequest {
                            id: aid,
                            ..Default::default()
                        })
                        .await
                        {
                            Ok(a) => Some(Ok(a)),
                            Err(_) => Some(Err(())),
                        };
                        agent_card.set(result);
                    });
                },
                div { class: "w-10 h-10 rounded-full {tone_classes} flex items-center justify-center font-bold {ring}",
                    "{avatar_initials(&name)}"
                }
            }
            // 浮层视觉走 `.orz-popover`（切角 + 渐变发丝边 + 浮起阴影 + 统一层叠档），
            // 不再硬编码原生 card 类（ui_design_system §8 红线）。
            // `mb-2` 已删：定位改 inline style 后外边距由 BUBBLE_GAP 控制，类上的 mb-2 无效。
            div {
                tabindex: 0,
                class: "dropdown-content orz-popover w-64 p-3",
                style: card_style,
                if has_agent_card {
                    match agent_card() {
                        None => rsx! {
                            div { class: "flex items-center justify-center gap-2 py-2 text-xs text-base-content/60",
                                Loading { size: "xs" }
                                span { "加载中…" }
                            }
                        },
                        Some(Err(())) => rsx! {
                            div { class: "text-xs text-error py-1", "信息加载失败" }
                        },
                        Some(Ok(a)) => rsx! { { agent_bubble_card_content(&a) } },
                    }
                } else {
                    { user_bubble_card_content(&name, user_id.as_deref()) }
                }
            }
        }
    }
}

/// Agent 信息卡内容（紧凑预览版）。
///
/// 身份行与徽章行复用 [`crate::components::agent_summary`]（与聊天侧栏 Agent Tab 同源）；
/// 简介截断到 3 行、能力最多 6 个，是气泡尺寸下的收敛策略。
///
/// 普通函数而非 `#[component]`：Dioxus 组件 Props derive 要求字段实现 PartialEq，
/// 而 common 的 `GetAgentResponse` 未实现（避免为此污染数据契约）。
fn agent_bubble_card_content(agent: &GetAgentResponse) -> Element {
    let aid = agent.id.clone();
    let desc = agent.description.clone().filter(|s| !s.is_empty());
    let capabilities = agent.capabilities.clone().unwrap_or_default();
    rsx! {
        div { class: "space-y-2",
            // 头像环传空串：状态已由下方生命周期徽章表达，同一张卡内不再说第二遍
            { agent_identity_row(agent, "") }
            { agent_badge_row(agent) }
            if let Some(d) = desc {
                p { class: "text-xs text-base-content/70 line-clamp-3", "{d}" }
            }
            if !capabilities.is_empty() {
                div { class: "flex flex-wrap gap-1",
                    for c in capabilities.iter().take(6) {
                        span { key: "{c}", class: "{crate::utils::tag_chip()}", "{c}" }
                    }
                }
            }
            Link {
                class: "btn hud-btn btn-ghost btn-xs",
                to: crate::pages::Route::HrAgentDetail { id: aid },
                "在详情页打开 →"
            }
        }
    }
}

/// 用户信息卡内容：静态展示（显示名 + 短 ID），无额外请求
fn user_bubble_card_content(name: &str, user_id: Option<&str>) -> Element {
    // utils 里有两个同名 short_id（status/message 各一），glob 重导出二义，
    // 故此处用 status 模块完整路径调用
    let uid_text = user_id.map(crate::utils::status::short_id);
    rsx! {
        div { class: "space-y-2",
            div { class: "flex items-center gap-2",
                div { class: "w-10 h-10 rounded-full bg-primary text-primary-content flex items-center justify-center font-bold",
                    "{avatar_initials(&name)}"
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-semibold truncate", "{name}" }
                    div { class: "text-xs text-base-content/60", "当前用户" }
                }
            }
            if let Some(text) = uid_text {
                div { class: "text-xs text-base-content/60", "ID：{text}" }
            }
        }
    }
}
