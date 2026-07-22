import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import {
  importExternalPaths,
  isExternalImportCancelled,
  type ExternalImportProgress,
} from '../components/file-manager/externalImport';
import { useFileOperationStore } from './fileOperationStore';
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
  pasteSystem: (targetDir: string) => Promise<number>;
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

interface SystemClipboardStatus {
  hasFiles: boolean;
  hasImage: boolean;
}

function updateTransferProgress(operationId: string, progress: ExternalImportProgress) {
  useFileOperationStore.getState().updateOperation(operationId, {
    currentName: progress.currentName,
    itemIndex: progress.itemIndex,
    itemCount: progress.itemCount,
    completedItems: progress.done ? progress.itemIndex : Math.max(0, progress.itemIndex - 1),
    bytesCompleted: progress.bytesCopied,
    totalBytes: progress.totalBytes,
  });
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

  paste: async (targetDir: string, _targetProjectPath: string) => {
    const { items } = get();
    if (items.length === 0) return false;

    const clipboardItems = [...items];
    const action = clipboardItems[0].action;
    const abortController = new AbortController();
    const operationStore = useFileOperationStore.getState();
    const operationId = operationStore.startOperation({
      kind: action === 'cut' ? 'move' : 'copy',
      title: action === 'cut' ? '正在移动文件' : '正在复制文件',
      detail: `目标：${targetDir}`,
      itemCount: clipboardItems.length,
      onCancel: () => abortController.abort(),
    });

    try {
      if (action === 'cut') {
        for (const [index, item] of clipboardItems.entries()) {
          if (abortController.signal.aborted) {
            throw new Error('粘贴已取消');
          }

          operationStore.updateOperation(operationId, {
            currentName: item.name,
            itemIndex: index + 1,
            completedItems: index,
            bytesCompleted: 0,
            totalBytes: 0,
          });
          await invoke('move_project_entry', {
            projectPath: item.projectPath,
            source: item.path,
            target: targetDir,
            conflictStrategy: 'error',
          });

          operationStore.updateOperation(operationId, {
            completedItems: index + 1,
          });
        }
      } else {
        const result = await importExternalPaths(
          clipboardItems.map((item) => item.path),
          targetDir,
          {
            signal: abortController.signal,
            onProgress: (progress) => updateTransferProgress(operationId, progress),
          },
        );

        if (result.failedItems.length > 0) {
          throw new Error(result.failedItems[0]);
        }
        if (result.skippedCount > 0 || result.successCount !== clipboardItems.length) {
          throw new Error('目标位置已存在同名文件或文件夹');
        }
      }

      if (action === 'cut') {
        set({ items: [] });
      }

      operationStore.completeOperation(operationId, {
        title: action === 'cut' ? '移动完成' : '复制完成',
        detail: `${clipboardItems.length} 个项目，目标：${targetDir}`,
        currentName: '',
        completedItems: clipboardItems.length,
      });
      return true;
    } catch (error) {
      if (abortController.signal.aborted || isExternalImportCancelled(error) || String(error).includes('粘贴已取消')) {
        operationStore.markOperationCancelled(operationId);
        return false;
      }

      console.error('Paste failed:', error);
      const message = String(error).startsWith('PM_CONFLICT:')
        ? '目标位置已存在同名文件'
        : '操作失败: ' + error;
      operationStore.failOperation(operationId, message, {
        title: action === 'cut' ? '移动失败' : '复制失败',
      });
      useUiStore.getState().showToast({
        title: '粘贴失败',
        message,
        tone: 'error',
      });
      return false;
    }
  },

  pasteSystem: async (targetDir: string) => {
    const status = await invoke<SystemClipboardStatus>('get_system_clipboard_status');
    if (!status.hasFiles && !status.hasImage) {
      return 0;
    }

    const operationStore = useFileOperationStore.getState();

    if (status.hasFiles) {
      const sourcePaths = await invoke<string[]>('get_system_clipboard_files');
      if (sourcePaths.length === 0) {
        return 0;
      }

      const abortController = new AbortController();
      const operationId = operationStore.startOperation({
        kind: 'paste',
        title: '正在粘贴文件',
        detail: `目标：${targetDir}`,
        itemCount: sourcePaths.length,
        onCancel: () => abortController.abort(),
      });

      try {
        const result = await importExternalPaths(sourcePaths, targetDir, {
          signal: abortController.signal,
          requestConflictChoice: async () => ({ action: 'rename' }),
          onProgress: (progress) => updateTransferProgress(operationId, progress),
        });

        if (result.failedItems.length > 0) {
          throw new Error(result.failedItems[0]);
        }

        operationStore.completeOperation(operationId, {
          title: '粘贴完成',
          detail: `${result.successCount} 个项目，目标：${targetDir}`,
          currentName: '',
          completedItems: result.successCount,
        });
        return result.successCount;
      } catch (error) {
        if (abortController.signal.aborted || isExternalImportCancelled(error)) {
          operationStore.markOperationCancelled(operationId);
          return 0;
        }

        operationStore.failOperation(operationId, String(error), { title: '粘贴失败' });
        throw error;
      }
    }

    const operationId = operationStore.startOperation({
      kind: 'paste',
      title: '正在粘贴图片',
      detail: `目标：${targetDir}`,
      itemCount: 1,
    });

    try {
      const pastedPaths = await invoke<string[]>('paste_system_clipboard', { targetDir });
      operationStore.completeOperation(operationId, {
        title: '粘贴完成',
        detail: `目标：${targetDir}`,
        completedItems: pastedPaths.length,
      });
      return pastedPaths.length;
    } catch (error) {
      operationStore.failOperation(operationId, String(error), { title: '粘贴失败' });
      throw error;
    }
  },

  clear: () => {
    set({ items: [] });
  },

  hasItem: () => {
    return get().items.length > 0;
  },
}));
