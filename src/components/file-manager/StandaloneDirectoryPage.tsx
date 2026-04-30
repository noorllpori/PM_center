import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { ArrowLeft } from 'lucide-react';
import { DirectoryTabSurface } from './DirectoryTabSurface';
import { openStandaloneDirectoryViewer } from './openStandaloneDirectoryViewer';
import { createWorkspaceTabStore, WorkspaceTabStoreProvider } from '../../stores/workspaceTabStore';
import {
  STANDALONE_RETURN_TO_WORKSPACE_EVENT,
  type StandaloneReturnToWorkspacePayload,
} from '../workspace/standaloneWindowReturn';

const TREE_PANEL_MIN_WIDTH = 220;
const TREE_PANEL_MAX_WIDTH = 520;
const TREE_PANEL_DEFAULT_WIDTH = 256;
const DETAILS_PANEL_MIN_WIDTH = 260;
const DETAILS_PANEL_MAX_WIDTH = 720;
const DETAILS_PANEL_DEFAULT_WIDTH = 320;

function getPathName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function clampPanelWidth(width: number, min: number, max: number) {
  return Math.min(max, Math.max(min, width));
}

export function isStandaloneDirectoryRoute(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  const searchParams = new URLSearchParams(window.location.search);
  return searchParams.get('view') === 'directory-viewer';
}

