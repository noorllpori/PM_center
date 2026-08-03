use std::fs;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use ::windows::core::{w, PCWSTR};
use ::windows::Win32::Foundation::{
    GlobalFree, BOOL, COLORREF, HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT,
    WPARAM,
};
use ::windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    GetMonitorInfoW, InvalidateRect, MonitorFromPoint, SelectObject, SetBkMode, SetTextColor,
    StretchDIBits, BITMAPINFO, CLIP_DEFAULT_PRECIS, COLORONCOLOR, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_TOP, FF_DONTCARE,
    FW_NORMAL, HBRUSH, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST, OUT_DEFAULT_PRECIS,
    PAINTSTRUCT, PROOF_QUALITY, SRCCOPY, TRANSPARENT,
};
use ::windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, RemoveClipboardFormatListener, SetClipboardData,
};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use ::windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
use ::windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId,
};
use ::windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, RegisterHotKey, SendInput, SetFocus, UnregisterHotKey, INPUT, INPUT_0,
    INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, MOD_CONTROL, MOD_NOREPEAT, VK_CONTROL, VK_DELETE,
    VK_DOWN, VK_ESCAPE, VK_OEM_3, VK_RETURN, VK_UP,
};
use ::windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsChild, LoadCursorW, MoveWindow, PostMessageW,
    PostQuitMessage, RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, UnregisterClassW, CREATESTRUCTW,
    CS_DBLCLKS, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, EN_CHANGE, ES_AUTOHSCROLL, GUITHREADINFO,
    GWLP_USERDATA, HMENU, IDC_ARROW, MSG, SWP_NOZORDER, SW_HIDE, SW_SHOWNORMAL, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_ACTIVATE, WM_CLIPBOARDUPDATE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY,
    WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_NCDESTROY, WM_PAINT, WM_SETFONT, WM_SIZE, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD,
    WS_CLIPCHILDREN, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP,
    WS_THICKFRAME, WS_VISIBLE,
};
use chrono::{Local, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

const WINDOW_CLASS: PCWSTR = w!("PMCenterSmartClipboardWindow");
const WINDOW_TITLE: PCWSTR = w!("智能剪贴板");
const WM_APP_SHOW: u32 = 0x8000 + 21;
const WM_APP_SHUTDOWN: u32 = 0x8000 + 22;
const MAX_ITEMS: usize = 500;
const RETENTION_DAYS: i64 = 30;
const VISIBLE_ITEM_LIMIT: usize = 200;
const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;
const WINDOW_WIDTH: i32 = 460;
const WINDOW_HEIGHT: i32 = 710;
const SEARCH_MARGIN: i32 = 12;
const SEARCH_HEIGHT: i32 = 34;
const LIST_TOP: i32 = SEARCH_MARGIN + SEARCH_HEIGHT + 10;
const ITEM_HEIGHT: i32 = 72;
const FOOTER_HEIGHT: i32 = 28;
const HOTKEY_ID: i32 = 0x504D43;
const EM_SETCUEBANNER: u32 = 0x1501;

static CONTROLLER: OnceLock<Mutex<Option<Controller>>> = OnceLock::new();

struct Controller {
    hwnd: isize,
    thread: thread::JoinHandle<Result<(), String>>,
}

unsafe impl Send for Controller {}
unsafe impl Sync for Controller {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipboardKind {
    Text,
    Image,
    Files,
}

impl ClipboardKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Files => "files",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "image" => Some(Self::Image),
            "files" => Some(Self::Files),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Text => "文本",
            Self::Image => "图像",
            Self::Files => "文件",
        }
    }
}

#[derive(Clone, Debug)]
struct HistoryItem {
    id: String,
    kind: ClipboardKind,
    preview: String,
    text_content: Option<String>,
    file_paths: Vec<String>,
    payload_path: Option<PathBuf>,
    clipboard_format: u32,
    created_at: i64,
}

#[derive(Debug)]
struct CapturedItem {
    kind: ClipboardKind,
    preview: String,
    search_text: String,
    text_content: Option<String>,
    file_paths: Vec<String>,
    payload: Option<Vec<u8>>,
    clipboard_format: u32,
    content_hash: String,
    byte_size: usize,
}

struct HistoryStore {
    connection: Connection,
    payload_dir: PathBuf,
}

impl HistoryStore {
    fn open(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|error| error.to_string())?;
        let payload_dir = root.join("payloads");
        fs::create_dir_all(&payload_dir).map_err(|error| error.to_string())?;

