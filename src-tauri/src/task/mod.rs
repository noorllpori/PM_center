use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::plugin::{
    parse_plugin_control_message, prepare_plugin_execution, PluginActionContext,
    PluginActionRunRequest, PluginInteractionResponse,
};
use crate::process_utils::tokio_command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonInlineTaskScript {
    pub code: String,
    pub r#type: String,
    pub interpreter: Option<String>,
    pub working_dir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionTaskScript {
    pub plugin_key: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub command_id: String,
    pub command_title: String,
    pub location: String,
    pub context: PluginActionContext,
    #[serde(default)]
    pub interaction_responses: Vec<PluginInteractionResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TaskScript {
    PythonInline(PythonInlineTaskScript),
    PluginAction(PluginActionTaskScript),
}

#[derive(Debug)]
struct ManagedTaskProcess {
    child: tokio::process::Child,
    cleanup_paths: Vec<PathBuf>,
}

type TaskProcesses = Arc<Mutex<HashMap<String, ManagedTaskProcess>>>;

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

struct PreparedTaskExecution {
    program: String,
    args: Vec<String>,
    working_dir: Option<String>,
    env_vars: HashMap<String, String>,
    cleanup_paths: Vec<PathBuf>,
    parse_plugin_controls: bool,
}

struct OutputReadSummary {
    plugin_error_message: Option<String>,
}

fn default_python_env() -> HashMap<String, String> {
    let mut env_vars = HashMap::new();
    env_vars.insert("PYTHONIOENCODING".to_string(), "utf-8".to_string());
    env_vars.insert("PYTHONUTF8".to_string(), "1".to_string());
    env_vars
}

async fn create_python_script(temp_dir: &PathBuf, code: &str) -> Result<PathBuf, String> {
    let filename = format!("task_{}.py", uuid::Uuid::new_v4());
    let script_path = temp_dir.join(&filename);

    let code_crlf = code.replace("\n", "\r\n").replace("\r\r\n", "\r\n");
    let mut file = File::create(&script_path)
        .await
        .map_err(|error| format!("创建脚本文件失败: {error}"))?;

    file.write_all(&[0xEF, 0xBB, 0xBF])
        .await
        .map_err(|error| format!("写入脚本 BOM 失败: {error}"))?;
    file.write_all(code_crlf.as_bytes())
        .await
        .map_err(|error| format!("写入脚本文件失败: {error}"))?;

    Ok(script_path)
}

async fn cleanup_paths(paths: Vec<PathBuf>) {
    for path in paths {
        let _ = tokio::fs::remove_file(path).await;
    }
}

async fn prepare_task_execution(
    app_handle: &tauri::AppHandle,
    script: &TaskScript,
    python_path: Option<String>,
) -> Result<PreparedTaskExecution, String> {
    match script {
        TaskScript::PythonInline(script) => {
            let python_exe = script
                .interpreter
                .clone()
                .or(python_path)
                .unwrap_or_else(|| "python".to_string());
            let temp_dir = std::env::temp_dir();
            let script_path = create_python_script(&temp_dir, &script.code).await?;

            let mut env_vars = default_python_env();
            if let Some(custom_env_vars) = &script.env_vars {
                for (key, value) in custom_env_vars {
                    env_vars.insert(key.clone(), value.clone());
                }
            }

            Ok(PreparedTaskExecution {
                program: python_exe,
                args: vec![script_path.to_string_lossy().to_string()],
                working_dir: script.working_dir.clone(),
                env_vars,
                cleanup_paths: vec![script_path],
                parse_plugin_controls: false,
            })
        }
        TaskScript::PluginAction(script) => {
            let prepared = prepare_plugin_execution(
                app_handle,
                &PluginActionRunRequest {
                    plugin_key: script.plugin_key.clone(),
                    command_id: script.command_id.clone(),
                    context: script.context.clone(),
                    interaction_responses: script.interaction_responses.clone(),
                },
            )?;

            Ok(PreparedTaskExecution {
                program: prepared.program,
                args: prepared.args,
                working_dir: prepared.working_dir,
                env_vars: prepared.env_vars,
                cleanup_paths: prepared.cleanup_paths,
                parse_plugin_controls: true,
            })
        }
    }
}

async fn wait_for_process(task_id: &str) -> Result<(i32, Vec<PathBuf>), String> {
    let managed_process = {
        let mut processes = TASK_PROCESSES.lock().unwrap();
        processes.remove(task_id)
    };

    if let Some(mut managed_process) = managed_process {
        let cleanup_paths = managed_process.cleanup_paths.clone();
        match managed_process.child.wait().await {
            Ok(status) => Ok((status.code().unwrap_or(-1), cleanup_paths)),
            Err(error) => Err(format!("等待进程失败: {error}")),
        }
    } else {
        Err("任务未找到".to_string())
    }
}

async fn kill_process(task_id: &str) {
    let managed_process = {
        let mut processes = TASK_PROCESSES.lock().unwrap();
        processes.remove(task_id)
    };

    if let Some(mut managed_process) = managed_process {
        let _ = managed_process.child.kill().await;
        cleanup_paths(managed_process.cleanup_paths).await;
    }
}

async fn read_stdout(
    task_id: String,
    stdout: tokio::process::ChildStdout,
    app_handle: tauri::AppHandle,
    parse_plugin_controls: bool,
) -> OutputReadSummary {
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::new();
    let mut plugin_error_message = None;

    loop {
        buffer.clear();

        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) => break,
            Ok(_) => {
                let line = decode_process_output(&buffer)
                    .trim_end_matches(['\r', '\n'])
                    .to_string();

                if parse_plugin_controls {
                    if let Some(control) = parse_plugin_control_message(&line) {
                        if control.r#type == "error" && plugin_error_message.is_none() {
                            plugin_error_message = control.message.clone();
                        }

                        let _ = app_handle.emit(
                            "task-control",
                            serde_json::json!({
                                "taskId": task_id,
                                "message": control,
                            }),
                        );
                        continue;
                    }
                }

                let _ = app_handle.emit(
                    "task-output",
                    serde_json::json!({
                        "taskId": task_id,
                        "line": line,
                    }),
                );
            }
            Err(error) => {
                let _ = app_handle.emit(
                    "task-output",
                    serde_json::json!({
                        "taskId": task_id,
                        "line": format!("[stdout-read-error] {}", error),
                    }),
                );
                break;
            }
        }
    }

    OutputReadSummary {
        plugin_error_message,
    }
}