export function StandaloneDirectoryPage() {
  const searchParams = useMemo(() => new URLSearchParams(window.location.search), []);
  const directoryPath = searchParams.get('path') || '';
  const projectPath = searchParams.get('projectPath') || '';
  const projectName = searchParams.get('projectName') || (projectPath ? getPathName(projectPath) : '');
  const title = searchParams.get('title') || (directoryPath ? getPathName(directoryPath) : '目录');
  const [workspaceTabStore] = useState(() => createWorkspaceTabStore({
    forceStandaloneFileOpen: true,
    standaloneProjectPath: projectPath || undefined,
    standaloneProjectName: projectName || undefined,
  }));
  const [treePanelWidth, setTreePanelWidth] = useState(TREE_PANEL_DEFAULT_WIDTH);
  const [detailsPanelWidth, setDetailsPanelWidth] = useState(DETAILS_PANEL_DEFAULT_WIDTH);
  const [isResizingTreePanel, setIsResizingTreePanel] = useState(false);
  const [isResizingDetailsPanel, setIsResizingDetailsPanel] = useState(false);
  const [isReturning, setIsReturning] = useState(false);
  const [returnErrorMessage, setReturnErrorMessage] = useState<string | null>(null);
  const treeResizeStateRef = useRef<{ startX: number; startWidth: number } | null>(null);
  const detailsResizeStateRef = useRef<{ startX: number; startWidth: number } | null>(null);

  useEffect(() => {
    document.title = title;
  }, [title]);

  const stopTreeResize = useCallback(() => {
    treeResizeStateRef.current = null;
    setIsResizingTreePanel(false);
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }, []);

  const stopDetailsResize = useCallback(() => {
    detailsResizeStateRef.current = null;
    setIsResizingDetailsPanel(false);
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }, []);

  useEffect(() => {
    if (!isResizingTreePanel) {
      return;
    }

    const handleMouseMove = (event: MouseEvent) => {
      const resizeState = treeResizeStateRef.current;
      if (!resizeState) {
        return;
      }

      setTreePanelWidth(
        clampPanelWidth(
          resizeState.startWidth + event.clientX - resizeState.startX,
          TREE_PANEL_MIN_WIDTH,
          TREE_PANEL_MAX_WIDTH,
        ),
      );
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', stopTreeResize);

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', stopTreeResize);
    };
  }, [isResizingTreePanel, stopTreeResize]);

  useEffect(() => {
    if (!isResizingDetailsPanel) {
      return;
    }

    const handleMouseMove = (event: MouseEvent) => {
      const resizeState = detailsResizeStateRef.current;
      if (!resizeState) {
        return;
      }

      setDetailsPanelWidth(
        clampPanelWidth(
          resizeState.startWidth - (event.clientX - resizeState.startX),
          DETAILS_PANEL_MIN_WIDTH,
          DETAILS_PANEL_MAX_WIDTH,
        ),
      );
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', stopDetailsResize);

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', stopDetailsResize);
    };
  }, [isResizingDetailsPanel, stopDetailsResize]);

  useEffect(() => {
    return () => {
      stopTreeResize();
      stopDetailsResize();
    };
  }, [stopDetailsResize, stopTreeResize]);

  const handleStartTreeResize = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    treeResizeStateRef.current = {
      startX: event.clientX,
      startWidth: treePanelWidth,
    };
    setIsResizingTreePanel(true);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, [treePanelWidth]);

  const handleStartDetailsResize = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    detailsResizeStateRef.current = {
      startX: event.clientX,
      startWidth: detailsPanelWidth,
    };
    setIsResizingDetailsPanel(true);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, [detailsPanelWidth]);

  const handleOpenDirectoryTab = useCallback(
    async (path: string) => {
      await openStandaloneDirectoryViewer({
        directoryPath: path,
        projectPath: projectPath || undefined,
        projectName: projectName || undefined,
      });
    },
    [projectName, projectPath],
  );

  const handleReturnToProject = async () => {
    if (!projectPath || !directoryPath || isReturning) {
      return;
    }

    setIsReturning(true);
    setReturnErrorMessage(null);

    const currentWindow = getCurrentWebviewWindow();
    const payload: StandaloneReturnToWorkspacePayload = {
      projectPath,
      filePath: directoryPath,
      fileType: 'directory',
    };

    try {
      await currentWindow.emit(STANDALONE_RETURN_TO_WORKSPACE_EVENT, payload);
      try {
        await currentWindow.close();
      } catch (closeError) {
        console.warn('Failed to close standalone directory window after return, falling back to hide:', closeError);
        await currentWindow.hide();
      }
    } catch (error) {
      setReturnErrorMessage(`回归失败：${String(error)}`);
      setIsReturning(false);
    }
  };

  if (!directoryPath) {
    return (
      <div className="flex h-screen items-center justify-center bg-white p-6 text-center text-gray-500">
        <div>
          <p className="text-base font-medium text-gray-800">没有收到要打开的目录路径</p>
          <p className="mt-2 text-sm text-gray-500">请从文件列表重新打开目录。</p>
        </div>
      </div>
    );
  }

  return (
    <WorkspaceTabStoreProvider store={workspaceTabStore}>
      <div className="relative h-screen bg-white dark:bg-gray-900">
        {projectPath && (
          <div className="pointer-events-none absolute right-3 top-3 z-40">
            <button
              type="button"
              onClick={handleReturnToProject}
              disabled={isReturning}
              className="pointer-events-auto inline-flex items-center gap-1.5 rounded-md border border-gray-300 bg-white/95 px-3 py-1.5 text-xs text-gray-700 shadow-sm transition-colors hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-gray-700 dark:bg-gray-900/95 dark:text-gray-200 dark:hover:bg-gray-800"
              title="回归到项目标签页"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              回归项目标签页
            </button>
          </div>
        )}

        {returnErrorMessage && (
          <div className="pointer-events-none absolute left-1/2 top-3 z-40 -translate-x-1/2 rounded-md border border-red-300 bg-white px-3 py-1.5 text-xs text-red-600 shadow">
            {returnErrorMessage}
          </div>
        )}

        <DirectoryTabSurface
          initialPath={directoryPath}
          isActive
          projectPath={projectPath || directoryPath}
          projectName={projectName || getPathName(projectPath || directoryPath)}
          onOpenDirectoryTab={handleOpenDirectoryTab}
          treePanelWidth={treePanelWidth}
          isResizingTreePanel={isResizingTreePanel}
          onStartTreeResize={handleStartTreeResize}
          detailsPanelWidth={detailsPanelWidth}
          isResizingDetailsPanel={isResizingDetailsPanel}
          onStartDetailsResize={handleStartDetailsResize}
        />
      </div>
    </WorkspaceTabStoreProvider>
  );
}
