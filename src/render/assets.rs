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
        let root = crate::paths::data_directory(plugin_root)
            .join("assets")
            .join("realm");
        let font = font_candidates(plugin_root).into_iter().find_map(load_font);
        Self { root, font }
    }

    pub fn font(&self) -> Option<&FontArc> {
        self.font.as_ref()
    }

    /// 技能图标（按技能名）；缺失时返回 None，由调用方降级绘制。
    pub fn skill_icon(&self, skill_name: &str) -> Option<DynamicImage> {
        self.skill_asset("skill_icons", skill_name)
    }

    /// 技能特效图（按技能名）；缺失时返回 None，由调用方降级绘制。
    pub fn skill_effect(&self, skill_name: &str) -> Option<DynamicImage> {
        self.skill_asset("skill_effects", skill_name)
    }

    fn skill_asset(&self, kind: &str, skill_name: &str) -> Option<DynamicImage> {
        if skill_name.is_empty()
            || skill_name.contains('/')
            || skill_name.contains('\\')
            || skill_name.contains("..")
            || skill_name.chars().any(char::is_control)
        {
            return None;
        }
        image::open(self.root.join(kind).join(format!("{skill_name}.png"))).ok()
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

    pub fn portrait_by_id(&self, character_id: &str) -> Option<DynamicImage> {
        if !portrait_id_is_safe(character_id) {
            return None;
        }
        image::open(
            self.root
                .join("portraits")
                .join(format!("{character_id}.png")),
        )
        .ok()
    }

    /// 可选角色形象列表（`portraits/` 下全部 PNG 的文件名主干）。
    pub fn portrait_ids(&self) -> Vec<String> {
        png_files(&self.root.join("portraits"))
            .iter()
            .filter_map(|path| path.file_stem()?.to_str().map(str::to_owned))
            .collect()
    }

    /// 装备图标：按物品定义确定性挑选，同一定义始终得到同一张图。
    pub fn equipment_icon(&self, definition_id: &str) -> Option<DynamicImage> {
        let directory = self.root.join("equipment");
        let images = png_files(&directory);
        if images.is_empty() {
            return None;
        }
        let digest = Sha256::digest(definition_id.as_bytes());
        let index = u64::from_be_bytes(digest[0..8].try_into().ok()?) as usize % images.len();
        image::open(&images[index]).ok()
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
    sync_directory(parent)?;
    if backup.exists() {
        fs::remove_file(backup)?;
        sync_directory(parent)?;
    }
    Ok(())
}

pub fn recover_asset_tree(plugin_root: &Path) -> io::Result<()> {
    let root = crate::paths::data_directory(plugin_root).join("assets");
    if !root.exists() {
        return Ok(());
    }
    recover_directory(&root)
}

pub fn recover_atomic_write(path: &Path) -> io::Result<()> {
    recover_interrupted_replace(
        path,
        &sibling_with_suffix(path, ".new"),
        &sibling_with_suffix(path, ".bak"),
    )
}

fn font_candidates(plugin_root: &Path) -> Vec<PathBuf> {
    [
        crate::paths::data_directory(plugin_root)
            .join("assets")
            .join("fonts")
            .join("font.ttf"),
        // 衬线字形与「洛界典籍」的黑金卡面更契合，故宋体优先于黑体。
        PathBuf::from(r"C:\Windows\Fonts\simsun.ttc"),
        PathBuf::from(r"C:\Windows\Fonts\msyh.ttc"),
        PathBuf::from(r"C:\Windows\Fonts\simhei.ttf"),
    ]
    .into()
}

fn load_font(path: PathBuf) -> Option<FontArc> {
    fs::read(path)
        .ok()
        .and_then(|bytes| FontArc::try_from_vec(bytes).ok())
}

/// 角色 ID 只允许出现在单一路径段中，杜绝路径穿越。
pub(crate) fn portrait_id_is_safe(character_id: &str) -> bool {
    !character_id.is_empty()
        && !character_id.contains('/')
        && !character_id.contains('\\')
        && !character_id.contains("..")
        && !character_id.chars().any(char::is_control)
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

fn recover_directory(directory: &Path) -> io::Result<()> {
    fs::read_dir(directory)?.try_for_each(|entry| {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if recovery_metadata_is_link(&metadata) {
            return Ok(());
        }
        if metadata.is_dir() {
            return recover_directory(&path);
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Ok(());
        };
        let original_name = name
            .strip_suffix(".new")
            .or_else(|| name.strip_suffix(".bak"));
        let Some(original_name) = original_name else {
            return Ok(());
        };
        let original = path.with_file_name(original_name);
        recover_interrupted_replace(
            &original,
            &sibling_with_suffix(&original, ".new"),
            &sibling_with_suffix(&original, ".bak"),
        )
    })
}

#[cfg(windows)]
fn recovery_metadata_is_link(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn recovery_metadata_is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> io::Result<()> {
    File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> io::Result<()> {
    Ok(())
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    path.with_file_name(format!("{name}{suffix}"))
}