        let connection = Connection::open(root.join("clipboard_history.db"))
            .map_err(|error| error.to_string())?;
        connection
            .busy_timeout(Duration::from_secs(3))
            .map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 CREATE TABLE IF NOT EXISTS clipboard_items (
                   id TEXT PRIMARY KEY,
                   kind TEXT NOT NULL,
                   preview TEXT NOT NULL,
                   search_text TEXT NOT NULL,
                   text_content TEXT,
                   file_paths_json TEXT,
                   payload_path TEXT,
                   clipboard_format INTEGER NOT NULL,
                   content_hash TEXT NOT NULL UNIQUE,
                   byte_size INTEGER NOT NULL DEFAULT 0,
                   created_at INTEGER NOT NULL,
                   last_used_at INTEGER
                 );
                 CREATE INDEX IF NOT EXISTS idx_clipboard_items_created_at
                   ON clipboard_items(created_at DESC);",
            )
            .map_err(|error| error.to_string())?;

        let store = Self {
            connection,
            payload_dir,
        };
        store.cleanup()?;
        Ok(store)
    }

    fn insert(&mut self, captured: CapturedItem) -> Result<(), String> {
        let now = Utc::now().timestamp();
        let existing = self
            .connection
            .query_row(
                "SELECT id, payload_path FROM clipboard_items WHERE content_hash = ?1",
                params![captured.content_hash],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        if let Some((id, _)) = existing {
            self.connection
                .execute(
                    "UPDATE clipboard_items
                     SET preview = ?1, search_text = ?2, created_at = ?3
                     WHERE id = ?4",
                    params![captured.preview, captured.search_text, now, id],
                )
                .map_err(|error| error.to_string())?;
            self.cleanup()?;
            return Ok(());
        }

        let id = Uuid::new_v4().to_string();
        let payload_path = if let Some(payload) = captured.payload {
            let extension = if captured.clipboard_format == CF_DIBV5.0 as u32 {
                "dibv5"
            } else {
                "dib"
            };
            let path = self.payload_dir.join(format!("{}.{}", id, extension));
            fs::write(&path, payload).map_err(|error| error.to_string())?;
            Some(path)
        } else {
            None
        };
        let file_paths_json = if captured.file_paths.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&captured.file_paths).map_err(|error| error.to_string())?)
        };

        let insert_result = self.connection.execute(
            "INSERT INTO clipboard_items (
               id, kind, preview, search_text, text_content, file_paths_json,
               payload_path, clipboard_format, content_hash, byte_size, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                captured.kind.as_str(),
                captured.preview,
                captured.search_text,
                captured.text_content,
                file_paths_json,
                payload_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                captured.clipboard_format,
                captured.content_hash,
                captured.byte_size as i64,
                now,
            ],
        );

        if let Err(error) = insert_result {
            if let Some(path) = payload_path {
                let _ = fs::remove_file(path);
            }
            return Err(error.to_string());
        }

        self.cleanup()
    }

    fn list(&self, query: &str) -> Result<Vec<HistoryItem>, String> {
        let normalized = query.trim().to_lowercase();
        let mut statement = if normalized.is_empty() {
            self.connection
                .prepare(
                    "SELECT id, kind, preview, text_content, file_paths_json,
                            payload_path, clipboard_format, created_at
                     FROM clipboard_items
                     ORDER BY created_at DESC
                     LIMIT ?1",
                )
                .map_err(|error| error.to_string())?
        } else {
            self.connection
                .prepare(
                    "SELECT id, kind, preview, text_content, file_paths_json,
                            payload_path, clipboard_format, created_at
                     FROM clipboard_items
                     WHERE lower(search_text) LIKE ?1 ESCAPE '\\'
                     ORDER BY created_at DESC
                     LIMIT ?2",
                )
                .map_err(|error| error.to_string())?
        };

        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<HistoryItem> {
            let kind_value: String = row.get(1)?;
            let file_paths_json: Option<String> = row.get(4)?;
            Ok(HistoryItem {
                id: row.get(0)?,
                kind: ClipboardKind::from_str(&kind_value).unwrap_or(ClipboardKind::Text),
                preview: row.get(2)?,
                text_content: row.get(3)?,
                file_paths: file_paths_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok())
                    .unwrap_or_default(),
                payload_path: row.get::<_, Option<String>>(5)?.map(PathBuf::from),
                clipboard_format: row.get(6)?,
                created_at: row.get(7)?,
            })
        };

        let rows = if normalized.is_empty() {
            statement
                .query_map(params![VISIBLE_ITEM_LIMIT as i64], map_row)
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
        } else {
            statement
                .query_map(
                    params![
                        format!("%{}%", escape_like(&normalized)),
                        VISIBLE_ITEM_LIMIT as i64
                    ],
                    map_row,
                )
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
        };

        rows.map_err(|error| error.to_string())
    }

    fn touch(&self, id: &str) -> Result<(), String> {
        let now = Utc::now().timestamp();
        self.connection
            .execute(
                "UPDATE clipboard_items SET last_used_at = ?1, created_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn delete(&mut self, id: &str) -> Result<(), String> {
        let payload_path = self
            .connection
            .query_row(
                "SELECT payload_path FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .flatten();
        self.connection
            .execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])
            .map_err(|error| error.to_string())?;
        if let Some(path) = payload_path {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    fn cleanup(&self) -> Result<(), String> {
        let cutoff = Utc::now().timestamp() - RETENTION_DAYS * 24 * 60 * 60;
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, payload_path, created_at
                 FROM clipboard_items
                 ORDER BY created_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(statement);

        for (index, (id, payload_path, created_at)) in rows.into_iter().enumerate() {
            if index < MAX_ITEMS && created_at >= cutoff {
                continue;
            }
            self.connection
                .execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])
                .map_err(|error| error.to_string())?;
            if let Some(path) = payload_path {
                let _ = fs::remove_file(path);
            }
        }
        self.cleanup_orphan_payloads()
    }

    fn cleanup_orphan_payloads(&self) -> Result<(), String> {
        let mut statement = self
            .connection
            .prepare("SELECT payload_path FROM clipboard_items WHERE payload_path IS NOT NULL")
            .map_err(|error| error.to_string())?;
        let referenced = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<std::collections::HashSet<_>, _>>()
            .map_err(|error| error.to_string())?;

        let entries = fs::read_dir(&self.payload_dir).map_err(|error| error.to_string())?;
        for entry in entries {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_file() && !referenced.contains(&path.to_string_lossy().to_string()) {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }
}

struct WindowState {
    hwnd: HWND,
    search_hwnd: HWND,
    font: HFONT,
    store: HistoryStore,
    items: Vec<HistoryItem>,
    selected_index: usize,
    scroll_offset: usize,
    previous_foreground: HWND,
    previous_focus: HWND,
    suppressed_sequence: u32,
    activated_since_show: bool,
    shown_at: Option<Instant>,
}

impl WindowState {
    fn new(store: HistoryStore) -> Self {
        Self {
            hwnd: HWND(0),
            search_hwnd: HWND(0),
            font: HFONT(0),
            store,
            items: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            previous_foreground: HWND(0),
            previous_focus: HWND(0),
            suppressed_sequence: 0,
            activated_since_show: false,
            shown_at: None,
        }
    }

    fn refresh(&mut self) {
        let query = unsafe { window_text(self.search_hwnd) };
        match self.store.list(&query) {
            Ok(items) => self.items = items,
            Err(error) => {
                eprintln!("[smart-clipboard] 读取历史失败: {error}");
                self.items.clear();
            }
        }
        self.selected_index = self.selected_index.min(self.items.len().saturating_sub(1));
        self.ensure_selection_visible();
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, true);
        }
    }

    fn visible_rows(&self) -> usize {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        ((rect.bottom - LIST_TOP - FOOTER_HEIGHT).max(ITEM_HEIGHT) / ITEM_HEIGHT) as usize
    }

    fn ensure_selection_visible(&mut self) {
        let rows = self.visible_rows().max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + rows {
            self.scroll_offset = self.selected_index + 1 - rows;
        }
        let max_offset = self.items.len().saturating_sub(rows);
        self.scroll_offset = self.scroll_offset.min(max_offset);
    }

    fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let max_index = self.items.len() - 1;
        self.selected_index = if delta.is_negative() {
            self.selected_index.saturating_sub(delta.unsigned_abs())
        } else {
            (self.selected_index + delta as usize).min(max_index)
        };
        self.ensure_selection_visible();
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, true);
        }
    }

    fn delete_selected(&mut self) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };
        if let Err(error) = self.store.delete(&item.id) {
            eprintln!("[smart-clipboard] 删除历史失败: {error}");
        }
        self.refresh();
    }

    fn activate_selected(&mut self, auto_paste: bool) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };
        if let Err(error) = restore_item_to_clipboard(self.hwnd, &item) {
            eprintln!("[smart-clipboard] 恢复剪贴板失败: {error}");
            return;
        }
        self.suppressed_sequence = unsafe { GetClipboardSequenceNumber() };
        let _ = self.store.touch(&item.id);
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
        if auto_paste {
            unsafe {
                paste_to_previous_window(self.previous_foreground, self.previous_focus);
            }
        }
    }

    fn capture_current(&mut self) {
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if sequence == 0 || sequence == self.suppressed_sequence {
            self.suppressed_sequence = 0;
            return;
        }

        match capture_clipboard(self.hwnd) {
            Ok(Some(item)) => {
                if let Err(error) = self.store.insert(item) {
                    eprintln!("[smart-clipboard] 保存历史失败: {error}");
                }
                self.refresh();
            }
            Ok(None) => {}
            Err(error) => eprintln!("[smart-clipboard] 捕获剪贴板失败: {error}"),
        }
    }

    fn show(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.activated_since_show = false;
        self.shown_at = Some(Instant::now());
        unsafe {
            self.previous_foreground = GetForegroundWindow();
            self.previous_focus = focused_window(self.previous_foreground);
            let _ = SetWindowTextW(self.search_hwnd, w!(""));
            position_window_near_cursor(self.hwnd);
            ShowWindow(self.hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.hwnd);
            SetFocus(self.search_hwnd);
            self.activated_since_show = GetForegroundWindow() == self.hwnd;
        }
        self.refresh();
    }
}

