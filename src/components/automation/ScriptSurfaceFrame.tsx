import { useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { AlertTriangle, Loader2, RefreshCw, ShieldAlert } from 'lucide-react';
import { getScriptSurfaceDocument } from '../../api/scriptAutomation';
import { useAutomationStore } from '../../stores/automationStore';
import type { ScriptSurfaceDocument } from '../../types/automation';
import type { JsonValue } from '../../types/platform';

interface ScriptSurfaceFrameProps {
  componentId: string;
  surfaceId: string;
  projectPath?: string | null;
  extensionContext?: JsonValue;
  /** A template/editor preview may render a surface but must never execute it. */
  preview?: boolean;
}

interface SurfaceInvokeMessage {
  type: 'nexora-script-invoke';
  nonce: string;
  requestId: string;
  command: string;
  input?: JsonValue;
}

interface ScriptSurfaceEventPayload {
  componentId: string;
  surfaceId: string;
  event: string;
  payload?: JsonValue;
  runId?: string;
}

export function ScriptSurfaceFrame({
  componentId,
  surfaceId,
  projectPath,
  extensionContext,
  preview = false,
}: ScriptSurfaceFrameProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [document, setDocument] = useState<ScriptSurfaceDocument | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const startRun = useAutomationStore((state) => state.startRun);
  const initialize = useAutomationStore((state) => state.initialize);
  const resolveAttention = useAutomationStore((state) => state.resolveAttention);
  const runs = useAutomationStore((state) => state.snapshot?.recentRuns ?? []);
  const matchingRuns = useMemo(
    () => runs.filter((run) => run.componentId === componentId && run.triggerId === surfaceId),
    [componentId, runs, surfaceId],
  );
  const waitingPermissionRun = matchingRuns.find((run) => run.status === 'waiting-permission') ?? null;

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      setDocument(await getScriptSurfaceDocument(componentId, surfaceId));
    } catch (nextError) {
      setDocument(null);
      setError(String(nextError));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void initialize();
    void load();
  }, [componentId, initialize, surfaceId]);

  useEffect(() => {
    const handleMessage = (event: MessageEvent<SurfaceInvokeMessage>) => {
      if (!document || event.source !== iframeRef.current?.contentWindow) return;
      const message = event.data;
      if (
        !message
        || message.type !== 'nexora-script-invoke'
        || message.nonce !== document.nonce
        || !document.allowedCommands.includes(message.command)
      ) {
        return;
      }
      if (preview) {
        iframeRef.current?.contentWindow?.postMessage({
          type: 'nexora-script-result',
          requestId: message.requestId,
          ok: false,
          error: '界面模板预览不执行组件命令。请应用方案后再运行该页面。',
        }, '*');
        return;
      }
      void startRun({
        componentId,
        command: message.command,
        input: message.input ?? {},
        projectPath,
        triggerKind: 'surface',
        triggerId: surfaceId,
      }).then((run) => {
        iframeRef.current?.contentWindow?.postMessage({
          type: 'nexora-script-result',
          requestId: message.requestId,
          ok: true,
          result: run,
        }, '*');
      }).catch((nextError) => {
        iframeRef.current?.contentWindow?.postMessage({
          type: 'nexora-script-result',
          requestId: message.requestId,
          ok: false,
          error: String(nextError),
        }, '*');
      });
    };
    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [componentId, document, preview, projectPath, startRun, surfaceId]);

  useEffect(() => {
    if (!document) return;
    iframeRef.current?.contentWindow?.postMessage({
      type: 'nexora-script-event',
      nonce: document.nonce,
      event: { type: 'runs-changed', runs: matchingRuns },
    }, '*');
  }, [componentId, document, matchingRuns, surfaceId]);

  useEffect(() => {
    if (!document || !extensionContext) return;
    iframeRef.current?.contentWindow?.postMessage({
      type: 'nexora-script-event',
      nonce: document.nonce,
      event: { type: 'ui-extension-context', payload: extensionContext },
    }, '*');
  }, [document, extensionContext]);

  useEffect(() => {
    if (!document) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ScriptSurfaceEventPayload>('nexora:script-surface-event', (event) => {
      const payload = event.payload;
      if (
        disposed
        || payload.componentId !== componentId
        || payload.surfaceId !== surfaceId
      ) {
        return;
      }
      iframeRef.current?.contentWindow?.postMessage({
        type: 'nexora-script-event',
        nonce: document.nonce,
        event: {
          type: payload.event,
          payload: payload.payload,
          runId: payload.runId,
        },
      }, '*');
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [componentId, document, surfaceId]);

  if (loading) {
    return <div className="flex h-full items-center justify-center text-sm text-gray-500"><Loader2 className="mr-2 h-4 w-4 animate-spin" />正在加载隔离页面...</div>;
  }

  if (error || !document) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <AlertTriangle className="h-8 w-8 text-amber-500" />
        <div>
          <p className="text-sm font-medium text-gray-900 dark:text-gray-100">脚本页面加载失败</p>
          <p className="mt-1 max-w-xl text-xs text-gray-500">{error || '页面文档不可用'}</p>
        </div>
        <button type="button" onClick={() => void load()} className="inline-flex items-center gap-1.5 rounded-md border border-gray-300 px-3 py-1.5 text-xs text-gray-700 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"><RefreshCw className="h-3.5 w-3.5" />重新加载</button>
      </div>
    );
  }

  return (
    <div className="relative h-full w-full">
      <iframe
        ref={iframeRef}
        title={document.title}
        sandbox="allow-scripts"
        srcDoc={document.html}
        onLoad={() => {
          if (!document) return;
          if (extensionContext) {
            iframeRef.current?.contentWindow?.postMessage({
              type: 'nexora-script-event',
              nonce: document.nonce,
              event: { type: 'ui-extension-context', payload: extensionContext },
            }, '*');
          }
          if (preview) {
            iframeRef.current?.contentWindow?.postMessage({
              type: 'nexora-script-event',
              nonce: document.nonce,
              event: { type: 'template-preview', preview: true },
            }, '*');
          }
        }}
        className="h-full w-full border-0 bg-white"
      />
      {waitingPermissionRun ? (
        <div className="absolute inset-0 flex items-center justify-center bg-white/70 p-4 backdrop-blur-sm dark:bg-gray-950/75">
          <div className="w-full max-w-md rounded-md border border-amber-200 bg-white p-4 shadow-lg dark:border-amber-900/70 dark:bg-gray-900">
            <div className="flex items-start gap-3">
              <ShieldAlert className="mt-0.5 h-5 w-5 shrink-0 text-amber-600 dark:text-amber-400" />
              <div className="min-w-0">
                <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">需要授权</h3>
                <p className="mt-1 text-xs leading-5 text-gray-600 dark:text-gray-300">{waitingPermissionRun.commandName} 正在请求 {String(waitingPermissionRun.permissionRequest?.capability ?? '所需能力')}。</p>
              </div>
            </div>
            <div className="mt-4 flex flex-wrap justify-end gap-2">
              <button type="button" onClick={() => void resolveAttention(waitingPermissionRun.id, 'deny')} className="h-8 rounded-md px-3 text-xs text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800">拒绝</button>
              <button type="button" onClick={() => void resolveAttention(waitingPermissionRun.id, 'allowOnce')} className="h-8 rounded-md border border-amber-300 px-3 text-xs text-amber-800 hover:bg-amber-50 dark:border-amber-800 dark:text-amber-200 dark:hover:bg-amber-950/40">仅本次</button>
              <button type="button" onClick={() => void resolveAttention(waitingPermissionRun.id, 'allowSession')} className="h-8 rounded-md border border-amber-300 px-3 text-xs text-amber-800 hover:bg-amber-50 dark:border-amber-800 dark:text-amber-200 dark:hover:bg-amber-950/40">本次会话</button>
              <button type="button" onClick={() => void resolveAttention(waitingPermissionRun.id, 'allowAlways')} className="h-8 rounded-md bg-amber-600 px-3 text-xs font-medium text-white hover:bg-amber-700">始终允许</button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
