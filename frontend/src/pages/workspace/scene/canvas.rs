//! 席位场专用 Canvas
//!
//! 与通用 `components::canvas_scene::CanvasScene` 的区别：
//! - **无常驻渲染循环**：只在数据 / 尺寸 / 镜头变化时画一帧，空闲不产生任何帧；
//! - **镜头即机位**：全景固定（世界坐标 = 屏幕坐标），选中某席时缓动推进到它；
//! - **只画席位**：没有力导向、连线、卡片与粒子，席位坐标由常量表决定。
//!
//! 坐标先于数据：席位位置只由「第几席」和画布尺寸推导，实体状态变化不会移动座位。

use dioxus::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use super::seats::{SEAT_COUNT, SEAT_POSITIONS};

/// 颜色过渡时长（毫秒）：类别变化时做一次交叉淡变，而不是瞬间跳色
const COLOR_TRANSITION_MS: f64 = 240.0;
/// 高亮（需要我出手的外圈柔光）淡入淡出时长（毫秒）
const HIGHLIGHT_TRANSITION_MS: f64 = 240.0;
/// 过渡动画帧间隔（毫秒，约 60fps）；只在有过渡时运行，静止后不再排帧
const FRAME_MS: u32 = 16;

/// 全景镜头：世界坐标 1:1（席位按画布尺寸算好后直接落在屏幕中心）
const PANORAMA_SCALE: f64 = 1.0;
/// 聚焦镜头放大倍数：足以看清名称与状态标签，又不至于只剩一个光点
const FOCUS_SCALE: f64 = 1.9;
/// 镜头缓动时长（毫秒）
const CAMERA_EASE_MS: u32 = 300;
/// 缓动步长（毫秒，约 60fps）
const CAMERA_EASE_STEP_MS: u32 = 16;
/// 缓动步数
const CAMERA_EASE_STEPS: u32 = CAMERA_EASE_MS / CAMERA_EASE_STEP_MS;

/// LOD 阈值：缩放小于它只画光点，大于它加名称，再大于它加状态标签
const LOD_NAME_SCALE: f64 = 0.8;
const LOD_DETAIL_SCALE: f64 = 1.4;

/// 席位布局：横向 / 纵向间距相对画布尺寸的比例
const LAYOUT_GAP_X_RATIO: f64 = 0.2;
const LAYOUT_GAP_Y_RATIO: f64 = 0.22;
/// 布局参考尺寸上限：超大屏上席位间距不再线性放大，避免 8 席散得太开
const LAYOUT_MAX_WIDTH: f64 = 1400.0;
const LAYOUT_MAX_HEIGHT: f64 = 900.0;

/// 空席位半径（固定视觉占位，不随实体密度变化）
const EMPTY_SEAT_RADIUS: f64 = 10.0;
const EMPTY_SEAT_FILL: &str = "rgba(30, 41, 59, 0.45)";
const EMPTY_SEAT_STROKE: &str = "rgba(71, 85, 105, 0.55)";

/// 文本与选中环颜色
const TEXT_COLOR: &str = "#e2e8f0";
const DETAIL_TEXT_COLOR: &str = "#94a3b8";
const SELECTION_RING_COLOR: &str = "#f8fafc";

/// 最小点击半径（光点缩小时仍可点中）
const MIN_HIT_RADIUS: f64 = 16.0;

/// 聚焦时画面中心在纵向的偏置：给底部横幅留出空间
const FOCUS_CENTER_Y_RATIO: f64 = 0.45;

const TAU: f64 = std::f64::consts::TAU;

/// 一席的内容（`None` 表示空席）
#[derive(Debug, Clone, PartialEq)]
pub struct SeatContent {
    /// 业务 id（Agent id）
    pub id: String,
    /// 展示名称
    pub label: String,
    /// 光点颜色（由语义类别映射）
    pub color: String,
    /// 光点半径（由关注度映射）
    pub radius: f64,
    /// 是否等我出手（外圈高亮）
    pub highlighted: bool,
    /// 状态标签（LOD 最高层展示）
    pub detail: Option<String>,
}

/// 细节层级：由镜头缩放决定画到哪一层
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lod {
    /// 只有光点 + 状态环
    Dot,
    /// 光点 + 名称
    Name,
    /// 光点 + 名称 + 状态标签
    Detail,
}

