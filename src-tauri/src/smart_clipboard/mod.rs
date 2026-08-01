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

#[tauri::command]
pub fn open_smart_clipboard() -> Result<(), String> {
    #[cfg(windows)]
    {
        return show();
    }

    #[cfg(not(windows))]
    {
        Err("智能剪贴板目前仅支持 Windows".to_string())
    }
}

pub fn shutdown() {
    #[cfg(windows)]
    windows::shutdown();
}