impl Drop for WindowState {
    fn drop(&mut self) {
        unsafe {
            if self.font.0 != 0 {
                let _ = DeleteObject(self.font);
            }
        }
    }
}

pub fn initialize(app_data_dir: &Path) -> Result<(), String> {
    let controller = CONTROLLER.get_or_init(|| Mutex::new(None));
    let mut controller = controller
        .lock()
        .map_err(|_| "智能剪贴板控制器锁已损坏".to_string())?;
    if let Some(current) = controller.as_ref() {
        if !current.thread.is_finished() {
            return Ok(());
        }
    }
    if let Some(finished) = controller.take() {
        if let Err(error) = join_controller(finished) {
            eprintln!("[smart-clipboard] 回收已退出线程失败，准备重新启动: {error}");
        }
    }

    let root = app_data_dir.join("smart_clipboard");
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let window_thread = thread::Builder::new()
        .name("pm-center-smart-clipboard".to_string())
        .spawn(move || {
            let result = run_window_thread(&root, ready_tx);
            if let Err(error) = &result {
                eprintln!("[smart-clipboard] 原生窗口线程退出: {error}");
            }
            result
        })
        .map_err(|error| error.to_string())?;

    let hwnd = match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => result?,
        Err(_) => {
            if window_thread.is_finished() {
                return join_controller(Controller {
                    hwnd: 0,
                    thread: window_thread,
                });
            }
            return Err("智能剪贴板窗口初始化超时".to_string());
        }
    };
    *controller = Some(Controller {
        hwnd,
        thread: window_thread,
    });
    Ok(())
}

