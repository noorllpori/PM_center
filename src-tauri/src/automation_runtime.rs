use crate::process_utils::terminate_pid_tree;
use std::collections::{BTreeMap, HashMap};
use std::process::Output;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const AUTOMATION_RUNTIME_MODULE_ID: &str = "builtin.automation-runtime";
pub const AUTOMATION_RUNTIME_DISABLED: &str = "AUTOMATION_RUNTIME_MODULE_DISABLED";
pub const AUTOMATION_RUNTIME_STARTING: &str = "AUTOMATION_RUNTIME_MODULE_STARTING";
pub const AUTOMATION_RUNTIME_STOPPING: &str = "AUTOMATION_RUNTIME_MODULE_STOPPING";

const PHASE_DISABLED: u8 = 0;
const PHASE_STARTING: u8 = 1;
const PHASE_RUNNING: u8 = 2;
const PHASE_STOPPING: u8 = 3;

static PHASE: AtomicU8 = AtomicU8::new(PHASE_DISABLED);

#[derive(Debug, Clone)]
struct ManagedProcessRecord {
    pid: u32,
    kind: String,
    label: String,
    started_at: Instant,
}

lazy_static::lazy_static! {
    static ref PROCESSES: Mutex<HashMap<Uuid, ManagedProcessRecord>> =
        Mutex::new(HashMap::new());
}

pub struct ManagedProcessLease {
    id: Uuid,
}

impl Drop for ManagedProcessLease {
    fn drop(&mut self) {
        PROCESSES.lock().unwrap().remove(&self.id);
    }
}

pub fn initialize_lifecycle_control() {
    PHASE.store(PHASE_DISABLED, Ordering::SeqCst);
    PROCESSES.lock().unwrap().clear();
}

pub fn set_initial_desired_enabled(desired_enabled: bool) {
    PHASE.store(
        if desired_enabled {
            PHASE_STARTING
        } else {
            PHASE_DISABLED
        },
        Ordering::SeqCst,
    );
}

pub fn start_runtime() {
    PHASE.store(PHASE_RUNNING, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    PHASE.load(Ordering::SeqCst) == PHASE_RUNNING
}

pub fn ensure_running() -> Result<(), String> {
    match PHASE.load(Ordering::SeqCst) {
        PHASE_RUNNING => Ok(()),
        PHASE_STARTING => Err(format!(
            "{AUTOMATION_RUNTIME_STARTING}: 任务、Python 与插件运行时正在启动"
        )),
        PHASE_STOPPING => Err(format!(
            "{AUTOMATION_RUNTIME_STOPPING}: 任务、Python 与插件运行时正在停止"
        )),
        _ => Err(format!(
            "{AUTOMATION_RUNTIME_DISABLED}: 任务、Python 与插件运行时已停用"
        )),
    }
}

pub async fn wait_until_running() -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match PHASE.load(Ordering::SeqCst) {
            PHASE_RUNNING => return Ok(()),
            PHASE_STARTING if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            _ => return ensure_running(),
        }
    }
}

pub fn register_process(
    pid: u32,
    kind: impl Into<String>,
    label: impl Into<String>,
) -> Result<ManagedProcessLease, String> {
    ensure_running()?;
    let id = Uuid::new_v4();
    let record = ManagedProcessRecord {
        pid,
        kind: kind.into(),
        label: label.into(),
        started_at: Instant::now(),
    };
    let mut processes = PROCESSES.lock().unwrap();
    if !is_running() {
        return Err(format!(
            "{AUTOMATION_RUNTIME_STOPPING}: 运行时已开始停止，拒绝登记新进程"
        ));
    }
    processes.insert(id, record);
    Ok(ManagedProcessLease { id })
}

pub fn active_process_count() -> usize {
    PROCESSES.lock().unwrap().len()
}

