// Python 环境管理 - 检测系统 Python 和管理 venv
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonEnv {
    pub id: String,
    pub name: String,
    pub path: String,
    pub version: Option<String>,
    pub is_system: bool,
    pub is_venv: bool,
    pub venv_path: Option<String>,
}

/// 检测系统可用的 Python 环境
#[tauri::command]
pub async fn detect_system_python() -> Result<Vec<PythonEnv>, String> {
    let mut envs = vec![];
    
    // 尝试检测的常见 Python 命令
    let python_commands = vec![
        "python",
        "python3",
        "python3.12",
        "python3.11",
        "python3.10",
        "python3.9",
        "python3.8",
        "py",
    ];
    
    for cmd in python_commands {
        if let Some(env) = check_python(cmd).await {
            // 检查是否已添加（避免重复）
            if !envs.iter().any(|e: &PythonEnv| e.path == env.path) {
                envs.push(env);
            }
        }
    }
    
    // Windows 上检测常用安装路径
    #[cfg(windows)]
    {
        let app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let program_files = std::env::var("ProgramFiles").unwrap_or_default();
        
        let common_paths = vec![
            format!("{}\\Programs\\Python", app_data),
            format!("{}\\Python", program_files),
        ];
        
        for base_path in common_paths {
            if let Ok(entries) = std::fs::read_dir(&base_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let python_exe = path.join("python.exe");
                        if python_exe.exists() {
                            if let Some(env) = check_python_path(&python_exe).await {
                                if !envs.iter().any(|e: &PythonEnv| e.path == env.path) {
                                    envs.push(env);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // 按版本排序（新版本在前）
    envs.sort_by(|a, b| {
        let version_a = a.version.as_deref().unwrap_or("0");
        let version_b = b.version.as_deref().unwrap_or("0");
        version_b.cmp(version_a)
    });
    
    Ok(envs)
}

/// 检查指定命令的 Python
async fn check_python(cmd: &str) -> Option<PythonEnv> {
    let output = Command::new(cmd)
        .args(&["--version"])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    // 获取版本信息
    let version_str = String::from_utf8_lossy(&output.stdout);
    let version_str = version_str.trim();
    let version = if version_str.is_empty() {
        // 有些 Python 输出到 stderr
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        version_str.to_string()
    };
    
    // 提取版本号
    let version_num = version.split_whitespace()
        .nth(1)
        .map(|s| s.to_string());
    
    // 获取完整路径
    #[cfg(windows)]
    let which_cmd = "where";
    #[cfg(not(windows))]
    let which_cmd = "which";
    
    let which_output = Command::new(which_cmd)
        .arg(cmd)
        .output()
        .ok()?;
    
    let path = String::from_utf8_lossy(&which_output.stdout)
        .lines()
        .next()
        .unwrap_or(cmd)
        .trim()
        .to_string();
    
    // 生成 ID
    let id = format!("system_{}", cmd.replace('.', "_"));
    
    Some(PythonEnv {
        id,
        name: format!("Python {} (系统)", version_num.as_deref().unwrap_or("未知")),
        path,
        version: version_num,
        is_system: true,
        is_venv: false,
        venv_path: None,
    })
}

/// 检查指定路径的 Python
async fn check_python_path(path: &PathBuf) -> Option<PythonEnv> {
    let output = Command::new(path)
        .args(&["--version"])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let version_str = String::from_utf8_lossy(&output.stdout);
    let version_str = version_str.trim();
    let version = if version_str.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        version_str.to_string()
    };
    
    let version_num = version.split_whitespace()
        .nth(1)
        .map(|s| s.to_string());
    
    let path_str = path.to_string_lossy().to_string();
    let id = format!("system_{}", path_str.replace(['/', '\\', ':', '.'], "_"));
    
    Some(PythonEnv {
        id,
        name: format!("Python {} (系统)", version_num.as_deref().unwrap_or("未知")),
        path: path_str,
        version: version_num,
        is_system: true,
        is_venv: false,
        venv_path: None,
    })
}

/// 创建 venv 虚拟环境
#[tauri::command]
pub async fn create_venv(
    name: String,
    base_python_path: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    // 确定基础 Python
    let base_python = base_python_path.unwrap_or_else(|| "python".to_string());
    
    // 创建 venv 目录（在应用数据目录下）
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {}", e))?;
    
    let venv_dir = app_data_dir.join("venvs").join(&name);
    
    // 确保父目录存在
    std::fs::create_dir_all(&venv_dir)
        .map_err(|e| format!("创建目录失败: {}", e))?;
    
    println!("[PythonEnv] 创建 venv: {:?} 使用 {}", venv_dir, base_python);
    
    // 创建 venv
    let output = Command::new(&base_python)
        .args(&["-m", "venv", &venv_dir.to_string_lossy().to_string()])
        .output()
        .map_err(|e| format!("创建 venv 失败: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("创建 venv 失败: {}", stderr));
    }
    
    println!("[PythonEnv] venv 创建成功: {:?}", venv_dir);
    
    Ok(venv_dir.to_string_lossy().to_string())
}

/// 扫描应用数据目录下的 venvs
#[tauri::command]
pub async fn scan_app_venvs(app_handle: tauri::AppHandle) -> Result<Vec<PythonEnv>, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {}", e))?;
    
    let venvs_dir = app_data_dir.join("venvs");
    
    if !venvs_dir.exists() {
        return Ok(vec![]);
    }
    
    let mut envs = vec![];
    
    if let Ok(entries) = std::fs::read_dir(&venvs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("venv")
                .to_string();
            
            // 检查是否是有效的 venv
            #[cfg(windows)]
            let python_exe = path.join("Scripts").join("python.exe");
            #[cfg(not(windows))]
            let python_exe = path.join("bin").join("python");
            
            if !python_exe.exists() {
                continue;
            }
            
            // 获取版本
            let version = check_python_path(&python_exe).await.map(|e| e.version).flatten();
            
            let id = format!("venv_{}", name.replace(['/', '\\', ':', '.', ' '], "_"));
            
            envs.push(PythonEnv {
                id,
                name: format!("{} (venv)", name),
                path: python_exe.to_string_lossy().to_string(),
                version,
                is_system: false,
                is_venv: true,
                venv_path: Some(path.to_string_lossy().to_string()),
            });
        }
    }
    
    Ok(envs)
}

/// 删除 venv
#[tauri::command]
pub async fn delete_venv(venv_path: String) -> Result<(), String> {
    let path = PathBuf::from(&venv_path);
    
    if !path.exists() {
        return Ok(());
    }
    
    // 递归删除目录
    std::fs::remove_dir_all(&path)
        .map_err(|e| format!("删除 venv 失败: {}", e))?;
    
    println!("[PythonEnv] venv 已删除: {}", venv_path);
    Ok(())
}

/// pip 安装包
#[tauri::command]
pub async fn pip_install_package(python_path: String, package_name: String) -> Result<String, String> {
    println!("[PythonEnv] 安装包: {} 使用 {}", package_name, python_path);
    
    let output = Command::new(&python_path)
        .args(&["-m", "pip", "install", &package_name])
        .output()
        .map_err(|e| format!("运行 pip 失败: {}", e))?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    if !output.status.success() {
        return Err(format!("安装失败: {}", stderr));
    }
    
    Ok(format!("{}{}", stdout, stderr))
}

/// pip 卸载包
#[tauri::command]
pub async fn pip_uninstall_package(python_path: String, package_name: String) -> Result<String, String> {
    println!("[PythonEnv] 卸载包: {} 使用 {}", package_name, python_path);
    
    let output = Command::new(&python_path)
        .args(&["-m", "pip", "uninstall", "-y", &package_name])
        .output()
        .map_err(|e| format!("运行 pip 失败: {}", e))?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    if !output.status.success() {
        return Err(format!("卸载失败: {}", stderr));
    }
    
    Ok(format!("{}{}", stdout, stderr))
}

/// 获取已安装的包列表
#[tauri::command]
pub async fn pip_list_packages(python_path: String) -> Result<Vec<String>, String> {
    let output = Command::new(&python_path)
        .args(&["-m", "pip", "list", "--format=freeze"])
        .output()
        .map_err(|e| format!("运行 pip list 失败: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("获取包列表失败: {}", stderr));
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let packages: Vec<String> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();
    
    Ok(packages)
}
