use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub user_id: String,
    pub display_name: String,
    pub level: u32,
    pub experience: u64,
    pub coins: u64,
    pub marks: u64,
    pub base_hp: i64,
    pub base_attack: i64,
    pub base_defense: i64,
    pub critical_rate: f64,
    pub critical_multiplier: f64,
    pub speed: i64,
    #[serde(default)]
    pub wins: u64,
    #[serde(default)]
    pub losses: u64,
}

impl Player {
    pub fn new(user_id: u64) -> Self {
        Self {
            user_id: user_id.to_string(),
            display_name: "LR·旅者".into(),
            level: 1,
            experience: 0,
            coins: 0,
            marks: 0,
            base_hp: 1000,
            base_attack: 100,
            base_defense: 50,
            critical_rate: 5.0,
            critical_multiplier: 1.5,
            speed: 10,
            wins: 0,
            losses: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CombatFrame {
    pub round: u32,
    pub attacker_id: String,
    pub defender_id: String,
    pub skill: String,
    pub damage: i64,
    pub critical: bool,
    pub left_hp: i64,
    pub right_hp: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CombatResult {
    pub winner_id: String,
    pub rounds: u32,
    pub left_hp: i64,
    pub right_hp: i64,
    pub seed: u64,
    pub frames: Vec<CombatFrame>,
}

pub struct Combatant<'a> {
    pub player: &'a Player,
    pub skills: &'a [&'a str],
}

fn next_random(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state as f64) / (u64::MAX as f64)
}

pub fn stable_seed(day: &str, scope: &str, identifier: &str, salt: &str) -> u64 {
    let digest = Sha256::digest(format!("{day}|{scope}|{identifier}|{salt}").as_bytes());
    u64::from_be_bytes(digest[0..8].try_into().expect("SHA-256 长度固定"))
}

pub fn simulate_combat(
    left: Combatant<'_>,
    right: Combatant<'_>,
    seed: u64,
    max_rounds: u32,
) -> CombatResult {
    let mut state = seed.max(1);
    let players = [left.player, right.player];
    let skills = [left.skills, right.skills];
    let mut hp = [left.player.base_hp, right.player.base_hp];
    let mut frames = Vec::new();
    let order = if (left.player.speed, &left.player.user_id)
        >= (right.player.speed, &right.player.user_id)
    {
        [0, 1]
    } else {
        [1, 0]
    };
    let max_rounds = max_rounds.max(1);
    for round in 1..=max_rounds {
        for attacker in order {
            let defender = 1 - attacker;
            if hp[attacker] <= 0 || hp[defender] <= 0 {
                break;
            }
            let attacker_player = players[attacker];
            let defender_player = players[defender];
            let variance = 0.9 + next_random(&mut state) * 0.2;
            let mut damage = ((attacker_player.base_attack - defender_player.base_defense).max(1)
                as f64)
                * variance;
            let critical =
                next_random(&mut state) * 100.0 < attacker_player.critical_rate.clamp(0.0, 75.0);
            if critical {
                damage *= attacker_player.critical_multiplier;
            }
            let damage = (damage as i64).max(1).min(hp[defender]);
            hp[defender] -= damage;
            frames.push(CombatFrame {
                round,
                attacker_id: attacker_player.user_id.clone(),
                defender_id: defender_player.user_id.clone(),
                skill: selected_skill(skills[attacker], &mut state).into(),
                damage,
                critical,
                left_hp: hp[0].max(0),
                right_hp: hp[1].max(0),
            });
        }
        if hp[0] <= 0 || hp[1] <= 0 {
            return CombatResult {
                winner_id: if hp[0] > 0 {
                    left.player.user_id.clone()
                } else {
                    right.player.user_id.clone()
                },
                rounds: round,
                left_hp: hp[0].max(0),
                right_hp: hp[1].max(0),
                seed,
                frames,
            };
        }
    }
    CombatResult {
        winner_id: if hp[0] >= hp[1] {
            left.player.user_id.clone()
        } else {
            right.player.user_id.clone()
        },
        rounds: max_rounds,
        left_hp: hp[0].max(0),
        right_hp: hp[1].max(0),
        seed,
        frames,
    }
}

fn selected_skill<'a>(skills: &'a [&'a str], state: &mut u64) -> &'a str {
    if skills.is_empty() {
        return "普通攻击";
    }
    let index = (next_random(state) * skills.len() as f64) as usize;
    skills[index.min(skills.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seed_is_stable() {
        assert_eq!(
            stable_seed("2026-08-30", "group", "1", "luo-realm-v1"),
            stable_seed("2026-08-30", "group", "1", "luo-realm-v1")
        );
    }
    #[test]
    fn stronger_player_wins() {
        let mut stronger_player = Player::new(1);
        let regular_player = Player::new(2);
        stronger_player.base_attack = 500;

        let result = simulate_combat(
            Combatant {
                player: &stronger_player,
                skills: &[],
            },
            Combatant {
                player: &regular_player,
                skills: &[],
            },
            42,
            30,
        );

        assert_eq!(result.winner_id, "1");
    }
}
