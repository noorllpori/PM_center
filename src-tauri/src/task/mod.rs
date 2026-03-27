// 任务执行系统 - Python only
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::process_utils::tokio_command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskScript {
    pub code: String,
    pub r#type: String, // 保留字段兼容性，但只支持 "python"
    pub interpreter: Option<String>,
    pub working_dir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
}

// 全局任务进程管理器
type TaskProcesses = Arc<Mutex<HashMap<String, tokio::process::Child>>>;

lazy_static::lazy_static! {
    static ref TASK_PROCESSES: TaskProcesses = Arc::new(Mutex::new(HashMap::new()));
}

fn decode_process_output(bytes: &[u8]) -> String {
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => text,
        Err(_) => {
            #[cfg(windows)]
            {
                let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
                decoded.into_owned()
            }

            #[cfg(not(windows))]
            {
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
    }
}

/// 运行任务
#[tauri::command]
pub async fn run_task(
    task_id: String,
    script: TaskScript,
    timeout_seconds: u64,
    python_path: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    println!("[Task] 开始运行任务: {}, 超时: {}秒", task_id, timeout_seconds);
    
    // 确定使用的 Python 解释器
    let python_exe = python_path.unwrap_or_else(|| "python".to_string());
    println!("[Task] 使用 Python: {}", python_exe);
    
    // 创建临时脚本文件
    let temp_dir = std::env::temp_dir();
    println!("[Task] 临时目录: {:?}", temp_dir);
    
    let script_path = match create_python_script(&temp_dir, &script.code).await {
        Ok(path) => {
            println!("[Task] Python 脚本创建成功: {:?}", path);
            path
        }
        Err(e) => {
            println!("[Task] 脚本文件创建失败: {}", e);
            let _ = app_handle.emit(
                "task-error",
                serde_json::json!({
                    "taskId": task_id,
                    "error": format!("创建脚本文件失败: {}", e),
                }),
            );
            return Err(e);
        }
    };

    // 构建命令 - 使用指定的 Python
    let mut cmd = tokio_command(&python_exe);
    cmd.arg(&script_path);

    // 设置工作目录
    if let Some(work_dir) = &script.working_dir {
        println!("[Task] 设置工作目录: {}", work_dir);
        cmd.current_dir(work_dir);
    }

    // 尽量统一 Python 管道输出编码，避免中文输出在 Windows 上退回到 GBK。
    cmd.env("PYTHONIOENCODING", "utf-8");
    cmd.env("PYTHONUTF8", "1");

    // 设置环境变量
    if let Some(env_vars) = &script.env_vars {
        for (key, value) in env_vars {
            println!("[Task] 设置环境变量: {}={}", key, value);
            cmd.env(key, value);
        }
    }

    // 配置管道
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    println!("[Task] 准备启动进程...");
    
    // 检查生成的脚本文件内容
    match tokio::fs::read_to_string(&script_path).await {
        Ok(content) => {
            let preview: String = content.chars().take(200).collect();
            println!("[Task] 脚本文件内容预览:\n{}", preview);
        }
        Err(e) => println!("[Task] 无法读取脚本文件: {}", e),
    }

    // 启动进程
    println!("[Task] 正在启动进程: {:?}", cmd);
    let mut child = match cmd.spawn() {
        Ok(c) => {
            println!("[Task] 进程启动成功, PID: {:?}", c.id());
            c
        }
        Err(e) => {
            println!("[Task] 进程启动失败: {}", e);
            let _ = app_handle.emit(
                "task-error",
                serde_json::json!({
                    "taskId": task_id,
                    "error": format!("启动任务失败: {}", e),
                }),
            );
            return Err(format!("启动任务失败: {}", e));
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            println!("[Task] 无法获取 stdout");
            let _ = app_handle.emit(
                "task-error",
                serde_json::json!({
                    "taskId": task_id,
                    "error": "无法获取 stdout",
                }),
            );
            return Err("无法获取 stdout".to_string());
        }
    };
    
    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => {
            println!("[Task] 无法获取 stderr");
            let _ = app_handle.emit(
                "task-error",
                serde_json::json!({
                    "taskId": task_id,
                    "error": "无法获取 stderr",
                }),
            );
            return Err("无法获取 stderr".to_string());
        }
    };

    // 保存进程
    {
        let mut processes = TASK_PROCESSES.lock().unwrap();
        processes.insert(task_id.clone(), child);
        println!("[Task] 进程已保存到管理器");
    }

    // 读取 stdout
    let task_id_clone = task_id.clone();
    let app_handle_clone = app_handle.clone();
    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        
        println!("[Task] 开始读取 stdout...");
        loop {
            buffer.clear();

            match reader.read_until(b'\n', &mut buffer).await {
                Ok(0) => break,
                Ok(_) => {
                    let line = decode_process_output(&buffer)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();

                    println!("[Task] [{}] stdout: {}", task_id_clone, line);
                    let _ = app_handle_clone.emit(
                        "task-output",
                        serde_json::json!({
                            "taskId": task_id_clone,
                            "line": line,
                        }),
                    );
                }
                Err(error) => {
                    println!("[Task] [{}] stdout 读取失败: {}", task_id_clone, error);
                    let _ = app_handle_clone.emit(
                        "task-output",
                        serde_json::json!({
                            "taskId": task_id_clone,
                            "line": format!("[stdout-read-error] {}", error),
                        }),
                    );
                    break;
                }
            }
        }
        println!("[Task] stdout 读取结束");
    });

    // 读取 stderr（使用阻塞读取确保完整）
    let task_id_clone = task_id.clone();
    let app_handle_clone = app_handle.clone();
    let stderr_handle = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut stderr_reader = stderr;
        let mut stderr_buffer = Vec::new();
        
        println!("[Task] 开始读取 stderr...");
        // 读取所有 stderr 数据
        if let Ok(_) = stderr_reader.read_to_end(&mut stderr_buffer).await {
            if !stderr_buffer.is_empty() {
                let stderr_text = decode_process_output(&stderr_buffer);
                println!("[Task] [{}] 完整 stderr:\n{}", task_id_clone, stderr_text);
                
                // 按行发送
                for line in stderr_text.lines() {
                    let _ = app_handle_clone.emit(
                        "task-output",
                        serde_json::json!({
                            "taskId": task_id_clone,
                            "line": format!("[stderr] {}", line),
                        }),
                    );
                }
            }
        }
        println!("[Task] stderr 读取结束");
    });

    // 等待读取完成
    println!("[Task] 等待输出读取完成...");
    let _ = tokio::join!(stdout_handle, stderr_handle);
    println!("[Task] 输出读取已完成");

    // 等待进程完成（带超时）
    let task_id_clone = task_id.clone();
    let exit_code = if timeout_seconds > 0 {
        println!("[Task] 等待进程完成（超时: {}秒）...", timeout_seconds);
        // 有超时
        let timeout_future = tokio::time::timeout(
            Duration::from_secs(timeout_seconds),
            wait_for_process(&task_id_clone)
        ).await;
        
        match timeout_future {
            Ok(code) => {
                let code = code?;
                println!("[Task] 进程正常完成, 退出码: {}", code);
                code
            }
            Err(_) => {
                // 超时，杀死进程
                println!("[Task] 任务执行超时，正在终止...");
                kill_process(&task_id_clone).await;
                let _ = app_handle.emit(
                    "task-error",
                    serde_json::json!({
                        "taskId": task_id,
                        "error": "任务执行超时",
                    }),
                );
                return Err("任务执行超时".to_string());
            }
        }
    } else {
        // 无超时
        println!("[Task] 等待进程完成（无超时）...");
        let code = wait_for_process(&task_id_clone).await?;
        println!("[Task] 进程完成, 退出码: {}", code);
        code
    };

    // 发送完成事件
    println!("[Task] 发送完成事件, exit_code: {}", exit_code);
    let _ = app_handle.emit(
        "task-completed",
        serde_json::json!({
            "taskId": task_id,
            "exitCode": exit_code,
        }),
    );

    // 清理临时文件
    let _ = tokio::fs::remove_file(&script_path).await;
    println!("[Task] 任务结束: {}", task_id);

    Ok(())
}

