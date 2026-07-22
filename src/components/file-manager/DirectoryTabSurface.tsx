import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ArrowUp, Grid, List, RefreshCw } from 'lucide-react';
import { createProjectStore, ProjectStoreProvider } from '../../stores/projectStore';
import type { ProjectStoreApi } from '../../stores/projectStore';
import { useClipboardStore } from '../../stores/clipboardStore';
import { useFileDragStore } from '../../stores/fileDragStore';
import { useFileOperationStore } from '../../stores/fileOperationStore';
import { useUiStore } from '../../stores/uiStore';
import { FileList } from './FileList';
import { FileDetail } from './FileDetail';
import { FileTree } from './FileTree';
import {
  buildRenamedFileName,
  getParentPath,
  getPathLabel as getProjectPathLabel,
  isExternalFileDrag,
  joinPath,
  normalizePath,
} from './dragDrop';
import {
  importExternalDrop,
  isExternalImportCancelled,
  type ConflictResolution,
} from './externalImport';
import { MoveConflictDialog } from './MoveConflictDialog';

interface ProjectFsChangeEventPayload {
  projectPath: string;
  filePath: string;
  changeType: 'created' | 'modified' | 'deleted' | 'renamed' | string;
  isDir: boolean;
  isRename: boolean;
  timestamp: number;
}

interface ThumbnailCacheUpdatedEventPayload {
  projectPath: string;
  directoryPath: string;
  updatedCount: number;
}

interface DirectoryTabSurfaceProps {
  initialPath: string;
  isActive: boolean;
  projectPath?: string | null;
  projectName?: string | null;
  onOpenDirectoryTab?: (path: string) => Promise<void> | void;
  toolbarActions?: ReactNode;
  treePanelWidth: number;
  isResizingTreePanel: boolean;
  onStartTreeResize: (event: React.MouseEvent<HTMLDivElement>) => void;
  detailsPanelWidth: number;
  isResizingDetailsPanel: boolean;
  onStartDetailsResize: (event: React.MouseEvent<HTMLDivElement>) => void;
}

function getPathName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  const tagName = target.tagName.toLowerCase();
  return tagName === 'input' || tagName === 'textarea' || tagName === 'select' || target.isContentEditable;
}

function getPathLabel(path: string, projectPath?: string | null, projectName?: string | null) {
  if (!projectPath || !projectName) {
    return path;
  }

  const normalizedProjectPath = normalizePath(projectPath);
  const normalizedPath = normalizePath(path);

  if (normalizedPath === normalizedProjectPath) {
    return projectName;
  }

  if (normalizedPath.startsWith(`${normalizedProjectPath}/`)) {
    return normalizedPath.replace(normalizedProjectPath, projectName);
  }

  return path;
}

function isSameOrDirectChildPath(eventPath: string, directoryPath: string): boolean {
  const normalizedEventPath = normalizePath(eventPath);
  const normalizedDirectoryPath = normalizePath(directoryPath);

  if (normalizedEventPath === normalizedDirectoryPath) {
    return true;
  }

  return normalizePath(getParentPath(eventPath)) === normalizedDirectoryPath;
}

function createDirectoryTabStore(
  initialPath: string,
  projectPath?: string | null,
  projectName?: string | null,
) {
  const store = createProjectStore();

  store.setState({
    projectPath: projectPath || null,
    projectName: projectName || (projectPath ? getPathName(projectPath) : null),
    isInitialized: true,
    currentPath: initialPath,
    expandedKeys: new Set(projectPath ? [projectPath, initialPath] : [initialPath]),
  });

  return store;
}

