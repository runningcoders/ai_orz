//! 固定席位表与落座分配
//!
//! 席位是**表现层**概念：后端只给排序后的实体，前端决定谁坐哪儿。
//! 三条不变式：
//! 1. 已落座的实体永不换位（数据重排不导致坐标跳动）；
//! 2. 新实体取编号最小的空席位；
//! 3. 离开关注集的实体释放席位，供后来者复用。
//!
//! 映射持久化在 localStorage：刷新后同一 Agent 仍坐同一席位。

use std::collections::{HashMap, HashSet};

/// 席位总数（与后端 `seat_limit` 上限一致）
pub const SEAT_COUNT: usize = 8;

/// 席位坐标（单位化环：左上 → 右上 → 右下 → 左下）
///
/// `(0, -1)` 这类值在渲染时按画布尺寸缩放，不写死像素。
pub const SEAT_POSITIONS: [(f64, f64); SEAT_COUNT] = [
    (-1.0, -1.0),
    (0.0, -1.0),
    (1.0, -1.0),
    (1.0, 0.0),
    (1.0, 1.0),
    (0.0, 1.0),
    (-1.0, 1.0),
    (-1.0, 0.0),
];

/// localStorage 键（带版本号，字段变化时换版本而不是做迁移）
const STORAGE_KEY: &str = "ai_orz.workspace.seats.v1";

/// Agent id → 席位编号
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeatMap {
    assignments: HashMap<String, usize>,
}

impl SeatMap {
    /// 从 localStorage 读取；任何异常都退化为空表（席位可重算，不是关键数据）
    pub fn load() -> Self {
        let Some(raw) = storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten()) else {
            return Self::default();
        };
        let parsed: HashMap<String, usize> = serde_json::from_str(&raw).unwrap_or_default();
        Self::sanitized(parsed)
    }

    /// 清洗持久化数据：越界席位丢弃；重复席位只保留先出现的一条
    ///
    /// 正常流程不会产生重复，但 localStorage 可能被旧版本或外部工具写坏；
    /// 不清理会导致两个 Agent 叠在同一坐标上。
    fn sanitized(parsed: HashMap<String, usize>) -> Self {
        let mut assignments = HashMap::new();
        let mut used = [false; SEAT_COUNT];
        for (id, seat) in parsed {
            if seat < SEAT_COUNT && !used[seat] {
                used[seat] = true;
                assignments.insert(id, seat);
            }
        }
        Self { assignments }
    }

    /// 写回 localStorage（失败静默：只影响下次刷新的落座）
    pub fn save(&self) {
        let Some(storage) = storage() else {
            return;
        };
        if let Ok(raw) = serde_json::to_string(&self.assignments) {
            let _ = storage.set_item(STORAGE_KEY, &raw);
        }
    }

    /// 按当前关注集调整落座；返回是否发生变化（调用方据此决定是否回写）
    pub fn assign(&mut self, ids: &[String]) -> bool {
        let live: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let before = self.assignments.len();
        self.assignments.retain(|id, _| live.contains(id.as_str()));
        let mut changed = self.assignments.len() != before;

        let mut used = [false; SEAT_COUNT];
        for seat in self.assignments.values() {
            if let Some(slot) = used.get_mut(*seat) {
                *slot = true;
            }
        }
        for id in ids {
            if self.assignments.contains_key(id) {
                continue;
            }
            let Some(seat) = used.iter().position(|taken| !taken) else {
                // 席位已满：超出关注集上限的实体不落座（后端已按 seat_limit 截断）
                break;
            };
            self.assignments.insert(id.clone(), seat);
            used[seat] = true;
            changed = true;
        }
        changed
    }

    /// 查询实体所在席位
    pub fn seat_of(&self, id: &str) -> Option<usize> {
        self.assignments.get(id).copied()
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn existing_seats_never_move_when_order_changes() {
        let mut map = SeatMap::default();
        map.assign(&ids(&["a", "b", "c"]));
        assert_eq!(map.seat_of("a"), Some(0));
        assert_eq!(map.seat_of("b"), Some(1));
        // 关注集排序反转（attention 变化）不改变已落座实体
        map.assign(&ids(&["c", "b", "a"]));
        assert_eq!(map.seat_of("a"), Some(0));
        assert_eq!(map.seat_of("c"), Some(2));
    }

    #[test]
    fn new_entity_takes_lowest_free_seat() {
        let mut map = SeatMap::default();
        map.assign(&ids(&["a", "b"]));
        map.assign(&ids(&["a", "b", "c"]));
        assert_eq!(map.seat_of("c"), Some(2));
    }

    #[test]
    fn departed_entity_frees_seat_for_newcomer() {
        let mut map = SeatMap::default();
        map.assign(&ids(&["a", "b"]));
        map.assign(&ids(&["a"]));
        assert_eq!(map.seat_of("b"), None);
        map.assign(&ids(&["a", "c"]));
        assert_eq!(map.seat_of("c"), Some(1));
    }

    #[test]
    fn overflow_entities_stay_seatless() {
        let all: Vec<String> = (0..SEAT_COUNT + 2).map(|i| format!("a{i}")).collect();
        let mut map = SeatMap::default();
        map.assign(&all);
        assert!(map.seat_of(&all[SEAT_COUNT - 1]).is_some());
        assert_eq!(map.seat_of(&all[SEAT_COUNT]), None);
    }

    #[test]
    fn assign_reports_change_only_when_membership_changes() {
        let mut map = SeatMap::default();
        assert!(map.assign(&ids(&["a"])));
        assert!(!map.assign(&ids(&["a"])));
    }

    /// 损坏的持久化数据：越界丢弃、重复席位只留一个，避免叠坐标
    #[test]
    fn sanitized_drops_out_of_range_and_duplicate_seats() {
        let parsed = HashMap::from([
            ("a".to_string(), 2),
            ("b".to_string(), 2),
            ("c".to_string(), SEAT_COUNT),
            ("d".to_string(), 5),
        ]);
        let map = SeatMap::sanitized(parsed);
        assert_eq!(map.assignments.len(), 2);
        assert_eq!(map.seat_of("d"), Some(5));
        assert_eq!(
            map.assignments.values().filter(|seat| **seat == 2).count(),
            1
        );
    }
}
