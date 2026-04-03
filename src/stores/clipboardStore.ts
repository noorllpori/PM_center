import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from './uiStore';

type ClipboardAction = 'cut' | 'copy';

interface ClipboardItem {
  path: string;
  name: string;
  action: ClipboardAction;
  projectPath: string;
}

interface ClipboardState {
  item: ClipboardItem | null;
  
  // 剪切
  cut: (path: string, name: string, projectPath: string) => void;
  // 复制
  copy: (path: string, name: string, projectPath: string) => void;
  // 粘贴
  paste: (targetDir: string, targetProjectPath: string) => Promise<boolean>;
  // 清空
  clear: () => void;
  // 是否有内容
  hasItem: () => boolean;
}

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  item: null,

  cut: (path: string, name: string, projectPath: string) => {
    set({ item: { path, name, action: 'cut', projectPath } });
  },

  copy: (path: string, name: string, projectPath: string) => {
    set({ item: { path, name, action: 'copy', projectPath } });
  },

  paste: async (targetDir: string, targetProjectPath: string) => {
    const { item } = get();
    if (!item) return false;

    try {
      if (item.action === 'cut') {
        await invoke('move_project_entry', { 
          projectPath: item.projectPath,
          source: item.path, 
          target: targetDir,
          conflictStrategy: 'error',
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
      const message = String(error).startsWith('PM_CONFLICT:')
        ? '目标位置已存在同名文件'
        : '操作失败: ' + error;
      useUiStore.getState().showToast({
        title: '粘贴失败',
        message,
        tone: 'error',
      });
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
