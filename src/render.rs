use crate::core::{CombatResult, Player};
use image::{
    Delay, Frame, ImageBuffer, Rgba,
    codecs::gif::{GifEncoder, Repeat},
};
use std::{fs::File, io, path::Path};

fn canvas() -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    ImageBuffer::from_pixel(576, 360, Rgba([30, 24, 32, 255]))
}

fn rectangle(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: Rgba<u8>,
) {
    for py in y..(y + height).min(image.height()) {
        for px in x..(x + width).min(image.width()) {
            image.put_pixel(px, py, color);
        }
    }
}

pub fn profile(player: &Player, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut image = canvas();
    rectangle(&mut image, 0, 0, 576, 72, Rgba([132, 36, 64, 255]));
    let power = (player.base_attack + player.base_defense + i64::from(player.level) * 20)
        .clamp(1, 1000) as u32;
    rectangle(
        &mut image,
        42,
        130,
        power.min(480),
        30,
        Rgba([225, 82, 110, 255]),
    );
    rectangle(
        &mut image,
        42,
        210,
        ((player.coins / 100).min(480)) as u32,
        24,
        Rgba([235, 190, 72, 255]),
    );
    image.save(path).map_err(io::Error::other)
}

pub fn battle(result: &CombatResult, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut encoder = GifEncoder::new(File::create(path)?);
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(io::Error::other)?;
    (0..=result.rounds.min(12)).try_for_each(|index| {
        let mut image = canvas();
        let progress = index as f32 / result.rounds.max(1) as f32;
        let left_won = result.left_hp >= result.right_hp;
        let left = (480.0 * (1.0 - progress * if left_won { 0.35 } else { 0.9 })) as u32;
        let right = (480.0 * (1.0 - progress * if left_won { 0.9 } else { 0.35 })) as u32;
        rectangle(&mut image, 48, 105, left, 34, Rgba([75, 180, 235, 255]));
        rectangle(&mut image, 48, 220, right, 34, Rgba([235, 80, 95, 255]));
        encoder
            .encode_frame(Frame::from_parts(
                image,
                0,
                0,
                Delay::from_numer_denom_ms(450, 1),
            ))
            .map_err(io::Error::other)
    })
}
