import { useEffect, useMemo, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { ArrowLeft } from 'lucide-react';
import { TextEditorSurface } from './TextEditorSurface';
import {
  createTextDetachAckEvent,
  createTextDetachPayloadEvent,
  createTextDetachReadyEvent,
  type TextEditorDetachAckPayload,
  type TextEditorDetachReadyPayload,
  type TextEditorTransferPayload,
} from './textEditorWindowTransfer';
import {
  STANDALONE_RETURN_TO_WORKSPACE_EVENT,
  type StandaloneReturnToWorkspacePayload,
} from '../workspace/standaloneWindowReturn';

function getFileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

export function isStandaloneTextEditorRoute(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  const searchParams = new URLSearchParams(window.location.search);
  return searchParams.get('view') === 'text-editor';
}

export function StandaloneTextEditorPage() {
  const searchParams = useMemo(() => new URLSearchParams(window.location.search), []);
  const sourcePath = searchParams.get('path') || '';
  const requestedTitle = searchParams.get('title') || '';
  const transferId = searchParams.get('transferId') || '';
  const projectPath = searchParams.get('projectPath') || '';
  const [transferredState, setTransferredState] = useState<TextEditorTransferPayload | null>(null);
  const [transferErrorMessage, setTransferErrorMessage] = useState<string | null>(null);
  const [hasAcknowledgedTransfer, setHasAcknowledgedTransfer] = useState(false);
  const [latestEditorSnapshot, setLatestEditorSnapshot] = useState<TextEditorTransferPayload | null>(null);
  const [isReturning, setIsReturning] = useState(false);
  const [returnMessage, setReturnMessage] = useState<string | null>(null);
  const effectiveTitle = transferredState?.title || requestedTitle || (sourcePath ? getFileNameFromPath(sourcePath) : '文本编辑器');

  useEffect(() => {
    document.title = effectiveTitle;
  }, [effectiveTitle]);

  useEffect(() => {
    if (!transferId) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;
    const currentWindow = getCurrentWebviewWindow();
    const payloadEvent = createTextDetachPayloadEvent(transferId);
    const readyEvent = createTextDetachReadyEvent(transferId);

    const registerTransferListener = async () => {
      try {
        unlisten = await currentWindow.once<TextEditorTransferPayload>(payloadEvent, (event) => {
          if (!isActive) {
            return;
          }

          setTransferredState(event.payload);
          setTransferErrorMessage(null);
        });

        if (!isActive) {
          await unlisten();
          unlisten = null;
          return;
        }

        const readyPayload: TextEditorDetachReadyPayload = {
          targetLabel: currentWindow.label,
        };
        await currentWindow.emit(readyEvent, readyPayload);
      } catch (error) {
        if (!isActive) {
          return;
        }

        setTransferErrorMessage(`无法接收编辑内容：${String(error)}`);
      }
    };

    void registerTransferListener();

    return () => {
      isActive = false;
      if (unlisten) {
        void unlisten();
      }
    };
  }, [transferId]);

  useEffect(() => {
    if (!transferId || !transferredState || hasAcknowledgedTransfer) {
      return;
    }

    let isActive = true;
    const currentWindow = getCurrentWebviewWindow();
    const ackEvent = createTextDetachAckEvent(transferId);
    const ackPayload: TextEditorDetachAckPayload = {
      targetLabel: currentWindow.label,
    };

    const acknowledgeTransfer = async () => {
      try {
        await currentWindow.emit(ackEvent, ackPayload);
        if (isActive) {
          setHasAcknowledgedTransfer(true);
        }
      } catch (error) {
        if (!isActive) {
          return;
        }

        setTransferErrorMessage(`无法确认已接收编辑内容：${String(error)}`);
      }
    };

    void acknowledgeTransfer();

    return () => {
      isActive = false;
    };
  }, [hasAcknowledgedTransfer, transferId, transferredState]);

  const handleReturnToProject = async () => {
    if (!projectPath || !sourcePath || isReturning) {
      return;
    }

    if (!latestEditorSnapshot) {
      setReturnMessage('编辑器还在加载，请稍后再试。');
      return;
    }

    if (latestEditorSnapshot.isDirty) {
      setReturnMessage('当前文本有未保存修改，请先保存后再回归项目标签页。');
      return;
    }

    setIsReturning(true);
    setReturnMessage(null);

    const currentWindow = getCurrentWebviewWindow();
    const payload: StandaloneReturnToWorkspacePayload = {
      projectPath,
      filePath: sourcePath,
      fileType: 'text',
      textEditorSnapshot: latestEditorSnapshot,
    };

    try {
      await currentWindow.emit(STANDALONE_RETURN_TO_WORKSPACE_EVENT, payload);
      try {
        await currentWindow.close();
      } catch (closeError) {
        console.warn('Failed to close standalone text window after return, falling back to hide:', closeError);
        await currentWindow.hide();
      }
    } catch (error) {
      setReturnMessage(`回归失败：${String(error)}`);
      setIsReturning(false);
    }
  };

  if (!sourcePath) {
    return (
      <div className="flex h-screen items-center justify-center bg-white p-6 text-center text-gray-500">
        <div>
          <p className="text-base font-medium text-gray-800">没有收到要打开的文件路径</p>
          <p className="mt-2 text-sm text-gray-500">请从文件列表重新打开文本文件。</p>
        </div>
      </div>
    );
  }

  if (transferId && transferErrorMessage) {
    return (
      <div className="flex h-screen items-center justify-center bg-white p-6 text-center text-gray-500">
        <div className="max-w-lg">
          <p className="text-base font-medium text-gray-800">无法接收迁移过来的编辑内容</p>
          <p className="mt-2 break-all text-sm text-gray-500">{transferErrorMessage}</p>
        </div>
      </div>
    );
  }

  if (transferId && !transferredState) {
    return (
      <div className="flex h-screen items-center justify-center bg-white p-6 text-center text-gray-500">
        <div>
          <p className="text-base font-medium text-gray-800">正在接收编辑内容...</p>
          <p className="mt-2 text-sm text-gray-500">窗口会在接收完成后显示当前未保存的修改。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="relative h-screen bg-white">
      {projectPath && (
        <div className="pointer-events-none absolute right-3 top-3 z-40">
          <button
            type="button"
            onClick={handleReturnToProject}
            disabled={isReturning}
            className="pointer-events-auto inline-flex items-center gap-1.5 rounded-md border border-gray-300 bg-white/95 px-3 py-1.5 text-xs text-gray-700 shadow-sm transition-colors hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-60"
            title="回归到项目标签页"
          >
            <ArrowLeft className="h-3.5 w-3.5" />
            回归项目标签页
          </button>
        </div>
      )}

      {returnMessage && (
        <div className="pointer-events-none absolute left-1/2 top-3 z-40 -translate-x-1/2 rounded-md border border-amber-300 bg-white px-3 py-1.5 text-xs text-amber-700 shadow">
          {returnMessage}
        </div>
      )}

      <TextEditorSurface
        title={effectiveTitle}
        filePath={sourcePath}
        initialContent={transferredState?.content}
        initialOriginalContent={transferredState?.originalContent}
        initialLanguage={transferredState?.language}
        initialMarkdownViewMode={transferredState?.markdownViewMode}
        onEditorStateChange={setLatestEditorSnapshot}
      />
    </div>
  );
}