pub fn show() -> Result<(), String> {
    let controller = CONTROLLER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "智能剪贴板控制器锁已损坏".to_string())?;
    let controller = controller
        .as_ref()
        .filter(|controller| !controller.thread.is_finished())
        .ok_or_else(|| "智能剪贴板监听线程未运行".to_string())?;
    unsafe {
        PostMessageW(HWND(controller.hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn is_running() -> bool {
    CONTROLLER
        .get()
        .and_then(|controller| controller.lock().ok())
        .and_then(|controller| {
            controller
                .as_ref()
                .map(|controller| !controller.thread.is_finished())
        })
        .unwrap_or(false)
}

pub fn shutdown() -> Result<(), String> {
    let Some(controller) = CONTROLLER.get() else {
        return Ok(());
    };
    let controller = controller
        .lock()
        .map_err(|_| "智能剪贴板控制器锁已损坏".to_string())?
        .take();
    let Some(controller) = controller else {
        return Ok(());
    };

    if !controller.thread.is_finished() && controller.hwnd != 0 {
        if let Err(error) =
            unsafe { PostMessageW(HWND(controller.hwnd), WM_APP_SHUTDOWN, WPARAM(0), LPARAM(0)) }
        {
            if !controller.thread.is_finished() {
                CONTROLLER
                    .get()
                    .expect("controller initialized")
                    .lock()
                    .map_err(|_| "智能剪贴板控制器锁已损坏".to_string())?
                    .replace(controller);
                return Err(format!("请求智能剪贴板线程退出失败: {error}"));
            }
        }
    }

    let deadline = Instant::now() + Duration::from_secs(3);
    while !controller.thread.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !controller.thread.is_finished() {
        CONTROLLER
            .get()
            .expect("controller initialized")
            .lock()
            .map_err(|_| "智能剪贴板控制器锁已损坏".to_string())?
            .replace(controller);
        return Err("等待智能剪贴板监听线程退出超时".to_string());
    }
    join_controller(controller)
}

fn join_controller(controller: Controller) -> Result<(), String> {
    controller
        .thread
        .join()
        .map_err(|_| "智能剪贴板监听线程发生 panic".to_string())?
}

fn run_window_thread(
    root: &Path,
    ready_tx: std::sync::mpsc::SyncSender<Result<isize, String>>,
) -> Result<(), String> {
    let store = match HistoryStore::open(root) {
        Ok(store) => store,
        Err(error) => {
            let _ = ready_tx.send(Err(error.clone()));
            return Err(error);
        }
    };
    let instance = match unsafe { GetModuleHandleW(None) } {
        Ok(instance) => instance,
        Err(error) => {
            let error = error.to_string();
            let _ = ready_tx.send(Err(error.clone()));
            return Err(error);
        }
    };
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() };
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
        lpfnWndProc: Some(window_proc),
        hInstance: HINSTANCE(instance.0),
        hCursor: cursor,
        lpszClassName: WINDOW_CLASS,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        let error = ::windows::core::Error::from_win32().to_string();
        let _ = ready_tx.send(Err(error.clone()));
        return Err(error);
    }

    let state = Box::new(WindowState::new(store));
    let state_ptr = Box::into_raw(state);
    let style = WINDOW_STYLE(
        WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_CLIPCHILDREN.0,
    );
    let ex_style = WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0);
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            WINDOW_CLASS,
            WINDOW_TITLE,
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            HWND(0),
            HMENU(0),
            HINSTANCE(instance.0),
            Some(state_ptr.cast()),
        )
    };
    if hwnd.0 == 0 {
        unsafe {
            drop(Box::from_raw(state_ptr));
            let _ = UnregisterClassW(WINDOW_CLASS, HINSTANCE(instance.0));
        }
        let error = ::windows::core::Error::from_win32().to_string();
        let _ = ready_tx.send(Err(error.clone()));
        return Err(error);
    }

    let _ = ready_tx.send(Ok(hwnd.0));

    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, HWND(0), 0, 0).as_bool() {
            if handle_keyboard_message(hwnd, &message) {
                continue;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let _ = UnregisterClassW(WINDOW_CLASS, HINSTANCE(instance.0));
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam.0 as *const CREATESTRUCTW);
        let state_ptr = create.lpCreateParams as *mut WindowState;
        if !state_ptr.is_null() {
            (*state_ptr).hwnd = hwnd;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        }
    }

    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if state_ptr.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let state = &mut *state_ptr;

    match message {
        WM_CREATE => {
            state.font = CreateFontW(
                -16,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                PROOF_QUALITY.0 as u32,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                w!("Segoe UI"),
            );
            let search_style = WINDOW_STYLE(
                WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | WS_BORDER.0 | ES_AUTOHSCROLL as u32,
            );
            state.search_hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                search_style,
                SEARCH_MARGIN,
                SEARCH_MARGIN,
                WINDOW_WIDTH - SEARCH_MARGIN * 2 - 16,
                SEARCH_HEIGHT,
                hwnd,
                HMENU(1001),
                HINSTANCE(0),
                None,
            );
            if state.font.0 != 0 {
                SendMessageW(
                    state.search_hwnd,
                    WM_SETFONT,
                    WPARAM(state.font.0 as usize),
                    LPARAM(1),
                );
            }
            let search_hint = "搜索剪贴板历史"
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>();
            SendMessageW(
                state.search_hwnd,
                EM_SETCUEBANNER,
                WPARAM(1),
                LPARAM(search_hint.as_ptr() as isize),
            );
            if let Err(error) = AddClipboardFormatListener(hwnd) {
                eprintln!("[smart-clipboard] 注册剪贴板监听失败: {error}");
            }
            if let Err(error) = RegisterHotKey(
                hwnd,
                HOTKEY_ID,
                MOD_CONTROL | MOD_NOREPEAT,
                VK_OEM_3.0 as u32,
            ) {
                eprintln!("[smart-clipboard] Ctrl+` 全局快捷键注册失败: {error}");
            }
            state.refresh();
            LRESULT(0)
        }
        WM_SIZE => {
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let _ = MoveWindow(
                state.search_hwnd,
                SEARCH_MARGIN,
                SEARCH_MARGIN,
                (rect.right - SEARCH_MARGIN * 2).max(120),
                SEARCH_HEIGHT,
                true,
            );
            state.ensure_selection_visible();
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_COMMAND => {
            let control_id = (wparam.0 & 0xFFFF) as u16;
            let notification = ((wparam.0 >> 16) & 0xFFFF) as u16;
            if control_id == 1001 && notification as u32 == EN_CHANGE {
                state.selected_index = 0;
                state.scroll_offset = 0;
                state.refresh();
            }
            LRESULT(0)
        }
        WM_CLIPBOARDUPDATE => {
            state.capture_current();
            LRESULT(0)
        }
        WM_PAINT => {
            paint_window(state);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as u16) as i16;
            state.move_selection(if delta > 0 { -1 } else { 1 });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let y = ((lparam.0 >> 16) as u16) as i16 as i32;
            if y >= LIST_TOP {
                let row = ((y - LIST_TOP) / ITEM_HEIGHT).max(0) as usize;
                let index = state.scroll_offset + row;
                if index < state.items.len() {
                    state.selected_index = index;
                    let _ = InvalidateRect(hwnd, None, true);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            let y = ((lparam.0 >> 16) as u16) as i16 as i32;
            if y >= LIST_TOP {
                let row = ((y - LIST_TOP) / ITEM_HEIGHT).max(0) as usize;
                let index = state.scroll_offset + row;
                if index < state.items.len() {
                    state.selected_index = index;
                    state.activate_selected(true);
                }
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) != 0 {
                state.activated_since_show = true;
            } else if state.activated_since_show
                && state
                    .shown_at
                    .is_none_or(|shown_at| shown_at.elapsed() >= Duration::from_millis(300))
            {
                ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_APP_SHOW => {
            state.show();
            LRESULT(0)
        }
        WM_HOTKEY => {
            if wparam.0 as i32 == HOTKEY_ID {
                state.show();
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_APP_SHUTDOWN => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = RemoveClipboardFormatListener(hwnd);
            let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_ptr));
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn handle_keyboard_message(hwnd: HWND, message: &MSG) -> bool {
    if message.message != WM_KEYDOWN
        || (message.hwnd != hwnd && !IsChild(hwnd, message.hwnd).as_bool())
    {
        return false;
    }
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if state_ptr.is_null() {
        return false;
    }
    let state = &mut *state_ptr;
    match message.wParam.0 as u16 {
        value if value == VK_DOWN.0 => state.move_selection(1),
        value if value == VK_UP.0 => state.move_selection(-1),
        value if value == VK_RETURN.0 => {
            let ctrl_pressed = GetKeyState(VK_CONTROL.0 as i32) < 0;
            state.activate_selected(!ctrl_pressed);
        }
        value if value == VK_DELETE.0 => state.delete_selected(),
        value if value == VK_ESCAPE.0 => {
            ShowWindow(hwnd, SW_HIDE);
        }
        _ => return false,
    }
    true
}

unsafe fn paint_window(state: &WindowState) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = BeginPaint(state.hwnd, &mut paint);
    let mut client = RECT::default();
    let _ = GetClientRect(state.hwnd, &mut client);

    let background = OwnedBrush::new(rgb(250, 250, 251));
    FillRect(hdc, &client, background.handle());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, rgb(31, 41, 55));
    if state.font.0 != 0 {
        SelectObject(hdc, state.font);
    }

    let list_bottom = client.bottom - FOOTER_HEIGHT;
    let rows = ((list_bottom - LIST_TOP).max(0) / ITEM_HEIGHT) as usize;
    for row in 0..rows {
        let index = state.scroll_offset + row;
        let Some(item) = state.items.get(index) else {
            break;
        };
        let top = LIST_TOP + row as i32 * ITEM_HEIGHT;
        let item_rect = RECT {
            left: SEARCH_MARGIN,
            top,
            right: client.right - SEARCH_MARGIN,
            bottom: top + ITEM_HEIGHT - 4,
        };
        let selected = index == state.selected_index;
        let brush = OwnedBrush::new(if selected {
            rgb(224, 238, 255)
        } else {
            rgb(255, 255, 255)
        });
        FillRect(hdc, &item_rect, brush.handle());

        let preview_left = if item.kind == ClipboardKind::Image {
            draw_image_thumbnail(hdc, item, item_rect.left + 8, item_rect.top + 6, 54, 54);
            item_rect.left + 72
        } else {
            item_rect.left + 12
        };

        let mut type_rect = RECT {
            left: preview_left,
            top: item_rect.top + 7,
            right: item_rect.right - 110,
            bottom: item_rect.top + 28,
        };
        let mut type_text = to_wide_no_null(item.kind.label());
        SetTextColor(hdc, rgb(37, 99, 235));
        DrawTextW(
            hdc,
            &mut type_text,
            &mut type_rect,
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_NOPREFIX,
        );

        let mut time_rect = RECT {
            left: item_rect.right - 105,
            top: item_rect.top + 7,
            right: item_rect.right - 10,
            bottom: item_rect.top + 28,
        };
        let mut time_text = to_wide_no_null(&format_relative_time(item.created_at));
        SetTextColor(hdc, rgb(107, 114, 128));
        DrawTextW(
            hdc,
            &mut time_text,
            &mut time_rect,
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );

        let mut preview_rect = RECT {
            left: preview_left,
            top: item_rect.top + 32,
            right: item_rect.right - 10,
            bottom: item_rect.bottom - 6,
        };
        let mut preview = to_wide_no_null(&item.preview.replace(['\r', '\n'], " "));
        SetTextColor(hdc, rgb(31, 41, 55));
        DrawTextW(
            hdc,
            &mut preview,
            &mut preview_rect,
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }

    if state.items.is_empty() {
        let mut empty_rect = RECT {
            left: SEARCH_MARGIN,
            top: LIST_TOP + 80,
            right: client.right - SEARCH_MARGIN,
            bottom: LIST_TOP + 120,
        };
        let mut empty = to_wide_no_null("暂无匹配的剪贴板历史");
        SetTextColor(hdc, rgb(107, 114, 128));
        DrawTextW(
            hdc,
            &mut empty,
            &mut empty_rect,
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_NOPREFIX,
        );
    }

    let mut footer_rect = RECT {
        left: SEARCH_MARGIN,
        top: client.bottom - FOOTER_HEIGHT + 4,
        right: client.right - SEARCH_MARGIN,
        bottom: client.bottom - 2,
    };
    let footer = format!(
        "{} 条  Enter 粘贴  Ctrl+Enter 仅恢复  Delete 删除  Esc 关闭",
        state.items.len()
    );
    let mut footer_text = to_wide_no_null(&footer);
    SetTextColor(hdc, rgb(107, 114, 128));
    DrawTextW(
        hdc,
        &mut footer_text,
        &mut footer_rect,
        DT_LEFT | DT_TOP | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );

    EndPaint(state.hwnd, &paint);
}

struct OwnedBrush(HBRUSH);

impl OwnedBrush {
    unsafe fn new(color: COLORREF) -> Self {
        Self(CreateSolidBrush(color))
    }

    fn handle(&self) -> HBRUSH {
        self.0
    }
}

impl Drop for OwnedBrush {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0);
        }
    }
}

