use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, RwLock},
    time::Duration,
};
use tauri::{Emitter, Manager};
use tokio::sync::oneshot;
use uuid::Uuid;

pub const LOCAL_WEB_CONSOLE_MODULE_ID: &str = "builtin.local-web-console";
pub const LOCAL_WEB_CONSOLE_TOOL_ID: &str = "builtin.local-web-console.tool";
pub const LOCAL_WEB_SETTINGS_CHANGED_EVENT: &str = "pm-center:local-web-settings-changed";
const DEFAULT_PORT: u16 = 31530;
const CONFIG_FILE_NAME: &str = "local-web-console.json";
const SETTINGS_FILE_NAME: &str = "settings.json";

static RUNTIME: OnceLock<Arc<RwLock<Option<RuntimeInfo>>>> = OnceLock::new();
static SETTINGS_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn runtime_state() -> &'static Arc<RwLock<Option<RuntimeInfo>>> {
    RUNTIME.get_or_init(|| Arc::new(RwLock::new(None)))
}

fn settings_write_lock() -> &'static Mutex<()> {
    SETTINGS_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Clone)]
struct RuntimeInfo {
    address: SocketAddr,
    launch_url: String,
    started_at: i64,
    token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredConfig {
    #[serde(default = "default_port")]
    preferred_port: u16,
    #[serde(default = "default_true")]
    allow_settings_write: bool,
    #[serde(default = "default_true")]
    allow_restart: bool,
    #[serde(default = "default_true")]
    allow_exit: bool,
    #[serde(default)]
    access_token: String,
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self {
            preferred_port: DEFAULT_PORT,
            allow_settings_write: true,
            allow_restart: true,
            allow_exit: true,
            access_token: Uuid::new_v4().to_string(),
        }
    }
}

