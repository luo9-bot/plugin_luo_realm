use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use ab_glyph::FontArc;
use image::{ImageFormat, ImageReader, Limits};
use serde::Serialize;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 5_000;

const REALM_CATEGORIES: &[(&str, &str)] = &[
    ("portraits", "角色立绘"),
    ("realm_badges", "境界徽章"),
    ("skill_icons", "技能图标"),
    ("skill_effects", "技能特效"),
    ("equipment", "装备"),
    ("item_rarities", "品质标识"),
    ("monsters", "妖兽"),
    ("shop_characters", "商店角色"),
    ("enhancement_items", "强化素材"),
    ("true_damage", "真实伤害"),
    ("ui", "界面元素"),
];

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("invalid asset path")]
    InvalidPath,
    #[error("unsupported asset type")]
    UnsupportedType,
    #[error("asset is too large")]
    TooLarge,
    #[error("invalid image data")]
    InvalidImage,
    #[error("asset archive is invalid: {0}")]
    InvalidArchive(String),
    #[error("asset not found")]
    NotFound,
    #[error("asset storage failed: {0}")]
    Storage(#[from] std::io::Error),
}

#[derive(Serialize)]
pub struct AssetCategory {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Serialize)]
pub struct AssetFile {
    pub path: String,
    pub name: String,
    pub category: String,
    pub byte_size: u64,
    pub modified_at: i64,
    pub previewable: bool,
}

#[derive(Serialize)]
pub struct AssetPage {
    pub categories: Vec<AssetCategory>,
    pub items: Vec<AssetFile>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
}

pub struct ImportSummary {
    pub imported: usize,
    pub replaced: usize,
}

pub fn list(
    plugin_root: &Path,
    category: &str,
    search: &str,
    page: usize,
    limit: usize,
) -> Result<AssetPage, AssetError> {
    let assets_root = crate::paths::data_directory(plugin_root).join("assets");
    if !assets_root.exists() {
        return Ok(AssetPage {
            categories: categories(),
            items: Vec::new(),
            total: 0,
            page: 1,
            limit: limit.clamp(1, 100),
        });
    }
    reject_link(&assets_root)?;
    let mut files = collect_files(&assets_root)?
        .into_iter()
        .filter_map(|path| describe_file(&assets_root, path).ok())
        .filter(|file| category.is_empty() || file.category == category)
        .filter(|file| {
            search.is_empty() || file.path.to_lowercase().contains(&search.to_lowercase())
        })
        .collect::<Vec<_>>();
    files.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    let page = page.max(1);
    let limit = limit.clamp(1, 100);
    let total = files.len();
    let items = files
        .into_iter()
        .skip(page.saturating_sub(1).saturating_mul(limit))
        .take(limit)
        .collect();
    Ok(AssetPage {
        categories: categories(),
        items,
        total,
        page,
        limit,
    })
}

pub fn read(plugin_root: &Path, relative: &str) -> Result<(Vec<u8>, &'static str), AssetError> {
    let relative = validate_relative_path(relative)?;
    let path = resolve_asset_path(plugin_root, &relative, false)?;
    let bytes = fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AssetError::NotFound
        } else {
            AssetError::Storage(error)
        }
    })?;
    Ok((bytes, content_type(&relative)))
}

pub fn write(plugin_root: &Path, relative: &str, bytes: &[u8]) -> Result<bool, AssetError> {
    let relative = validate_relative_path(relative)?;
    validate_asset(&relative, bytes)?;
    let path = resolve_asset_path(plugin_root, &relative, true)?;
    let replaced = path.exists();
    crate::render::assets::atomic_write(&path, bytes)?;
    Ok(replaced)
}

pub fn remove(plugin_root: &Path, relative: &str) -> Result<(), AssetError> {
    let relative = validate_relative_path(relative)?;
    let path = resolve_asset_path(plugin_root, &relative, false)?;
    fs::remove_file(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AssetError::NotFound
        } else {
            AssetError::Storage(error)
        }
    })
}

