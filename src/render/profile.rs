use std::{io, io::Cursor, path::Path};

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba, imageops};

use super::{ProfileRenderData, assets, card};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;

pub fn render(root: &Path, data: &ProfileRenderData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = ImageBuffer::from_pixel(WIDTH, HEIGHT, Rgba([244, 242, 236, 255]));
    card::fill(&mut image, 0, 0, WIDTH, 58, Rgba([36, 45, 40, 255]));
    card::fill(
        &mut image,
        0,
        58,
        16,
        HEIGHT - 58,
        card::system_color(data.system_id),
    );
    card::fill(&mut image, 38, 82, 364, 424, Rgba([225, 224, 216, 255]));

    if let Some(portrait) = assets
        .portrait_by_id(&data.player.character_id)
        .or_else(|| assets.portrait(&data.player.user_id))
    {
        let portrait = portrait.resize_exact(340, 408, imageops::FilterType::Lanczos3);
        imageops::overlay(&mut image, &portrait, 50, 90);
    } else {
        card::fill(&mut image, 50, 90, 340, 408, Rgba([104, 112, 105, 255]));
    }
    if let Some(badge) = assets.realm_badge(data.realm_index) {
        let badge = badge.resize_exact(132, 60, imageops::FilterType::Lanczos3);
        imageops::overlay(&mut image, &badge, 774, 78);
    }

    card::fill(&mut image, 438, 154, 468, 2, Rgba([195, 190, 178, 255]));
    card::stat_bar(
        &mut image,
        438,
        254,
        data.player.base_hp.max(0) as f64 / 5000.0,
        Rgba([36, 139, 91, 255]),
    );
    card::stat_bar(
        &mut image,
        438,
        316,
        data.player.base_attack.max(0) as f64 / 1200.0,
        Rgba([185, 62, 52, 255]),
    );
    card::stat_bar(
        &mut image,
        438,
        378,
        data.player.base_defense.max(0) as f64 / 1000.0,
        Rgba([50, 102, 151, 255]),
    );
    card::stat_bar(
        &mut image,
        438,
        440,
        data.power / 10000.0,
        Rgba([198, 151, 42, 255]),
    );

    if let Some(font) = assets.font() {
        card::label(
            &mut image,
            font,
            30.0,
            34,
            10,
            Rgba([247, 246, 241, 255]),
            "LUO REALM / 角色档案",
        );
        card::label(
            &mut image,
            font,
            38.0,
            438,
            76,
            Rgba([36, 45, 40, 255]),
            &data.player.display_name,
        );
        card::label(
            &mut image,
            font,
            22.0,
            438,
            122,
            card::system_color(data.system_id),
            &format!("{} · {}", data.system_name, data.realm_name),
        );
        card::label(
            &mut image,
            font,
            20.0,
            438,
            194,
            Rgba([83, 82, 76, 255]),
            &format!("等级 {}    修为进度 {}", data.player.level, data.progress),
        );
        card::label(
            &mut image,
            font,
            20.0,
            438,
            226,
            Rgba([83, 82, 76, 255]),
            &format!(
                "金币 {}    刻印 {}    胜负 {}/{}",
                data.player.coins, data.player.marks, data.player.wins, data.player.losses
            ),
        );
        card::label(
            &mut image,
            font,
            18.0,
            438,
            267,
            Rgba([45, 48, 45, 255]),
            &format!("生命 {}", data.player.base_hp),
        );
        card::label(
            &mut image,
            font,
            18.0,
            438,
            329,
            Rgba([45, 48, 45, 255]),
            &format!("攻击 {}", data.player.base_attack),
        );
        card::label(
            &mut image,
            font,
            18.0,
            438,
            391,
            Rgba([45, 48, 45, 255]),
            &format!("防御 {}", data.player.base_defense),
        );
        card::label(
            &mut image,
            font,
            18.0,
            438,
            453,
            Rgba([45, 48, 45, 255]),
            &format!("综合战力 {:.0}", data.power),
        );
    }

    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(io::Error::other)?;
    assets::atomic_write(path, bytes.get_ref())
}
