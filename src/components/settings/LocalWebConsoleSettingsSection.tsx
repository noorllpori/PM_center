import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  ExternalLink,
  Globe2,
  RefreshCw,
  RotateCcw,
  Save,
  ShieldCheck,
} from 'lucide-react';
import {
  LOCAL_WEB_CONSOLE_MODULE_ID,
  getLocalWebConsoleStatus,
  openLocalWebConsole,
  setLocalWebConsoleEnabled,
  updateLocalWebConsoleConfig,
  type LocalWebConsoleConfig,
  type LocalWebConsoleStatus,
} from '../../api/localWebConsole';
import { getPlatformModule, restartPlatformModule } from '../../api/platformModules';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import type { PlatformModuleDiagnostic } from '../../types/platformRuntime';

type PendingAction = 'load' | 'toggle' | 'save' | 'restart' | 'open' | null;

const DEFAULT_CONFIG: LocalWebConsoleConfig = {
  preferredPort: 31530,
  allowSettingsWrite: true,
  allowRestart: true,
  allowExit: true,
};

function formatError(error: unknown) {
  if (error && typeof error === 'object' && 'message' in error) {
    return String((error as { message?: unknown }).message || error);
  }
  return String(error);
}

function ToggleRow({
  checked,
  disabled,
  label,
  description,
  onChange,
}: {
  checked: boolean;
  disabled: boolean;
  label: string;
  description: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 py-3">
      <div className="min-w-0">
        <p className="text-sm font-medium text-gray-900 dark:text-gray-100">{label}</p>
        <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{description}</p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative h-6 w-11 shrink-0 rounded-full transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
          checked ? 'bg-blue-600' : 'bg-gray-300 dark:bg-gray-700'
        }`}
      >
        <span
          className={`absolute top-1 h-4 w-4 rounded-full bg-white shadow-sm transition-transform ${
            checked ? 'translate-x-6' : 'translate-x-1'
          }`}
        />
      </button>
    </div>
  );
}

export function LocalWebConsoleSettingsSection() {
  const refreshContributions = useContributionRegistryStore((state) => state.refresh);
  const refreshProfiles = useWorkspaceProfileStore((state) => state.refresh);
  const [status, setStatus] = useState<LocalWebConsoleStatus | null>(null);
  const [module, setModule] = useState<PlatformModuleDiagnostic | null>(null);
  const [draft, setDraft] = useState<LocalWebConsoleConfig>(DEFAULT_CONFIG);
  const [pending, setPending] = useState<PendingAction>('load');
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [restartRequired, setRestartRequired] = useState(false);

  const load = useCallback(async () => {
    setPending((current) => current ?? 'load');
    setError(null);
    try {
      const [nextStatus, nextModule] = await Promise.all([
        getLocalWebConsoleStatus(),
        getPlatformModule(LOCAL_WEB_CONSOLE_MODULE_ID),
      ]);
      setStatus(nextStatus);
      setModule(nextModule);
      setDraft(nextStatus.config);
    } catch (nextError) {
      setError(formatError(nextError));
    } finally {
      setPending((current) => (current === 'load' ? null : current));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const moduleRunning = status?.running === true && module?.state === 'running';
  const stateLabel = useMemo(() => {
    switch (module?.state) {
      case 'running':
        return '运行中';
      case 'starting':
      case 'resolving':
        return '启动中';
      case 'stopping':
        return '停止中';
      case 'error':
        return '启动失败';
      case 'blocked':
        return '被阻止';
      default:
        return '已停用';
    }
  }, [module?.state]);

  const runAction = async (action: Exclude<PendingAction, 'load' | null>, operation: () => Promise<void>) => {
    setPending(action);
    setMessage(null);
    setError(null);
    try {
      await operation();
      await load();
    } catch (nextError) {
      setError(formatError(nextError));
    } finally {
      setPending(null);
    }
  };

  const handleToggle = () => runAction('toggle', async () => {
    await setLocalWebConsoleEnabled(!moduleRunning);
    await Promise.all([refreshContributions(), refreshProfiles()]);
    setRestartRequired(false);
    setMessage(moduleRunning ? '网页控制台已停用' : '网页控制台已启用');
  });

  const handleSave = () => runAction('save', async () => {
    if (!Number.isInteger(draft.preferredPort) || draft.preferredPort < 1024 || draft.preferredPort > 65535) {
      throw new Error('端口必须是 1024-65535 之间的整数');
    }
    const nextStatus = await updateLocalWebConsoleConfig(draft);
    setStatus(nextStatus);
    setDraft(nextStatus.config);
    setRestartRequired(nextStatus.running);
    setMessage(nextStatus.running ? '配置已保存，重启服务后生效' : '配置已保存');
  });

  const handleRestart = () => runAction('restart', async () => {
    await restartPlatformModule(LOCAL_WEB_CONSOLE_MODULE_ID);
    await refreshContributions();
    setRestartRequired(false);
    setMessage('网页控制台服务已重启');
  });

  const handleOpen = () => runAction('open', async () => {
    await openLocalWebConsole();
  });

  const busy = pending !== null;

  return (
    <section id="settings-global-web-console" className="scroll-mt-4 rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-sky-50 text-sky-600 dark:bg-sky-950/40 dark:text-sky-300">
            <Globe2 className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">本机网页控制台</h4>
              <span className={`rounded px-2 py-0.5 text-[11px] font-medium ${
                moduleRunning
                  ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300'
                  : module?.state === 'error'
                    ? 'bg-red-100 text-red-700 dark:bg-red-950/40 dark:text-red-300'
                    : 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300'
              }`}>
                {stateLabel}
              </span>
              <span className="rounded bg-gray-100 px-2 py-0.5 text-[11px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">仅 127.0.0.1</span>
            </div>
            <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
              在真实浏览器中查看状态和部分设置；默认关闭，不提供 Shell、文件系统或任意命令入口。
            </p>
            {status?.address ? (
              <p className="mt-1 break-all font-mono text-[11px] text-gray-400">{status.address}</p>
            ) : null}
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => void load()}
            disabled={busy}
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
            title="刷新状态"
          >
            <RefreshCw className={`h-4 w-4 ${pending === 'load' ? 'animate-spin' : ''}`} />
          </button>
          {moduleRunning ? (
            <button
              type="button"
              onClick={() => void handleOpen()}
              disabled={busy}
              className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-1.5 text-xs text-white hover:bg-blue-700 disabled:opacity-50"
            >
              <ExternalLink className="h-3.5 w-3.5" />
              打开浏览器
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => void handleToggle()}
            disabled={busy}
            className={`rounded-md px-3 py-1.5 text-xs font-medium disabled:opacity-50 ${
              moduleRunning
                ? 'border border-gray-200 text-gray-700 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800'
                : 'bg-blue-600 text-white hover:bg-blue-700'
            }`}
          >
            {pending === 'toggle' ? '处理中...' : moduleRunning ? '停用' : '启用'}
          </button>
        </div>
      </div>

      {module?.lastError ? (
        <div className="mt-3 flex items-start gap-2 rounded-md bg-red-50 px-3 py-2 text-xs text-red-700 dark:bg-red-950/30 dark:text-red-300">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{module.lastError.message}</span>
        </div>
      ) : null}

      <div className="mt-4 border-t border-gray-100 dark:border-gray-800">
        <div className="grid gap-4 py-4 sm:grid-cols-[180px_minmax(0,1fr)] sm:items-center">
          <div>
            <p className="text-sm font-medium text-gray-900 dark:text-gray-100">监听端口</p>
            <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">固定回环地址，不接受局域网连接。</p>
          </div>
          <input
            type="number"
            min={1024}
            max={65535}
            value={draft.preferredPort}
            disabled={busy}
            onChange={(event) => setDraft((current) => ({
              ...current,
              preferredPort: Number(event.target.value),
            }))}
            className="h-9 w-full max-w-48 rounded-md border border-gray-200 bg-white px-3 text-sm text-gray-900 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/15 disabled:opacity-50 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100"
          />
        </div>

        <div className="divide-y divide-gray-100 border-t border-gray-100 dark:divide-gray-800 dark:border-gray-800">
          <ToggleRow
            checked={draft.allowSettingsWrite}
            disabled={busy}
            label="允许修改部分设置"
            description="仅限会话恢复、标签关闭提示和项目根目录。"
            onChange={(checked) => setDraft((current) => ({ ...current, allowSettingsWrite: checked }))}
          />
          <ToggleRow
            checked={draft.allowRestart}
            disabled={busy}
            label="允许重启 Nexora"
            description="执行前仍需在浏览器确认。"
            onChange={(checked) => setDraft((current) => ({ ...current, allowRestart: checked }))}
          />
          <ToggleRow
            checked={draft.allowExit}
            disabled={busy}
            label="允许退出 Nexora"
            description="退出前会先停止模块和后台资源。"
            onChange={(checked) => setDraft((current) => ({ ...current, allowExit: checked }))}
          />
        </div>
      </div>

      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-gray-100 pt-4 dark:border-gray-800">
        <div className="flex min-w-0 items-center gap-2 text-xs">
          <ShieldCheck className="h-4 w-4 shrink-0 text-emerald-500" />
          <span className={error ? 'text-red-600 dark:text-red-300' : 'text-gray-500 dark:text-gray-400'}>
            {error || message || '访问令牌保存在应用数据目录，不会放入网页地址查询参数。'}
          </span>
        </div>
        <div className="flex items-center gap-2">
          {moduleRunning && restartRequired ? (
            <button
              type="button"
              onClick={() => void handleRestart()}
              disabled={busy}
              className="inline-flex items-center gap-1.5 rounded-md border border-amber-200 bg-amber-50 px-3 py-1.5 text-xs text-amber-700 hover:bg-amber-100 disabled:opacity-50 dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-300"
            >
              <RotateCcw className="h-3.5 w-3.5" />
              重启服务
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={busy}
            className="inline-flex items-center gap-1.5 rounded-md bg-gray-900 px-3 py-1.5 text-xs text-white hover:bg-black disabled:opacity-50 dark:bg-gray-100 dark:text-gray-900 dark:hover:bg-white"
          >
            <Save className="h-3.5 w-3.5" />
            {pending === 'save' ? '保存中...' : '保存配置'}
          </button>
        </div>
      </div>
    </section>
  );
}