impl Lod {
    /// 按缩放比选择细节层级
    pub fn for_scale(scale: f64) -> Self {
        if scale < LOD_NAME_SCALE {
            Self::Dot
        } else if scale < LOD_DETAIL_SCALE {
            Self::Name
        } else {
            Self::Detail
        }
    }
}

/// RGB 颜色（过渡插值用）
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgb {
    r: f64,
    g: f64,
    b: f64,
}

impl Rgb {
    /// 解析 `#rrggbb`；格式不符返回 `None`（调用方回退到中性色）
    fn parse(hex: &str) -> Option<Self> {
        let hex = hex.trim().trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let channel =
            |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok().map(f64::from);
        Some(Self {
            r: channel(0..2)?,
            g: channel(2..4)?,
            b: channel(4..6)?,
        })
    }

    fn lerp(from: Self, to: Self, t: f64) -> Self {
        Self {
            r: from.r + (to.r - from.r) * t,
            g: from.g + (to.g - from.g) * t,
            b: from.b + (to.b - from.b) * t,
        }
    }

    fn to_css(self) -> String {
        format!(
            "rgb({}, {}, {})",
            self.r.round() as u8,
            self.g.round() as u8,
            self.b.round() as u8
        )
    }
}

/// 阶段进度：`since` 起经过 `duration_ms` 后收敛到 1
fn progress(now: f64, since: f64, duration_ms: f64) -> f64 {
    if duration_ms <= 0.0 {
        return 1.0;
    }
    ((now - since) / duration_ms).clamp(0.0, 1.0)
}

/// 单席表现状态：颜色交叉淡变 + 高亮淡入淡出
///
/// 只在不连续的状态变化时产生过渡；过渡结束后各值等于目标值，绘制结果静止。
#[derive(Debug, Clone, Copy)]
struct SeatVisual {
    color_from: Rgb,
    color_to: Rgb,
    color_since: f64,
    highlight_from: f64,
    highlight_to: f64,
    highlight_since: f64,
}

impl SeatVisual {
    fn color_at(&self, now: f64) -> Rgb {
        Rgb::lerp(
            self.color_from,
            self.color_to,
            progress(now, self.color_since, COLOR_TRANSITION_MS),
        )
    }

    fn highlight_at(&self, now: f64) -> f64 {
        let t = progress(now, self.highlight_since, HIGHLIGHT_TRANSITION_MS);
        self.highlight_from + (self.highlight_to - self.highlight_from) * t
    }

    /// 颜色与高亮都收敛到目标值
    fn settled(&self, now: f64) -> bool {
        let color_done = self.color_from == self.color_to
            || progress(now, self.color_since, COLOR_TRANSITION_MS) >= 1.0;
        let highlight_done = (self.highlight_from - self.highlight_to).abs() <= f64::EPSILON
            || progress(now, self.highlight_since, HIGHLIGHT_TRANSITION_MS) >= 1.0;
        color_done && highlight_done
    }
}

/// 全部席位的表现状态表
///
/// 不持有计时器：`sync` 比较最新数据与当前呈现值，只有变化时才开启一段过渡；
/// 调用方据此决定是否排下一帧（无变化 ⇒ 不再绘制）。
#[derive(Debug, Default)]
struct VisualBook {
    entries: HashMap<String, SeatVisual>,
}

impl VisualBook {
    /// 用最新席位数据推进状态；返回是否仍有未完成的过渡
    fn sync(&mut self, seats: &[Option<SeatContent>], now: f64) -> bool {
        let live: std::collections::HashSet<&str> = seats
            .iter()
            .flatten()
            .map(|content| content.id.as_str())
            .collect();
        self.entries.retain(|id, _| live.contains(id.as_str()));

        for content in seats.iter().flatten() {
            let color = Rgb::parse(&content.color).unwrap_or(Rgb {
                r: 100.0,
                g: 116.0,
                b: 139.0,
            });
            let highlight = if content.highlighted { 1.0 } else { 0.0 };
            let entry = self
                .entries
                .entry(content.id.clone())
                .or_insert(SeatVisual {
                    color_from: color,
                    color_to: color,
                    color_since: now,
                    highlight_from: highlight,
                    highlight_to: highlight,
                    highlight_since: now,
                });
            // 以「当前呈现值」为新过渡起点：连续变化时不会跳回上一段起点
            if entry.color_to != color {
                entry.color_from = entry.color_at(now);
                entry.color_to = color;
                entry.color_since = now;
            }
            if (entry.highlight_to - highlight).abs() > f64::EPSILON {
                entry.highlight_from = entry.highlight_at(now);
                entry.highlight_to = highlight;
                entry.highlight_since = now;
            }
        }

        self.has_active(now)
    }

