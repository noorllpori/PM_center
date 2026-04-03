import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Upload } from 'lucide-react';
import { FileTree } from './FileTree';
import { FileList } from './FileList';
import { ColumnSettings } from './ColumnSettings';
import { FileDetail } from './FileDetail';
import { getPathLabel, isExternalFileDrag } from './dragDrop';
import { importExternalDrop } from './externalImport';
import { ChangeLog } from '../ChangeLog';
import { ImageViewerSurface } from '../image-viewer/ImageViewerSurface';
import { TextEditorSurface } from '../text-editor/TextEditorSurface';
import { WorkspaceTabBar } from '../workspace/WorkspaceTabBar';
import { useProjectStoreApi, useProjectStoreShallow } from '../../stores/projectStore';
import { useClipboardStore } from '../../stores/clipboardStore';
import { useUiStore } from '../../stores/uiStore';
import { useWorkspaceTabStore } from '../../stores/workspaceTabStore';

const FILE_TREE_PANEL_WIDTH_KEY = 'pm-center:file-tree-panel-width';
const FILE_TREE_PANEL_MIN_WIDTH = 220;
const FILE_TREE_PANEL_MAX_WIDTH = 520;
const FILE_TREE_PANEL_DEFAULT_WIDTH = 256;
const FILE_DETAILS_PANEL_WIDTH_KEY = 'pm-center:file-details-panel-width';
const FILE_DETAILS_PANEL_MIN_WIDTH = 260;
const FILE_DETAILS_PANEL_MAX_WIDTH = 720;
const FILE_DETAILS_PANEL_DEFAULT_WIDTH = 320;

interface SystemClipboardStatus {
  hasFiles: boolean;
  hasImage: boolean;
}

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  const tagName = target.tagName.toLowerCase();
  return tagName === 'input' || tagName === 'textarea' || tagName === 'select' || target.isContentEditable;
}

function clampFileTreePanelWidth(width: number) {
  return Math.min(FILE_TREE_PANEL_MAX_WIDTH, Math.max(FILE_TREE_PANEL_MIN_WIDTH, width));
}

function getInitialFileTreePanelWidth() {
  if (typeof window === 'undefined') {
    return FILE_TREE_PANEL_DEFAULT_WIDTH;
  }

  const storedWidth = window.localStorage.getItem(FILE_TREE_PANEL_WIDTH_KEY);
  const parsedWidth = storedWidth ? Number.parseInt(storedWidth, 10) : Number.NaN;

  if (!Number.isFinite(parsedWidth)) {
    return FILE_TREE_PANEL_DEFAULT_WIDTH;
  }

  return clampFileTreePanelWidth(parsedWidth);
}

function clampFileDetailsPanelWidth(width: number) {
  return Math.min(FILE_DETAILS_PANEL_MAX_WIDTH, Math.max(FILE_DETAILS_PANEL_MIN_WIDTH, width));
}

function getInitialFileDetailsPanelWidth() {
  if (typeof window === 'undefined') {
    return FILE_DETAILS_PANEL_DEFAULT_WIDTH;
  }

  const storedWidth = window.localStorage.getItem(FILE_DETAILS_PANEL_WIDTH_KEY);
  const parsedWidth = storedWidth ? Number.parseInt(storedWidth, 10) : Number.NaN;

  if (!Number.isFinite(parsedWidth)) {
    return FILE_DETAILS_PANEL_DEFAULT_WIDTH;
  }

  return clampFileDetailsPanelWidth(parsedWidth);
}

function getPathName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

