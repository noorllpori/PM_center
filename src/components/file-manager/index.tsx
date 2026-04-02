import { useCallback, useEffect, useRef, useState } from 'react';
import { FileTree } from './FileTree';
import { FileList } from './FileList';
import { Toolbar } from './Toolbar';
import { ColumnSettings } from './ColumnSettings';
import { FileDetail } from './FileDetail';
import { getPathLabel, isExternalFileDrag } from './dragDrop';
import { importExternalDrop } from './externalImport';
import { ChangeLog } from '../ChangeLog';
import { TaskButton } from '../TaskButton';
import { LauncherButton } from '../Launcher';
import { WelcomeScreen } from '../WelcomeScreen';
import { P2PChat } from '../P2PChat';
import { PythonEnvManager } from '../PythonEnvManager';
import { ImageViewerSurface } from '../image-viewer/ImageViewerSurface';
import { TextEditorSurface } from '../text-editor/TextEditorSurface';
import { WorkspaceTabBar } from '../workspace/WorkspaceTabBar';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import { History, MessageCircle, Terminal, Upload, X } from 'lucide-react';

const FILE_DETAILS_PANEL_WIDTH_KEY = 'pm-center:file-details-panel-width';
const FILE_DETAILS_PANEL_MIN_WIDTH = 260;
const FILE_DETAILS_PANEL_MAX_WIDTH = 720;
const FILE_DETAILS_PANEL_DEFAULT_WIDTH = 320;

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  const tagName = target.tagName.toLowerCase();
  return tagName === 'input' || tagName === 'textarea' || tagName === 'select' || target.isContentEditable;
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

