use std::{
    collections::HashSet,
    fs,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::ImageReader;
use serde::{Deserialize, Serialize};
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;
use walkdir::WalkDir;
use zip::ZipArchive;

const MAX_SPRITESHEET_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PACKAGE_ENTRIES: usize = 512;
static PET_MUTATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetManifest {
    id: String,
    display_name: String,
    description: String,
    sprite_version_number: Option<u8>,
    spritesheet_path: String,
    #[serde(rename = "sidecarSpritesheetPath")]
    copets_spritesheet_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetSummary {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub sprite_version_number: u8,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedPet {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub sprite_version_number: u8,
    pub spritesheet_data_url: String,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub render_scale: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetPackageIssue {
    pub folder_name: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCatalog {
    pub pets: Vec<PetSummary>,
    pub issues: Vec<PetPackageIssue>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetImportPreview {
    pub pet: LoadedPet,
    pub target_exists: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetInstallResult {
    pub pet: PetSummary,
    pub replaced: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetRemovalResult {
    pub removed_id: String,
    pub catalog: PetCatalog,
}

struct ValidatedPet {
    manifest: PetManifest,
    spritesheet: Vec<u8>,
    mime: &'static str,
    atlas_width: u32,
    atlas_height: u32,
    cell_width: u32,
    cell_height: u32,
    render_scale: u32,
}

impl ValidatedPet {
    fn summary(&self) -> PetSummary {
        PetSummary {
            id: self.manifest.id.clone(),
            display_name: self.manifest.display_name.clone(),
            description: self.manifest.description.clone(),
            sprite_version_number: self.manifest.sprite_version_number.unwrap_or(1),
        }
    }

    fn loaded(&self) -> Result<LoadedPet, String> {
        Ok(LoadedPet {
            id: self.manifest.id.clone(),
            display_name: self.manifest.display_name.clone(),
            description: self.manifest.description.clone(),
            sprite_version_number: self.manifest.sprite_version_number.unwrap_or(1),
            spritesheet_data_url: format!(
                "data:{};base64,{}",
                self.mime,
                STANDARD.encode(&self.spritesheet)
            ),
            atlas_width: self.atlas_width,
            atlas_height: self.atlas_height,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
            render_scale: self.render_scale,
        })
    }
}

pub struct PetPackageManager {
    root: PathBuf,
}

impl PetPackageManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn catalog(&self) -> Result<PetCatalog, String> {
        let mut pets = Vec::new();
        let mut issues = Vec::new();
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PetCatalog { pets, issues });
            }
            Err(error) => return Err(error.to_string()),
        };
        for entry in entries.flatten() {
            let folder_name = entry.file_name().to_string_lossy().into_owned();
            if folder_name.starts_with('.') {
                continue;
            }
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    issues.push(PetPackageIssue {
                        folder_name,
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            if file_type.is_symlink() {
                issues.push(PetPackageIssue {
                    folder_name,
                    message: "installed pet folder cannot be a symbolic link".into(),
                });
                continue;
            }
            if !file_type.is_dir() {
                continue;
            }
            match validate_installed_package(&self.root, &folder_name) {
                Ok(pet) => pets.push(pet.summary()),
                Err(message) => issues.push(PetPackageIssue {
                    folder_name,
                    message,
                }),
            }
        }
        pets.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        issues.sort_by(|left, right| left.folder_name.cmp(&right.folder_name));
        Ok(PetCatalog { pets, issues })
    }

    pub fn load(&self, id: &str) -> Result<LoadedPet, String> {
        validate_pet_id(id)?;
        validate_installed_package(&self.root, id)?.loaded()
    }

    pub fn preview_import(&self, source: &Path) -> Result<PetImportPreview, String> {
        let prepared = self.prepare_source(source)?;
        let pet = validate_package(&prepared.package_root, None)?;
        Ok(PetImportPreview {
            target_exists: self.root.join(&pet.manifest.id).exists(),
            pet: pet.loaded()?,
        })
    }

    pub fn install(&self, source: &Path, replace: bool) -> Result<PetInstallResult, String> {
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let prepared = self.prepare_source(source)?;
        let staging = StagingDirectory::create(&self.root)?;
        let staged_package = staging.path.join("package");
        copy_package_tree(&prepared.package_root, &staged_package)?;
        let pet = validate_package(&staged_package, None)?;
        let summary = pet.summary();
        let _mutation = mutation_guard()?;
        let target = self.root.join(&summary.id);
        let target_metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        if target_metadata
            .as_ref()
            .is_some_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err("installed pet folder cannot be a symbolic link".into());
        }
        if target_metadata
            .as_ref()
            .is_some_and(|metadata| !metadata.is_dir())
        {
            return Err("installed pet target must be a folder".into());
        }
        let replaced = target_metadata.is_some();
        if replaced {
            if !replace {
                return Err(format!("A pet named '{}' is already installed", summary.id));
            }
            atomic_replace(&staged_package, &target)?;
        } else {
            fs::rename(&staged_package, &target).map_err(|error| error.to_string())?;
        }
        Ok(PetInstallResult {
            pet: summary,
            replaced,
        })
    }

    pub fn remove(&self, id: &str) -> Result<PetRemovalResult, String> {
        validate_pet_id(id)?;
        let _mutation = mutation_guard()?;
        let target = self.root.join(id);
        if !target.is_dir() {
            return Err(format!("Pet '{id}' is not installed"));
        }
        let staging = StagingDirectory::create(&self.root)?;
        let removed = staging.path.join("removed-package");
        fs::rename(&target, &removed).map_err(|error| error.to_string())?;
        if let Err(remove_error) = fs::remove_dir_all(&removed) {
            return match fs::rename(&removed, &target) {
                Ok(()) => Err(format!(
                    "removal failed and was rolled back: {remove_error}"
                )),
                Err(rollback_error) => Err(format!(
                    "removal failed: {remove_error}; rollback failed: {rollback_error}"
                )),
            };
        }
        Ok(PetRemovalResult {
            removed_id: id.to_string(),
            catalog: self.catalog()?,
        })
    }

    fn prepare_source(&self, source: &Path) -> Result<PreparedPackage, String> {
        if let Ok(folder) = selected_folder_source(source) {
            return Ok(PreparedPackage {
                package_root: find_package_root(folder)?,
                _staging: None,
            });
        }
        let is_zip = source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
        if !source.is_file() || !is_zip {
            return Err("choose a pet folder, pet.json, or ZIP package".into());
        }
        let archive_size = fs::metadata(source)
            .map_err(|error| error.to_string())?
            .len();
        if archive_size > MAX_PACKAGE_BYTES {
            return Err("ZIP package exceeds 128 MiB".into());
        }
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let staging = StagingDirectory::create(&self.root)?;
        let extracted = staging.path.join("extracted");
        fs::create_dir(&extracted).map_err(|error| error.to_string())?;
        extract_zip(source, &extracted)?;
        let package_root = find_package_root(&extracted)?;
        Ok(PreparedPackage {
            package_root,
            _staging: Some(staging),
        })
    }
}

fn mutation_guard() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    PET_MUTATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "pet package manager lock is poisoned".to_string())
}

struct PreparedPackage {
    package_root: PathBuf,
    _staging: Option<StagingDirectory>,
}

struct StagingDirectory {
    path: PathBuf,
}

impl StagingDirectory {
    fn create(pets_root: &Path) -> Result<Self, String> {
        let parent = pets_root.join(".copets-staging");
        fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let path = parent.join(Uuid::new_v4().to_string());
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self { path })
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
        if let Some(parent) = self.path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

fn codex_home() -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(home));
    }
    dirs::home_dir()
        .map(|home| home.join(".codex"))
        .ok_or("home directory unavailable".into())
}

fn pets_root() -> Result<PathBuf, String> {
    Ok(codex_home()?.join("pets"))
}

fn read_manifest(folder: &Path) -> Result<PetManifest, String> {
    let bytes = fs::read(folder.join("pet.json")).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid pet.json: {error}"))
}

fn validate_pet_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || id == "."
        || id == ".."
        || id.contains(['/', '\\'])
        || id.chars().any(char::is_control)
    {
        return Err("invalid pet id".into());
    }
    Ok(())
}

