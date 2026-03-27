import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

type ClipboardAction = 'cut' | 'copy';

interface ClipboardItem {
  path: string;
  name: string;
  action: ClipboardAction;
}

interface ClipboardState {
  item: ClipboardItem | null;
  
  // 剪切
  cut: (path: string, name: string) => void;
  // 复制
  copy: (path: string, name: string) => void;
  // 粘贴
  paste: (targetDir: string) => Promise<boolean>;
  // 清空
  clear: () => void;
  // 是否有内容
  hasItem: () => boolean;
}

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  item: null,

  cut: (path: string, name: string) => {
    set({ item: { path, name, action: 'cut' } });
  },

  copy: (path: string, name: string) => {
    set({ item: { path, name, action: 'copy' } });
  },

  paste: async (targetDir: string) => {
    const { item } = get();
    if (!item) return false;

    try {
      if (item.action === 'cut') {
        await invoke('move_file', { 
          source: item.path, 
          target: targetDir 
        });
        // 移动后清空剪贴板
        set({ item: null });
      } else {
        await invoke('copy_file', { 
          source: item.path, 
          target: targetDir 
        });
        // 复制后保留剪贴板
      }
      return true;
    } catch (error) {
      console.error('Paste failed:', error);
      alert('操作失败: ' + error);
      return false;
    }
  },

  clear: () => {
    set({ item: null });
  },

  hasItem: () => {
    return get().item !== null;
  },
}));
