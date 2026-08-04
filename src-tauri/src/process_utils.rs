use std::ffi::OsStr;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn std_command(program: impl AsRef<OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    hide_std_command_window(&mut command);
    command
}

pub fn tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    hide_tokio_command_window(&mut command);
    command
}

pub fn terminate_pid_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = std_command("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }

    #[cfg(not(windows))]
    {
        let _ = std_command("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .output();
    }
}

#[cfg(windows)]
fn hide_std_command_window(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_std_command_window(_command: &mut std::process::Command) {}

#[cfg(windows)]
fn hide_tokio_command_window(command: &mut tokio::process::Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_tokio_command_window(_command: &mut tokio::process::Command) {}