fn preferred_sheet_path(manifest: &PetManifest) -> &str {
    manifest
        .copets_spritesheet_path
        .as_deref()
        .unwrap_or(&manifest.spritesheet_path)
}

fn resolve_sheet(folder: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("spritesheetPath must stay inside the pet folder".into());
    }
    let folder = folder.canonicalize().map_err(|error| error.to_string())?;
    let sheet = folder
        .join(relative)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !sheet.starts_with(&folder) {
        return Err("spritesheetPath escapes the pet folder".into());
    }
    Ok(sheet)
}

fn validate_geometry(version: u8, width: u32, height: u32) -> Result<(u32, u32, u32), String> {
    let rows = if version == 2 { 11 } else { 9 };
    if !width.is_multiple_of(8) || !height.is_multiple_of(rows) {
        return Err(format!("atlas must be an 8x{rows} grid"));
    }
    let cell_width = width / 8;
    let cell_height = height / rows;
    if cell_width * 208 != cell_height * 192
        || !cell_width.is_multiple_of(192)
        || !cell_height.is_multiple_of(208)
    {
        return Err("cell geometry must be an integer scale of 192x208".into());
    }
    let scale = cell_width / 192;
    if scale == 0 || scale > 4 {
        return Err("supported atlas scale is 1x through 4x".into());
    }
    Ok((cell_width, cell_height, scale))
}