    /// 是否仍有过渡进行中
    fn has_active(&self, now: f64) -> bool {
        self.entries.values().any(|visual| !visual.settled(now))
    }
}

/// 镜头：世界坐标 → 屏幕坐标
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub scale: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}

impl Camera {
    /// 全景机位（世界坐标即屏幕坐标）
    pub fn panorama() -> Self {
        Self {
            scale: PANORAMA_SCALE,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }

    /// 聚焦机位：让世界坐标 `(x, y)` 落在画面中心
    pub fn focused(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            scale: FOCUS_SCALE,
            pan_x: width / 2.0 - x * FOCUS_SCALE,
            pan_y: height * FOCUS_CENTER_Y_RATIO - y * FOCUS_SCALE,
        }
    }

    /// 世界坐标 → 屏幕坐标
    pub fn to_screen(self, x: f64, y: f64) -> (f64, f64) {
        (x * self.scale + self.pan_x, y * self.scale + self.pan_y)
    }

    /// 线性插值（缓动曲线由调用方给出）
    fn lerp(from: Self, to: Self, t: f64) -> Self {
        Self {
            scale: from.scale + (to.scale - from.scale) * t,
            pan_x: from.pan_x + (to.pan_x - from.pan_x) * t,
            pan_y: from.pan_y + (to.pan_y - from.pan_y) * t,
        }
    }
}

/// 席位 → 世界坐标
///
/// 围绕画布中心布局；全景镜头下即为屏幕坐标。坐标只依赖席位编号与画布尺寸，
/// 因此数据重排不会移动已落座的实体。
pub fn seat_world_pos(seat: usize, width: f64, height: f64) -> (f64, f64) {
    let (nx, ny) = SEAT_POSITIONS[seat % SEAT_COUNT];
    let cx = width / 2.0;
    let cy = height / 2.0;
    let gap_x = width.min(LAYOUT_MAX_WIDTH) * LAYOUT_GAP_X_RATIO;
    let gap_y = height.min(LAYOUT_MAX_HEIGHT) * LAYOUT_GAP_Y_RATIO;
    (cx + nx * gap_x, cy + ny * gap_y)
}

/// 目标镜头：选中实体 → 推进到它；未选中 → 回到全景
pub fn target_camera(
    selected: Option<&str>,
    seats: &[Option<SeatContent>],
    width: f64,
    height: f64,
) -> Option<Camera> {
    let Some(id) = selected else {
        return Some(Camera::panorama());
    };
    let (seat, _) = seats
        .iter()
        .enumerate()
        .find(|(_, slot)| slot.as_ref().is_some_and(|c| c.id == id))?;
    let (x, y) = seat_world_pos(seat, width, height);
    Some(Camera::focused(x, y, width, height))
}

/// 命中检测：返回鼠标下已落座实体的 id；空席与空白都返回 `None`
pub fn hit_test(
    seats: &[Option<SeatContent>],
    camera: Camera,
    width: f64,
    height: f64,
    x: f64,
    y: f64,
) -> Option<String> {
    seats.iter().enumerate().rev().find_map(|(seat, slot)| {
        let content = slot.as_ref()?;
        let (wx, wy) = seat_world_pos(seat, width, height);
        let (sx, sy) = camera.to_screen(wx, wy);
        let r = (content.radius * camera.scale).max(MIN_HIT_RADIUS);
        let (dx, dy) = (x - sx, y - sy);
        (dx * dx + dy * dy <= r * r).then(|| content.id.clone())
    })
}

/// 席位场画布属性
#[derive(Props, Clone, PartialEq)]
pub struct SeatCanvasProps {
    /// 各席位内容，下标即席位编号（`None` = 空席）
    pub seats: Vec<Option<SeatContent>>,
    /// 受控选中实体 id
    #[props(default)]
    pub selected: Option<String>,
    /// 点选结果：`Some(id)` = 选中该席，`None` = 点空白 / 空席，退出聚焦
    pub on_select: EventHandler<Option<String>>,
}

