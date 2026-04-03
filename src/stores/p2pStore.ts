import { create } from 'zustand';
import { load } from '@tauri-apps/plugin-store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface P2PUser {
  id: string;
  name: string;
  ip: string;
  lastSeen: number;
}

export interface P2PMessage {
  id: string;
  fromId: string;
  fromName: string;
  toId: string | null; // null 表示广播
  content: string;
  timestamp: number;
}

interface RawP2PUser {
  id: string;
  name: string;
  ip: string;
  lastSeen?: number;
  last_seen?: number;
}

interface RawP2PMessage {
  id: string;
  fromId?: string;
  from_id?: string;
  fromName?: string;
  from_name?: string;
  toId?: string | null;
  to_id?: string | null;
  content: string;
  timestamp: number;
}

interface P2PState {
  // 当前用户
  userId: string | null;
  userName: string;
  
  // 在线用户
  onlineUsers: P2PUser[];
  
  // 消息
  messages: P2PMessage[];
  unreadCount: number;
  
  // 状态
  isDiscoveryEnabled: boolean;
  
  // 加载设置
  loadSettings: () => Promise<void>;
  
  // 设置用户名称
  setUserName: (name: string) => Promise<void>;
  
  // 发现控制
  startDiscovery: () => Promise<void>;
  stopDiscovery: () => Promise<void>;
  
  // 用户管理
  updateOnlineUsers: (users: P2PUser[]) => void;
  removeOfflineUsers: () => void;
  
  // 消息
  sendMessage: (toId: string | null, content: string) => Promise<void>;
  receiveMessage: (message: P2PMessage) => void;
  markAllAsRead: () => void;
  clearMessages: () => void;
}

const STORE_FILE = 'p2p-settings.json';
const USER_ID_KEY = 'p2p-user-id';
const USER_NAME_KEY = 'p2p-user-name';
let p2pEventListenersInitialized = false;

function normalizeP2PUser(user: RawP2PUser): P2PUser {
  return {
    id: user.id,
    name: user.name,
    ip: user.ip,
    lastSeen: user.lastSeen ?? user.last_seen ?? Date.now(),
  };
}

function normalizeP2PMessage(message: RawP2PMessage): P2PMessage {
  return {
    id: message.id,
    fromId: message.fromId ?? message.from_id ?? '',
    fromName: message.fromName ?? message.from_name ?? '未知用户',
    toId: message.toId ?? message.to_id ?? null,
    content: message.content,
    timestamp: message.timestamp,
  };
}

// 生成 UUID
function generateUUID(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

export const useP2PStore = create<P2PState>((set, get) => ({
  userId: null,
  userName: '匿名用户',
  onlineUsers: [],
  messages: [],
  unreadCount: 0,
  isDiscoveryEnabled: false,

  loadSettings: async () => {
    try {
      const store = await load(STORE_FILE);
      
      // 加载或生成用户 ID
      let userId = await store.get<string>(USER_ID_KEY);
      if (!userId) {
        userId = generateUUID();
        await store.set(USER_ID_KEY, userId);
        await store.save();
      }
      
      // 加载用户名
      const userName = await store.get<string>(USER_NAME_KEY) || '匿名用户';
      
      set({ userId, userName });
      
      // 初始化后端 P2P
      await invoke('init_p2p', { userId, userName });
      
      // 监听事件
      setupEventListeners(get().receiveMessage, get().updateOnlineUsers);

      // 默认进入发现状态
      await get().startDiscovery();
    } catch (error) {
      console.error('Failed to load P2P settings:', error);
    }
  },

  setUserName: async (name: string) => {
    try {
      const store = await load(STORE_FILE);
      await store.set(USER_NAME_KEY, name);
      await store.save();
      
      set({ userName: name });
      
      // 更新后端
      const { userId } = get();
      if (userId) {
        await invoke('update_p2p_user', { userId, userName: name });
      }
    } catch (error) {
      console.error('Failed to set user name:', error);
    }
  },

  startDiscovery: async () => {
    try {
      if (get().isDiscoveryEnabled) {
        return;
      }

      await invoke('start_p2p_discovery');
      set({ isDiscoveryEnabled: true });
    } catch (error) {
      set({ isDiscoveryEnabled: false });
      console.error('Failed to start discovery:', error);
    }
  },

  stopDiscovery: async () => {
    try {
      await invoke('stop_p2p_discovery');
      set({ isDiscoveryEnabled: false, onlineUsers: [] });
    } catch (error) {
      console.error('Failed to stop discovery:', error);
    }
  },

  updateOnlineUsers: (users: P2PUser[]) => {
    set({ onlineUsers: users.map(normalizeP2PUser) });
  },

  removeOfflineUsers: () => {
    const now = Date.now();
    const { onlineUsers } = get();
    const filtered = onlineUsers.filter(u => now - u.lastSeen < 30000); // 30秒超时
    set({ onlineUsers: filtered });
  },

  sendMessage: async (toId: string | null, content: string) => {
    try {
      const { userId, userName } = get();
      if (!userId) return;

      const message: P2PMessage = {
        id: generateUUID(),
        fromId: userId,
        fromName: userName,
        toId,
        content,
        timestamp: Date.now(),
      };

      await invoke('send_p2p_message', { message });

      set(state => ({
        messages: [...state.messages, message],
      }));
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  },

  receiveMessage: (message: P2PMessage) => {
    set(state => {
      if (state.messages.some(existing => existing.id === message.id)) {
        return state;
      }

      return {
        messages: [...state.messages, message],
        unreadCount: message.fromId === state.userId ? state.unreadCount : state.unreadCount + 1,
      };
    });
  },

  markAllAsRead: () => {
    set({ unreadCount: 0 });
  },

  clearMessages: () => {
    set({ messages: [], unreadCount: 0 });
  },
}));

// 设置事件监听
function setupEventListeners(
  onMessage: (msg: P2PMessage) => void,
  onUsersUpdate: (users: P2PUser[]) => void
) {
  if (p2pEventListenersInitialized) {
    return;
  }

  p2pEventListenersInitialized = true;

  // 监听新消息
  void listen<RawP2PMessage>('p2p-message', (event) => {
    onMessage(normalizeP2PMessage(event.payload));
  });
  
  // 监听在线用户更新
  void listen<{ users: RawP2PUser[] }>('p2p-users', (event) => {
    onUsersUpdate(event.payload.users.map(normalizeP2PUser));
  });
}
