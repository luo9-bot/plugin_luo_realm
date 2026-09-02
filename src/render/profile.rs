use std::{io, io::Cursor, path::Path};

use ab_glyph::PxScale;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba, imageops};
use imageproc::{drawing::draw_text_mut, rect::Rect};

use super::{ProfileRenderData, assets};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;

pub fn render(root: &Path, data: &ProfileRenderData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = ImageBuffer::from_pixel(WIDTH, HEIGHT, Rgba([244, 242, 236, 255]));
    fill(&mut image, 0, 0, WIDTH, 58, Rgba([36, 45, 40, 255]));
    fill(
        &mut image,
        0,
        58,
        16,
        HEIGHT - 58,
        system_color(data.system_id),
    );
    fill(&mut image, 38, 82, 364, 424, Rgba([225, 224, 216, 255]));

    if let Some(portrait) = assets
        .portrait_by_id(&data.player.character_id)
        .or_else(|| assets.portrait(&data.player.user_id))
    {
        let portrait = portrait.resize_exact(340, 408, imageops::FilterType::Lanczos3);
        imageops::overlay(&mut image, &portrait, 50, 90);
    } else {
        fill(&mut image, 50, 90, 340, 408, Rgba([104, 112, 105, 255]));
    }
    if let Some(badge) = assets.realm_badge(data.realm_index) {
        let badge = badge.resize_exact(132, 60, imageops::FilterType::Lanczos3);
        imageops::overlay(&mut image, &badge, 774, 78);
    }

    fill(&mut image, 438, 154, 468, 2, Rgba([195, 190, 178, 255]));
    stat_bar(
        &mut image,
        438,
        254,
        data.player.base_hp.max(0) as f64 / 5000.0,
        Rgba([36, 139, 91, 255]),
    );
    stat_bar(
        &mut image,
        438,
        316,
        data.player.base_attack.max(0) as f64 / 1200.0,
        Rgba([185, 62, 52, 255]),
    );
    stat_bar(
        &mut image,
        438,
        378,
        data.player.base_defense.max(0) as f64 / 1000.0,
        Rgba([50, 102, 151, 255]),
    );
    stat_bar(
        &mut image,
        438,
        440,
        data.power / 10000.0,
        Rgba([198, 151, 42, 255]),
    );

    if let Some(font) = assets.font() {
        label(
            &mut image,
            font,
            30.0,
            34,
            10,
            Rgba([247, 246, 241, 255]),
            "LUO REALM / 角色档案",
        );
        label(
            &mut image,
            font,
            38.0,
            438,
            76,
            Rgba([36, 45, 40, 255]),
            &data.player.display_name,
        );
        label(
            &mut image,
            font,
            22.0,
            438,
            122,
            system_color(data.system_id),
            &format!("{} · {}", data.system_name, data.realm_name),
        );
        label(
            &mut image,
            font,
            20.0,
            438,
            194,
            Rgba([83, 82, 76, 255]),
            &format!("等级 {}    修为进度 {}", data.player.level, data.progress),
        );
        label(
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
        label(
            &mut image,
            font,
            18.0,
            438,
            267,
            Rgba([45, 48, 45, 255]),
            &format!("生命 {}", data.player.base_hp),
        );
        label(
            &mut image,
            font,
            18.0,
            438,
            329,
            Rgba([45, 48, 45, 255]),
            &format!("攻击 {}", data.player.base_attack),
        );
        label(
            &mut image,
            font,
            18.0,
            438,
            391,
            Rgba([45, 48, 45, 255]),
            &format!("防御 {}", data.player.base_defense),
        );
        label(
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

fn stat_bar(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    ratio: f64,
    color: Rgba<u8>,
) {
    fill(image, x, y, 468, 14, Rgba([219, 216, 207, 255]));
    fill(
        image,
        x,
        y,
        (468.0 * ratio.clamp(0.02, 1.0)) as u32,
        14,
        color,
    );
}

fn fill(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: Rgba<u8>,
) {
    if width == 0 || height == 0 {
        return;
    }
    imageproc::drawing::draw_filled_rect_mut(
        image,
        Rect::at(x as i32, y as i32).of_size(width, height),
        color,
    );
}

fn label(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: &ab_glyph::FontArc,
    size: f32,
    x: i32,
    y: i32,
    color: Rgba<u8>,
    text: &str,
) {
    draw_text_mut(image, color, x, y, PxScale::from(size), font, text);
}

fn system_color(system_id: &str) -> Rgba<u8> {
    match system_id {
        "sword" => Rgba([43, 105, 158, 255]),
        "body" => Rgba([169, 64, 47, 255]),
        "mage" => Rgba([77, 86, 154, 255]),
        "soul" => Rgba([103, 69, 132, 255]),
        "qi" => Rgba([35, 129, 114, 255]),
        "blood_demon" => Rgba([137, 35, 42, 255]),
        "formation" => Rgba([51, 111, 72, 255]),
        "alchemy_artifact" => Rgba([151, 108, 27, 255]),
        "summoner" => Rgba([78, 112, 67, 255]),
        "music" => Rgba([152, 70, 106, 255]),
        _ => Rgba([65, 104, 75, 255]),
    }
}