pub fn export(plugin_root: &Path) -> Result<Vec<u8>, AssetError> {
    let assets_root = crate::paths::data_directory(plugin_root).join("assets");
    if assets_root.exists() {
        reject_link(&assets_root)?;
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    collect_files(&assets_root)?
        .into_iter()
        .try_for_each(|path| {
            let relative = path
                .strip_prefix(&assets_root)
                .map_err(|_| AssetError::InvalidPath)?;
            validate_relative_path(&path_text(relative))?;
            writer
                .start_file(path_text(relative), options)
                .map_err(zip_error)?;
            let bytes = fs::read(path)?;
            writer.write_all(&bytes).map_err(AssetError::Storage)
        })?;

    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(zip_error)
}

pub fn import(plugin_root: &Path, bytes: &[u8]) -> Result<ImportSummary, AssetError> {
    recover_bundle_import(plugin_root)?;
    let data_directory = crate::paths::data_directory(plugin_root);
    let assets_root = data_directory.join("assets");
    if assets_root.exists() {
        reject_link(&assets_root)?;
    }
    let staging = data_directory.join(".assets.import");
    let backup = data_directory.join(".assets.backup");
    if assets_root.exists() {
        if let Err(error) = copy_tree(&assets_root, &staging) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    } else {
        fs::create_dir(&staging)?;
    }

    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if archive.len() > MAX_ARCHIVE_FILES {
        let _ = fs::remove_dir_all(&staging);
        return Err(AssetError::TooLarge);
    }

    let mut expanded_size = 0_u64;
    let mut imported = 0_usize;
    let mut replaced = 0_usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        if entry.is_dir() {
            continue;
        }
        expanded_size = expanded_size.saturating_add(entry.size());
        if expanded_size > MAX_ARCHIVE_BYTES || entry.size() > MAX_ASSET_BYTES as u64 {
            let _ = fs::remove_dir_all(&staging);
            return Err(AssetError::TooLarge);
        }
        let relative = validate_relative_path(entry.name())?;
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        validate_asset(&relative, &data)?;
        replaced += usize::from(assets_root.join(&relative).exists());
        if let Err(error) = crate::render::assets::atomic_write(&staging.join(relative), &data) {
            let _ = fs::remove_dir_all(&staging);
            return Err(AssetError::Storage(error));
        }
        imported += 1;
    }
    if imported == 0 {
        let _ = fs::remove_dir_all(&staging);
        return Err(AssetError::InvalidArchive("压缩包中没有可导入素材".into()));
    }

    if let Err(error) = commit_staging(plugin_root, &assets_root, &staging, &backup) {
        let _ = recover_bundle_import(plugin_root);
        return Err(error);
    }
    Ok(ImportSummary { imported, replaced })
}

fn categories() -> Vec<AssetCategory> {
    REALM_CATEGORIES
        .iter()
        .map(|(id, label)| AssetCategory { id, label })
        .chain(std::iter::once(AssetCategory {
            id: "fonts",
            label: "字体",
        }))
        .collect()
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, AssetError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    fs::read_dir(root)?.try_fold(Vec::new(), |mut files, entry| {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata_is_link(&metadata) {
            return Err(AssetError::InvalidPath);
        }
        if metadata.is_dir() {
            files.extend(collect_files(&path)?);
        } else if metadata.is_file() {
            files.push(path);
        }
        Ok(files)
    })
}

pub fn recover_bundle_import(plugin_root: &Path) -> Result<(), AssetError> {
    let data_directory = crate::paths::data_directory(plugin_root);
    let assets = data_directory.join("assets");
    let staging = data_directory.join(".assets.import");
    let backup = data_directory.join(".assets.backup");
    let pending = data_directory.join(".assets.pending");

    if pending.exists() {
        let had_assets = fs::read_to_string(&pending)?.trim() == "existing";
        if backup.exists() {
            if assets.exists() {
                fs::remove_dir_all(&assets)?;
            }
            fs::rename(&backup, &assets)?;
        } else if !had_assets && assets.exists() && !staging.exists() {
            fs::remove_dir_all(&assets)?;
        }
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::remove_file(&pending)?;
        sync_directory(plugin_root)?;
    }

    if staging.exists() {
        if !assets.exists() && backup.exists() {
            fs::rename(&backup, &assets)?;
        }
        fs::remove_dir_all(&staging)?;
    }
    if backup.exists() {
        if assets.exists() {
            fs::remove_dir_all(&backup)?;
        } else {
            fs::rename(&backup, &assets)?;
        }
    }
    Ok(())
}

pub fn finalize_bundle_import(plugin_root: &Path) -> Result<(), AssetError> {
    let data_directory = crate::paths::data_directory(plugin_root);
    let pending = data_directory.join(".assets.pending");
    let backup = data_directory.join(".assets.backup");
    if pending.exists() {
        fs::remove_file(pending)?;
        sync_directory(plugin_root)?;
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
        sync_directory(plugin_root)?;
    }
    Ok(())
}

pub fn rollback_bundle_import(plugin_root: &Path) -> Result<(), AssetError> {
    recover_bundle_import(plugin_root)
}

