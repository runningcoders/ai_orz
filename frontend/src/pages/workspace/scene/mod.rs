//! 场景层：固定席位场
//!
//! 与关系图的区别是**坐标先于数据**：席位由常量表决定，实体只改变「谁坐在哪儿」，
//! 数据重排不会移动已落座的实体。渲染由席位场专用 Canvas 承担
//! （[`canvas::SeatCanvas`]）：无常驻渲染循环、无粒子、空席暗置。
//!
//! 当前只承载 Agent 实体（v1 投影只产出 Agent）；Project / Task 的详情仍走图谱视图。

pub mod canvas;
pub mod seats;

use dioxus::prelude::*;

use common::api::{SceneAttention, SceneEntity};

use super::presentation::{category_label, color_for, radius_for};
use canvas::{SeatCanvas, SeatContent};
use seats::{SEAT_COUNT, SeatMap};

/// 场景层属性
#[derive(Props, Clone, PartialEq)]
pub struct SceneFieldProps {
    /// 关注集实体（已由后端按 attention 排序并截断）
    pub entities: Vec<SceneEntity>,
    /// 受控选中实体 id（详情视图高亮并推进镜头）
    #[props(default)]
    pub selected: Option<String>,
    /// 点选结果：`Some(id)` = 选中该席；`None` = 点空白 / 空席
    pub on_select: EventHandler<Option<String>>,
}

/// 固定席位场
#[component]
pub fn SceneField(props: SceneFieldProps) -> Element {
    let seat_map = use_signal(SeatMap::load);
    let mut seat_map_sig = seat_map;

    // 落座：已有席位不变，新实体取最小空位；成员变化才回写 localStorage
    let ids: Vec<String> = props.entities.iter().map(|e| e.id.clone()).collect();
    use_effect(use_reactive!(|ids| {
        let mut map = seat_map_sig.read().clone();
        if map.assign(&ids) {
            map.save();
            seat_map_sig.set(map);
        }
    }));

    let map = seat_map.read().clone();
    let mut seats: Vec<Option<SeatContent>> = vec![None; SEAT_COUNT];
    for entity in &props.entities {
        let Some(seat) = map.seat_of(&entity.id) else {
            // 席位已满（超出上限的实体不落座）
            continue;
        };
        let Some(slot) = seats.get_mut(seat) else {
            continue;
        };
        *slot = Some(SeatContent {
            id: entity.id.clone(),
            label: entity.display_name.clone(),
            color: color_for(entity.dims.category).to_string(),
            radius: radius_for(entity.attention),
            highlighted: entity.attention == SceneAttention::NeedsYou,
            detail: entity
                .labels
                .first()
                .map(|label| label.text.clone())
                .or_else(|| Some(category_label(entity.dims.category).to_string())),
        });
    }

    rsx! {
        div { class: "relative w-full h-full",
            SeatCanvas {
                seats,
                selected: props.selected.clone(),
                on_select: props.on_select,
            }
        }
    }
}
