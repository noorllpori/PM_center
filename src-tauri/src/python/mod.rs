use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonEnv {
    pub python_path: String,
    pub env_type: EnvType,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvType {
    System,
    Embedded,
    Blender,
    Custom,
}

// 检测可用的 Python 环境
#[tauri::command]
pub async fn detect_python_envs() -> Vec<PythonEnv> {
    let mut envs = Vec::new();

    for cmd in &["python", "python3", "py"] {
        if let Ok(output) = Command::new(cmd)
            .args(&["--version"])
            .output()
            .await
        {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_string();
                envs.push(PythonEnv {
                    python_path: cmd.to_string(),
                    env_type: EnvType::System,
                    version: if version.is_empty() {
                        String::from_utf8_lossy(&output.stderr).trim().to_string()
                    } else {
                        version
                    },
                });
            }
        }
    }

    #[cfg(target_os = "windows")]
    let blender_paths = vec![
        "C:\\Program Files\\Blender Foundation\\Blender 4.5\\blender.exe",
        "C:\\Program Files\\Blender Foundation\\Blender 4.4\\blender.exe",
        "C:\\Program Files\\Blender Foundation\\Blender 4.3\\blender.exe",
        "C:\\Program Files\\Blender Foundation\\Blender 4.2\\blender.exe",
        "C:\\Program Files\\Blender Foundation\\Blender 4.1\\blender.exe",
        "C:\\Program Files\\Blender Foundation\\Blender 4.0\\blender.exe",
    ];

    #[cfg(target_os = "macos")]
    let blender_paths: Vec<&str> = vec![
        "/Applications/Blender.app/Contents/MacOS/Blender",
    ];

    #[cfg(target_os = "linux")]
    let blender_paths: Vec<&str> = vec![
        "/usr/bin/blender",
        "/usr/local/bin/blender",
    ];

    for blender_path in blender_paths {
        if std::path::Path::new(blender_path).exists() {
            envs.push(PythonEnv {
                python_path: blender_path.to_string(),
                env_type: EnvType::Blender,
                version: "Blender".to_string(),
            });
            break;
        }
    }

    let embedded_path = get_embedded_python_path();
    if embedded_path.exists() {
        if let Ok(output) = Command::new(&embedded_path)
            .args(&["--version"])
            .output()
            .await
        {
            if output.status.success() {
                envs.push(PythonEnv {
                    python_path: embedded_path.to_string_lossy().to_string(),
                    env_type: EnvType::Embedded,
                    version: String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .to_string(),
                });
            }
        }
    }

    envs
}

fn get_embedded_python_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    
    #[cfg(target_os = "windows")]
    return exe_dir.join("python").join("python.exe");
    
    #[cfg(not(target_os = "windows"))]
    return exe_dir.join("python").join("bin").join("python3");
}

// 运行 Python 脚本
#[tauri::command]
pub async fn run_python_script(
    env_type: EnvType,
    python_path: String,
    script: String,
    working_dir: Option<String>,
    env_vars: Option<std::collections::HashMap<String, String>>,
) -> Result<ScriptResult, String> {
    let mut cmd = match env_type {
        EnvType::Blender => {
            let mut c = Command::new(&python_path);
            c.args(&["--background", "--python", "-c", &script]);
            c
        }
        _ => {
            let mut c = Command::new(&python_path);
            c.arg("-c").arg(&script);
            c
        }
    };

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    if let Some(vars) = env_vars {
        for (key, value) in vars {
            cmd.env(key, value);
        }
    }

    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to execute: {}", e))?;

    Ok(ScriptResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}

// 运行 Python 文件
#[tauri::command]
pub async fn run_python_file(
    env_type: EnvType,
    python_path: String,
    script_path: String,
    args: Vec<String>,
    working_dir: Option<String>,
) -> Result<ScriptResult, String> {
    let mut cmd = match env_type {
        EnvType::Blender => {
            let mut c = Command::new(&python_path);
            c.args(&["--background", "--python", &script_path]);
            c.args(&args);
            c
        }
        _ => {
            let mut c = Command::new(&python_path);
            c.arg(&script_path).args(&args);
            c
        }
    };

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to execute: {}", e))?;

    Ok(ScriptResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}

// 执行 pip 安装
#[tauri::command]
pub async fn pip_install(
    python_path: String,
    packages: Vec<String>,
) -> Result<ScriptResult, String> {
    let mut cmd = Command::new(&python_path);
    cmd.args(&["-m", "pip", "install"]);
    cmd.args(&packages);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to execute pip: {}", e))?;

    Ok(ScriptResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}

mod blender;
pub use blender::get_blender_file_info;
