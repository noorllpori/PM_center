use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IDataObject};
use windows::Win32::System::Ole::{IDropSource, OleInitialize, OleUninitialize, DROPEFFECT_COPY};
use windows::Win32::UI::Shell::{
    Common::ITEMIDLIST, ILClone, ILFindLastID, SHCreateDataObject, SHDoDragDrop, SHParseDisplayName,
};

use super::ExternalFileDragResult;

const EXTERNAL_DRAG_LOG_FILE: &str = "pm_center_external_drag.log";

struct OwnedItemIdList(*mut ITEMIDLIST);

impl OwnedItemIdList {
    fn from_path(path: &Path) -> Result<Self, String> {
        let mut raw = std::ptr::null_mut();
        let wide = to_wide(path.as_os_str());

        unsafe {
            SHParseDisplayName(
                PCWSTR(wide.as_ptr()),
                Option::<&IBindCtx>::None,
                &mut raw,
                0,
                None,
            )
            .map_err(|e| format!("failed to parse shell path ({}): {}", path.display(), e))?;
        }

        if raw.is_null() {
            return Err(format!(
                "failed to parse shell path ({}): empty PIDL",
                path.display()
            ));
        }

        Ok(Self(raw))
    }

    fn clone_last_item(&self, path: &Path) -> Result<Self, String> {
        let last = unsafe { ILFindLastID(self.as_ptr()) };
        if last.is_null() {
            return Err(format!(
                "failed to extract child shell item for {}",
                path.display()
            ));
        }

        let cloned = unsafe { ILClone(last.cast_const()) };
        if cloned.is_null() {
            return Err(format!(
                "failed to clone child shell item for {}",
                path.display()
            ));
        }

        Ok(Self(cloned))
    }

    fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0.cast_const()
    }
}

impl Drop for OwnedItemIdList {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CoTaskMemFree(Some(self.0.cast()));
            }
        }
    }
}

pub fn start_external_file_drag(
    hwnd: HWND,
    paths: Vec<String>,
) -> Result<ExternalFileDragResult, String> {
    log_drag("---- external drag begin ----");
    log_drag(&format!("requested hwnd={:?}", hwnd));

    unsafe {
        OleInitialize(None).map_err(|e| format!("failed to initialize OLE: {}", e))?;
    }

    let result = run_external_file_drag_inner(hwnd, paths);

    unsafe {
        OleUninitialize();
    }

    match &result {
        Ok(result) => log_drag(&format!("drag completed with status={}", result.status)),
        Err(error) => log_drag(&format!("drag failed: {}", error)),
    }

    result
}

fn run_external_file_drag_inner(
    hwnd: HWND,
    paths: Vec<String>,
) -> Result<ExternalFileDragResult, String> {
    let validated_paths = validate_paths(paths)?;
    log_drag(&format!("validated paths={:?}", validated_paths));

    let parent_dir = derive_common_parent_dir(&validated_paths)?;
    log_drag(&format!("common parent={}", parent_dir.display()));

    let parent_pidl = OwnedItemIdList::from_path(&parent_dir)?;
    let absolute_pidls = validated_paths
        .iter()
        .map(|path| OwnedItemIdList::from_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    let child_pidls = absolute_pidls
        .iter()
        .zip(validated_paths.iter())
        .map(|(pidl, path)| pidl.clone_last_item(path))
        .collect::<Result<Vec<_>, _>>()?;
    let child_refs = child_pidls
        .iter()
        .map(OwnedItemIdList::as_ptr)
        .collect::<Vec<_>>();

    log_drag(&format!("shell child count={}", child_refs.len()));

    let data_object: IDataObject = unsafe {
        SHCreateDataObject(
            Some(parent_pidl.as_ptr()),
            Some(child_refs.as_slice()),
            Option::<&IDataObject>::None,
        )
        .map_err(|e| format!("failed to create drag data object: {}", e))?
    };

    log_drag("shell data object created successfully");

    let effect = unsafe {
        SHDoDragDrop(
            hwnd,
            &data_object,
            Option::<&IDropSource>::None,
            DROPEFFECT_COPY,
        )
        .map_err(|e| format!("failed to start shell drag session: {}", e))?
    };

    log_drag(&format!("shell drag returned effect={}", effect.0));

    Ok(ExternalFileDragResult {
        status: if effect.0 == 0 {
            "cancelled".to_string()
        } else {
            "dropped".to_string()
        },
    })
}

fn validate_paths(paths: Vec<String>) -> Result<Vec<PathBuf>, String> {
    let mut validated = Vec::new();
    let mut seen = HashSet::new();

    for raw_path in paths {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return Err("drag path cannot be empty".to_string());
        }

        let path = PathBuf::from(trimmed);
        if !path.is_absolute() {
            return Err(format!("drag path must be absolute: {}", path.display()));
        }

        if !path.exists() {
            return Err(format!("drag path does not exist: {}", path.display()));
        }

        let dedupe_key = normalize_path_key(&path);
        if seen.insert(dedupe_key) {
            validated.push(path);
        }
    }

    if validated.is_empty() {
        return Err("no valid file or folder paths to drag".to_string());
    }

    Ok(validated)
}

fn derive_common_parent_dir(paths: &[PathBuf]) -> Result<PathBuf, String> {
    let first_parent = paths
        .first()
        .and_then(|path| path.parent())
        .map(PathBuf::from)
        .ok_or_else(|| "failed to determine drag parent directory".to_string())?;

    let first_parent_key = normalize_path_key(&first_parent);

    for path in paths.iter().skip(1) {
        let parent = path
            .parent()
            .ok_or_else(|| format!("failed to determine parent directory: {}", path.display()))?;

        if normalize_path_key(parent) != first_parent_key {
            return Err(format!(
                "external drag currently requires all items to come from the same directory: {} vs {}",
                first_parent.display(),
                parent.display()
            ));
        }
    }

    Ok(first_parent)
}

fn normalize_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

fn to_wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn log_drag(message: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let log_path = std::env::temp_dir().join(EXTERNAL_DRAG_LOG_FILE);

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "[{}] {}", timestamp, message);
    }
}