fn validate_package(folder: &Path, expected_id: Option<&str>) -> Result<ValidatedPet, String> {
    let manifest = read_manifest(folder)?;
    validate_pet_id(&manifest.id)?;
    if let Some(expected_id) = expected_id
        && manifest.id != expected_id
    {
        return Err("folder id and manifest id differ".into());
    }
    let version = manifest.sprite_version_number.unwrap_or(1);
    if !matches!(version, 1 | 2) {
        return Err(format!("unsupported spriteVersionNumber: {version}"));
    }
    let sheet = resolve_sheet(folder, preferred_sheet_path(&manifest))?;
    let spritesheet = fs::read(&sheet).map_err(|error| error.to_string())?;
    if spritesheet.len() as u64 > MAX_SPRITESHEET_BYTES {
        return Err("spritesheet exceeds 64 MiB".into());
    }
    let reader = ImageReader::new(Cursor::new(&spritesheet))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let (atlas_width, atlas_height) = reader
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    let (cell_width, cell_height, render_scale) =
        validate_geometry(version, atlas_width, atlas_height)?;
    let mime = match sheet
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => return Err("spritesheet must be PNG or WebP".into()),
    };
    Ok(ValidatedPet {
        manifest,
        spritesheet,
        mime,
        atlas_width,
        atlas_height,
        cell_width,
        cell_height,
        render_scale,
    })
}

fn validate_installed_package(root: &Path, id: &str) -> Result<ValidatedPet, String> {
    validate_pet_id(id)?;
    let folder = root.join(id);
    let metadata = fs::symlink_metadata(&folder).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("installed pet folder cannot be a symbolic link".into());
    }
    if !metadata.is_dir() {
        return Err("installed pet target must be a folder".into());
    }
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let canonical_folder = folder.canonicalize().map_err(|error| error.to_string())?;
    if canonical_folder.parent() != Some(canonical_root.as_path()) {
        return Err("installed pet folder escapes the pets root".into());
    }
    validate_package(&canonical_folder, Some(id))
}

fn selected_folder_source(source: &Path) -> Result<&Path, String> {
    if source.is_dir() {
        return Ok(source);
    }
    if source.is_file() && source.file_name().and_then(|name| name.to_str()) == Some("pet.json") {
        return source
            .parent()
            .ok_or_else(|| "pet.json has no containing folder".into());
    }
    Err("choose a pet folder, pet.json, or ZIP package".into())
}

fn find_package_root(source: &Path) -> Result<PathBuf, String> {
    if source.join("pet.json").is_file() {
        return Ok(source.to_path_buf());
    }
    let mut candidates = Vec::new();
    for entry in WalkDir::new(source)
        .min_depth(1)
        .max_depth(2)
        .follow_links(false)
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let hidden = relative.components().any(|component| match component {
            std::path::Component::Normal(value) => {
                let value = value.to_string_lossy();
                value.starts_with('.') || value == "__MACOSX"
            }
            _ => false,
        });
        if hidden || !entry.file_type().is_file() || entry.file_name() != "pet.json" {
            continue;
        }
        if let Some(parent) = entry.path().parent() {
            candidates.push(parent.to_path_buf());
        }
    }
    match candidates.as_slice() {
        [folder] => Ok(folder.clone()),
        [] => Err("pet package does not contain pet.json".into()),
        _ => Err("pet package contains more than one pet.json".into()),
    }
}