export function FileManager() {
  const {
    isInitialized,
    projectPath,
    projectName,
    currentPath,
    setProject,
    refresh,
    toggleShowExcludedFiles,
    showExcludedFiles,
  } = useProjectStore();
  const { toast, showToast, hideToast } = useUiStore();
  const { 
    loadSettings, 
    addRecentProject, 
  } = useSettingsStore();
  const tabs = useWorkspaceTabStore((state) => state.tabs);
  const activeTabId = useWorkspaceTabStore((state) => state.activeTabId);
  const activateTab = useWorkspaceTabStore((state) => state.activateTab);
  const closeTab = useWorkspaceTabStore((state) => state.closeTab);
  const reorderTabs = useWorkspaceTabStore((state) => state.reorderTabs);
  const openLogsTab = useWorkspaceTabStore((state) => state.openLogsTab);
  const updateTabDirty = useWorkspaceTabStore((state) => state.updateTabDirty);
  const resetTabs = useWorkspaceTabStore((state) => state.resetTabs);

  const [isP2PChatOpen, setIsP2PChatOpen] = useState(false);
  const [isPythonEnvOpen, setIsPythonEnvOpen] = useState(false);
  const [isDragImportActive, setIsDragImportActive] = useState(false);
  const [isImportingDrop, setIsImportingDrop] = useState(false);
  const [fileDetailsPanelWidth, setFileDetailsPanelWidth] = useState(getInitialFileDetailsPanelWidth);
  const [isResizingFileDetails, setIsResizingFileDetails] = useState(false);
  const externalDragDepthRef = useRef(0);
  const fileDetailsResizeStateRef = useRef<{ startX: number; startWidth: number } | null>(null);
  const activeWorkspaceTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const isFilesWorkspaceActive = activeWorkspaceTab?.type === 'files';

  // 初始化：加载设置
  useEffect(() => {
    loadSettings();
  }, []);

  useEffect(() => {
    if (!isInitialized) {
      resetTabs();
    }
  }, [isInitialized, resetTabs]);

  // 当成功打开项目后，添加到历史
  useEffect(() => {
    if (isInitialized && projectPath && projectName) {
      addRecentProject(projectPath, projectName);
    }
  }, [isInitialized, projectPath, projectName]);

  // 处理打开项目
  const handleOpenProject = async (path: string) => {
    try {
      await setProject(path);
    } catch (error) {
      console.error('Failed to open project:', error);
    }
  };

  useEffect(() => {
    if (!toast.isOpen) {
      return;
    }

    const timeout = window.setTimeout(() => {
      hideToast();
    }, toast.tone === 'error' ? 6000 : 3500);

    return () => window.clearTimeout(timeout);
  }, [hideToast, toast.isOpen, toast.tone]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!isInitialized || !isFilesWorkspaceActive) {
        return;
      }

      if (!(event.ctrlKey || event.metaKey) || event.shiftKey || event.altKey) {
        return;
      }

      if (event.key.toLowerCase() !== 'h') {
        return;
      }

      if (isEditableTarget(event.target)) {
        return;
      }

      event.preventDefault();

      const nextShowExcluded = !useProjectStore.getState().showExcludedFiles;
      toggleShowExcludedFiles();
      void refresh();
      showToast({
        title: nextShowExcluded ? '已显示排除项' : '已隐藏排除项',
        message: nextShowExcluded
          ? '当前目录会显示被排除规则隐藏的文件，按 Ctrl+H 可切回隐藏。'
          : '当前目录已恢复隐藏被排除规则匹配的文件。',
        tone: 'info',
      });
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isFilesWorkspaceActive, isInitialized, refresh, showToast, toggleShowExcludedFiles]);

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

    window.localStorage.setItem(FILE_DETAILS_PANEL_WIDTH_KEY, String(fileDetailsPanelWidth));
  }, [fileDetailsPanelWidth]);

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
      stopFileDetailsResize();
    };
  }, [stopFileDetailsResize]);

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
      const { successCount, failedItems } = await importExternalDrop(event.dataTransfer, targetDir);

      try {
        await refresh();
      } catch (error) {
        console.error('Refresh after drop import failed:', error);
      }

      if (failedItems.length > 0) {
        console.warn('External drop import completed with failures:', {
          successCount,
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
  const toastStyles = {
    info: 'border-blue-200 bg-white text-gray-900',
    success: 'border-green-200 bg-white text-gray-900',
    warning: 'border-yellow-200 bg-white text-gray-900',
    error: 'border-red-200 bg-white text-gray-900',
  };
  const toastAccentStyles = {
    info: 'bg-blue-500',
    success: 'bg-green-500',
    warning: 'bg-yellow-500',
    error: 'bg-red-500',
  };

  return (
    <div className="h-screen flex flex-col bg-gray-50 dark:bg-gray-900">
      {/* 顶部栏：工具栏 + 全局按钮 */}
      <div className="flex items-center border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900">
        <div className="flex-1">
          <Toolbar />
        </div>
        {/* 全局按钮区域 */}
        <div className="flex items-center gap-2 px-3 border-l border-gray-200 dark:border-gray-700">
          {isInitialized && (
            <button
              onClick={() => openLogsTab()}
              className={`p-2 rounded-lg transition-colors ${
                activeWorkspaceTab?.type === 'logs'
                  ? 'bg-amber-50 text-amber-600 dark:bg-amber-900/20 dark:text-amber-300'
                  : 'text-gray-500 hover:text-amber-600 dark:text-gray-400 dark:hover:text-amber-300 hover:bg-amber-50 dark:hover:bg-amber-900/20'
              }`}
              title="日志"
            >
              <History className="w-5 h-5" />
            </button>
          )}
          <button
            onClick={() => setIsPythonEnvOpen(true)}
            className="p-2 text-gray-500 hover:text-green-600 dark:text-gray-400 
                       dark:hover:text-green-400 hover:bg-green-50 dark:hover:bg-green-900/20 
                       rounded-lg transition-colors"
            title="Python 环境管理"
          >
            <Terminal className="w-5 h-5" />
          </button>
          <button
            onClick={() => setIsP2PChatOpen(true)}
            className="p-2 text-gray-500 hover:text-blue-600 dark:text-gray-400 
                       dark:hover:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20 
                       rounded-lg transition-colors"
            title="局域网消息"
          >
            <MessageCircle className="w-5 h-5" />
          </button>
          <TaskButton />
          <LauncherButton />
        </div>
      </div>

      {isInitialized && isFilesWorkspaceActive && showExcludedFiles && (
        <div className="border-b border-amber-200 bg-amber-50 px-4 py-2 text-xs text-amber-700">
          当前正在显示排除规则隐藏的文件，按 `Ctrl+H` 可切回隐藏。
        </div>
      )}

      {isInitialized && (
        <WorkspaceTabBar
          tabs={tabs}
          activeTabId={activeTabId}
          onActivateTab={activateTab}
          onCloseTab={closeTab}
          onReorderTabs={reorderTabs}
        />
      )}

      {/* 主内容区 */}
      <div
        className="relative flex-1 flex overflow-hidden"
        onDragEnter={handleExternalDragEnter}
        onDragOver={handleExternalDragOver}
        onDragLeave={handleExternalDragLeave}
        onDrop={handleExternalDrop}
      >
        {!isInitialized ? (
          <WelcomeScreen onOpenProject={handleOpenProject} />
        ) : (
          <div className="flex-1 overflow-hidden">
            {tabs.map((tab) => {
              const isActive = tab.id === activeTabId;

              return (
                <div
                  key={tab.id}
                  className={`${isActive ? 'h-full w-full min-w-0' : 'hidden h-full w-full min-w-0'}`}
                >
                  {tab.type === 'files' && (
                    <div className="flex h-full w-full min-w-0">
                      <div className="w-64 border-r border-gray-200 dark:border-gray-700 flex-shrink-0">
                        <FileTree />
                      </div>

                      <div className="flex-1 overflow-hidden">
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
                    <div className="h-full w-full min-w-0">
                      <ChangeLog />
                    </div>
                  )}

                  {tab.type === 'image' && tab.filePath && (
                    <div className="h-full w-full min-w-0">
                      <ImageViewerSurface
                        title={tab.title}
                        source={tab.filePath}
                      />
                    </div>
                  )}

                  {tab.type === 'text' && tab.filePath && (
                    <div className="h-full w-full min-w-0">
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
        )}

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

      {/* 列设置 */}
      {isInitialized && isFilesWorkspaceActive && <ColumnSettings />}

      {/* P2P 聊天 */}
      <P2PChat 
        isOpen={isP2PChatOpen} 
        onClose={() => setIsP2PChatOpen(false)} 
      />

      {/* Python 环境管理 */}
      <PythonEnvManager 
        isOpen={isPythonEnvOpen} 
        onClose={() => setIsPythonEnvOpen(false)} 
      />

      {toast.isOpen && (
        <div className="fixed right-4 bottom-20 z-[120] w-[360px] max-w-[calc(100vw-2rem)]">
          <div className={`relative overflow-hidden rounded-xl border shadow-xl ${toastStyles[toast.tone]}`}>
            <div className={`absolute left-0 top-0 h-full w-1 ${toastAccentStyles[toast.tone]}`} />
            <div className="flex items-start gap-3 px-4 py-3 pl-5">
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold">{toast.title}</p>
                <p className="mt-1 text-sm text-gray-600">{toast.message}</p>
              </div>
              <button
                onClick={hideToast}
                className="rounded-md p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600 transition-colors"
                title="关闭提示"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