unsafe fn draw_image_thumbnail(
    hdc: ::windows::Win32::Graphics::Gdi::HDC,
    item: &HistoryItem,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    let Some(path) = &item.payload_path else {
        return;
    };
    let Ok(data) = fs::read(path) else {
        return;
    };
    let Some(info) = dib_info(&data) else {
        return;
    };
    ::windows::Win32::Graphics::Gdi::SetStretchBltMode(hdc, COLORONCOLOR);
    StretchDIBits(
        hdc,
        x,
        y,
        width,
        height,
        0,
        0,
        info.width,
        info.height.abs(),
        Some(data.as_ptr().add(info.pixel_offset).cast()),
        data.as_ptr().cast::<BITMAPINFO>(),
        DIB_RGB_COLORS,
        SRCCOPY,
    );
}

struct DibInfo {
    width: i32,
    height: i32,
    pixel_offset: usize,
}

fn dib_info(data: &[u8]) -> Option<DibInfo> {
    if data.len() < 40 {
        return None;
    }
    let header_size = read_u32(data, 0)? as usize;
    if header_size < 40 || header_size > data.len() {
        return None;
    }
    let width = read_i32(data, 4)?;
    let height = read_i32(data, 8)?;
    let bit_count = read_u16(data, 14)? as usize;
    let compression = read_u32(data, 16)?;
    let colors_used = read_u32(data, 32).unwrap_or(0) as usize;
    let palette_entries = if colors_used > 0 {
        colors_used
    } else if bit_count <= 8 {
        1usize << bit_count
    } else {
        0
    };
    let masks = if header_size == 40 && compression == 3 {
        12
    } else {
        0
    };
    let pixel_offset = header_size
        .checked_add(masks)?
        .checked_add(palette_entries.checked_mul(4)?)?;
    if pixel_offset >= data.len() || width == 0 || height == 0 {
        return None;
    }
    Some(DibInfo {
        width: width.abs(),
        height,
        pixel_offset,
    })
}

