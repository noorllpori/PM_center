import { create } from 'zustand';
import { load } from '@tauri-apps/plugin-store';

export interface LauncherItem {
  id: string;
  name: string;
  path: string;
  icon?: string; // base64 或路径
}

interface LauncherState {
  items: LauncherItem[];
  isOpen: boolean;
  
  // 加载配置
  loadItems: () => Promise<void>;
  // 保存配置
  saveItems: () => Promise<void>;
  // 添加软件
  addItem: (item: Omit<LauncherItem, 'id'>) => Promise<void>;
  // 删除软件
  removeItem: (id: string) => Promise<void>;
  // 更新软件
  updateItem: (id: string, updates: Partial<LauncherItem>) => Promise<void>;
  // 打开/关闭面板
  toggleOpen: () => void;
  setOpen: (open: boolean) => void;
}

const STORE_FILE = 'launcher.json';

export const useLauncherStore = create<LauncherState>((set, get) => ({
  items: [],
  isOpen: false,

  loadItems: async () => {
    try {
      const store = await load(STORE_FILE);
      const items = await store.get<LauncherItem[]>('items');
      if (items) {
        set({ items });
      } else {
        // 默认添加一些常用软件
        set({
          items: [
            { id: '1', name: 'Blender', path: 'C:\\Program Files\\Blender Foundation\\Blender 4.5\\blender.exe' },
            { id: '2', name: 'After Effects', path: 'C:\\Program Files\\Adobe\\Adobe After Effects 2024\\Support Files\\AfterFX.exe' },
          ],
        });
        await get().saveItems();
      }
    } catch (error) {
      console.error('Failed to load launcher items:', error);
    }
  },

  saveItems: async () => {
    try {
      const store = await load(STORE_FILE);
      await store.set('items', get().items);
      await store.save();
    } catch (error) {
      console.error('Failed to save launcher items:', error);
    }
  },

  addItem: async (item) => {
    const newItem: LauncherItem = {
      ...item,
      id: `launcher_${Date.now()}`,
    };
    set((state) => ({ items: [...state.items, newItem] }));
    await get().saveItems();
  },

  removeItem: async (id) => {
    set((state) => ({ items: state.items.filter((i) => i.id !== id) }));
    await get().saveItems();
  },

  updateItem: async (id, updates) => {
    set((state) => ({
      items: state.items.map((i) =>
        i.id === id ? { ...i, ...updates } : i
      ),
    }));
    await get().saveItems();
  },

  toggleOpen: () => set((state) => ({ isOpen: !state.isOpen })),
  setOpen: (open) => set({ isOpen: open }),
}));