const fn default_port() -> u16 {
    DEFAULT_PORT
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalWebConsoleConfig {
    pub preferred_port: u16,
    pub allow_settings_write: bool,
    pub allow_restart: bool,
    pub allow_exit: bool,
}

impl From<&StoredConfig> for LocalWebConsoleConfig {
    fn from(value: &StoredConfig) -> Self {
        Self {
            preferred_port: value.preferred_port,
            allow_settings_write: value.allow_settings_write,
            allow_restart: value.allow_restart,
            allow_exit: value.allow_exit,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalWebConsoleStatus {
    pub running: bool,
    pub address: Option<String>,
    pub launch_url: Option<String>,
    pub started_at: Option<i64>,
    pub config: LocalWebConsoleConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLocalWebConsoleConfigRequest {
    pub preferred_port: u16,
    pub allow_settings_write: bool,
    pub allow_restart: bool,
    pub allow_exit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebEditableSettings {
    auto_open_last_project: bool,
    confirm_project_tab_close: bool,
    confirm_file_tab_close: bool,
    projects_root_dir: Option<String>,
}

#[derive(Clone)]
struct WebState {
    app_handle: tauri::AppHandle,
    app_data_dir: PathBuf,
    token: String,
    config: LocalWebConsoleConfig,
    address: SocketAddr,
    started_at: i64,
}

pub struct LocalWebConsoleServer {
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<Result<(), String>>,
    token: String,
}

impl LocalWebConsoleServer {
    pub async fn shutdown(mut self) -> Result<(), String> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let result = tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .map_err(|_| "等待本机 Web 控制台停止超时".to_string())?
            .map_err(|error| format!("等待本机 Web 控制台任务失败: {error}"))?;
        clear_runtime_if_token_matches(&self.token);
        result
    }
}

pub async fn start_server(
    app_handle: tauri::AppHandle,
    app_data_dir: PathBuf,
) -> Result<LocalWebConsoleServer, String> {
    if is_running() {
        return Err("本机 Web 控制台已经在运行".into());
    }
    let stored = load_or_create_config(&app_data_dir)?;
    let port = stored.preferred_port;
    if port == 0 {
        return Err("网页控制台端口不能为 0".into());
    }
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|error| format!("绑定本机 Web 控制台 127.0.0.1:{port} 失败: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("读取本机 Web 控制台地址失败: {error}"))?;
    let config = LocalWebConsoleConfig::from(&stored);
    let token = stored.access_token.clone();
    let launch_url = format!("http://127.0.0.1:{}/#token={token}", address.port());
    let started_at = chrono::Utc::now().timestamp_millis();
    let state = WebState {
        app_handle,
        app_data_dir,
        token: token.clone(),
        config,
        address,
        started_at,
    };
    let router = Router::new()
        .route("/", get(index_page))
        .route("/health", get(health))
        .route("/assets/nexora-logo.png", get(logo))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/settings", put(update_settings))
        .route("/api/window/show", post(show_window))
        .route("/api/window/hide", post(hide_window))
        .route("/api/app/restart", post(restart_app))
        .route("/api/app/exit", post(exit_app))
        .layer(middleware::from_fn(security_headers))
        .with_state(state);
    *runtime_state()
        .write()
        .map_err(|_| "本机 Web 控制台状态锁损坏".to_string())? = Some(RuntimeInfo {
        address,
        launch_url,
        started_at,
        token: token.clone(),
    });
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task_token = token.clone();
    let task = tokio::spawn(async move {
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .map_err(|error| format!("本机 Web 控制台服务异常退出: {error}"));
        clear_runtime_if_token_matches(&task_token);
        result
    });
    Ok(LocalWebConsoleServer {
        shutdown: Some(shutdown_tx),
        task,
        token,
    })
}

fn clear_runtime_if_token_matches(token: &str) {
    if let Ok(mut guard) = runtime_state().write() {
        if guard.as_ref().is_some_and(|info| info.token == token) {
            *guard = None;
        }
    }
}

pub fn is_running() -> bool {
    runtime_state()
        .read()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
}

pub fn runtime_address() -> Option<String> {
    runtime_state().read().ok().and_then(|guard| {
        guard
            .as_ref()
            .map(|info| format!("http://{}", info.address))
    })
}

#[tauri::command]
pub fn get_local_web_console_status(
    app_handle: tauri::AppHandle,
) -> Result<LocalWebConsoleStatus, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("读取应用数据目录失败: {error}"))?;
    let config = load_or_create_config(&app_data_dir)?;
    let runtime = runtime_state()
        .read()
        .map_err(|_| "本机 Web 控制台状态锁损坏".to_string())?
        .clone();
    Ok(LocalWebConsoleStatus {
        running: runtime.is_some(),
        address: runtime
            .as_ref()
            .map(|info| format!("http://{}", info.address)),
        launch_url: runtime.as_ref().map(|info| info.launch_url.clone()),
        started_at: runtime.as_ref().map(|info| info.started_at),
        config: LocalWebConsoleConfig::from(&config),
    })
}

#[tauri::command]
pub fn update_local_web_console_config(
    app_handle: tauri::AppHandle,
    request: UpdateLocalWebConsoleConfigRequest,
) -> Result<LocalWebConsoleStatus, String> {
    if request.preferred_port < 1024 {
        return Err("网页控制台端口必须在 1024-65535 之间".into());
    }
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("读取应用数据目录失败: {error}"))?;
    let mut stored = load_or_create_config(&app_data_dir)?;
    stored.preferred_port = request.preferred_port;
    stored.allow_settings_write = request.allow_settings_write;
    stored.allow_restart = request.allow_restart;
    stored.allow_exit = request.allow_exit;
    persist_config(&app_data_dir, &stored)?;
    get_local_web_console_status(app_handle)
}

#[tauri::command]
pub fn open_local_web_console(app_handle: tauri::AppHandle) -> Result<(), String> {
    let launch_url = runtime_state()
        .read()
        .map_err(|_| "本机 Web 控制台状态锁损坏".to_string())?
        .as_ref()
        .map(|info| info.launch_url.clone())
        .ok_or_else(|| "本机 Web 控制台尚未启动".to_string())?;
    tauri_plugin_opener::OpenerExt::opener(&app_handle)
        .open_url(launch_url, None::<&str>)
        .map_err(|error| format!("打开网页控制台失败: {error}"))
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("local_web_console/page.html"))
}

async fn logo() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
        include_bytes!("../../src/assets/nexora-logo.png").as_slice(),
    )
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "app": "Nexora" }))
}

