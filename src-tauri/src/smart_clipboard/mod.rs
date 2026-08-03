#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub fn initialize(app_data_dir: &std::path::Path) -> Result<(), String> {
    windows::initialize(app_data_dir)
}

#[cfg(not(windows))]
pub fn initialize(_app_data_dir: &std::path::Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub fn show() -> Result<(), String> {
    windows::show()
}

#[cfg(not(windows))]
pub fn show() -> Result<(), String> {
    Err("智能剪贴板目前仅支持 Windows".to_string())
}

pub fn is_running() -> bool {
    #[cfg(windows)]
    {
        return windows::is_running();
    }

    #[cfg(not(windows))]
    {
        false
    }
}

pub(crate) fn ensure_module_running(
    manager: &crate::platform::ModuleManager,
) -> Result<(), String> {
    let snapshot = manager
        .snapshot(crate::platform::SMART_CLIPBOARD_MODULE_ID)
        .map_err(|error| error.to_string())?;
    if snapshot.state != crate::platform::ModuleState::Running {
        return Err(
            "SMART_CLIPBOARD_MODULE_DISABLED: 智能剪贴板模块已停用，请先在设置的模块管理中启用"
                .to_string(),
        );
    }
    Ok(())
}

#[tauri::command]
pub fn open_smart_clipboard(
    runtime: tauri::State<'_, crate::platform::PlatformRuntime>,
) -> Result<(), String> {
    ensure_module_running(&runtime.manager)?;
    #[cfg(windows)]
    {
        return show();
    }

    #[cfg(not(windows))]
    {
        Err("智能剪贴板目前仅支持 Windows".to_string())
    }
}

pub fn shutdown() -> Result<(), String> {
    #[cfg(windows)]
    {
        return windows::shutdown();
    }

    #[cfg(not(windows))]
    {
        Ok(())
    }
}