pub fn active_process_summary() -> String {
    let processes = PROCESSES.lock().unwrap();
    if processes.is_empty() {
        return "没有活动子进程".into();
    }

    let mut counts = BTreeMap::<String, usize>::new();
    let mut oldest = Duration::ZERO;
    let mut sample = Vec::new();
    for process in processes.values() {
        *counts.entry(process.kind.clone()).or_default() += 1;
        oldest = oldest.max(process.started_at.elapsed());
        if sample.len() < 3 {
            sample.push(format!("{} (PID {})", process.label, process.pid));
        }
    }
    let kinds = counts
        .into_iter()
        .map(|(kind, count)| format!("{kind} {count}"))
        .collect::<Vec<_>>()
        .join("，");
    format!(
        "活动进程 {} 个（{}），最长运行 {} 秒：{}",
        processes.len(),
        kinds,
        oldest.as_secs(),
        sample.join("；")
    )
}

pub async fn run_tokio_output(
    mut command: tokio::process::Command,
    kind: &str,
    label: impl Into<String>,
) -> Result<Output, String> {
    ensure_running()?;
    command.kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("启动进程失败: {error}"))?;
    let pid = child.id().ok_or_else(|| "无法获取子进程 PID".to_string())?;
    let lease = match register_process(pid, kind, label) {
        Ok(lease) => lease,
        Err(error) => {
            terminate_pid_tree(pid);
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
    };
    let result = child
        .wait_with_output()
        .await
        .map_err(|error| format!("等待进程失败: {error}"));
    drop(lease);
    result
}

pub async fn stop_runtime() -> Result<(), String> {
    let previous = PHASE.swap(PHASE_STOPPING, Ordering::SeqCst);
    if previous == PHASE_DISABLED {
        return Ok(());
    }

    crate::task::cancel_all_tasks("自动化运行时已停用");
    terminate_registered_processes();

    let deadline = Instant::now() + Duration::from_secs(8);
    while active_process_count() > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let remaining = active_process_count();
    PHASE.store(PHASE_DISABLED, Ordering::SeqCst);
    if remaining == 0 {
        Ok(())
    } else {
        Err(format!("停止超时，仍有 {remaining} 个自动化子进程未退出"))
    }
}

fn terminate_registered_processes() {
    let pids = PROCESSES
        .lock()
        .unwrap()
        .values()
        .map(|process| process.pid)
        .collect::<Vec<_>>();
    for pid in pids {
        terminate_pid_tree(pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process_utils::tokio_command;

    lazy_static::lazy_static! {
        static ref TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::new(());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn guards_leases_and_stop_cleanup_work_together() {
        let _guard = TEST_LOCK.lock().await;
        initialize_lifecycle_control();
        let error = match register_process(1, "test", "disabled") {
            Ok(_) => panic!("disabled runtime unexpectedly accepted a process"),
            Err(error) => error,
        };
        assert!(error.starts_with(AUTOMATION_RUNTIME_DISABLED));
        assert_eq!(active_process_count(), 0);

        start_runtime();
        let lease = register_process(424_242, "test", "lease").unwrap();
        assert_eq!(active_process_count(), 1);
        drop(lease);
        assert_eq!(active_process_count(), 0);

        #[cfg(windows)]
        let mut command = {
            let mut command = tokio_command("cmd");
            command.args(["/C", "ping", "127.0.0.1", "-n", "30"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = tokio_command("sh");
            command.args(["-c", "sleep 30"]);
            command
        };
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        let process =
            tokio::spawn(
                async move { run_tokio_output(command, "test", "long-running process").await },
            );

        let deadline = Instant::now() + Duration::from_secs(2);
        while active_process_count() == 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(active_process_count(), 1);

        stop_runtime().await.unwrap();
        let output = process.await.unwrap().unwrap();
        assert!(!output.status.success());
        assert_eq!(active_process_count(), 0);
        assert!(ensure_running()
            .unwrap_err()
            .starts_with(AUTOMATION_RUNTIME_DISABLED));
    }
}
