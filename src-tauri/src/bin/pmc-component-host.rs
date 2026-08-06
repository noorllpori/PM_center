use serde_json::{json, Value};
use std::env;
use std::io::{self, BufRead, Write};

const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[cfg(target_os = "windows")]
fn main() {
    use std::mem::transmute;
    use windows::core::{PCSTR, PCWSTR};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    let dll = argument("--dll").unwrap_or_default();
    if dll.is_empty() {
        emit(json!({ "type": "error", "code": "HOST_ARGUMENT_INVALID", "error": "缺少 --dll" }));
        std::process::exit(2);
    }
    let wide = dll
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let module = match unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) } {
        Ok(module) => module,
        Err(_) => {
            emit(
                json!({ "type": "error", "code": "DLL_LOAD_FAILED", "error": format!("无法加载 DLL: {dll}") }),
            );
            std::process::exit(3);
        }
    };
    if module.0 == 0 {
        emit(
            json!({ "type": "error", "code": "DLL_LOAD_FAILED", "error": format!("无法加载 DLL: {dll}") }),
        );
        std::process::exit(3);
    }

    let abi_name = b"nexora_component_abi_v1\0";
    let abi = unsafe { GetProcAddress(module, PCSTR(abi_name.as_ptr())) };
    let Some(abi) = abi else {
        emit(
            json!({ "type": "error", "code": "ABI_SYMBOL_MISSING", "error": "DLL 缺少 nexora_component_abi_v1 导出" }),
        );
        std::process::exit(4);
    };
    let abi_version = unsafe {
        let function: unsafe extern "system" fn() -> u32 = transmute(abi);
        function()
    };
    if abi_version != 1 {
        emit(
            json!({ "type": "error", "code": "ABI_VERSION_UNSUPPORTED", "abiVersion": abi_version, "error": "不支持的 Nexora 组件 ABI 版本" }),
        );
        std::process::exit(5);
    }

    emit(
        json!({ "type": "host-ready", "protocol": "nexora.component.host.v1", "abiVersion": abi_version }),
    );
    let invoke_name = b"nexora_component_invoke_v1\0";
    let invoke = unsafe { GetProcAddress(module, PCSTR(invoke_name.as_ptr())) };
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            emit(
                json!({ "type": "error", "code": "HOST_PROTOCOL_INVALID", "error": "请求不是有效 JSON" }),
            );
            continue;
        };
        match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "ping" | "health" => {
                emit(json!({ "type": "health", "ok": true, "abiVersion": abi_version }))
            }
            "shutdown" => break,
            "initialize" => emit(
                json!({ "type": "initialized", "componentId": value.get("componentId"), "apiVersion": value.get("apiVersion") }),
            ),
            "invoke" => {
                let Some(invoke) = invoke else {
                    emit(
                        json!({ "type": "error", "code": "ABI_INVOKE_SYMBOL_MISSING", "error": "DLL 缺少 nexora_component_invoke_v1 导出" }),
                    );
                    continue;
                };
                let mut request = value.clone();
                if let Some(object) = request.as_object_mut() {
                    object.remove("type");
                }
                let request_bytes = match serde_json::to_vec(&request) {
                    Ok(bytes) if bytes.len() <= MAX_REQUEST_BYTES => bytes,
                    Ok(_) => {
                        emit(
                            json!({ "type": "error", "code": "ABI_REQUEST_TOO_LARGE", "error": "组件调用请求超过 8 MiB 限制" }),
                        );
                        continue;
                    }
                    Err(error) => {
                        emit(
                            json!({ "type": "error", "code": "HOST_PROTOCOL_INVALID", "error": error.to_string() }),
                        );
                        continue;
                    }
                };
                let mut response = vec![0u8; MAX_RESPONSE_BYTES];
                let result = unsafe {
                    let function: unsafe extern "system" fn(
                        *const u8,
                        usize,
                        *mut u8,
                        usize,
                    ) -> i64 = transmute(invoke);
                    function(
                        request_bytes.as_ptr(),
                        request_bytes.len(),
                        response.as_mut_ptr(),
                        response.len(),
                    )
                };
                if result < 0 || result as usize > response.len() {
                    emit(
                        json!({ "type": "error", "code": "ABI_INVOKE_FAILED", "error": format!("组件调用失败，返回码 {result}") }),
                    );
                    continue;
                }
                let output = match serde_json::from_slice::<Value>(&response[..result as usize]) {
                    Ok(value) => value,
                    Err(error) => {
                        emit(
                            json!({ "type": "error", "code": "ABI_RESPONSE_INVALID", "error": format!("组件返回不是有效 JSON: {error}") }),
                        );
                        continue;
                    }
                };
                emit(json!({ "type": "result", "result": output }));
            }
            _ => emit(
                json!({ "type": "error", "code": "HOST_PROTOCOL_INVALID", "error": "未知宿主命令" }),
            ),
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    emit(
        json!({ "type": "error", "code": "HOST_PLATFORM_UNSUPPORTED", "error": "当前隔离宿主第一版只支持 Windows" }),
    );
    std::process::exit(1);
}

fn argument(name: &str) -> Option<String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args.next();
        }
    }
    None
}

fn emit(value: Value) {
    let _ = writeln!(io::stdout(), "{}", value);
    let _ = io::stdout().flush();
}