async fn read_stderr(
    task_id: String,
    stderr: tokio::process::ChildStderr,
    app_handle: tauri::AppHandle,
) {
    let mut stderr_reader = stderr;
    let mut stderr_buffer = Vec::new();

    if stderr_reader.read_to_end(&mut stderr_buffer).await.is_ok() && !stderr_buffer.is_empty() {
        let stderr_text = decode_process_output(&stderr_buffer);
        for line in stderr_text.lines() {
            let _ = app_handle.emit(
                "task-output",
                serde_json::json!({
                    "taskId": task_id,
                    "line": format!("[stderr] {}", line),
                }),
            );
        }
    }
}

#[tauri::command]
pub async fn run_task(
    task_id: String,
    script: TaskScript,
    timeout_seconds: u64,
    python_path: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let prepared = match prepare_task_execution(&app_handle, &script, python_path).await {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = app_handle.emit(
                "task-error",
                serde_json::json!({
                    "taskId": task_id,
                    "error": error,
                }),
            );
            return Err("准备任务执行失败".to_string());
        }
    };

    let mut command = tokio_command(&prepared.program);
    command.args(&prepared.args);
    if let Some(working_dir) = &prepared.working_dir {
        command.current_dir(working_dir);
    }
    for (key, value) in &prepared.env_vars {
        command.env(key, value);
    }
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            cleanup_paths(prepared.cleanup_paths).await;
            let message = format!("启动任务失败: {error}");
            let _ = app_handle.emit(
                "task-error",
                serde_json::json!({
                    "taskId": task_id,
                    "error": message,
                }),
            );
            return Err("启动任务失败".to_string());
        }
    };

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            cleanup_paths(prepared.cleanup_paths).await;
            return Err("无法获取 stdout".to_string());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            cleanup_paths(prepared.cleanup_paths).await;
            return Err("无法获取 stderr".to_string());
        }
    };

    {
        let mut processes = TASK_PROCESSES.lock().unwrap();
        processes.insert(
            task_id.clone(),
            ManagedTaskProcess {
                child,
                cleanup_paths: prepared.cleanup_paths.clone(),
            },
        );
    }

    let stdout_handle = tokio::spawn(read_stdout(
        task_id.clone(),
        stdout,
        app_handle.clone(),
        prepared.parse_plugin_controls,
    ));
    let stderr_handle = tokio::spawn(read_stderr(task_id.clone(), stderr, app_handle.clone()));

    let (mut exit_code, cleanup_paths_to_remove) = if timeout_seconds > 0 {
        match tokio::time::timeout(
            Duration::from_secs(timeout_seconds),
            wait_for_process(&task_id),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                kill_process(&task_id).await;
                let message = "任务执行超时".to_string();
                let _ = app_handle.emit(
                    "task-error",
                    serde_json::json!({
                        "taskId": task_id,
                        "error": message,
                    }),
                );
                return Err("任务执行超时".to_string());
            }
        }
    } else {
        wait_for_process(&task_id).await?
    };

    let stdout_summary = stdout_handle
        .await
        .map_err(|error| format!("读取 stdout 失败: {error}"))?;
    let _ = stderr_handle.await;

    if stdout_summary.plugin_error_message.is_some() && exit_code == 0 {
        exit_code = 1;
    }

    cleanup_paths(cleanup_paths_to_remove).await;

    let _ = app_handle.emit(
        "task-completed",
        serde_json::json!({
            "taskId": task_id,
            "exitCode": exit_code,
        }),
    );

    Ok(())
}

#[tauri::command]
pub async fn cancel_task(task_id: String) -> Result<(), String> {
    kill_process(&task_id).await;
    Ok(())
}
