// P2P 局域网消息系统
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

// 默认端口
const DISCOVERY_PORT: u16 = 31523;
const MESSAGE_PORT: u16 = 31524;
const BROADCAST_ADDR: &str = "255.255.255.255";

// 用户发现广播间隔
const BROADCAST_INTERVAL: Duration = Duration::from_secs(5);
const USER_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PUser {
    pub id: String,
    pub name: String,
    pub ip: String,
    #[serde(alias = "lastSeen")]
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PMessage {
    pub id: String,
    #[serde(alias = "fromId")]
    pub from_id: String,
    #[serde(alias = "fromName")]
    pub from_name: String,
    #[serde(alias = "toId")]
    pub to_id: Option<String>, // None 表示广播
    pub content: String,
    pub timestamp: u64,
}

// 发现广播包
#[derive(Debug, Clone, Serialize, Deserialize)]
enum DiscoveryPacket {
    Announce { user_id: String, user_name: String },
}

// 全局状态
type P2PState = Arc<Mutex<P2PStateInner>>;

struct P2PStateInner {
    user_id: String,
    user_name: String,
    online_users: HashMap<String, P2PUser>, // id -> user
    is_running: bool,
}

lazy_static::lazy_static! {
    static ref P2P_GLOBAL: P2PState = Arc::new(Mutex::new(P2PStateInner {
        user_id: String::new(),
        user_name: String::new(),
        online_users: HashMap::new(),
        is_running: false,
    }));
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 初始化 P2P
#[tauri::command]
pub async fn init_p2p(user_id: String, user_name: String) -> Result<(), String> {
    let mut state = P2P_GLOBAL.lock().map_err(|e| e.to_string())?;
    state.user_id = user_id;
    state.user_name = user_name;
    println!("[P2P] 初始化: {} ({})", state.user_name, state.user_id);
    Ok(())
}

/// 更新用户信息
#[tauri::command]
pub async fn update_p2p_user(user_id: String, user_name: String) -> Result<(), String> {
    let mut state = P2P_GLOBAL.lock().map_err(|e| e.to_string())?;
    state.user_id = user_id;
    state.user_name = user_name;
    println!("[P2P] 更新用户信息: {} ({})", state.user_name, state.user_id);
    Ok(())
}

/// 启动发现服务
#[tauri::command]
pub async fn start_p2p_discovery(app_handle: tauri::AppHandle) -> Result<(), String> {
    {
        let mut state = P2P_GLOBAL.lock().map_err(|e| e.to_string())?;
        
        if state.is_running {
            return Ok(());
        }
        
        state.is_running = true;
        println!("[P2P] 发现服务已启动");
    }
    
    // 启动后台任务
    let state_clone = P2P_GLOBAL.clone();
    let app_handle_clone = app_handle.clone();
    
    tokio::spawn(async move {
        run_discovery_loop(state_clone, app_handle_clone).await;
    });
    
    // 启动 TCP 消息接收服务
    let state_clone = P2P_GLOBAL.clone();
    tokio::spawn(async move {
        run_tcp_server(state_clone, app_handle).await;
    });
    
    Ok(())
}

/// 停止发现服务
#[tauri::command]
pub async fn stop_p2p_discovery() -> Result<(), String> {
    let mut state = P2P_GLOBAL.lock().map_err(|e| e.to_string())?;
    state.is_running = false;
    state.online_users.clear();
    println!("[P2P] 发现服务已停止");
    Ok(())
}

/// 发送消息
#[tauri::command]
pub async fn send_p2p_message(message: P2PMessage) -> Result<(), String> {
    // 复制需要的数据，避免长时间持有锁
    let targets: Vec<(String, String)> = {
        let state = P2P_GLOBAL.lock().map_err(|e| e.to_string())?;
        
        if let Some(ref to_id) = message.to_id {
            // 私聊
            if let Some(user) = state.online_users.get(to_id) {
                vec![(to_id.clone(), user.ip.clone())]
            } else {
                return Err("用户不在线".to_string());
            }
        } else {
            // 广播 - 发送给所有在线用户
            state.online_users
                .iter()
                .filter(|(id, _)| **id != message.from_id)
                .map(|(id, user)| (id.clone(), user.ip.clone()))
                .collect()
        }
    };
    
    // 发送消息（不持有锁）
    for (_, ip) in targets {
        let _ = send_tcp_message(&ip, &message).await;
    }
    
    Ok(())
}

// 发现循环
async fn run_discovery_loop(state: P2PState, app_handle: tauri::AppHandle) {
    // 创建 UDP 发现 socket
    let socket = match UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT)) {
        Ok(s) => {
            println!("[P2P] UDP 发现端口绑定成功: {}", DISCOVERY_PORT);
            s
        }
        Err(e) => {
            println!("[P2P] 绑定发现端口失败: {}", e);
            return;
        }
    };
    
    if let Err(e) = socket.set_broadcast(true) {
        println!("[P2P] 设置广播失败: {}", e);
        return;
    }
    
    if let Err(e) = socket.set_nonblocking(true) {
        println!("[P2P] 设置非阻塞失败: {}", e);
        return;
    }
    
    let socket = Arc::new(socket);
    let mut last_broadcast = std::time::Instant::now() - BROADCAST_INTERVAL;
    
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // 检查运行状态并获取用户数据
        let (user_id, user_name, is_running, users_changed) = {
            let mut locked = state.lock().unwrap();
            
            if !locked.is_running {
                break;
            }
            
            // 清理超时的用户
            let now = now_millis();
            let before_count = locked.online_users.len();
            locked.online_users.retain(|_, user| {
                now - user.last_seen < USER_TIMEOUT.as_millis() as u64
            });
            let after_count = locked.online_users.len();
            
            // 发送用户列表更新
            let users: Vec<P2PUser> = locked.online_users.values().cloned().collect();
            drop(locked);
            
            let _ = app_handle.emit("p2p-users", serde_json::json!({ "users": users }));
            
            let locked = state.lock().unwrap();
            (
                locked.user_id.clone(),
                locked.user_name.clone(),
                locked.is_running,
                before_count != after_count,
            )
        };
        
        if !is_running {
            break;
        }
        
        if users_changed {
            println!("[P2P] 在线用户列表已更新");
        }
        
        // 定期广播自己的存在
        if last_broadcast.elapsed() >= BROADCAST_INTERVAL {
            let packet = DiscoveryPacket::Announce {
                user_id,
                user_name,
            };
            
            let data = serde_json::to_vec(&packet).unwrap_or_default();
            let addr = format!("{}:{}", BROADCAST_ADDR, DISCOVERY_PORT);
            
            if let Err(e) = socket.send_to(&data, &addr) {
                println!("[P2P] 广播失败: {}", e);
            }
            
            last_broadcast = std::time::Instant::now();
        }
        
        // 接收广播
        receive_discovery_broadcast(&state, &socket);
    }
}