fn extract_zip(source: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(source).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("invalid ZIP: {error}"))?;
    if archive.len() > MAX_PACKAGE_ENTRIES {
        return Err(format!("ZIP package exceeds {MAX_PACKAGE_ENTRIES} entries"));
    }
    if archive
        .decompressed_size()
        .is_some_and(|size| size > u128::from(MAX_PACKAGE_BYTES))
    {
        return Err("ZIP package expands beyond 128 MiB".into());
    }
    if archive
        .has_overlapping_files()
        .map_err(|error| format!("invalid ZIP: {error}"))?
    {
        return Err("ZIP package contains overlapping file data".into());
    }

    let mut total_bytes = 0u64;
    let mut entry_names = HashSet::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("invalid ZIP entry: {error}"))?;
        if file.encrypted() {
            return Err("encrypted ZIP packages are not supported".into());
        }
        let raw_name = Path::new(file.name());
        if file.name().contains('\\')
            || raw_name.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err("ZIP entry escapes the package root".into());
        }
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| "ZIP entry escapes the package root".to_string())?;
        let normalized_name = enclosed.to_string_lossy().to_lowercase();
        if !entry_names.insert(normalized_name) {
            return Err("ZIP package contains duplicate entry names".into());
        }
        if let Some(mode) = file.unix_mode() {
            let file_type = mode & 0o170000;
            if file_type != 0 && file_type != 0o040000 && file_type != 0o100000 {
                return Err("ZIP packages cannot contain symbolic links or special files".into());
            }
        }
        let target = destination.join(enclosed);
        if file.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let remaining = MAX_PACKAGE_BYTES.saturating_sub(total_bytes);
        let copied = io::copy(
            &mut Read::take(&mut file, remaining.saturating_add(1)),
            &mut fs::File::create(&target).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if copied > remaining {
            return Err("ZIP package expands beyond 128 MiB".into());
        }
        total_bytes += copied;
    }
    Ok(())
}

