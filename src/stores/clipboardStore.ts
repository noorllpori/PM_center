import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from './uiStore';

type ClipboardAction = 'cut' | 'copy';

export interface ClipboardItem {
  path: string;
  name: string;
  action: ClipboardAction;
  projectPath: string;
}

interface ClipboardSourceItem {
  path: string;
  name: string;
  projectPath: string;
}

interface ClipboardState {
  items: ClipboardItem[];
  
  // 剪切
  cut: (path: string, name: string, projectPath: string) => void;
  cutItems: (items: ClipboardSourceItem[]) => void;
  // 复制
  copy: (path: string, name: string, projectPath: string) => void;
  copyItems: (items: ClipboardSourceItem[]) => void;
  // 粘贴
  paste: (targetDir: string, targetProjectPath: string) => Promise<boolean>;
  // 清空
  clear: () => void;
  // 是否有内容
  hasItem: () => boolean;
}

function buildClipboardItems(items: ClipboardSourceItem[], action: ClipboardAction): ClipboardItem[] {
  return items.map((item) => ({
    ...item,
    action,
  }));
}

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  items: [],

  cut: (path: string, name: string, projectPath: string) => {
    set({
      items: buildClipboardItems([{ path, name, projectPath }], 'cut'),
    });
  },

  cutItems: (items: ClipboardSourceItem[]) => {
    set({
      items: buildClipboardItems(items, 'cut'),
    });
  },

  copy: (path: string, name: string, projectPath: string) => {
    set({
      items: buildClipboardItems([{ path, name, projectPath }], 'copy'),
    });
  },

  copyItems: (items: ClipboardSourceItem[]) => {
    set({
      items: buildClipboardItems(items, 'copy'),
    });
  },

  paste: async (targetDir: string, targetProjectPath: string) => {
    const { items } = get();
    if (items.length === 0) return false;

    try {
      const action = items[0].action;

      for (const item of items) {
        if (action === 'cut') {
          await invoke('move_project_entry', {
            projectPath: item.projectPath,
            source: item.path,
            target: targetDir,
            conflictStrategy: 'error',
          });
        } else {
          await invoke('copy_file', {
            source: item.path,
            target: targetDir,
          });
        }
      }

      if (action === 'cut') {
        set({ items: [] });
      } else {
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
    set({ items: [] });
  },

  hasItem: () => {
    return get().items.length > 0;
  },
}));