// 等待进程完成
async fn wait_for_process(task_id: &str) -> Result<i32, String> {
    let child_opt = {
        let mut processes = TASK_PROCESSES.lock().unwrap();
        processes.remove(task_id)
    };
    
    if let Some(mut child) = child_opt {
        match child.wait().await {
            Ok(status) => Ok(status.code().unwrap_or(-1)),
            Err(e) => Err(format!("等待进程失败: {}", e)),
        }
    } else {
        Err("任务未找到".to_string())
    }
}

// 杀死进程
async fn kill_process(task_id: &str) {
    let child_opt = {
        let mut processes = TASK_PROCESSES.lock().unwrap();
        processes.remove(task_id)
    };
    
    if let Some(mut child) = child_opt {
        let _ = child.kill().await;
    }
}

/// 取消任务
#[tauri::command]
pub async fn cancel_task(task_id: String) -> Result<(), String> {
    println!("[Task] 取消任务: {}", task_id);
    kill_process(&task_id).await;
    Ok(())
}

/// 创建 Python 临时脚本文件
async fn create_python_script(
    temp_dir: &PathBuf,
    code: &str,
) -> Result<PathBuf, String> {
    let filename = format!("task_{}.py", uuid::Uuid::new_v4());
    let script_path = temp_dir.join(&filename);
    println!("[Task] 创建 Python 脚本文件: {:?}", script_path);
    
    // 转换换行符为 CRLF (Windows 标准)
    let code_crlf = code.replace("\n", "\r\n").replace("\r\r\n", "\r\n");
    
    let mut file = File::create(&script_path)
        .await
        .map_err(|e| format!("创建脚本文件失败: {}", e))?;
    
    // 写入 UTF-8 BOM (让 Windows 正确识别中文)
    file.write_all(&[0xEF, 0xBB, 0xBF]).await
        .map_err(|e| format!("写入 BOM 失败: {}", e))?;
    file.write_all(code_crlf.as_bytes())
        .await
        .map_err(|e| format!("写入脚本文件失败: {}", e))?;

    println!("[Task] Python 脚本文件创建成功");
    Ok(script_path)
}