fn copy_package_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let mut entries = 0usize;
    let mut total_bytes = 0u64;
    for entry in WalkDir::new(source).min_depth(1).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        entries += 1;
        if entries > MAX_PACKAGE_ENTRIES {
            return Err(format!("pet package exceeds {MAX_PACKAGE_ENTRIES} entries"));
        }
        if entry.file_type().is_symlink() {
            return Err("pet packages cannot contain symbolic links".into());
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        } else if entry.file_type().is_file() {
            let bytes = entry.metadata().map_err(|error| error.to_string())?.len();
            total_bytes = total_bytes
                .checked_add(bytes)
                .ok_or_else(|| "pet package size overflow".to_string())?;
            if total_bytes > MAX_PACKAGE_BYTES {
                return Err("pet package exceeds 128 MiB".into());
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(entry.path(), &target).map_err(|error| error.to_string())?;
        } else {
            return Err("pet packages can contain only regular files and folders".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn atomic_replace(staged: &Path, target: &Path) -> Result<(), String> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let staged = CString::new(staged.as_os_str().as_bytes())
        .map_err(|_| "staging path contains a null byte".to_string())?;
    let target = CString::new(target.as_os_str().as_bytes())
        .map_err(|_| "target path contains a null byte".to_string())?;
    // Both directories live under the same pets root. macOS swaps their names atomically.
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            staged.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_SWAP,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

#[cfg(not(target_os = "macos"))]
fn atomic_replace(staged: &Path, target: &Path) -> Result<(), String> {
    let backup = staged
        .parent()
        .ok_or_else(|| "staging directory has no parent".to_string())?
        .join("previous-package");
    fs::rename(target, &backup).map_err(|error| error.to_string())?;
    if let Err(install_error) = fs::rename(staged, target) {
        return match fs::rename(&backup, target) {
            Ok(()) => Err(format!(
                "replacement failed and was rolled back: {install_error}"
            )),
            Err(rollback_error) => Err(format!(
                "replacement failed: {install_error}; rollback failed: {rollback_error}"
            )),
        };
    }
    let _ = fs::remove_dir_all(&backup);
    Ok(())
}

#[tauri::command]
pub async fn list_pets() -> Result<PetCatalog, String> {
    let root = pets_root()?;
    tauri::async_runtime::spawn_blocking(move || PetPackageManager::new(root).catalog())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn load_pet(id: String) -> Result<LoadedPet, String> {
    let root = pets_root()?;
    tauri::async_runtime::spawn_blocking(move || PetPackageManager::new(root).load(&id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_pet_import(source_path: String) -> Result<PetImportPreview, String> {
    let root = pets_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        PetPackageManager::new(root).preview_import(Path::new(&source_path))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn install_pet(source_path: String, replace: bool) -> Result<PetInstallResult, String> {
    let root = pets_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        PetPackageManager::new(root).install(Path::new(&source_path), replace)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn remove_pet(id: String) -> Result<PetRemovalResult, String> {
    let root = pets_root()?;
    tauri::async_runtime::spawn_blocking(move || PetPackageManager::new(root).remove(&id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn open_pets_folder(app: tauri::AppHandle) -> Result<(), String> {
    let root = pets_root()?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{self, Write},
        path::PathBuf,
    };

    use image::{ImageBuffer, ImageFormat, Luma};
    use uuid::Uuid;
    use walkdir::WalkDir;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::{PetManifest, PetPackageManager, preferred_sheet_path, validate_geometry};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("copets-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_valid_pet(folder: &std::path::Path, id: &str, display_name: &str) {
        fs::create_dir_all(folder).unwrap();
        fs::write(
            folder.join("pet.json"),
            serde_json::json!({
                "id": id,
                "displayName": display_name,
                "description": "A test pet",
                "spriteVersionNumber": 1,
                "spritesheetPath": "spritesheet.png"
            })
            .to_string(),
        )
        .unwrap();
        ImageBuffer::<Luma<u8>, Vec<u8>>::from_pixel(1536, 1872, Luma([0]))
            .save_with_format(folder.join("spritesheet.png"), ImageFormat::Png)
            .unwrap();
    }

    fn zip_folder(source: &std::path::Path, archive_path: &std::path::Path, prefix: &str) {
        let file = fs::File::create(archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for entry in WalkDir::new(source).min_depth(1) {
            let entry = entry.unwrap();
            let relative = entry.path().strip_prefix(source).unwrap();
            let name = std::path::Path::new(prefix).join(relative);
            if entry.file_type().is_dir() {
                archive
                    .add_directory(name.to_string_lossy(), options)
                    .unwrap();
            } else {
                archive.start_file(name.to_string_lossy(), options).unwrap();
                io::copy(&mut fs::File::open(entry.path()).unwrap(), &mut archive).unwrap();
            }
        }
        archive.finish().unwrap();
    }

    #[test]
    fn imports_a_package_selected_by_its_manifest() {
        let sandbox = TestDirectory::new();
        let source = sandbox.path().join("downloaded pet");
        let pets_root = sandbox.path().join("codex-home/pets");
        write_valid_pet(&source, "imported-pet", "Imported Pet");
        let manager = PetPackageManager::new(pets_root.clone());

        let preview = manager.preview_import(&source.join("pet.json")).unwrap();
        assert_eq!(preview.pet.id, "imported-pet");
        assert_eq!(preview.pet.display_name, "Imported Pet");
        assert!(!preview.target_exists);

        let installed = manager.install(&source.join("pet.json"), false).unwrap();
        assert_eq!(installed.pet.id, "imported-pet");
        assert!(!installed.replaced);
        assert!(pets_root.join("imported-pet/pet.json").is_file());

        let catalog = manager.catalog().unwrap();
        assert_eq!(catalog.pets.len(), 1);
        assert_eq!(catalog.pets[0].id, "imported-pet");
        assert!(catalog.issues.is_empty());
    }

    #[test]
    fn imports_a_zip_with_one_wrapping_folder() {
        let sandbox = TestDirectory::new();
        let source = sandbox.path().join("zip source");
        let archive = sandbox.path().join("wrapped-pet.zip");
        let pets_root = sandbox.path().join("codex-home/pets");
        write_valid_pet(&source, "zip-pet", "ZIP Pet");
        zip_folder(&source, &archive, "ZIP Pet Package");
        let manager = PetPackageManager::new(pets_root.clone());

        let preview = manager.preview_import(&archive).unwrap();
        assert_eq!(preview.pet.id, "zip-pet");
        assert!(!preview.target_exists);

        let installed = manager.install(&archive, false).unwrap();
        assert_eq!(installed.pet.display_name, "ZIP Pet");
        assert!(pets_root.join("zip-pet/pet.json").is_file());
    }

    #[test]
    fn rejects_a_zip_with_more_than_one_wrapping_folder() {
        let sandbox = TestDirectory::new();
        let source = sandbox.path().join("source");
        let archive = sandbox.path().join("nested.zip");
        write_valid_pet(&source, "nested-pet", "Nested Pet");
        zip_folder(&source, &archive, "one/two");

        let manager = PetPackageManager::new(sandbox.path().join("codex-home/pets"));
        let error = manager.preview_import(&archive).unwrap_err();

        assert_eq!(error, "pet package does not contain pet.json");
    }

    #[test]
    fn rejects_duplicate_zip_entries() {
        let sandbox = TestDirectory::new();
        let archive_path = sandbox.path().join("duplicate.zip");
        let file = fs::File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, id) in [("pet.json", "first"), ("PET.JSON", "second")] {
            archive.start_file(name, options).unwrap();
            archive
                .write_all(
                    serde_json::json!({
                        "id": id,
                        "displayName": id,
                        "description": "duplicate",
                        "spritesheetPath": "spritesheet.png"
                    })
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
        }
        archive.finish().unwrap();

        let manager = PetPackageManager::new(sandbox.path().join("codex-home/pets"));
        let error = manager.preview_import(&archive_path).unwrap_err();

        assert_eq!(error, "ZIP package contains duplicate entry names");
    }

    #[cfg(unix)]
    #[test]
    fn installed_package_directory_symlinks_cannot_escape_the_pets_root() {
        use std::os::unix::fs::symlink;

        let sandbox = TestDirectory::new();
        let pets_root = sandbox.path().join("codex-home/pets");
        let outside = sandbox.path().join("outside");
        write_valid_pet(&outside, "escaped", "Escaped Pet");
        fs::create_dir_all(&pets_root).unwrap();
        symlink(&outside, pets_root.join("escaped")).unwrap();
        let manager = PetPackageManager::new(pets_root);

        let catalog = manager.catalog().unwrap();
        assert!(catalog.pets.is_empty());
        assert_eq!(catalog.issues.len(), 1);
        assert_eq!(catalog.issues[0].folder_name, "escaped");
        assert_eq!(
            catalog.issues[0].message,
            "installed pet folder cannot be a symbolic link"
        );
        assert_eq!(
            manager.load("escaped").unwrap_err(),
            "installed pet folder cannot be a symbolic link"
        );
    }

    #[test]
    fn replacement_requires_confirmation_and_swaps_the_complete_package() {
        let sandbox = TestDirectory::new();
        let source = sandbox.path().join("replacement");
        let pets_root = sandbox.path().join("codex-home/pets");
        let installed = pets_root.join("same-id");
        write_valid_pet(&installed, "same-id", "Old Pet");
        write_valid_pet(&source, "same-id", "New Pet");
        let manager = PetPackageManager::new(pets_root.clone());

        let preview = manager.preview_import(&source).unwrap();
        assert!(preview.target_exists);
        let conflict = manager.install(&source, false).unwrap_err();
        assert!(conflict.contains("already installed"));
        assert_eq!(manager.load("same-id").unwrap().display_name, "Old Pet");

        let replaced = manager.install(&source, true).unwrap();
        assert!(replaced.replaced);
        assert_eq!(replaced.pet.display_name, "New Pet");
        assert_eq!(manager.load("same-id").unwrap().display_name, "New Pet");
        assert!(!pets_root.join(".copets-staging").exists());
    }

    #[test]
    fn removal_returns_the_safe_remaining_catalog() {
        let sandbox = TestDirectory::new();
        let pets_root = sandbox.path().join("codex-home/pets");
        write_valid_pet(&pets_root.join("first"), "first", "First Pet");
        write_valid_pet(&pets_root.join("second"), "second", "Second Pet");
        let manager = PetPackageManager::new(pets_root.clone());

        let removed = manager.remove("first").unwrap();
        assert_eq!(removed.removed_id, "first");
        assert_eq!(removed.catalog.pets.len(), 1);
        assert_eq!(removed.catalog.pets[0].id, "second");
        assert!(!pets_root.join("first").exists());
        assert!(pets_root.join("second/pet.json").is_file());
        assert!(!pets_root.join(".copets-staging").exists());
    }

    #[test]
    fn rejects_zip_path_traversal_without_writing_outside_staging() {
        let sandbox = TestDirectory::new();
        let archive_path = sandbox.path().join("escape.zip");
        let file = fs::File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        archive
            .start_file("../escaped.txt", SimpleFileOptions::default())
            .unwrap();
        io::Write::write_all(&mut archive, b"not allowed").unwrap();
        archive.finish().unwrap();
        let manager = PetPackageManager::new(sandbox.path().join("codex-home/pets"));

        let error = manager.preview_import(&archive_path).unwrap_err();
        assert!(error.contains("escapes the package root"));
        assert!(!sandbox.path().join("escaped.txt").exists());
    }

    #[test]
    fn invalid_replacement_never_changes_the_installed_package() {
        let sandbox = TestDirectory::new();
        let pets_root = sandbox.path().join("codex-home/pets");
        let installed = pets_root.join("same-id");
        let source = sandbox.path().join("invalid replacement");
        write_valid_pet(&installed, "same-id", "Safe Pet");
        write_valid_pet(&source, "same-id", "Broken Pet");
        fs::write(source.join("spritesheet.png"), b"not an image").unwrap();
        let manager = PetPackageManager::new(pets_root.clone());

        assert!(manager.install(&source, true).is_err());
        assert_eq!(manager.load("same-id").unwrap().display_name, "Safe Pet");
    }

    #[test]
    fn catalog_reports_invalid_manual_packages() {
        let sandbox = TestDirectory::new();
        let pets_root = sandbox.path().join("codex-home/pets");
        write_valid_pet(&pets_root.join("valid"), "valid", "Valid Pet");
        fs::create_dir_all(pets_root.join("broken")).unwrap();
        fs::write(pets_root.join("broken/pet.json"), b"not json").unwrap();
        let manager = PetPackageManager::new(pets_root);

        let catalog = manager.catalog().unwrap();
        assert_eq!(catalog.pets.len(), 1);
        assert_eq!(catalog.issues.len(), 1);
        assert_eq!(catalog.issues[0].folder_name, "broken");
        assert!(catalog.issues[0].message.contains("invalid pet.json"));
    }

    #[test]
    fn accepts_v1_v2_and_scaled_atlases() {
        assert_eq!(validate_geometry(1, 1536, 1872), Ok((192, 208, 1)));
        assert_eq!(validate_geometry(1, 3072, 3744), Ok((384, 416, 2)));
        assert_eq!(validate_geometry(2, 1536, 2288), Ok((192, 208, 1)));
        assert_eq!(validate_geometry(2, 3072, 4576), Ok((384, 416, 2)));
    }

    #[test]
    fn rejects_incompatible_geometry() {
        assert!(validate_geometry(2, 1536, 1872).is_err());
        assert!(validate_geometry(1, 2048, 1872).is_err());
    }

    #[test]
    fn prefers_copets_sheet_without_changing_official_path() {
        let manifest = PetManifest {
            id: "sample-pet-hd".into(),
            display_name: "Sample Pet HD".into(),
            description: String::new(),
            sprite_version_number: None,
            spritesheet_path: "spritesheet.webp".into(),
            copets_spritesheet_path: Some("spritesheet-native-2x.webp".into()),
        };

        assert_eq!(
            preferred_sheet_path(&manifest),
            "spritesheet-native-2x.webp"
        );
        assert_eq!(manifest.spritesheet_path, "spritesheet.webp");
    }
}
