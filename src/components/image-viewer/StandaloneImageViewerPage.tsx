import { useEffect, useMemo, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { ArrowLeft } from 'lucide-react';
import { ImageViewerSurface } from './ImageViewerSurface';
import {
  STANDALONE_RETURN_TO_WORKSPACE_EVENT,
  type StandaloneReturnToWorkspacePayload,
} from '../workspace/standaloneWindowReturn';

function getFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function isStandaloneImageViewerRoute(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  const searchParams = new URLSearchParams(window.location.search);
  return searchParams.get('view') === 'image-viewer';
}

export function StandaloneImageViewerPage() {
  const searchParams = useMemo(() => new URLSearchParams(window.location.search), []);
  const sourcePath = searchParams.get('path') || '';
  const projectPath = searchParams.get('projectPath') || '';
  const title = searchParams.get('title') || (sourcePath ? getFileNameFromPath(sourcePath) : '图片查看器');
  const [isReturning, setIsReturning] = useState(false);
  const [returnErrorMessage, setReturnErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    document.title = title;
  }, [title]);

  const handleReturnToProject = async () => {
    if (!projectPath || !sourcePath || isReturning) {
      return;
    }

    setIsReturning(true);
    setReturnErrorMessage(null);

    const currentWindow = getCurrentWebviewWindow();
    const payload: StandaloneReturnToWorkspacePayload = {
      projectPath,
      filePath: sourcePath,
      fileType: 'image',
    };

    try {
      await currentWindow.emit(STANDALONE_RETURN_TO_WORKSPACE_EVENT, payload);
      try {
        await currentWindow.close();
      } catch (closeError) {
        console.warn('Failed to close standalone image window after return, falling back to hide:', closeError);
        await currentWindow.hide();
      }
    } catch (error) {
      setReturnErrorMessage(`回归失败：${String(error)}`);
      setIsReturning(false);
    }
  };

  if (!sourcePath) {
    return (
      <div className="flex h-screen items-center justify-center bg-gray-950 p-6 text-center text-gray-300">
        <div>
          <p className="text-base font-medium">没有收到要打开的图片路径</p>
          <p className="mt-2 text-sm text-gray-400">请从文件列表重新打开图片。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="relative h-screen bg-gray-950">
      {projectPath && (
        <div className="pointer-events-none absolute right-3 top-3 z-40">
          <button
            type="button"
            onClick={handleReturnToProject}
            disabled={isReturning}
            className="pointer-events-auto inline-flex items-center gap-1.5 rounded-md border border-white/20 bg-black/50 px-3 py-1.5 text-xs text-white transition-colors hover:bg-black/65 disabled:cursor-not-allowed disabled:opacity-60"
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

      <ImageViewerSurface
        title={title}
        source={sourcePath}
      />
    </div>
  );
}