fn capture_clipboard(owner: HWND) -> Result<Option<CapturedItem>, String> {
    let _guard = ClipboardGuard::open(owner)?;
    if clipboard_monitoring_excluded() {
        return Ok(None);
    }

    if unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_ok() } {
        let paths = read_clipboard_files()?;
        if !paths.is_empty() {
            let preview = if paths.len() == 1 {
                Path::new(&paths[0])
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&paths[0])
                    .to_string()
            } else {
                format!(
                    "{} 个文件/文件夹 · {}",
                    paths.len(),
                    summarize_paths(&paths)
                )
            };
            let search_text = paths.join("\n");
            let content_hash = blake3::hash(search_text.as_bytes()).to_hex().to_string();
            return Ok(Some(CapturedItem {
                kind: ClipboardKind::Files,
                preview,
                search_text,
                text_content: None,
                file_paths: paths,
                payload: None,
                clipboard_format: CF_HDROP.0 as u32,
                content_hash,
                byte_size: 0,
            }));
        }
    }

    for format in [CF_DIBV5.0 as u32, CF_DIB.0 as u32] {
        if unsafe { IsClipboardFormatAvailable(format).is_err() } {
            continue;
        }
        let payload = read_clipboard_bytes(format)?;
        if payload.is_empty() || payload.len() > MAX_CAPTURE_BYTES {
            continue;
        }
        let dimensions = dib_info(&payload)
            .map(|info| format!("{} × {}", info.width, info.height.abs()))
            .unwrap_or_else(|| "剪贴板图像".to_string());
        let content_hash = blake3::hash(&payload).to_hex().to_string();
        return Ok(Some(CapturedItem {
            kind: ClipboardKind::Image,
            preview: dimensions.clone(),
            search_text: format!("图像 图片 image {dimensions}"),
            text_content: None,
            file_paths: Vec::new(),
            byte_size: payload.len(),
            payload: Some(payload),
            clipboard_format: format,
            content_hash,
        }));
    }

    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_ok() } {
        let text = read_clipboard_text()?;
        if !text.trim().is_empty() {
            let bytes = text.as_bytes();
            let truncated = if bytes.len() > MAX_CAPTURE_BYTES {
                String::from_utf8_lossy(&bytes[..MAX_CAPTURE_BYTES]).to_string()
            } else {
                text
            };
            let content_hash = blake3::hash(truncated.as_bytes()).to_hex().to_string();
            return Ok(Some(CapturedItem {
                kind: ClipboardKind::Text,
                preview: summarize_text(&truncated),
                search_text: truncated.clone(),
                text_content: Some(truncated.clone()),
                file_paths: Vec::new(),
                payload: None,
                clipboard_format: CF_UNICODETEXT.0 as u32,
                content_hash,
                byte_size: truncated.len(),
            }));
        }
    }

    Ok(None)
}

