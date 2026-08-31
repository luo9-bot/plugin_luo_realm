pub const EVENTS: [&str; 5] = ["平静的一日", "资源潮汐", "魔物入侵", "竞技庆典", "奥术风暴"];
pub fn daily_event(seed: u64) -> &'static str {
    EVENTS[(seed as usize) % EVENTS.len()]
}
