// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
fn enable_windows_extension_point_protection() {
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::Win32::System::Threading::{
        ProcessExtensionPointDisablePolicy, SetProcessMitigationPolicy,
    };

    const DISABLE_EXTENSION_POINTS: u32 = 1;
    // Block legacy window-hook/AppInit DLL injection before Tauri creates a window.
    let result = unsafe {
        SetProcessMitigationPolicy(
            ProcessExtensionPointDisablePolicy,
            &DISABLE_EXTENSION_POINTS as *const u32 as *const c_void,
            size_of::<u32>(),
        )
    };

    if let Err(error) = result {
        eprintln!("[compat] failed to disable legacy extension points: {error}");
    }
}

fn main() {
    #[cfg(target_os = "windows")]
    enable_windows_extension_point_protection();

    nexora_lib::run()
}