fn clipboard_monitoring_excluded() -> bool {
    for (name, exclude_when_zero) in [
        (w!("ExcludeClipboardContentFromMonitorProcessing"), false),
        (w!("CanIncludeInClipboardHistory"), true),
    ] {
        let format = unsafe { RegisterClipboardFormatW(name) };
        if format == 0 || unsafe { IsClipboardFormatAvailable(format).is_err() } {
            continue;
        }
        if let Ok(bytes) = read_clipboard_bytes(format) {
            if bytes.len() >= 4 {
                let value = u32::from_le_bytes(bytes[..4].try_into().unwrap_or_default());
                if (exclude_when_zero && value == 0) || (!exclude_when_zero && value != 0) {
                    return true;
                }
            }
        }
    }
    false
}

struct ClipboardGuard;

impl ClipboardGuard {
    fn open(owner: HWND) -> Result<Self, String> {
        for _ in 0..8 {
            if unsafe { OpenClipboard(owner).is_ok() } {
                return Ok(Self);
            }
            thread::sleep(Duration::from_millis(8));
        }
        Err("系统剪贴板正被其他程序占用".to_string())
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn read_clipboard_bytes(format: u32) -> Result<Vec<u8>, String> {
    let handle = unsafe { GetClipboardData(format).map_err(|error| error.to_string())? };
    let global = HGLOBAL(handle.0 as *mut std::ffi::c_void);
    let size = unsafe { GlobalSize(global) };
    if size == 0 || size > MAX_CAPTURE_BYTES {
        return Ok(Vec::new());
    }
    let pointer = unsafe { GlobalLock(global) };
    if pointer.is_null() {
        return Err("无法锁定剪贴板数据".to_string());
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size).to_vec() };
    unsafe {
        let _ = GlobalUnlock(global);
    }
    Ok(bytes)
}

fn read_clipboard_text() -> Result<String, String> {
    let bytes = read_clipboard_bytes(CF_UNICODETEXT.0 as u32)?;
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|value| *value != 0)
        .collect::<Vec<_>>();
    Ok(String::from_utf16_lossy(&units))
}

fn read_clipboard_files() -> Result<Vec<String>, String> {
    let handle = unsafe { GetClipboardData(CF_HDROP.0 as u32).map_err(|error| error.to_string())? };
    let drop_handle = HDROP(handle.0);
    let count = unsafe { DragQueryFileW(drop_handle, u32::MAX, None) };
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        let length = unsafe { DragQueryFileW(drop_handle, index, None) };
        if length == 0 {
            continue;
        }
        let mut buffer = vec![0u16; length as usize + 1];
        unsafe {
            DragQueryFileW(drop_handle, index, Some(&mut buffer));
        }
        buffer.truncate(length as usize);
        paths.push(String::from_utf16_lossy(&buffer));
    }
    Ok(paths)
}

fn restore_item_to_clipboard(owner: HWND, item: &HistoryItem) -> Result<(), String> {
    let formats = prepare_item_formats(item)?;
    let _guard = ClipboardGuard::open(owner)?;
    unsafe {
        EmptyClipboard().map_err(|error| error.to_string())?;
    }
    for (format, bytes) in formats {
        set_clipboard_bytes(format, &bytes)?;
    }
    Ok(())
}

fn prepare_item_formats(item: &HistoryItem) -> Result<Vec<(u32, Vec<u8>)>, String> {
    match item.kind {
        ClipboardKind::Text => {
            let text = item
                .text_content
                .as_deref()
                .ok_or_else(|| "文本历史内容已经丢失".to_string())?;
            let mut bytes = Vec::with_capacity((text.encode_utf16().count() + 1) * 2);
            for unit in text.encode_utf16().chain(Some(0)) {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(vec![(CF_UNICODETEXT.0 as u32, bytes)])
        }
        ClipboardKind::Image => {
            let path = item
                .payload_path
                .as_ref()
                .ok_or_else(|| "图像历史内容已经丢失".to_string())?;
            let payload = fs::read(path).map_err(|error| error.to_string())?;
            if dib_info(&payload).is_none() {
                return Err("图像历史内容已经损坏".to_string());
            }
            Ok(vec![(item.clipboard_format, payload)])
        }
        ClipboardKind::Files => {
            let existing = item
                .file_paths
                .iter()
                .filter(|path| Path::new(path).exists())
                .cloned()
                .collect::<Vec<_>>();
            if existing.is_empty() {
                return Err("这些文件或文件夹已经不存在".to_string());
            }
            let drop_data = build_drop_files(&existing);
            let mut formats = vec![(CF_HDROP.0 as u32, drop_data)];
            let preferred_effect = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
            if preferred_effect != 0 {
                formats.push((preferred_effect, 1u32.to_le_bytes().to_vec()));
            }
            Ok(formats)
        }
    }
}

fn set_clipboard_bytes(format: u32, bytes: &[u8]) -> Result<(), String> {
    let global =
        unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|error| error.to_string())? };
    let pointer = unsafe { GlobalLock(global) };
    if pointer.is_null() {
        unsafe {
            let _ = GlobalFree(global);
        }
        return Err("无法分配剪贴板数据".to_string());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(global);
    }
    match unsafe { SetClipboardData(format, HANDLE(global.0 as isize)) } {
        Ok(_) => Ok(()),
        Err(error) => {
            unsafe {
                let _ = GlobalFree(global);
            }
            Err(error.to_string())
        }
    }
}