export function ProjectWorkspace() {
  const projectStore = useProjectStoreApi();
  const {
    isInitialized,
    projectPath,
    projectName,
    currentPath,
    refresh,
    toggleShowExcludedFiles,
    showExcludedFiles,
  } = useProjectStoreShallow((state) => ({
    isInitialized: state.isInitialized,
    projectPath: state.projectPath,
    projectName: state.projectName,
    currentPath: state.currentPath,
    refresh: state.refresh,
    toggleShowExcludedFiles: state.toggleShowExcludedFiles,
    showExcludedFiles: state.showExcludedFiles,
  }));
  const showToast = useUiStore((state) => state.showToast);
  const tabs = useWorkspaceTabStore((state) => state.tabs);
  const activeTabId = useWorkspaceTabStore((state) => state.activeTabId);
  const activateTab = useWorkspaceTabStore((state) => state.activateTab);
  const closeTab = useWorkspaceTabStore((state) => state.closeTab);
  const reorderTabs = useWorkspaceTabStore((state) => state.reorderTabs);
  const updateTabDirty = useWorkspaceTabStore((state) => state.updateTabDirty);

  const [isDragImportActive, setIsDragImportActive] = useState(false);
  const [isImportingDrop, setIsImportingDrop] = useState(false);
  const [fileTreePanelWidth, setFileTreePanelWidth] = useState(getInitialFileTreePanelWidth);
  const [isResizingFileTree, setIsResizingFileTree] = useState(false);
  const [fileDetailsPanelWidth, setFileDetailsPanelWidth] = useState(getInitialFileDetailsPanelWidth);
  const [isResizingFileDetails, setIsResizingFileDetails] = useState(false);
  const externalDragDepthRef = useRef(0);
  const fileTreeResizeStateRef = useRef<{ startX: number; startWidth: number } | null>(null);
  const fileDetailsResizeStateRef = useRef<{ startX: number; startWidth: number } | null>(null);
  const activeWorkspaceTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const isFilesWorkspaceActive = activeWorkspaceTab?.type === 'files';

  const getSelectedClipboardItems = useCallback(() => {
    const state = projectStore.getState();
    if (!state.projectPath) {
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
  }, [projectStore]);

  const handleCopySelection = useCallback((action: 'copy' | 'cut') => {
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
  }, [getSelectedClipboardItems, showToast]);

  const handlePasteIntoCurrentDirectory = useCallback(async () => {
    const state = projectStore.getState();
    const targetDir = state.currentPath || state.projectPath;
    if (!state.projectPath || !targetDir) {
      return false;
    }

    try {
      const clipboardStore = useClipboardStore.getState();
      const internalClipboardItems = clipboardStore.items;
      let pastedCount = 0;
      let success = false;

      if (internalClipboardItems.length > 0) {
        pastedCount = internalClipboardItems.length;
        success = await clipboardStore.paste(targetDir, state.projectPath);
      } else {
        const clipboardStatus = await invoke<SystemClipboardStatus>('get_system_clipboard_status');
        if (!clipboardStatus.hasFiles && !clipboardStatus.hasImage) {
          return false;
        }

        const pastedPaths = await invoke<string[]>('paste_system_clipboard', { targetDir });
        pastedCount = pastedPaths.length;
        success = pastedCount > 0;
      }

      if (!success) {
        return false;
      }

      await refresh();
      showToast({
        title: '已粘贴',
        message: pastedCount > 1 ? `已粘贴 ${pastedCount} 个项目。` : '已粘贴到当前目录。',
        tone: 'success',
      });
      return true;
    } catch (error) {
      console.error('Failed to paste from keyboard shortcut:', error);
      showToast({
        title: '粘贴失败',
        message: String(error),
        tone: 'error',
      });
      return false;
    }
  }, [projectStore, refresh, showToast]);

  const handleSelectAllVisibleFiles = useCallback(() => {
    const state = projectStore.getState();
    const displayFiles = state.searchQuery ? state.searchResults : state.files;

    if (displayFiles.length === 0) {
      return false;
    }

    projectStore.setState({
      selectedFiles: new Set(displayFiles.map((file) => file.path)),
    });
    return true;
  }, [projectStore]);

  const handleDeleteSelection = useCallback(async () => {
    const state = projectStore.getState();
    const selectedPaths = Array.from(state.selectedFiles);
    if (selectedPaths.length === 0) {
      return false;
    }

    try {
      const deletedCount = await invoke<number>('delete_paths', { paths: selectedPaths });
      await refresh();

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
      console.error('Failed to delete from keyboard shortcut:', error);
      showToast({
        title: '删除失败',
        message: String(error),
        tone: 'error',
      });
      return false;
    }
  }, [projectStore, refresh, showToast]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!isInitialized || !isFilesWorkspaceActive) {
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
          const nextShowExcluded = !projectStore.getState().showExcludedFiles;
          toggleShowExcludedFiles();
          void refresh();
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
    handleCopySelection,
    handleDeleteSelection,
    handlePasteIntoCurrentDirectory,
    handleSelectAllVisibleFiles,
    isFilesWorkspaceActive,
    isInitialized,
    projectStore,
    refresh,
    showToast,
    toggleShowExcludedFiles,
  ]);

  const resetExternalDragState = useCallback(() => {
    externalDragDepthRef.current = 0;
    setIsDragImportActive(false);
  }, []);

  useEffect(() => {
    if (!isFilesWorkspaceActive || !isInitialized) {
      resetExternalDragState();
    }
  }, [isFilesWorkspaceActive, isInitialized, resetExternalDragState]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    window.localStorage.setItem(FILE_TREE_PANEL_WIDTH_KEY, String(fileTreePanelWidth));
  }, [fileTreePanelWidth]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    window.localStorage.setItem(FILE_DETAILS_PANEL_WIDTH_KEY, String(fileDetailsPanelWidth));
  }, [fileDetailsPanelWidth]);

  const stopFileTreeResize = useCallback(() => {
    fileTreeResizeStateRef.current = null;
    setIsResizingFileTree(false);
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }, []);

  useEffect(() => {
    if (!isResizingFileTree) {
      return;
    }

    const handleMouseMove = (event: MouseEvent) => {
      const resizeState = fileTreeResizeStateRef.current;
      if (!resizeState) {
        return;
      }

      const nextWidth = resizeState.startWidth + (event.clientX - resizeState.startX);
      setFileTreePanelWidth(clampFileTreePanelWidth(nextWidth));
    };

    const handleMouseUp = () => {
      stopFileTreeResize();
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizingFileTree, stopFileTreeResize]);

  const stopFileDetailsResize = useCallback(() => {
    fileDetailsResizeStateRef.current = null;
    setIsResizingFileDetails(false);
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }, []);

  useEffect(() => {
    if (!isResizingFileDetails) {
      return;
    }

    const handleMouseMove = (event: MouseEvent) => {
      const resizeState = fileDetailsResizeStateRef.current;
      if (!resizeState) {
        return;
      }

      const nextWidth = resizeState.startWidth - (event.clientX - resizeState.startX);
      setFileDetailsPanelWidth(clampFileDetailsPanelWidth(nextWidth));
    };

    const handleMouseUp = () => {
      stopFileDetailsResize();
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizingFileDetails, stopFileDetailsResize]);

  useEffect(() => {
    return () => {
      stopFileTreeResize();
      stopFileDetailsResize();
    };
  }, [stopFileDetailsResize, stopFileTreeResize]);

  const handleStartFileTreeResize = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    fileTreeResizeStateRef.current = {
      startX: event.clientX,
      startWidth: fileTreePanelWidth,
    };
    setIsResizingFileTree(true);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, [fileTreePanelWidth]);

  const handleStartFileDetailsResize = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    fileDetailsResizeStateRef.current = {
      startX: event.clientX,
      startWidth: fileDetailsPanelWidth,
    };
    setIsResizingFileDetails(true);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, [fileDetailsPanelWidth]);

  const handleExternalDragEnter = useCallback((event: React.DragEvent<HTMLDivElement>) => {
    if (!isInitialized || !isFilesWorkspaceActive || !isExternalFileDrag(event.dataTransfer)) {
      return;
    }

    event.preventDefault();
    externalDragDepthRef.current += 1;
    setIsDragImportActive(true);
  }, [isFilesWorkspaceActive, isInitialized]);

  const handleExternalDragOver = useCallback((event: React.DragEvent<HTMLDivElement>) => {
    if (!isInitialized || !isFilesWorkspaceActive || !isExternalFileDrag(event.dataTransfer)) {
      return;
    }

    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';

    if (!isDragImportActive) {
      setIsDragImportActive(true);
    }
  }, [isDragImportActive, isFilesWorkspaceActive, isInitialized]);

  const handleExternalDragLeave = useCallback((event: React.DragEvent<HTMLDivElement>) => {
    if (!isInitialized || !isFilesWorkspaceActive || !isExternalFileDrag(event.dataTransfer)) {
      return;
    }

    event.preventDefault();
    externalDragDepthRef.current = Math.max(0, externalDragDepthRef.current - 1);

    if (externalDragDepthRef.current === 0 && !isImportingDrop) {
      setIsDragImportActive(false);
    }
  }, [isFilesWorkspaceActive, isImportingDrop, isInitialized]);

  const handleExternalDrop = useCallback(async (event: React.DragEvent<HTMLDivElement>) => {
    if (!isInitialized || !isFilesWorkspaceActive || !isExternalFileDrag(event.dataTransfer)) {
      return;
    }

    event.preventDefault();
    resetExternalDragState();

    const targetDir = currentPath || projectPath;
    if (!targetDir) {
      return;
    }

    setIsImportingDrop(true);

    try {
      const { failedItems } = await importExternalDrop(event.dataTransfer, targetDir);

      try {
        await refresh();
      } catch (error) {
        console.error('Refresh after drop import failed:', error);
      }

      if (failedItems.length > 0) {
        console.warn('External drop import completed with failures:', {
          failedItems,
          targetDir,
        });
      }
    } finally {
      setIsImportingDrop(false);
    }
  }, [currentPath, isFilesWorkspaceActive, isInitialized, projectPath, refresh, resetExternalDragState]);

  const dropTargetLabel = getPathLabel(currentPath || projectPath, projectPath, projectName);
  const showDropOverlay = isInitialized && isFilesWorkspaceActive && (isDragImportActive || isImportingDrop);

  if (!isInitialized) {
    return (
      <div className="flex h-full min-h-0 items-center justify-center bg-white text-sm text-gray-400 dark:bg-gray-900">
        正在打开项目...
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col">
      {showExcludedFiles && isFilesWorkspaceActive && (
        <div className="border-b border-amber-200 bg-amber-50 px-4 py-2 text-xs text-amber-700">
          当前正在显示排除规则隐藏的文件，按 `Ctrl+H` 可切回隐藏。
        </div>
      )}

      <WorkspaceTabBar
        tabs={tabs}
        activeTabId={activeTabId}
        onActivateTab={activateTab}
        onCloseTab={closeTab}
        onReorderTabs={reorderTabs}
      />

      <div
        className="relative flex-1 min-h-0 overflow-hidden"
        onDragEnter={handleExternalDragEnter}
        onDragOver={handleExternalDragOver}
        onDragLeave={handleExternalDragLeave}
        onDrop={handleExternalDrop}
      >
        <div className="h-full min-h-0 overflow-hidden">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;

            return (
              <div
                key={tab.id}
                className={`${isActive ? 'h-full w-full min-w-0 min-h-0' : 'hidden h-full w-full min-w-0 min-h-0'}`}
              >
                {tab.type === 'files' && (
                  <div className="flex h-full w-full min-w-0 min-h-0">
                    <div
                      className="border-r border-gray-200 dark:border-gray-700 flex-shrink-0 min-w-0"
                      style={{ width: `${fileTreePanelWidth}px` }}
                    >
                      <FileTree />
                    </div>

                    <div
                      className="group relative w-2 cursor-col-resize flex-shrink-0 bg-transparent"
                      onMouseDown={handleStartFileTreeResize}
                      title="拖动调整目录栏宽度"
                    >
                      <div
                        className={`absolute inset-y-0 left-1/2 w-px -translate-x-1/2 transition-colors ${
                          isResizingFileTree
                            ? 'bg-blue-500'
                            : 'bg-gray-200 dark:bg-gray-700 group-hover:bg-blue-400 dark:group-hover:bg-blue-500'
                        }`}
                      />
                    </div>

                    <div className="flex-1 min-w-0 min-h-0 overflow-hidden">
                      <FileList />
                    </div>

                    <div
                      className="group relative w-2 cursor-col-resize flex-shrink-0 bg-transparent"
                      onMouseDown={handleStartFileDetailsResize}
                      title="拖动调整详情栏宽度"
                    >
                      <div
                        className={`absolute inset-y-0 left-1/2 w-px -translate-x-1/2 transition-colors ${
                          isResizingFileDetails
                            ? 'bg-blue-500'
                            : 'bg-gray-200 dark:bg-gray-700 group-hover:bg-blue-400 dark:group-hover:bg-blue-500'
                        }`}
                      />
                    </div>

                    <div
                      className="border-l border-gray-200 dark:border-gray-700 flex-shrink-0 bg-white dark:bg-gray-900 min-w-0"
                      style={{ width: `${fileDetailsPanelWidth}px` }}
                    >
                      <FileDetail />
                    </div>
                  </div>
                )}

                {tab.type === 'logs' && (
                  <div className="h-full w-full min-w-0 min-h-0">
                    <ChangeLog />
                  </div>
                )}

                {tab.type === 'image' && tab.filePath && (
                  <div className="h-full w-full min-w-0 min-h-0 overflow-hidden">
                    <ImageViewerSurface
                      title={tab.title}
                      source={tab.filePath}
                    />
                  </div>
                )}

                {tab.type === 'text' && tab.filePath && (
                  <div className="h-full w-full min-w-0 min-h-0">
                    <TextEditorSurface
                      title={tab.title}
                      filePath={tab.filePath}
                      onDirtyChange={(isDirty) => updateTabDirty(tab.id, isDirty)}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {showDropOverlay && (
          <div className="absolute inset-0 z-40 flex items-center justify-center bg-blue-500/10 backdrop-blur-[1px] pointer-events-none">
            <div className="w-[420px] max-w-[90vw] rounded-2xl border-2 border-dashed border-blue-400 bg-white/95 shadow-xl px-6 py-8 text-center">
              <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-blue-50 text-blue-600">
                <Upload className="w-7 h-7" />
              </div>
              <h3 className="text-lg font-semibold text-gray-900">
                {isImportingDrop ? '正在导入文件...' : '松开鼠标即可导入'}
              </h3>
              <p className="mt-2 text-sm text-gray-600">
                {isImportingDrop
                  ? `正在复制到 ${dropTargetLabel}`
                  : `外部拖入的文件或文件夹会复制到 ${dropTargetLabel}`}
              </p>
            </div>
          </div>
        )}
      </div>

      {isFilesWorkspaceActive && <ColumnSettings />}
    </div>
  );
}