/// 席位场画布
#[component]
pub fn SeatCanvas(props: SeatCanvasProps) -> Element {
    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);
    let mut size = use_signal(|| (0.0f64, 0.0f64));
    let camera = use_signal(Camera::panorama);
    let mut camera_sig = camera;
    // 动画代：选中切换时自增，旧动画在下一步自行退出
    let generation = use_signal(|| 0u64);
    let mut generation_sig = generation;
    // 表现状态机与帧信号：过渡只在状态变化时开启，结束后不再排帧
    let book = use_hook(|| Rc::new(RefCell::new(VisualBook::default())));
    let animating = use_hook(|| Rc::new(Cell::new(false)));
    let tick = use_signal(|| 0u64);
    let mut tick_sig = tick;

    // 重绘：依赖被显式订阅 —— 数据 / 选中变化走 use_reactive，
    // 镜头、尺寸与动画帧由闭包内 read() 订阅。空闲时无人写这些信号，因此不绘制。
    use_effect(use_reactive(
        (&props.seats, &props.selected),
        move |(seats, selected)| {
            let Some(canvas) = canvas_ref.read().clone() else {
                return;
            };
            let (width, height) = *size.read();
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            // 订阅动画帧：过渡进行中由 tick 驱动重绘
            let _ = *tick.read();

            let now = js_sys::Date::now();
            let mut visuals = book.borrow_mut();
            let still_animating = visuals.sync(&seats, now);
            draw_scene(
                &canvas,
                *camera.read(),
                &seats,
                selected.as_deref(),
                width,
                height,
                &visuals,
                now,
            );
            drop(visuals);

            // 只在有过渡时排帧：无事件 ⇒ 不进入循环，画面保持静止
            if still_animating && !animating.get() {
                animating.set(true);
                let book = book.clone();
                let animating = animating.clone();
                spawn(async move {
                    loop {
                        gloo_timers::future::TimeoutFuture::new(FRAME_MS).await;
                        let now = js_sys::Date::now();
                        let active = book.borrow().has_active(now);
                        let next = tick_sig().wrapping_add(1);
                        tick_sig.set(next);
                        if !active {
                            // 先放开标志再让最后一帧重绘；若期间又变化，重绘会重新起循环
                            animating.set(false);
                            break;
                        }
                    }
                });
            }
        },
    ));

    // 镜头推进：选中 → 聚焦该席；取消选中 → 回到全景。300ms 缓动，逐帧写 camera 信号，
    // 由上面的重绘 effect 消费。尺寸变化也会重跑（首帧测量晚于深链选中的情况）。
    use_effect(use_reactive(
        (&props.selected, &props.seats),
        move |(selected, seats)| {
            let (width, height) = *size.read();
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let Some(target) = target_camera(selected.as_deref(), &seats, width, height) else {
                return;
            };
            let from = *camera_sig.peek();
            if (from.scale - target.scale).abs() < 1e-3
                && (from.pan_x - target.pan_x).abs() < 1e-3
                && (from.pan_y - target.pan_y).abs() < 1e-3
            {
                return;
            }
            let my_generation = generation_sig.peek().wrapping_add(1);
            generation_sig.set(my_generation);
            spawn(async move {
                for step in 1..=CAMERA_EASE_STEPS {
                    gloo_timers::future::TimeoutFuture::new(CAMERA_EASE_STEP_MS).await;
                    if *generation_sig.peek() != my_generation {
                        return;
                    }
                    let t = ease_out_cubic(step as f64 / CAMERA_EASE_STEPS as f64);
                    camera_sig.set(Camera::lerp(from, target, t));
                }
                if *generation_sig.peek() == my_generation {
                    // 精确落位，避免缓动尾部累积误差
                    camera_sig.set(target);
                }
            });
        },
    ));

    // 跟随窗口尺寸：窗口缩放后重新测量，避免画布 buffer 与 CSS 尺寸错位
    // （否则画面被拉伸，席位坐标也停在旧布局上）。
    {
        #[allow(clippy::type_complexity)]
        let mut resize_listener =
            use_signal(|| Option::<(web_sys::Window, Closure<dyn FnMut()>)>::None);
        let mut size = size;
        use_effect(move || {
            let Some(window) = web_sys::window() else {
                return;
            };
            let cb = Closure::<dyn FnMut()>::new(move || {
                let Some(canvas) = (*canvas_ref.peek()).clone() else {
                    return;
                };
                let rect = canvas.get_bounding_client_rect();
                if rect.width() > 0.0 && rect.height() > 0.0 {
                    size.set((rect.width(), rect.height()));
                }
            });
            let _ = window.add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref());
            resize_listener.set(Some((window, cb)));
        });
        use_drop(move || {
            if let Some((window, cb)) = resize_listener.take() {
                let _ = window
                    .remove_event_listener_with_callback("resize", cb.as_ref().unchecked_ref());
            }
        });
    }

    let on_select = props.on_select;
    let seats_for_click = props.seats.clone();

    rsx! {
        canvas {
            style: "width: 100%; height: 100%; display: block; background: transparent; cursor: pointer;",
            onmounted: move |evt: MountedEvent| {
                if let Some(element) = evt.data().downcast::<web_sys::Element>() {
                    let canvas = element.clone().unchecked_into::<HtmlCanvasElement>();
                    canvas_ref.set(Some(canvas.clone()));
                    let rect = canvas.get_bounding_client_rect();
                    if rect.width() > 0.0 && rect.height() > 0.0 {
                        size.set((rect.width(), rect.height()));
                    }
                }
            },
            onclick: move |evt: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else {
                    return;
                };
                let (width, height) = *size.read();
                if width <= 0.0 || height <= 0.0 {
                    return;
                }
                let rect = canvas.get_bounding_client_rect();
                let coords = evt.client_coordinates();
                let hit = hit_test(
                    &seats_for_click,
                    *camera.read(),
                    width,
                    height,
                    coords.x - rect.left(),
                    coords.y - rect.top(),
                );
                on_select.call(hit);
            },
        }
    }
}