async fn bootstrap(
    State(state): State<WebState>,
    headers: HeaderMap,
) -> Result<Json<Value>, WebError> {
    authorize(&headers, &state)?;
    let settings = read_web_settings(&state.app_data_dir)?;
    let window_visible = state
        .app_handle
        .get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    let (modules, resource_count, profile) = if let Some(runtime) = state
        .app_handle
        .try_state::<crate::platform::PlatformRuntime>(
    ) {
        let overview = runtime.manager.overview();
        let module_values = overview
            .modules
            .iter()
            .filter(|module| !module.diagnostic)
            .map(|module| {
                json!({
                    "id": module.manifest.id,
                    "name": module.manifest.name,
                    "state": module.state,
                    "desiredEnabled": module.desired_enabled,
                    "health": module.health,
                    "resourceCount": module.resources.len(),
                })
            })
            .collect::<Vec<_>>();
        let profile_value = runtime
            .profiles
            .snapshot(
                &overview
                    .modules
                    .iter()
                    .filter(|module| !module.diagnostic)
                    .map(|module| module.manifest.clone())
                    .collect::<Vec<_>>(),
            )
            .ok()
            .map(|snapshot| {
                json!({
                    "id": snapshot.current_profile.id,
                    "name": snapshot.current_profile.name,
                    "revision": snapshot.current_profile.revision,
                })
            });
        (module_values, overview.resource_count, profile_value)
    } else {
        (Vec::new(), 0, None)
    };
    Ok(Json(json!({
        "app": {
            "name": "Nexora",
            "version": env!("CARGO_PKG_VERSION"),
            "windowVisible": window_visible,
            "startedAt": state.started_at,
            "address": format!("http://{}", state.address),
        },
        "permissions": state.config,
        "settings": settings,
        "modules": modules,
        "resourceCount": resource_count,
        "profile": profile,
    })))
}

async fn update_settings(
    State(state): State<WebState>,
    headers: HeaderMap,
    Json(mut settings): Json<WebEditableSettings>,
) -> Result<Json<Value>, WebError> {
    authorize(&headers, &state)?;
    if !state.config.allow_settings_write {
        return Err(WebError::forbidden("网页设置修改已被桌面端关闭"));
    }
    settings.projects_root_dir = settings
        .projects_root_dir
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if settings
        .projects_root_dir
        .as_ref()
        .is_some_and(|path| path.len() > 32_768)
    {
        return Err(WebError::bad_request("项目根目录路径过长"));
    }
    write_web_settings(&state.app_data_dir, &settings)?;
    state
        .app_handle
        .emit(LOCAL_WEB_SETTINGS_CHANGED_EVENT, &settings)
        .map_err(|error| WebError::internal(format!("同步桌面设置失败: {error}")))?;
    Ok(Json(json!({ "ok": true, "settings": settings })))
}

async fn show_window(
    State(state): State<WebState>,
    headers: HeaderMap,
) -> Result<Json<Value>, WebError> {
    authorize(&headers, &state)?;
    if let Some(window) = state.app_handle.get_webview_window("main") {
        window
            .show()
            .map_err(|error| WebError::internal(format!("显示主窗口失败: {error}")))?;
        if window.is_minimized().unwrap_or(false) {
            let _ = window.unminimize();
        }
        let _ = window.set_focus();
    }
    Ok(Json(json!({ "ok": true })))
}

