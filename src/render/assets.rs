use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use ab_glyph::FontArc;
use image::DynamicImage;
use sha2::{Digest, Sha256};

pub struct RealmAssets {
    root: PathBuf,
    font: Option<FontArc>,
}

impl RealmAssets {
    pub fn discover(plugin_root: &Path) -> Self {
        let root = plugin_root.join("assets").join("realm");
        let font = font_candidates(plugin_root).into_iter().find_map(load_font);
        Self { root, font }
    }

    pub fn font(&self) -> Option<&FontArc> {
        self.font.as_ref()
    }

    pub fn portrait(&self, player_id: &str) -> Option<DynamicImage> {
        let directory = self.root.join("portraits");
        let images = png_files(&directory);
        if images.is_empty() {
            return None;
        }
        let digest = Sha256::digest(player_id.as_bytes());
        let index = u64::from_be_bytes(digest[0..8].try_into().ok()?) as usize % images.len();
        image::open(&images[index]).ok()
    }

    pub fn realm_badge(&self, realm_index: u32) -> Option<DynamicImage> {
        let path = self
            .root
            .join("realm_badges")
            .join(format!("{}.png", realm_index.min(9)));
        image::open(path).ok()
    }

    pub fn skill_icon(&self, skill: &str) -> Option<DynamicImage> {
        let directory = self.root.join("skill_icons");
        named_or_stable_image(&directory, skill)
    }

    pub fn skill_effect(&self, skill: &str) -> Option<DynamicImage> {
        let directory = self.root.join("skill_effects");
        named_or_stable_image(&directory, skill)
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = sibling_with_suffix(path, ".new");
    let backup = sibling_with_suffix(path, ".bak");

    recover_interrupted_replace(path, &temporary, &backup)?;
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;

    if path.exists() {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(error);
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn font_candidates(plugin_root: &Path) -> Vec<PathBuf> {
    [
        plugin_root.join("assets").join("fonts").join("font.ttf"),
        PathBuf::from(r"C:\Windows\Fonts\msyh.ttc"),
        PathBuf::from(r"C:\Windows\Fonts\simhei.ttf"),
        PathBuf::from(r"C:\Windows\Fonts\simsun.ttc"),
    ]
    .into()
}

fn load_font(path: PathBuf) -> Option<FontArc> {
    fs::read(path)
        .ok()
        .and_then(|bytes| FontArc::try_from_vec(bytes).ok())
}

fn png_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        })
        .collect::<Vec<_>>();
    files.sort_unstable();
    files
}

fn named_or_stable_image(directory: &Path, name: &str) -> Option<DynamicImage> {
    let exact = directory.join(format!("{name}.png"));
    if let Ok(image) = image::open(exact) {
        return Some(image);
    }

    let images = png_files(directory);
    if images.is_empty() {
        return None;
    }
    let digest = Sha256::digest(name.as_bytes());
    let index = u64::from_be_bytes(digest[0..8].try_into().ok()?) as usize % images.len();
    image::open(&images[index]).ok()
}

fn recover_interrupted_replace(path: &Path, temporary: &Path, backup: &Path) -> io::Result<()> {
    if temporary.exists() {
        fs::remove_file(temporary)?;
    }
    if backup.exists() {
        if path.exists() {
            fs::remove_file(backup)?;
        } else {
            fs::rename(backup, path)?;
        }
    }
    Ok(())
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    path.with_file_name(format!("{name}{suffix}"))
}