// 接收发现广播
fn receive_discovery_broadcast(
    state: &P2PState,
    socket: &UdpSocket,
) {
    let mut buf = [0u8; 1024];
    
    while let Ok((len, addr)) = socket.recv_from(&mut buf) {
        if let Ok(packet) = serde_json::from_slice::<DiscoveryPacket>(&buf[..len]) {
            match packet {
                DiscoveryPacket::Announce { user_id, user_name } => {
                    let mut locked = state.lock().unwrap();
                    
                    // 不处理自己的广播
                    if user_id == locked.user_id {
                        continue;
                    }

                    let previous_user = locked.online_users.get(&user_id).cloned();
                    
                    let user = P2PUser {
                        id: user_id.clone(),
                        name: user_name,
                        ip: addr.ip().to_string(),
                        last_seen: now_millis(),
                    };

                    let is_new_user = previous_user.is_none();
                    let user_changed = previous_user
                        .map(|existing| existing.name != user.name || existing.ip != user.ip)
                        .unwrap_or(false);

                    if is_new_user || user_changed {
                        println!("[P2P] 发现用户: {} ({}) @ {}", user.name, user.id, user.ip);
                    }

                    locked.online_users.insert(user_id, user);
                }
            }
        }
    }
}

// TCP 消息服务器
async fn run_tcp_server(state: P2PState, app_handle: tauri::AppHandle) {
    let listener = match TcpListener::bind(format!("0.0.0.0:{}", MESSAGE_PORT)).await {
        Ok(l) => {
            println!("[P2P] TCP 服务器已启动，端口: {}", MESSAGE_PORT);
            l
        }
        Err(e) => {
            println!("[P2P] TCP 服务器启动失败: {}", e);
            return;
        }
    };
    
    loop {
        // 检查是否还在运行
        {
            let locked = state.lock().unwrap();
            if !locked.is_running {
                break;
            }
        }
        
        match tokio::time::timeout(Duration::from_secs(1), listener.accept()).await {
            Ok(Ok((stream, addr))) => {
                println!("[P2P] 新连接: {}", addr);
                let state_clone = state.clone();
                let app_handle_clone = app_handle.clone();
                
                tokio::spawn(async move {
                    handle_tcp_connection(stream, state_clone, app_handle_clone).await;
                });
            }
            Ok(Err(e)) => {
                println!("[P2P] 接受连接失败: {}", e);
            }
            Err(_) => {
                // 超时，继续循环检查 is_running
            }
        }
    }
}

// 处理 TCP 连接
async fn handle_tcp_connection(
    stream: tokio::net::TcpStream,
    _state: P2PState,
    app_handle: tauri::AppHandle,
) {
    let (reader, _writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                // 连接关闭
                break;
            }
            Ok(_) => {
                if let Ok(message) = serde_json::from_str::<P2PMessage>(&line.trim()) {
                    println!("[P2P] 收到消息 from {}: {}", message.from_name, message.content);
                    let _ = app_handle.emit("p2p-message", message);
                }
            }
            Err(e) => {
                println!("[P2P] 读取消息失败: {}", e);
                break;
            }
        }
    }
}

// 发送 TCP 消息
async fn send_tcp_message(ip: &str, message: &P2PMessage) -> Result<(), String> {
    let addr = format!("{}:{}", ip, MESSAGE_PORT);
    
    let stream = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("连接 {} 失败: {}", addr, e))?;
    
    let (_reader, mut writer) = stream.into_split();
    
    let data = serde_json::to_string(message).map_err(|e| e.to_string())?;
    let data = format!("{}\n", data);
    
    writer.write_all(data.as_bytes())
        .await
        .map_err(|e| format!("发送失败: {}", e))?;
    
    Ok(())
}