async fn hide_window(
    State(state): State<WebState>,
    headers: HeaderMap,
) -> Result<Json<Value>, WebError> {
    authorize(&headers, &state)?;
    if let Some(window) = state.app_handle.get_webview_window("main") {
        window
            .hide()
            .map_err(|error| WebError::internal(format!("隐藏主窗口失败: {error}")))?;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct ConfirmAction {
    confirmation: String,
}

async fn restart_app(
    State(state): State<WebState>,
    headers: HeaderMap,
    Json(request): Json<ConfirmAction>,
) -> Result<(StatusCode, Json<Value>), WebError> {
    authorize(&headers, &state)?;
    if !state.config.allow_restart {
        return Err(WebError::forbidden("网页重启控制已被桌面端关闭"));
    }
    if request.confirmation != "restart" {
        return Err(WebError::bad_request("缺少重启确认"));
    }
    let app = state.app_handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(350)).await;
        crate::restart_application(app).await;
    });
    Ok((StatusCode::ACCEPTED, Json(json!({ "accepted": true }))))
}

async fn exit_app(
    State(state): State<WebState>,
    headers: HeaderMap,
    Json(request): Json<ConfirmAction>,
) -> Result<(StatusCode, Json<Value>), WebError> {
    authorize(&headers, &state)?;
    if !state.config.allow_exit {
        return Err(WebError::forbidden("网页退出控制已被桌面端关闭"));
    }
    if request.confirmation != "exit" {
        return Err(WebError::bad_request("缺少退出确认"));
    }
    let app = state.app_handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(350)).await;
        crate::shutdown_application(app).await;
    });
    Ok((StatusCode::ACCEPTED, Json(json!({ "accepted": true }))))
}

fn authorize(headers: &HeaderMap, state: &WebState) -> Result<(), WebError> {
    let expected = format!("Bearer {}", state.token);
    let actual = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if actual != Some(expected.as_str()) {
        return Err(WebError::unauthorized(
            "访问令牌无效，请从 Nexora 重新打开网页控制台",
        ));
    }
    Ok(())
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
    );
    response
}

fn config_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(CONFIG_FILE_NAME)
}

fn load_or_create_config(app_data_dir: &Path) -> Result<StoredConfig, String> {
    fs::create_dir_all(app_data_dir).map_err(|error| format!("创建应用数据目录失败: {error}"))?;
    let path = config_path(app_data_dir);
    if !path.exists() {
        let config = StoredConfig::default();
        persist_config(app_data_dir, &config)?;
        return Ok(config);
    }
    let raw =
        fs::read_to_string(&path).map_err(|error| format!("读取网页控制台配置失败: {error}"))?;
    let mut config: StoredConfig =
        serde_json::from_str(&raw).map_err(|error| format!("解析网页控制台配置失败: {error}"))?;
    if config.access_token.trim().is_empty() {
        config.access_token = Uuid::new_v4().to_string();
        persist_config(app_data_dir, &config)?;
    }
    Ok(config)
}

fn persist_config(app_data_dir: &Path, config: &StoredConfig) -> Result<(), String> {
    fs::create_dir_all(app_data_dir).map_err(|error| format!("创建应用数据目录失败: {error}"))?;
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("序列化网页控制台配置失败: {error}"))?;
    atomic_write(&config_path(app_data_dir), &bytes)
}