/// 缓动曲线：ease-out cubic（起步快、收尾稳，像镜头被轻轻停住）
fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t.min(1.0)).powi(3)
}

/// 绘制一帧（只在需要时调用，不注册任何持续循环）
#[allow(clippy::too_many_arguments)]
fn draw_scene(
    canvas: &HtmlCanvasElement,
    camera: Camera,
    seats: &[Option<SeatContent>],
    selected: Option<&str>,
    width: f64,
    height: f64,
    visuals: &VisualBook,
    now: f64,
) {
    let Some(ctx) = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
    else {
        return;
    };

    // 高清屏适配：物理像素 = CSS 像素 * devicePixelRatio。
    // 每次按 dpr 重置变换（而不是 scale 叠加），重绘多少次都不会累积。
    let dpr = web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0);
    let pixel_width = (width * dpr).round() as u32;
    let pixel_height = (height * dpr).round() as u32;
    if canvas.width() != pixel_width || canvas.height() != pixel_height {
        canvas.set_width(pixel_width);
        canvas.set_height(pixel_height);
    }
    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    ctx.clear_rect(0.0, 0.0, width, height);

    let lod = Lod::for_scale(camera.scale);

    // 1. 空席（底层暗置占位）
    for (seat, slot) in seats.iter().enumerate() {
        if slot.is_some() {
            continue;
        }
        let (wx, wy) = seat_world_pos(seat, width, height);
        let (sx, sy) = camera.to_screen(wx, wy);
        let radius = EMPTY_SEAT_RADIUS * camera.scale;
        if offscreen(sx, sy, radius, width, height) {
            continue;
        }
        ctx.set_fill_style_str(EMPTY_SEAT_FILL);
        ctx.begin_path();
        let _ = ctx.arc(sx, sy, radius, 0.0, TAU);
        ctx.fill();
        ctx.set_stroke_style_str(EMPTY_SEAT_STROKE);
        ctx.set_line_width(1.0);
        ctx.stroke();
    }

    // 2. 已落座实体
    for (seat, slot) in seats.iter().enumerate() {
        let Some(content) = slot else {
            continue;
        };
        let (wx, wy) = seat_world_pos(seat, width, height);
        let (sx, sy) = camera.to_screen(wx, wy);
        let radius = content.radius * camera.scale;
        if offscreen(sx, sy, radius + 40.0, width, height) {
            continue;
        }

        // 过渡中的呈现值；无记录时直接用目标值（首帧）
        let visual = visuals.entries.get(&content.id);
        let color = visual
            .map(|v| v.color_at(now).to_css())
            .unwrap_or_else(|| content.color.clone());
        let highlight = visual
            .map(|v| v.highlight_at(now))
            .unwrap_or_else(|| if content.highlighted { 1.0 } else { 0.0 });

        // 等我出手：外圈柔光（静态，不呼吸 —— 无新事件时不制造「还在忙」的错觉）
        if highlight > 0.01 {
            ctx.set_global_alpha(0.22 * highlight);
            ctx.set_fill_style_str(&color);
            ctx.begin_path();
            let _ = ctx.arc(sx, sy, radius * 1.55, 0.0, TAU);
            ctx.fill();
            ctx.set_global_alpha(1.0);
        }

        // 状态环（LOD 最低层也保留：远看靠颜色 + 环判断状态）
        ctx.set_stroke_style_str(&color);
        ctx.set_global_alpha(0.55);
        ctx.set_line_width(1.0);
        ctx.begin_path();
        let _ = ctx.arc(sx, sy, radius + 3.0, 0.0, TAU);
        ctx.stroke();
        ctx.set_global_alpha(1.0);

        // 光点
        ctx.set_fill_style_str(&color);
        ctx.begin_path();
        let _ = ctx.arc(sx, sy, radius, 0.0, TAU);
        ctx.fill();

        // 选中环
        if selected == Some(content.id.as_str()) {
            ctx.set_stroke_style_str(SELECTION_RING_COLOR);
            ctx.set_line_width(2.0);
            ctx.begin_path();
            let _ = ctx.arc(sx, sy, radius + 6.0, 0.0, TAU);
            ctx.stroke();
        }

        // 名称（LOD ≥ Name）
        if matches!(lod, Lod::Name | Lod::Detail) {
            ctx.set_font("600 13px ui-sans-serif, system-ui, sans-serif");
            ctx.set_text_align("center");
            ctx.set_text_baseline("top");
            ctx.set_fill_style_str(TEXT_COLOR);
            let _ = ctx.fill_text(&content.label, sx, sy + radius + 10.0);
        }

        // 状态标签（LOD = Detail）
        if lod == Lod::Detail
            && let Some(detail) = &content.detail
        {
            ctx.set_font("11px ui-sans-serif, system-ui, sans-serif");
            ctx.set_text_align("center");
            ctx.set_text_baseline("top");
            ctx.set_fill_style_str(DETAIL_TEXT_COLOR);
            let _ = ctx.fill_text(detail, sx, sy + radius + 28.0);
        }
    }
}

