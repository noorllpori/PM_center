use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::core::{PCSTR, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx};
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::Win32::UI::Shell::{
    Common::ITEMIDLIST, IContextMenu, ILClone, ILFindLastID, IShellFolder, SHBindToParent,
    SHParseDisplayName, CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
    TrackPopupMenuEx, HMENU, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_NULL,
};

use super::SystemContextMenuResult;

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
                "failed to extract shell item for {}",
                path.display()
            ));
        }

        let cloned = unsafe { ILClone(last.cast_const()) };
        if cloned.is_null() {
            return Err(format!("failed to clone shell item for {}", path.display()));
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

struct OwnedMenu(HMENU);

impl OwnedMenu {
    fn create() -> Result<Self, String> {
        unsafe { CreatePopupMenu().map(Self) }
            .map_err(|e| format!("failed to create popup menu: {}", e))
    }

    fn handle(&self) -> HMENU {
        self.0
    }
}

impl Drop for OwnedMenu {
    fn drop(&mut self) {
        let _ = unsafe { DestroyMenu(self.0) };
    }
}

pub fn show_system_context_menu(
    hwnd: HWND,
    paths: Vec<String>,
) -> Result<SystemContextMenuResult, String> {
    unsafe {
        OleInitialize(None).map_err(|e| format!("failed to initialize OLE: {}", e))?;
    }

    let result = show_system_context_menu_inner(hwnd, paths);

    unsafe {
        OleUninitialize();
    }

    result
}

fn show_system_context_menu_inner(
    hwnd: HWND,
    paths: Vec<String>,
) -> Result<SystemContextMenuResult, String> {
    let validated_paths = validate_paths(paths)?;
    ensure_common_parent(&validated_paths)?;

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

    let parent_folder: IShellFolder = unsafe {
        SHBindToParent::<IShellFolder>(absolute_pidls[0].as_ptr(), None)
            .map_err(|e| format!("failed to bind parent shell folder: {}", e))?
    };

    let context_menu: IContextMenu = unsafe {
        parent_folder
            .GetUIObjectOf::<_, IContextMenu>(hwnd, child_refs.as_slice(), None)
            .map_err(|e| format!("failed to create shell context menu: {}", e))?
    };

    let menu = OwnedMenu::create()?;
    unsafe {
        context_menu
            .QueryContextMenu(menu.handle(), 0, 1, 0x7FFF, CMF_NORMAL)
            .map_err(|e| format!("failed to populate shell context menu: {}", e))?;
    }

    let cursor = current_cursor_position()?;

    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }

    let command_id = unsafe {
        TrackPopupMenuEx(
            menu.handle(),
            TPM_RETURNCMD.0 | TPM_RIGHTBUTTON.0,
            cursor.x,
            cursor.y,
            hwnd,
            None,
        )
        .0 as u32
    };

    unsafe {
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    }

    if command_id == 0 {
        return Ok(SystemContextMenuResult {
            status: "dismissed".to_string(),
        });
    }

    let command_offset = (command_id - 1) as usize;
    let invoke = CMINVOKECOMMANDINFOEX {
        cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
        fMask: CMIC_MASK_PTINVOKE,
        hwnd,
        lpVerb: PCSTR(command_offset as *const u8),
        lpParameters: PCSTR::null(),
        lpDirectory: PCSTR::null(),
        nShow: SW_SHOWNORMAL.0,
        dwHotKey: 0,
        hIcon: HANDLE(0),
        lpTitle: PCSTR::null(),
        lpVerbW: PCWSTR::null(),
        lpParametersW: PCWSTR::null(),
        lpDirectoryW: PCWSTR::null(),
        lpTitleW: PCWSTR::null(),
        ptInvoke: cursor,
    };

    unsafe {
        context_menu
            .InvokeCommand((&invoke as *const CMINVOKECOMMANDINFOEX).cast::<CMINVOKECOMMANDINFO>())
            .map_err(|e| format!("failed to invoke shell context menu command: {}", e))?;
    }

    Ok(SystemContextMenuResult {
        status: "invoked".to_string(),
    })
}

fn validate_paths(paths: Vec<String>) -> Result<Vec<PathBuf>, String> {
    let mut validated = Vec::new();
    let mut seen = HashSet::new();

    for raw_path in paths {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return Err("context menu path cannot be empty".to_string());
        }

        let path = PathBuf::from(trimmed);
        if !path.is_absolute() {
            return Err(format!(
                "context menu path must be absolute: {}",
                path.display()
            ));
        }

        if !path.exists() {
            return Err(format!(
                "context menu path does not exist: {}",
                path.display()
            ));
        }

        let dedupe_key = normalize_path_key(&path);
        if seen.insert(dedupe_key) {
            validated.push(path);
        }
    }

    if validated.is_empty() {
        return Err("no valid file or folder paths for system context menu".to_string());
    }

    Ok(validated)
}

fn ensure_common_parent(paths: &[PathBuf]) -> Result<(), String> {
    let first_parent = paths
        .first()
        .and_then(|path| path.parent())
        .ok_or_else(|| "failed to determine parent directory".to_string())?;
    let first_parent_key = normalize_path_key(first_parent);

    for path in paths.iter().skip(1) {
        let parent = path
            .parent()
            .ok_or_else(|| format!("failed to determine parent directory: {}", path.display()))?;

        if normalize_path_key(parent) != first_parent_key {
            return Err(
                "system context menu currently requires items from the same directory".to_string(),
            );
        }
    }

    Ok(())
}

fn current_cursor_position() -> Result<POINT, String> {
    let mut cursor = POINT::default();
    unsafe {
        GetCursorPos(&mut cursor).map_err(|e| format!("failed to get cursor position: {}", e))?;
    }
    Ok(cursor)
}

fn normalize_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

fn to_wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