fn read_web_settings(app_data_dir: &Path) -> Result<WebEditableSettings, WebError> {
    let path = app_data_dir.join(SETTINGS_FILE_NAME);
    let value = if path.exists() {
        let raw = fs::read_to_string(&path)
            .map_err(|error| WebError::internal(format!("读取设置失败: {error}")))?;
        serde_json::from_str::<Value>(&raw)
            .map_err(|error| WebError::internal(format!("解析设置失败: {error}")))?
    } else {
        Value::Object(Map::new())
    };
    Ok(WebEditableSettings {
        auto_open_last_project: value
            .get("autoOpenLastProject")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        confirm_project_tab_close: value
            .get("confirmProjectTabClose")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        confirm_file_tab_close: value
            .get("confirmFileTabClose")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        projects_root_dir: value
            .get("projectsRootDir")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn write_web_settings(app_data_dir: &Path, settings: &WebEditableSettings) -> Result<(), WebError> {
    let _guard = settings_write_lock()
        .lock()
        .map_err(|_| WebError::internal("设置写入锁损坏"))?;
    let path = app_data_dir.join(SETTINGS_FILE_NAME);
    let mut value = if path.exists() {
        let raw = fs::read_to_string(&path)
            .map_err(|error| WebError::internal(format!("读取设置失败: {error}")))?;
        serde_json::from_str::<Value>(&raw)
            .map_err(|error| WebError::internal(format!("解析设置失败: {error}")))?
    } else {
        Value::Object(Map::new())
    };
    let object = value
        .as_object_mut()
        .ok_or_else(|| WebError::internal("设置文件根节点不是对象"))?;
    object.insert(
        "autoOpenLastProject".into(),
        Value::Bool(settings.auto_open_last_project),
    );
    object.insert(
        "confirmProjectTabClose".into(),
        Value::Bool(settings.confirm_project_tab_close),
    );
    object.insert(
        "confirmFileTabClose".into(),
        Value::Bool(settings.confirm_file_tab_close),
    );
    match &settings.projects_root_dir {
        Some(path) => {
            object.insert("projectsRootDir".into(), Value::String(path.clone()));
        }
        None => {
            object.remove("projectsRootDir");
        }
    }
    let bytes = serde_json::to_vec_pretty(&value)
        .map_err(|error| WebError::internal(format!("序列化设置失败: {error}")))?;
    atomic_write(&path, &bytes).map_err(WebError::internal)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "目标文件缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败: {error}"))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config"),
        Uuid::new_v4()
    ));
    fs::write(&temporary, bytes).map_err(|error| format!("写入临时配置失败: {error}"))?;

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from = temporary
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let to = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|error| format!("提交配置文件失败: {error}"))?;
        }
    }

    #[cfg(not(windows))]
    fs::rename(&temporary, path).map_err(|error| format!("提交配置文件失败: {error}"))?;

    Ok(())
}

#[derive(Debug)]
struct WebError {
    status: StatusCode,
    message: String,
}

impl WebError {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_config_generates_persistent_token_and_safe_defaults() {
        let config = StoredConfig::default();
        assert_eq!(config.preferred_port, DEFAULT_PORT);
        assert!(config.allow_settings_write);
        assert!(config.allow_restart);
        assert!(config.allow_exit);
        assert!(!config.access_token.is_empty());
    }

    #[test]
    fn web_settings_update_preserves_unknown_settings() {
        let root = std::env::temp_dir().join(format!("pm-center-web-settings-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join(SETTINGS_FILE_NAME),
            serde_json::to_vec_pretty(&json!({ "unknown": 42, "autoOpenLastProject": true }))
                .unwrap(),
        )
        .unwrap();
        write_web_settings(
            &root,
            &WebEditableSettings {
                auto_open_last_project: false,
                confirm_project_tab_close: true,
                confirm_file_tab_close: false,
                projects_root_dir: Some("D:\\Project".into()),
            },
        )
        .unwrap();
        let saved: Value =
            serde_json::from_slice(&fs::read(root.join(SETTINGS_FILE_NAME)).unwrap()).unwrap();
        assert_eq!(saved["unknown"], 42);
        assert_eq!(saved["autoOpenLastProject"], false);
        assert_eq!(saved["projectsRootDir"], "D:\\Project");
        fs::remove_dir_all(root).unwrap();
    }
}