fn build_drop_files(paths: &[String]) -> Vec<u8> {
    let mut wide = Vec::<u16>::new();
    for path in paths {
        wide.extend(Path::new(path).as_os_str().encode_wide());
        wide.push(0);
    }
    wide.push(0);

    let header = DROPFILES {
        pFiles: size_of::<DROPFILES>() as u32,
        pt: POINT::default(),
        fNC: BOOL(0),
        fWide: BOOL(1),
    };
    let mut bytes = Vec::with_capacity(size_of::<DROPFILES>() + wide.len() * 2);
    unsafe {
        bytes.extend_from_slice(std::slice::from_raw_parts(
            (&header as *const DROPFILES).cast::<u8>(),
            size_of::<DROPFILES>(),
        ));
    }
    for unit in wide {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

unsafe fn focused_window(target: HWND) -> HWND {
    if target.0 == 0 {
        return HWND(0);
    }
    let thread_id = GetWindowThreadProcessId(target, None);
    if thread_id == 0 {
        return HWND(0);
    }
    let mut info = GUITHREADINFO {
        cbSize: size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    if GetGUIThreadInfo(thread_id, &mut info).is_ok() {
        info.hwndFocus
    } else {
        HWND(0)
    }
}

unsafe fn paste_to_previous_window(target: HWND, focus: HWND) {
    if target.0 == 0 {
        return;
    }
    let mut process_id = 0u32;
    let target_thread = GetWindowThreadProcessId(target, Some(&mut process_id));
    if process_id == GetCurrentProcessId() {
        return;
    }
    let current_thread = GetCurrentThreadId();
    let attached = target_thread != 0
        && target_thread != current_thread
        && AttachThreadInput(current_thread, target_thread, true).as_bool();
    let activated = SetForegroundWindow(target).as_bool();
    if focus.0 != 0 {
        SetFocus(focus);
    }
    if attached {
        AttachThreadInput(current_thread, target_thread, false);
    }
    if !activated {
        return;
    }
    thread::sleep(Duration::from_millis(80));
    let inputs = [
        keyboard_input(VK_CONTROL, false),
        keyboard_input(::windows::Win32::UI::Input::KeyboardAndMouse::VK_V, false),
        keyboard_input(::windows::Win32::UI::Input::KeyboardAndMouse::VK_V, true),
        keyboard_input(VK_CONTROL, true),
    ];
    let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
    if sent != inputs.len() as u32 {
        eprintln!(
            "[smart-clipboard] 自动粘贴按键发送不完整: {sent}/{}",
            inputs.len()
        );
    }
}

fn keyboard_input(
    key: ::windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    key_up: bool,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

unsafe fn position_window_near_cursor(hwnd: HWND) {
    let mut cursor = POINT::default();
    if GetCursorPos(&mut cursor).is_err() {
        return;
    }
    let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return;
    }
    let x = (cursor.x - WINDOW_WIDTH / 2)
        .max(info.rcWork.left)
        .min(info.rcWork.right - WINDOW_WIDTH);
    let y = (cursor.y + 16)
        .max(info.rcWork.top)
        .min(info.rcWork.bottom - WINDOW_HEIGHT);
    let _ = SetWindowPos(
        hwnd,
        HWND(0),
        x,
        y,
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        SWP_NOZORDER,
    );
}

unsafe fn window_text(hwnd: HWND) -> String {
    if hwnd.0 == 0 {
        return String::new();
    }
    let length = GetWindowTextLengthW(hwnd);
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let written = GetWindowTextW(hwnd, &mut buffer);
    buffer.truncate(written.max(0) as usize);
    String::from_utf16_lossy(&buffer)
}

fn summarize_text(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized.chars().take(180).collect()
}

fn summarize_paths(paths: &[String]) -> String {
    paths
        .iter()
        .take(3)
        .map(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path)
        })
        .collect::<Vec<_>>()
        .join("、")
}

fn format_relative_time(timestamp: i64) -> String {
    let now = Utc::now().timestamp();
    let seconds = now.saturating_sub(timestamp);
    if seconds < 60 {
        return "刚刚".to_string();
    }
    if seconds < 3600 {
        return format!("{} 分钟前", seconds / 60);
    }
    if seconds < 86_400 {
        return format!("{} 小时前", seconds / 3600);
    }
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.format("%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

fn escape_like(value: &str) -> String {
    value.replace('%', "\\%").replace('_', "\\_")
}

fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF(red as u32 | ((green as u32) << 8) | ((blue as u32) << 16))
}

fn to_wide_no_null(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_multiline_text() {
        assert_eq!(summarize_text(" hello\n  world "), "hello world");
    }

    #[test]
    fn builds_wide_drop_file_payload() {
        let payload = build_drop_files(&["C:\\Temp\\a.txt".to_string()]);
        assert!(payload.len() > size_of::<DROPFILES>() + 4);
        let offset = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
        assert_eq!(offset, size_of::<DROPFILES>());
        assert_eq!(&payload[payload.len() - 4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn parses_basic_dib_dimensions() {
        let mut dib = vec![0u8; 44];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&320i32.to_le_bytes());
        dib[8..12].copy_from_slice(&(-200i32).to_le_bytes());
        dib[14..16].copy_from_slice(&32u16.to_le_bytes());
        let info = dib_info(&dib).unwrap();
        assert_eq!(info.width, 320);
        assert_eq!(info.height, -200);
        assert_eq!(info.pixel_offset, 40);
    }

    #[test]
    fn rejects_missing_image_before_opening_clipboard() {
        let item = HistoryItem {
            id: "missing-image".to_string(),
            kind: ClipboardKind::Image,
            preview: "missing".to_string(),
            text_content: None,
            file_paths: Vec::new(),
            payload_path: Some(
                std::env::temp_dir().join(format!("pm-center-missing-{}.dib", Uuid::new_v4())),
            ),
            clipboard_format: CF_DIB.0 as u32,
            created_at: 0,
        };

        assert!(prepare_item_formats(&item).is_err());
    }
}
