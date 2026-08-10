import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { AlertTriangle, Loader2, RefreshCw } from 'lucide-react';
import { getScriptSurfaceDocument } from '../../api/scriptAutomation';
import { useAutomationStore } from '../../stores/automationStore';
import type { ScriptSurfaceDocument } from '../../types/automation';
import type { JsonValue } from '../../types/platform';

interface ScriptSurfaceFrameProps {
  componentId: string;
  surfaceId: string;
  projectPath?: string | null;
  extensionContext?: JsonValue;
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
}: ScriptSurfaceFrameProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [document, setDocument] = useState<ScriptSurfaceDocument | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const startRun = useAutomationStore((state) => state.startRun);
  const initialize = useAutomationStore((state) => state.initialize);
  const runs = useAutomationStore((state) => state.snapshot?.recentRuns ?? []);

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
  }, [componentId, document, projectPath, startRun, surfaceId]);

  useEffect(() => {
    if (!document) return;
    const matchingRuns = runs.filter((run) => run.componentId === componentId && run.triggerId === surfaceId);
    iframeRef.current?.contentWindow?.postMessage({
      type: 'nexora-script-event',
      nonce: document.nonce,
      event: { type: 'runs-changed', runs: matchingRuns },
    }, '*');
  }, [componentId, document, runs, surfaceId]);

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
    <iframe
      ref={iframeRef}
      title={document.title}
      sandbox="allow-scripts"
      srcDoc={document.html}
      onLoad={() => {
        if (!extensionContext) return;
        iframeRef.current?.contentWindow?.postMessage({
          type: 'nexora-script-event',
          nonce: document.nonce,
          event: { type: 'ui-extension-context', payload: extensionContext },
        }, '*');
      }}
      className="h-full w-full border-0 bg-white"
    />
  );
}