export function DirectoryTabSurface({
  initialPath,
  isActive,
  projectPath,
  projectName,
  onOpenDirectoryTab,
  toolbarActions,
  treePanelWidth,
  isResizingTreePanel,
  onStartTreeResize,
  detailsPanelWidth,
  isResizingDetailsPanel,
  onStartDetailsResize,
}: DirectoryTabSurfaceProps) {
  const hasActiveInternalDrag = useFileDragStore((state) => state.draggedPaths.length > 0);
  const showToast = useUiStore((state) => state.showToast);
  const [directoryStore, setDirectoryStore] = useState<ProjectStoreApi>(() =>
    createDirectoryTabStore(initialPath, projectPath, projectName),
  );
  const [currentDirectory, setCurrentDirectory] = useState(initialPath);
  const [viewMode, setViewMode] = useState(directoryStore.getState().viewMode);
  const [isLoadingInitialDirectory, setIsLoadingInitialDirectory] = useState(false);
  const [isDragImportActive, setIsDragImportActive] = useState(false);
  const [isImportingDrop, setIsImportingDrop] = useState(false);
  const externalImportAbortRef = useRef<AbortController | null>(null);
  const [externalDropConflictState, setExternalDropConflictState] = useState({
    isOpen: false,
    sourceName: '',
    targetLabel: '',
    renameName: '',
  });
  const externalDragDepthRef = useRef(0);
  const externalDropConflictResolverRef = useRef<((choice: ConflictResolution) => void) | null>(null);

  useEffect(() => {
    const nextStore = createDirectoryTabStore(initialPath, projectPath, projectName);
    setDirectoryStore(nextStore);
  }, [initialPath, projectName, projectPath]);

  useEffect(() => {
    let cancelled = false;
    const unsubscribe = directoryStore.subscribe((state, previous) => {
      if (state.currentPath !== previous.currentPath && state.currentPath) {
        setCurrentDirectory(state.currentPath);
      }
      if (state.viewMode !== previous.viewMode) {
        setViewMode(state.viewMode);
      }
    });

    setCurrentDirectory(directoryStore.getState().currentPath || initialPath);
    setViewMode(directoryStore.getState().viewMode);
    setIsLoadingInitialDirectory(true);
    void directoryStore
      .getState()
      .loadDirectory(initialPath, true)
      .then(async () => {
        const state = directoryStore.getState();
        await Promise.all([
          state.loadTree(true),
          state.loadTags(),
          state.refreshMdtIndex(),
        ]);
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoadingInitialDirectory(false);
        }
      });

    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, [directoryStore, initialPath]);

  const atInitialDirectory = useMemo(
    () => normalizePath(currentDirectory) === normalizePath(initialPath),
    [currentDirectory, initialPath],
  );

  const handleOpenParent = useCallback(() => {
    if (atInitialDirectory) {
      return;
    }

    const parentPath = getParentPath(currentDirectory);
    if (!parentPath || normalizePath(parentPath) === normalizePath(currentDirectory)) {
      return;
    }

    void directoryStore.getState().loadDirectory(parentPath);
  }, [atInitialDirectory, currentDirectory, directoryStore]);

  const handleRefresh = useCallback(() => {
    void directoryStore.getState().refresh(true, true);
  }, [directoryStore]);

  const handleSetViewMode = useCallback(
    (mode: 'list' | 'grid') => {
      directoryStore.getState().setViewMode(mode);
      setViewMode(mode);
    },
    [directoryStore],
  );

  const handleOpenDirectoryTab = useCallback(
    async (path: string) => {
      await onOpenDirectoryTab?.(path);
    },
    [onOpenDirectoryTab],
  );

  const getSelectedClipboardItems = useCallback(() => {
    const state = directoryStore.getState();
    if (!state.projectPath || state.selectedFiles.size === 0) {
      return [];
    }

    const displayFiles = state.searchQuery ? state.searchResults : state.files;
    const fileMap = new Map(
      [...state.files, ...displayFiles].map((file) => [file.path, file]),
    );

    return Array.from(state.selectedFiles).map((path) => {
      const file = fileMap.get(path);
      return {
        path,
        name: file?.name || getPathName(path),
        projectPath: state.projectPath!,
      };
    });
  }, [directoryStore]);

  const handleCopySelection = useCallback(
    (action: 'copy' | 'cut') => {
      const selectedItems = getSelectedClipboardItems();
      if (selectedItems.length === 0) {
        return false;
      }

      if (action === 'copy') {
        useClipboardStore.getState().copyItems(selectedItems);
      } else {
        useClipboardStore.getState().cutItems(selectedItems);
      }

      showToast({
        title: action === 'copy' ? '已复制' : '已剪切',
        message: selectedItems.length === 1
          ? selectedItems[0].name
          : `已选择 ${selectedItems.length} 个项目`,
        tone: 'success',
      });
      return true;
    },
    [getSelectedClipboardItems, showToast],
  );

  const handlePasteIntoCurrentDirectory = useCallback(async () => {
    const state = directoryStore.getState();
    const targetDir = state.currentPath || initialPath;
    if (!targetDir) {
      return false;
    }

    try {
      const clipboardStore = useClipboardStore.getState();
      const internalClipboardItems = clipboardStore.items;
      let pastedCount = 0;
      let success = false;

      if (internalClipboardItems.length > 0) {
        pastedCount = internalClipboardItems.length;
        success = await clipboardStore.paste(targetDir, state.projectPath || targetDir);
      } else {
        pastedCount = await clipboardStore.pasteSystem(targetDir);
        success = pastedCount > 0;
      }

      if (!success) {
        return false;
      }

      await directoryStore.getState().refresh(true, true);
      showToast({
        title: '已粘贴',
        message: pastedCount > 1 ? `已粘贴 ${pastedCount} 个项目。` : '已粘贴到当前目录。',
        tone: 'success',
      });
      return true;
    } catch (error) {
      console.error('Failed to paste in directory tab:', error);
      showToast({
        title: '粘贴失败',
        message: String(error),
        tone: 'error',
      });
      return false;
    }
  }, [directoryStore, initialPath, showToast]);

  const handleSelectAllVisibleFiles = useCallback(() => {
    const state = directoryStore.getState();
    const displayFiles = state.searchQuery ? state.searchResults : state.files;

    if (displayFiles.length === 0) {
      return false;
    }

    directoryStore.setState({
      selectedFiles: new Set(displayFiles.map((file) => file.path)),
    });
    return true;
  }, [directoryStore]);

  const handleDeleteSelection = useCallback(async () => {
    const state = directoryStore.getState();
    const selectedPaths = Array.from(state.selectedFiles);
    if (selectedPaths.length === 0) {
      return false;
    }

    try {
      const deletedCount = await invoke<number>('delete_paths', { paths: selectedPaths });
      await directoryStore.getState().refresh(true, true);

      if (deletedCount === 0) {
        showToast({
          title: '未删除任何项目',
          message: '选中的文件可能已经不存在，列表已刷新。',
          tone: 'warning',
        });
        return false;
      }

      showToast({
        title: deletedCount > 1 ? '已移到回收站' : '文件已移到回收站',
        message: deletedCount > 1
          ? `已将 ${deletedCount} 个项目移到回收站。`
          : `已将 ${getPathName(selectedPaths[0])} 移到回收站。`,
        tone: 'success',
      });
      return true;
    } catch (error) {
      console.error('Failed to delete in directory tab:', error);
      showToast({
        title: '删除失败',
        message: String(error),
        tone: 'error',
      });
      return false;
    }
  }, [directoryStore, showToast]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!isActive) {
        return;
      }

      if (isEditableTarget(event.target)) {
        return;
      }

      const lowerKey = event.key.toLowerCase();
      const hasCommandModifier = event.ctrlKey || event.metaKey;

      if (!hasCommandModifier && !event.altKey && !event.shiftKey && event.key === 'Delete') {
        event.preventDefault();
        void handleDeleteSelection();
        return;
      }

      if (!hasCommandModifier || event.shiftKey || event.altKey) {
        return;
      }

      switch (lowerKey) {
        case 'a':
          event.preventDefault();
          handleSelectAllVisibleFiles();
          return;
        case 'c':
          event.preventDefault();
          handleCopySelection('copy');
          return;
        case 'x':
          event.preventDefault();
          handleCopySelection('cut');
          return;
        case 'v':
          event.preventDefault();
          void handlePasteIntoCurrentDirectory();
          return;
        case 'h': {
          event.preventDefault();
          const state = directoryStore.getState();
          const nextShowExcluded = !state.showExcludedFiles;
          state.toggleShowExcludedFiles();
          void state.refresh(true, true);
          showToast({
            title: nextShowExcluded ? '已显示排除项' : '已隐藏排除项',
            message: nextShowExcluded
              ? '当前目录会显示被排除规则隐藏的文件，按 Ctrl+H 可切回隐藏。'
              : '当前目录已恢复隐藏被排除规则匹配的文件。',
            tone: 'info',
          });
          return;
        }
        default:
          return;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [
    directoryStore,
    handleCopySelection,
    handleDeleteSelection,
    handlePasteIntoCurrentDirectory,
    handleSelectAllVisibleFiles,
    isActive,
    showToast,
  ]);

  const resetExternalDragState = useCallback(() => {
    externalDragDepthRef.current = 0;
    setIsDragImportActive(false);
  }, []);

  const buildExternalDropSuggestedRename = useCallback(async (sourceName: string, targetDir: string) => {
    for (let index = 1; ; index += 1) {
      const candidate = buildRenamedFileName(sourceName, index);
      const exists = await invoke<boolean>('path_exists', {
        path: joinPath(targetDir, candidate),
      });
      if (!exists) {
        return candidate;
      }
    }
  }, []);

  const requestExternalDropConflictChoice = useCallback(
    async (sourceName: string, targetLabel: string, targetDir: string) => {
      const renameName = await buildExternalDropSuggestedRename(sourceName, targetDir);

      return new Promise<ConflictResolution>((resolve) => {
        externalDropConflictResolverRef.current = resolve;
        setExternalDropConflictState({
          isOpen: true,
          sourceName,
          targetLabel,
          renameName,
        });
      });
    },
    [buildExternalDropSuggestedRename],
  );

  const resolveExternalDropConflictChoice = useCallback((choice: ConflictResolution) => {
    externalDropConflictResolverRef.current?.(choice);
    externalDropConflictResolverRef.current = null;
    setExternalDropConflictState({
      isOpen: false,
      sourceName: '',
      targetLabel: '',
      renameName: '',
    });
  }, []);

  const handleCancelExternalImport = useCallback(() => {
    externalImportAbortRef.current?.abort();
    resolveExternalDropConflictChoice({ action: 'cancel' });
  }, [resolveExternalDropConflictChoice]);

  const handleExternalDragEnter = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (isImportingDrop || !isExternalFileDrag(event.dataTransfer, hasActiveInternalDrag)) {
        return;
      }

      event.preventDefault();
      externalDragDepthRef.current += 1;
      setIsDragImportActive(true);
    },
    [hasActiveInternalDrag, isImportingDrop],
  );

  const handleExternalDragOver = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (isImportingDrop || !isExternalFileDrag(event.dataTransfer, hasActiveInternalDrag)) {
        return;
      }

      event.preventDefault();
      event.dataTransfer.dropEffect = 'copy';
      setIsDragImportActive(true);
    },
    [hasActiveInternalDrag, isImportingDrop],
  );

  const handleExternalDragLeave = useCallback(
    (event: React.DragEvent<HTMLDivElement>) => {
      if (isImportingDrop || !isExternalFileDrag(event.dataTransfer, hasActiveInternalDrag)) {
        return;
      }

      event.preventDefault();
      externalDragDepthRef.current = Math.max(0, externalDragDepthRef.current - 1);

      if (externalDragDepthRef.current === 0 && !isImportingDrop) {
        setIsDragImportActive(false);
      }
    },
    [hasActiveInternalDrag, isImportingDrop],
  );

  const handleExternalDrop = useCallback(
    async (event: React.DragEvent<HTMLDivElement>) => {
      if (isImportingDrop || !isExternalFileDrag(event.dataTransfer, hasActiveInternalDrag)) {
        return;
      }

      event.preventDefault();
      resetExternalDragState();

      const targetDir = directoryStore.getState().currentPath || initialPath;
      setIsImportingDrop(true);
      const importAbortController = new AbortController();
      externalImportAbortRef.current = importAbortController;
      const operationStore = useFileOperationStore.getState();
      const operationId = operationStore.startOperation({
        kind: 'import',
        title: '正在导入文件',
        detail: `目标：${getProjectPathLabel(targetDir, projectPath || null, projectName || null)}`,
        onCancel: handleCancelExternalImport,
      });

      try {
        const {
          successCount,
          overwriteCount,
          renameCount,
          skippedCount,
          failedItems,
        } = await importExternalDrop(event.dataTransfer, targetDir, {
          targetLabel: getProjectPathLabel(targetDir, projectPath || null, projectName || null),
          requestConflictChoice: (sourceName, targetLabel) =>
            requestExternalDropConflictChoice(sourceName, targetLabel, targetDir),
          onProgress: (progress) => {
            operationStore.updateOperation(operationId, {
              currentName: progress.currentName,
              itemIndex: progress.itemIndex,
              itemCount: progress.itemCount,
              completedItems: progress.done ? progress.itemIndex : Math.max(0, progress.itemIndex - 1),
              bytesCompleted: progress.bytesCopied,
              totalBytes: progress.totalBytes,
            });
          },
          signal: importAbortController.signal,
        });

        const operationSummary = [
          successCount > 0 ? `导入 ${successCount} 个` : '',
          skippedCount > 0 ? `跳过 ${skippedCount} 个` : '',
          failedItems.length > 0 ? `失败 ${failedItems.length} 个` : '',
        ].filter(Boolean).join('，');

        if (failedItems.length > 0) {
          operationStore.failOperation(operationId, failedItems[0], {
            title: successCount > 0 ? '导入部分完成' : '导入失败',
            detail: operationSummary,
            completedItems: successCount,
          });
        } else {
          operationStore.completeOperation(operationId, {
            title: '导入完成',
            detail: operationSummary || `目标：${getProjectPathLabel(targetDir, projectPath || null, projectName || null)}`,
            completedItems: successCount + skippedCount,
          });
        }

        await directoryStore.getState().refresh(true, true);

        if (successCount > 0 || failedItems.length > 0 || (skippedCount > 0 && successCount > 0)) {
          const summaryParts = [];
          if (successCount > 0) summaryParts.push(`导入 ${successCount} 个`);
          if (overwriteCount > 0) summaryParts.push(`覆盖 ${overwriteCount} 个`);
          if (renameCount > 0) summaryParts.push(`重命名 ${renameCount} 个`);
          if (skippedCount > 0) summaryParts.push(`跳过 ${skippedCount} 个`);
          if (failedItems.length > 0) summaryParts.push(`失败 ${failedItems.length} 个`);

          showToast({
            title: failedItems.length > 0
              ? (successCount > 0 ? '导入部分完成' : '导入失败')
              : '导入完成',
            message: `${summaryParts.join('，')}，目标目录：${getProjectPathLabel(
              targetDir,
              projectPath || null,
              projectName || null,
            )}`,
            tone: failedItems.length > 0
              ? (successCount > 0 ? 'warning' : 'error')
            : 'success',
          });
        }
      } catch (error) {
        if (isExternalImportCancelled(error)) {
          operationStore.markOperationCancelled(operationId);
          showToast({
            title: '导入已取消',
            message: `已停止导入到 ${getProjectPathLabel(targetDir, projectPath || null, projectName || null)}`,
            tone: 'warning',
          });
        } else {
          operationStore.failOperation(operationId, String(error), { title: '导入失败' });
          showToast({
            title: '导入失败',
            message: String(error),
            tone: 'error',
          });
        }
      } finally {
        if (externalImportAbortRef.current === importAbortController) {
          externalImportAbortRef.current = null;
        }
        setIsImportingDrop(false);
      }
    },
    [
      directoryStore,
      handleCancelExternalImport,
      hasActiveInternalDrag,
      initialPath,
      isImportingDrop,
      projectName,
      projectPath,
      requestExternalDropConflictChoice,
      resetExternalDragState,
      showToast,
    ],
  );

  useEffect(() => {
    return () => {
      externalImportAbortRef.current?.abort();
      externalDropConflictResolverRef.current?.({ action: 'cancel' });
      externalDropConflictResolverRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!projectPath) {
      return;
    }

    let unlisten: (() => void) | null = null;
    let cancelled = false;

    const registerFsChangeListener = async () => {
      try {
        unlisten = await listen<ProjectFsChangeEventPayload>('pm-center:project-fs-change', (event) => {
          const payload = event.payload;
          if (!payload?.projectPath || !payload.filePath) {
            return;
          }

          if (normalizePath(payload.projectPath) !== normalizePath(projectPath)) {
            return;
          }

          if (
            payload.changeType !== 'created' &&
            payload.changeType !== 'deleted' &&
            payload.changeType !== 'renamed'
          ) {
            return;
          }

          const activeDirectory = directoryStore.getState().currentPath;
          if (!activeDirectory || !isSameOrDirectChildPath(payload.filePath, activeDirectory)) {
            return;
          }

          void directoryStore.getState().refresh(true, true);
        });

        if (cancelled && unlisten) {
          await unlisten();
          unlisten = null;
        }
      } catch (error) {
        console.error('Failed to listen directory tab fs changes:', error);
      }
    };

    void registerFsChangeListener();

    return () => {
      cancelled = true;
      if (unlisten) {
        void unlisten();
      }
    };
  }, [directoryStore, projectPath]);

  useEffect(() => {
    if (!projectPath) {
      return;
    }

    let unlisten: (() => void) | null = null;
    let cancelled = false;

    const registerThumbnailListener = async () => {
      try {
        unlisten = await listen<ThumbnailCacheUpdatedEventPayload>(
          'pm-center:thumbnail-cache-updated',
          (event) => {
            const payload = event.payload;
            if (!payload?.projectPath || !payload.directoryPath) {
              return;
            }

            if (normalizePath(payload.projectPath) !== normalizePath(projectPath)) {
              return;
            }

            const activeDirectory = directoryStore.getState().currentPath;
            if (!activeDirectory || normalizePath(activeDirectory) !== normalizePath(payload.directoryPath)) {
              return;
            }

            void directoryStore.getState().refresh(true, true);
          },
        );

        if (cancelled && unlisten) {
          await unlisten();
          unlisten = null;
        }
      } catch (error) {
        console.error('Failed to listen directory tab thumbnail updates:', error);
      }
    };

    void registerThumbnailListener();

    return () => {
      cancelled = true;
      if (unlisten) {
        void unlisten();
      }
    };
  }, [directoryStore, projectPath]);

  const dropTargetLabel = getProjectPathLabel(currentDirectory, projectPath || null, projectName || null);
  const showDropOverlay = isDragImportActive && !isImportingDrop;

  return (
    <ProjectStoreProvider store={directoryStore}>
      <div
        className="relative flex h-full min-h-0 flex-col bg-white dark:bg-gray-900"
        onDragEnter={handleExternalDragEnter}
        onDragOver={handleExternalDragOver}
        onDragLeave={handleExternalDragLeave}
        onDrop={handleExternalDrop}
      >
        <div className="flex items-center gap-2 border-b border-gray-200 px-3 py-2 dark:border-gray-700">
          <button
            type="button"
            onClick={handleOpenParent}
            disabled={atInitialDirectory}
            className="rounded-md p-1.5 text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-900 disabled:cursor-not-allowed disabled:opacity-40 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title={atInitialDirectory ? '已经在此小标签主目录' : '返回上级目录'}
          >
            <ArrowUp className="h-4 w-4" />
          </button>
          <button
            type="button"
            onClick={handleRefresh}
            className="rounded-md p-1.5 text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title="刷新"
          >
            <RefreshCw className="h-4 w-4" />
          </button>
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold text-gray-900 dark:text-gray-100">
              {getPathName(initialPath)}
            </div>
            <div className="truncate text-xs text-gray-500 dark:text-gray-400" title={currentDirectory}>
              {getPathLabel(currentDirectory, projectPath, projectName)}
            </div>
          </div>
          {toolbarActions && (
            <div className="flex shrink-0 items-center">
              {toolbarActions}
            </div>
          )}
          <div className="flex items-center rounded-md bg-gray-100 p-0.5 dark:bg-gray-800">
            <button
              type="button"
              onClick={() => handleSetViewMode('list')}
              className={`rounded p-1.5 transition-colors ${
                viewMode === 'list'
                  ? 'bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100'
                  : 'text-gray-500 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100'
              }`}
              title="列表视图"
            >
              <List className="h-4 w-4" />
            </button>
            <button
              type="button"
              onClick={() => handleSetViewMode('grid')}
              className={`rounded p-1.5 transition-colors ${
                viewMode === 'grid'
                  ? 'bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100'
                  : 'text-gray-500 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100'
              }`}
              title="网格视图"
            >
              <Grid className="h-4 w-4" />
            </button>
          </div>
        </div>

        <div className="flex min-h-0 flex-1">
          <div
            className="min-w-0 flex-shrink-0 border-r border-gray-200 dark:border-gray-700"
            style={{ width: `${treePanelWidth}px` }}
          >
            {isLoadingInitialDirectory ? (
              <div className="flex h-full items-center justify-center text-sm text-gray-400">
                正在读取目录...
              </div>
            ) : (
              <FileTree
                onOpenDirectoryTab={handleOpenDirectoryTab}
                rootPath={initialPath}
                rootTitle={getPathName(initialPath)}
              />
            )}
          </div>

          <div
            className="group relative w-2 flex-shrink-0 cursor-col-resize bg-transparent"
            onMouseDown={onStartTreeResize}
            title="拖动调整目录栏宽度"
          >
            <div
              className={`absolute inset-y-0 left-1/2 w-px -translate-x-1/2 transition-colors ${
                isResizingTreePanel
                  ? 'bg-blue-500'
                  : 'bg-gray-200 group-hover:bg-blue-400 dark:bg-gray-700 dark:group-hover:bg-blue-500'
              }`}
            />
          </div>

          <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
            {isLoadingInitialDirectory ? (
              <div className="flex h-full items-center justify-center text-sm text-gray-400">
                正在读取目录...
              </div>
            ) : (
              <FileList onOpenDirectoryTab={handleOpenDirectoryTab} />
            )}
          </div>

          <div
            className="group relative w-2 flex-shrink-0 cursor-col-resize bg-transparent"
            onMouseDown={onStartDetailsResize}
            title="拖动调整详情栏宽度"
          >
            <div
              className={`absolute inset-y-0 left-1/2 w-px -translate-x-1/2 transition-colors ${
                isResizingDetailsPanel
                  ? 'bg-blue-500'
                  : 'bg-gray-200 group-hover:bg-blue-400 dark:bg-gray-700 dark:group-hover:bg-blue-500'
              }`}
            />
          </div>

          <div
            className="min-w-0 flex-shrink-0 border-l border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900"
            style={{ width: `${detailsPanelWidth}px` }}
          >
            <FileDetail />
          </div>
        </div>

        {showDropOverlay && (
          <div className="pointer-events-none absolute inset-0 z-30 flex items-center justify-center bg-blue-500/10 backdrop-blur-[1px]">
            <div className="rounded-xl border border-blue-200 bg-white/95 px-5 py-4 text-center shadow-xl dark:border-blue-800 dark:bg-gray-900/95">
              <div className="text-sm font-semibold text-blue-700 dark:text-blue-200">
                释放后导入到当前目录
              </div>
              <div className="mt-1 max-w-[420px] truncate text-xs text-blue-600/80 dark:text-blue-300/80">
                {dropTargetLabel}
              </div>
            </div>
          </div>
        )}

        <MoveConflictDialog
          isOpen={externalDropConflictState.isOpen}
          sourceName={externalDropConflictState.sourceName}
          targetLabel={externalDropConflictState.targetLabel || dropTargetLabel}
          renameValue={externalDropConflictState.renameName}
          onRenameValueChange={(renameName) =>
            setExternalDropConflictState((state) => ({
              ...state,
              renameName,
            }))
          }
          actionLabel="导入"
          renameButtonText="重命名导入"
          overwriteButtonText="覆盖导入"
          onOverwrite={() => resolveExternalDropConflictChoice({ action: 'overwrite' })}
          onRename={() =>
            resolveExternalDropConflictChoice({
              action: 'rename',
              renameName: externalDropConflictState.renameName,
            })
          }
          onCancel={() => resolveExternalDropConflictChoice({ action: 'cancel' })}
        />
      </div>
    </ProjectStoreProvider>
  );
}
