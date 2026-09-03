pub const EVENTS: [&str; 5] = ["平静的一日", "资源潮汐", "魔物入侵", "竞技庆典", "奥术风暴"];

pub fn daily_event(seed: u64) -> &'static str {
    EVENTS[(seed as usize) % EVENTS.len()]
}

/// 机缘事件的一句话说明，供卡片与文字回复共用。
pub fn description(event: &str) -> &'static str {
    match event {
        "平静的一日" => "灵气平稳，宜静修调息，稳健积累。",
        "资源潮汐" => "灵材涌动，坊市兴盛，交易采集皆有裨益。",
        "魔物入侵" => "妖兽出没界缘，历练除魔可得厚赏。",
        "竞技庆典" => "群英汇聚演武场，切磋论道正当其时。",
        "奥术风暴" => "奥能激荡天地，危机与造化并存。",
        _ => "天地异动，机缘难测。",
    }
}

#[cfg(test)]
mod tests {
    use super::{EVENTS, daily_event, description};

    #[test]
    fn every_event_has_a_description() {
        for event in EVENTS {
            assert!(!description(event).is_empty(), "{event} 缺少说明文案");
        }
    }

    #[test]
    fn selection_is_deterministic() {
        assert_eq!(daily_event(0), EVENTS[0]);
        assert_eq!(daily_event(7), EVENTS[2]);
    }
}
