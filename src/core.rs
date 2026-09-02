use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub user_id: String,
    pub display_name: String,
    #[serde(default)]
    pub character_id: String,
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
            character_id: "default".into(),
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

pub fn stable_seed(day: &str, scope: &str, identifier: &str, salt: &str) -> u64 {
    let digest = Sha256::digest(format!("{day}|{scope}|{identifier}|{salt}").as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}