fn resolve_asset_path(
    plugin_root: &Path,
    relative: &Path,
    create_parent: bool,
) -> Result<PathBuf, AssetError> {
    let root = crate::paths::data_directory(plugin_root).join("assets");
    if !root.exists() {
        if !create_parent {
            return Err(AssetError::NotFound);
        }
        fs::create_dir(&root)?;
    }
    reject_link(&root)?;

    let mut parent = root;
    if let Some(relative_parent) = relative.parent() {
        for component in relative_parent.components() {
            parent.push(component.as_os_str());
            match fs::symlink_metadata(&parent) {
                Ok(metadata) if metadata_is_link(&metadata) || !metadata.is_dir() => {
                    return Err(AssetError::InvalidPath);
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && create_parent => {
                    fs::create_dir(&parent)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(AssetError::NotFound);
                }
                Err(error) => return Err(AssetError::Storage(error)),
            }
        }
    }
    let path = parent.join(relative.file_name().ok_or(AssetError::InvalidPath)?);
    if let Ok(metadata) = fs::symlink_metadata(&path)
        && metadata_is_link(&metadata)
    {
        return Err(AssetError::InvalidPath);
    }
    Ok(path)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), AssetError> {
    fs::create_dir(destination)?;
    fs::read_dir(source)?.try_for_each(|entry| {
        let entry = entry?;
        let source_path = entry.path();
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata_is_link(&metadata) {
            return Err(AssetError::InvalidPath);
        }
        let destination_path = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&source_path, &destination_path)
        } else if metadata.is_file() {
            fs::copy(source_path, destination_path)?;
            Ok(())
        } else {
            Err(AssetError::InvalidPath)
        }
    })
}

fn commit_staging(
    plugin_root: &Path,
    assets: &Path,
    staging: &Path,
    backup: &Path,
) -> Result<(), AssetError> {
    let pending = crate::paths::data_directory(plugin_root).join(".assets.pending");
    crate::render::assets::atomic_write(
        &pending,
        if assets.exists() { b"existing" } else { b"new" },
    )?;
    sync_directory(plugin_root)?;
    if assets.exists() {
        fs::rename(assets, backup)?;
    }
    if let Err(error) = fs::rename(staging, assets) {
        if backup.exists() {
            let _ = fs::rename(backup, assets);
        }
        return Err(AssetError::Storage(error));
    }
    sync_directory(plugin_root)?;
    Ok(())
}

fn reject_link(path: &Path) -> Result<(), AssetError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link(&metadata) {
        Err(AssetError::InvalidPath)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn metadata_is_link(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), AssetError> {
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> Result<(), AssetError> {
    Ok(())
}

fn describe_file(root: &Path, path: PathBuf) -> Result<AssetFile, AssetError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AssetError::InvalidPath)?;
    validate_relative_path(&path_text(relative))?;
    let metadata = fs::metadata(&path)?;
    let parts = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    let category = if parts.first() == Some(&"fonts") {
        "fonts"
    } else {
        parts.get(1).copied().unwrap_or_default()
    }
    .to_owned();
    Ok(AssetFile {
        path: path_text(relative),
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned(),
        category,
        byte_size: metadata.len(),
        modified_at: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default(),
        previewable: relative
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png")),
    })
}

fn validate_relative_path(value: &str) -> Result<PathBuf, AssetError> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AssetError::InvalidPath);
    }
    let parts = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    let allowed = match parts.as_slice() {
        ["fonts", "font.ttf"] => true,
        ["realm", category, ..] => {
            parts.len() >= 3
                && REALM_CATEGORIES
                    .iter()
                    .any(|(allowed, _)| allowed == category)
        }
        _ => false,
    };
    allowed
        .then(|| path.to_path_buf())
        .ok_or(AssetError::InvalidPath)
}

fn validate_asset(path: &Path, bytes: &[u8]) -> Result<(), AssetError> {
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES {
        return Err(AssetError::TooLarge);
    }
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("png") => {
            let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
            let mut limits = Limits::default();
            limits.max_image_width = Some(8_192);
            limits.max_image_height = Some(8_192);
            limits.max_alloc = Some(128 * 1024 * 1024);
            reader.limits(limits);
            reader
                .decode()
                .map(|_| ())
                .map_err(|_| AssetError::InvalidImage)
        }
        Some(extension) if extension.eq_ignore_ascii_case("ttf") => {
            FontArc::try_from_vec(bytes.to_vec())
                .map(|_| ())
                .map_err(|_| AssetError::UnsupportedType)
        }
        _ => Err(AssetError::UnsupportedType),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("png") => "image/png",
        Some(extension) if extension.eq_ignore_ascii_case("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn path_text(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn zip_error(error: zip::result::ZipError) -> AssetError {
    AssetError::InvalidArchive(error.to_string())
}
