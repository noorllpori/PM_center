import { useCallback, useEffect, useRef, useState } from 'react';
import { FileTree } from './FileTree';
import { FileList } from './FileList';
import { Toolbar } from './Toolbar';
import { ColumnSettings } from './ColumnSettings';
import { FileDetail } from './FileDetail';
import { getPathLabel, isExternalFileDrag } from './dragDrop';
import { importExternalDrop } from './externalImport';
import { ScriptRunner } from '../ScriptRunner';
import { ChangeLog } from '../ChangeLog';
import { TaskButton } from '../TaskButton';
import { LauncherButton } from '../Launcher';
import { WelcomeScreen } from '../WelcomeScreen';
import { P2PChat } from '../P2PChat';
import { PythonEnvManager } from '../PythonEnvManager';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { Folder, Code, Clock, History, MessageCircle, Terminal, Upload, X } from 'lucide-react';

export function FileManager() {
  const { isInitialized, projectPath, projectName, currentPath, setProject, refresh } = useProjectStore();
  const { toast, showToast, hideToast } = useUiStore();
  const { 
    loadSettings, 
    addRecentProject, 
  } = useSettingsStore();
  
  const [activeTab, setActiveTab] = useState<'files' | 'scripts' | 'logs'>('files');
  const [isP2PChatOpen, setIsP2PChatOpen] = useState(false);
  const [isPythonEnvOpen, setIsPythonEnvOpen] = useState(false);
  const [isDragImportActive, setIsDragImportActive] = useState(false);
  const [isImportingDrop, setIsImportingDrop] = useState(false);
  const externalDragDepthRef = useRef(0);

  // 初始化：加载设置
  useEffect(() => {
    loadSettings();
  }, []);

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

  const resetExternalDragState = useCallback(() => {
    externalDragDepthRef.current = 0;
    setIsDragImportActive(false);
  }, []);

  useEffect(() => {
    if (activeTab !== 'files' || !isInitialized) {
      resetExternalDragState();
    }
  }, [activeTab, isInitialized, resetExternalDragState]);

  const handleExternalDragEnter = useCallback((event: React.DragEvent<HTMLDivElement>) => {
    if (!isInitialized || activeTab !== 'files' || !isExternalFileDrag(event.dataTransfer)) {
      return;
    }

    event.preventDefault();
    externalDragDepthRef.current += 1;
    setIsDragImportActive(true);
  }, [activeTab, isInitialized]);

  const handleExternalDragOver = useCallback((event: React.DragEvent<HTMLDivElement>) => {
    if (!isInitialized || activeTab !== 'files' || !isExternalFileDrag(event.dataTransfer)) {
      return;
    }

    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';

    if (!isDragImportActive) {
      setIsDragImportActive(true);
    }
  }, [activeTab, isDragImportActive, isInitialized]);

  const handleExternalDragLeave = useCallback((event: React.DragEvent<HTMLDivElement>) => {
    if (!isInitialized || activeTab !== 'files' || !isExternalFileDrag(event.dataTransfer)) {
      return;
    }

    event.preventDefault();
    externalDragDepthRef.current = Math.max(0, externalDragDepthRef.current - 1);

    if (externalDragDepthRef.current === 0 && !isImportingDrop) {
      setIsDragImportActive(false);
    }
  }, [activeTab, isImportingDrop, isInitialized]);

  const handleExternalDrop = useCallback(async (event: React.DragEvent<HTMLDivElement>) => {
    if (!isInitialized || activeTab !== 'files' || !isExternalFileDrag(event.dataTransfer)) {
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
  }, [activeTab, currentPath, isInitialized, projectPath, refresh, resetExternalDragState]);

  const dropTargetLabel = getPathLabel(currentPath || projectPath, projectPath, projectName);
  const showDropOverlay = isInitialized && activeTab === 'files' && (isDragImportActive || isImportingDrop);
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

      {/* Tab 切换 */}
      {isInitialized && (
        <div className="flex border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900">
          <button
            onClick={() => setActiveTab('files')}
            className={`
              flex items-center gap-1.5 px-4 py-2 text-sm font-medium
              ${activeTab === 'files'
                ? 'text-blue-600 border-b-2 border-blue-600'
                : 'text-gray-600 hover:text-gray-900'
              }
            `}
          >
            <Folder className="w-4 h-4" />
            文件
          </button>
          <button
            onClick={() => setActiveTab('scripts')}
            className={`
              flex items-center gap-1.5 px-4 py-2 text-sm font-medium
              ${activeTab === 'scripts'
                ? 'text-blue-600 border-b-2 border-blue-600'
                : 'text-gray-600 hover:text-gray-900'
              }
            `}
          >
            <Code className="w-4 h-4" />
            脚本
          </button>
          <button
            onClick={() => setActiveTab('logs')}
            className={`
              flex items-center gap-1.5 px-4 py-2 text-sm font-medium
              ${activeTab === 'logs'
                ? 'text-blue-600 border-b-2 border-blue-600'
                : 'text-gray-600 hover:text-gray-900'
              }
            `}
          >
            <History className="w-4 h-4" />
            日志
          </button>
        </div>
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
        ) : activeTab === 'scripts' ? (
          <ScriptRunner />
        ) : activeTab === 'logs' ? (
          <ChangeLog />
        ) : (
          <>
            {/* 左侧：文件树 */}
            <div className="w-64 border-r border-gray-200 dark:border-gray-700 flex-shrink-0">
              <FileTree />
            </div>

            {/* 中间：文件列表 */}
            <div className="flex-1 overflow-hidden">
              <FileList />
            </div>

            {/* 右侧：文件详情 */}
            <div className="w-56 border-l border-gray-200 dark:border-gray-700 flex-shrink-0 bg-white dark:bg-gray-900">
              <FileDetail />
            </div>
          </>
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
      {isInitialized && activeTab === 'files' && <ColumnSettings />}

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
