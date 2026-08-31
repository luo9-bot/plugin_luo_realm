use sha2::{Digest, Sha256};

pub fn destiny_seed(day: &str, user_id: &str, version_salt: &str) -> u64 {
    let source = format!("{day}|{user_id}|{version_salt}");
    let digest = Sha256::digest(source.as_bytes());

    u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 摘要始终包含至少 8 字节"),
    )
}

pub fn destiny_multiplier(seed: u64) -> f64 {
    0.9 + (seed % 31) as f64 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_identity_has_stable_destiny() {
        assert_eq!(
            destiny_seed("2026-08-30", "10001", "luo-realm-v1"),
            destiny_seed("2026-08-30", "10001", "luo-realm-v1"),
        );
    }
}