/// 视口裁剪：完全落在画布外的席位不必绘制（聚焦时多数席位在屏幕外）
fn offscreen(sx: f64, sy: f64, radius: f64, width: f64, height: f64) -> bool {
    sx + radius < 0.0 || sx - radius > width || sy + radius < 0.0 || sy - radius > height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(id: &str) -> SeatContent {
        SeatContent {
            id: id.to_string(),
            label: id.to_string(),
            color: "#f59e0b".to_string(),
            radius: 20.0,
            highlighted: false,
            detail: None,
        }
    }

    /// 席位围绕画布中心对称，且全部落在画布内
    #[test]
    fn seats_are_centered_and_inside_canvas() {
        let (width, height) = (1200.0, 800.0);
        let (x0, _) = seat_world_pos(0, width, height);
        let (x2, _) = seat_world_pos(2, width, height);
        let (_, y0) = seat_world_pos(0, width, height);
        let (_, y6) = seat_world_pos(6, width, height);
        assert!(((x0 + x2) / 2.0 - width / 2.0).abs() < 1e-6);
        assert!(((y0 + y6) / 2.0 - height / 2.0).abs() < 1e-6);
        for seat in 0..SEAT_COUNT {
            let (x, y) = seat_world_pos(seat, width, height);
            assert!(x > 0.0 && x < width, "席位 {seat} 横向越界");
            assert!(y > 0.0 && y < height, "席位 {seat} 纵向越界");
        }
    }

    /// 8 席坐标互不重叠
    #[test]
    fn all_seats_are_distinct() {
        let (width, height) = (1200.0, 800.0);
        let mut seen = std::collections::HashSet::new();
        for seat in 0..SEAT_COUNT {
            let (x, y) = seat_world_pos(seat, width, height);
            assert!(
                seen.insert((x as i64, y as i64)),
                "席位 {seat} 与其他席位重叠"
            );
        }
    }

    /// LOD 分层按缩放比切换
    #[test]
    fn lod_switches_by_scale() {
        assert_eq!(Lod::for_scale(0.5), Lod::Dot);
        assert_eq!(Lod::for_scale(0.8), Lod::Name);
        assert_eq!(Lod::for_scale(1.3), Lod::Name);
        assert_eq!(Lod::for_scale(1.4), Lod::Detail);
        assert_eq!(Lod::for_scale(2.0), Lod::Detail);
    }

    /// 未选中回到全景；选中后目标镜头把该席放在画面中心
    #[test]
    fn target_camera_frames_selected_seat() {
        let (width, height) = (1200.0, 800.0);
        let mut seats: Vec<Option<SeatContent>> = vec![None; SEAT_COUNT];
        seats[3] = Some(content("a"));

        assert_eq!(
            target_camera(None, &seats, width, height),
            Some(Camera::panorama())
        );

        let camera = target_camera(Some("a"), &seats, width, height).expect("应命中已落座实体");
        let (wx, wy) = seat_world_pos(3, width, height);
        let (sx, sy) = camera.to_screen(wx, wy);
        assert!((sx - width / 2.0).abs() < 1e-6);
        assert!((sy - height * FOCUS_CENTER_Y_RATIO).abs() < 1e-6);
        assert!((camera.scale - FOCUS_SCALE).abs() < 1e-6);

        // 未落座的 id 没有目标镜头（保持当前机位）
        assert_eq!(target_camera(Some("missing"), &seats, width, height), None);
    }

    /// 命中检测：点中光点返回实体 id，点空白返回 None
    #[test]
    fn hit_test_picks_seat_and_ignores_background() {
        let (width, height) = (1200.0, 800.0);
        let mut seats: Vec<Option<SeatContent>> = vec![None; SEAT_COUNT];
        seats[4] = Some(content("b"));
        let camera = Camera::panorama();
        let (wx, wy) = seat_world_pos(4, width, height);

        assert_eq!(
            hit_test(&seats, camera, width, height, wx + 2.0, wy - 2.0),
            Some("b".to_string())
        );
        assert_eq!(hit_test(&seats, camera, width, height, 10.0, 10.0), None);
    }

    /// 颜色解析与线性插值
    #[test]
    fn rgb_parses_hex_and_interpolates() {
        let black = Rgb::parse("#000000").expect("合法色值");
        let white = Rgb::parse("#ffffff").expect("合法色值");
        assert_eq!(
            Rgb::lerp(black, white, 0.5),
            Rgb {
                r: 127.5,
                g: 127.5,
                b: 127.5
            }
        );
        assert_eq!(Rgb::parse("not-a-color"), None);
    }

    /// 首帧静止；类别变化开启过渡，结束后收敛且不再排帧
    #[test]
    fn visual_book_transitions_color_and_settles() {
        let now = 1_000.0;
        let mut book = VisualBook::default();
        let mut seats: Vec<Option<SeatContent>> = vec![None; SEAT_COUNT];
        let mut seat = content("a");
        seat.color = "#000000".to_string();
        seats[0] = Some(seat);

        // 首帧直接落到目标色，不产生动画帧
        assert!(!book.sync(&seats, now));
        assert_eq!(
            book.entries.get("a").expect("已建立状态").color_at(now),
            Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0
            }
        );

        // 类别变化 → 过渡进行中，中点约为两色平均
        seats[0].as_mut().expect("已落座").color = "#ffffff".to_string();
        let changed_at = now + 10.0;
        assert!(book.sync(&seats, changed_at));
        let mid = book
            .entries
            .get("a")
            .expect("已建立状态")
            .color_at(changed_at + COLOR_TRANSITION_MS / 2.0);
        assert!((mid.r - 127.5).abs() < 1.0);

        // 过渡结束：无活动帧（不装忙）
        assert!(!book.has_active(changed_at + COLOR_TRANSITION_MS + 1.0));
    }

    /// 关注度变化走淡入淡出，而不是瞬间出现柔光
    #[test]
    fn visual_book_fades_highlight() {
        let now = 2_000.0;
        let mut book = VisualBook::default();
        let mut seats: Vec<Option<SeatContent>> = vec![None; SEAT_COUNT];
        seats[0] = Some(content("a"));
        assert!(!book.sync(&seats, now));

        seats[0].as_mut().expect("已落座").highlighted = true;
        let changed_at = now + 10.0;
        assert!(book.sync(&seats, changed_at));
        let half = book
            .entries
            .get("a")
            .expect("已建立状态")
            .highlight_at(changed_at + HIGHLIGHT_TRANSITION_MS / 2.0);
        assert!((half - 0.5).abs() < 0.01);
        assert!(!book.has_active(changed_at + HIGHLIGHT_TRANSITION_MS + 1.0));
    }
}
