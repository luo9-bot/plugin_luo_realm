//! 规则注册表的验证。

use super::{builtin_tiers, rarity_by_code, rarity_tiers};

#[test]
fn builtin_tiers_cover_common_qualities() {
    let tiers = builtin_tiers();
    for code in ["legacy", "common", "fine", "rare", "epic", "legendary"] {
        assert!(
            rarity_by_code(&tiers, code).is_some(),
            "{code} 缺少品阶定义"
        );
    }
}

#[test]
fn colors_parse_to_rgb() {
    let tiers = builtin_tiers();
    let epic = rarity_by_code(&tiers, "epic").expect("epic tier");
    assert_eq!(epic.rgb(), (142, 92, 190));
    assert_eq!(
        rarity_by_code(&tiers, "unknown-code").map(|tier: &super::RarityTier| tier.rgb()),
        None
    );
}

#[test]
fn file_override_replaces_builtin() {
    let directory = tempfile::tempdir().expect("tempdir");
    let rules = directory
        .path()
        .join("data")
        .join("luo_realm")
        .join("rules");
    std::fs::create_dir_all(&rules).expect("create rules dir");
    std::fs::write(
        rules.join("rarities.toml"),
        "[[tier]]\ncode = \"mythic\"\ndisplay = \"神话\"\ncolor = \"#ff5577\"\nstars = 6\n",
    )
    .expect("write rules");
    let tiers = rarity_tiers(directory.path());
    assert_eq!(tiers.len(), 1);
    let mythic = rarity_by_code(&tiers, "mythic").expect("mythic tier");
    assert_eq!((mythic.display.as_str(), mythic.stars), ("神话", 6));
}

#[test]
fn missing_or_broken_file_falls_back_to_builtin() {
    let directory = tempfile::tempdir().expect("tempdir");
    assert_eq!(rarity_tiers(directory.path()).len(), builtin_tiers().len());

    let broken = directory
        .path()
        .join("data")
        .join("luo_realm")
        .join("rules");
    std::fs::create_dir_all(&broken).expect("create rules dir");
    std::fs::write(broken.join("rarities.toml"), "不是合法内容").expect("write broken");
    assert_eq!(rarity_tiers(directory.path()).len(), builtin_tiers().len());
}
