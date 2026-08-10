import { useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  Ban,
  CheckCircle2,
  Clock3,
  Loader2,
  Play,
  RefreshCw,
  RotateCcw,
  ShieldAlert,
  Square,
  TerminalSquare,
} from 'lucide-react';
import { useAutomationStore } from '../../stores/automationStore';
import type { AutomationRun, AutomationRunStatus } from '../../types/automation';

interface AutomationRunsPanelProps {
  projectPath?: string | null;
}

const STATUS_LABELS: Record<AutomationRunStatus, string> = {
  queued: '等待中',
  preparing: '准备中',
  running: '运行中',
  'waiting-permission': '等待授权',
  cancelling: '正在取消',
  completed: '已完成',
  failed: '失败',
  cancelled: '已取消',
  attention: '需要处理',
};

function statusTone(status: AutomationRunStatus) {
  if (status === 'completed') return 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300';
  if (status === 'failed') return 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-300';
  if (status === 'attention' || status === 'waiting-permission') return 'bg-amber-100 text-amber-700 dark:bg-amber-950/50 dark:text-amber-300';
  if (status === 'cancelled') return 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300';
  return 'bg-blue-100 text-blue-700 dark:bg-blue-950/50 dark:text-blue-300';
}

function formatTime(value?: number | null) {
  return value ? new Date(value).toLocaleString() : '-';
}

function duration(run: AutomationRun) {
  const end = run.finishedAt ?? (['running', 'preparing', 'cancelling'].includes(run.status) ? Date.now() : run.updatedAt);
  const start = run.startedAt ?? run.createdAt;
  const seconds = Math.max(0, Math.round((end - start) / 1000));
  return seconds < 60 ? `${seconds} 秒` : `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
}

export function AutomationRunsPanel({ projectPath }: AutomationRunsPanelProps) {
  const snapshot = useAutomationStore((state) => state.snapshot);
  const selectedRunId = useAutomationStore((state) => state.selectedRunId);
  const loading = useAutomationStore((state) => state.loading);
  const error = useAutomationStore((state) => state.error);
  const initialize = useAutomationStore((state) => state.initialize);
  const refresh = useAutomationStore((state) => state.refresh);
  const selectRun = useAutomationStore((state) => state.selectRun);
  const cancelRun = useAutomationStore((state) => state.cancelRun);
  const retryRun = useAutomationStore((state) => state.retryRun);
  const resolveAttention = useAutomationStore((state) => state.resolveAttention);
  const [scope, setScope] = useState<'all' | 'current'>('all');
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    void initialize();
  }, [initialize]);

  const runs = useMemo(() => {
    const all = snapshot?.recentRuns ?? [];
    return scope === 'current' && projectPath
      ? all.filter((run) => run.projectPath === projectPath)
      : all;
  }, [projectPath, scope, snapshot?.recentRuns]);
  const selectedRun = runs.find((run) => run.id === selectedRunId) ?? runs[0] ?? null;

  const perform = async (action: () => Promise<void>) => {
    setActionError(null);
    try {
      await action();
    } catch (nextError) {
      setActionError(String(nextError));
    }
  };

  if (!snapshot && loading) {
    return <div className="flex flex-1 items-center justify-center text-sm text-gray-500"><Loader2 className="mr-2 h-4 w-4 animate-spin" />正在加载自动化运行...</div>;
  }

  return (
    <div className="flex min-h-0 flex-1 overflow-hidden">
      <aside className="flex w-[390px] shrink-0 flex-col border-r border-gray-200 dark:border-gray-700">
        <div className="space-y-2 border-b border-gray-200 p-3 dark:border-gray-700">
          <div className="flex items-center justify-between gap-2">
            <div>
              <p className="text-sm font-medium text-gray-900 dark:text-gray-100">自动化运行</p>
              <p className="text-xs text-gray-500">{snapshot?.running ? '调度器运行中' : '脚本自动化组件已停用'}</p>
            </div>
            <button type="button" onClick={() => void refresh()} className="rounded-md p-2 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800" title="刷新运行记录">
              <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
            </button>
          </div>
          <div className="flex rounded-md bg-gray-100 p-1 dark:bg-gray-800">
            <button type="button" onClick={() => setScope('all')} className={`flex-1 rounded px-2 py-1 text-xs ${scope === 'all' ? 'bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-white' : 'text-gray-500'}`}>全部项目</button>
            <button type="button" onClick={() => setScope('current')} disabled={!projectPath} className={`flex-1 rounded px-2 py-1 text-xs disabled:opacity-40 ${scope === 'current' ? 'bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-white' : 'text-gray-500'}`}>当前项目</button>
          </div>
          <div className="flex gap-2 text-[11px] text-gray-500">
            <span>{snapshot?.activeCount ?? 0} 活动</span>
            <span>{snapshot?.waitingPermissionCount ?? 0} 等待授权</span>
            <span>{snapshot?.attentionCount ?? 0} 需处理</span>
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {runs.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center px-6 text-center text-sm text-gray-400">
              <TerminalSquare className="mb-3 h-10 w-10 opacity-40" />
              还没有自动化运行。可从脚本开发者工作台手动执行，或在装配方案中保存事件和定时绑定。
            </div>
          ) : runs.map((run) => (
            <button key={run.id} type="button" onClick={() => selectRun(run.id)} className={`block w-full border-b border-gray-100 px-3 py-3 text-left dark:border-gray-800 ${selectedRun?.id === run.id ? 'bg-blue-50 dark:bg-blue-950/20' : 'hover:bg-gray-50 dark:hover:bg-gray-800/60'}`}>
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium text-gray-900 dark:text-gray-100">{run.commandName}</p>
                  <p className="mt-0.5 truncate text-xs text-gray-500">{run.componentName} · {run.triggerKind}</p>
                </div>
                <span className={`shrink-0 rounded px-1.5 py-0.5 text-[11px] ${statusTone(run.status)}`}>{STATUS_LABELS[run.status]}</span>
              </div>
              <div className="mt-2 flex justify-between text-[11px] text-gray-400">
                <span>{run.projectPath?.split(/[\\/]/).pop() ?? '全局'}</span>
                <span>{formatTime(run.createdAt)}</span>
              </div>
              {run.status === 'running' && run.progress != null ? (
                <div className="mt-2">
                  <div className="h-1 overflow-hidden rounded bg-gray-200 dark:bg-gray-700">
                    <div className="h-full bg-blue-500 transition-[width]" style={{ width: `${Math.round(run.progress * 100)}%` }} />
                  </div>
                  <p className="mt-1 truncate text-[10px] text-gray-400">{run.progressMessage || `${Math.round(run.progress * 100)}%`}</p>
                </div>
              ) : null}
            </button>
          ))}
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col bg-gray-50 dark:bg-gray-950">
        {selectedRun ? (
          <>
            <div className="border-b border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <h3 className="truncate text-base font-semibold text-gray-900 dark:text-gray-100">{selectedRun.commandName}</h3>
                  <p className="mt-1 text-xs text-gray-500">{selectedRun.componentId}@{selectedRun.componentVersion} · 尝试 {selectedRun.attempt}/{selectedRun.maxAttempts}</p>
                </div>
                <span className={`rounded px-2 py-1 text-xs ${statusTone(selectedRun.status)}`}>{STATUS_LABELS[selectedRun.status]}</span>
              </div>
              <div className="mt-3 flex flex-wrap gap-2">
                {['queued', 'preparing', 'running', 'cancelling'].includes(selectedRun.status) ? (
                  <button type="button" onClick={() => void perform(() => cancelRun(selectedRun.id))} className="inline-flex items-center gap-1.5 rounded-md border border-red-200 px-2.5 py-1.5 text-xs text-red-600 hover:bg-red-50 dark:border-red-900 dark:hover:bg-red-950/30"><Square className="h-3.5 w-3.5" />取消</button>
                ) : null}
                {['failed', 'cancelled'].includes(selectedRun.status) && selectedRun.executionSemantics !== 'non-idempotent' ? (
                  <button type="button" onClick={() => void perform(() => retryRun(selectedRun.id))} className="inline-flex items-center gap-1.5 rounded-md border border-blue-200 px-2.5 py-1.5 text-xs text-blue-600 hover:bg-blue-50 dark:border-blue-900 dark:hover:bg-blue-950/30"><RotateCcw className="h-3.5 w-3.5" />安全重试</button>
                ) : null}
                {selectedRun.status === 'waiting-permission' ? (
                  <>
                    <button type="button" onClick={() => void perform(() => resolveAttention(selectedRun.id, 'allowOnce'))} className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-2.5 py-1.5 text-xs text-white hover:bg-blue-700"><Play className="h-3.5 w-3.5" />允许一次</button>
                    <button type="button" onClick={() => void perform(() => resolveAttention(selectedRun.id, 'allowSession'))} className="rounded-md border border-gray-300 px-2.5 py-1.5 text-xs text-gray-700 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800">本次会话</button>
                    <button type="button" onClick={() => void perform(() => resolveAttention(selectedRun.id, 'allowAlways'))} className="rounded-md border border-gray-300 px-2.5 py-1.5 text-xs text-gray-700 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800">始终允许</button>
                    <button type="button" onClick={() => void perform(() => resolveAttention(selectedRun.id, 'deny'))} className="inline-flex items-center gap-1.5 rounded-md border border-red-200 px-2.5 py-1.5 text-xs text-red-600"><Ban className="h-3.5 w-3.5" />拒绝</button>
                  </>
                ) : null}
                {selectedRun.status === 'attention' ? (
                  <button type="button" onClick={() => void perform(() => resolveAttention(selectedRun.id, 'markFailed'))} className="inline-flex items-center gap-1.5 rounded-md border border-amber-300 px-2.5 py-1.5 text-xs text-amber-700 dark:text-amber-300"><AlertTriangle className="h-3.5 w-3.5" />确认外部结果并标记失败</button>
                ) : null}
              </div>
              {(actionError || error || selectedRun.error) ? (
                <div className="mt-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">{actionError || error || selectedRun.error}</div>
              ) : null}
              {selectedRun.status === 'running' && selectedRun.progress != null ? (
                <div className="mt-3">
                  <div className="flex justify-between text-[11px] text-gray-500">
                    <span className="truncate">{selectedRun.progressMessage || '脚本正在运行'}</span>
                    <span>{Math.round(selectedRun.progress * 100)}%</span>
                  </div>
                  <div className="mt-1 h-1.5 overflow-hidden rounded bg-gray-200 dark:bg-gray-700">
                    <div className="h-full bg-blue-500 transition-[width]" style={{ width: `${Math.round(selectedRun.progress * 100)}%` }} />
                  </div>
                </div>
              ) : null}
            </div>

            <div className="grid grid-cols-2 gap-px border-b border-gray-200 bg-gray-200 text-xs dark:border-gray-700 dark:bg-gray-700 md:grid-cols-4">
              <RunFact icon={Clock3} label="创建" value={formatTime(selectedRun.createdAt)} />
              <RunFact icon={Loader2} label="耗时" value={duration(selectedRun)} />
              <RunFact icon={ShieldAlert} label="语义" value={selectedRun.executionSemantics} />
              <RunFact icon={CheckCircle2} label="Profile" value={`${selectedRun.profileId} r${selectedRun.profileRevision}`} />
            </div>

            <div className="grid min-h-0 flex-1 grid-cols-1 overflow-auto lg:grid-cols-2">
              <div className="space-y-4 border-r border-gray-200 p-4 dark:border-gray-700">
                <JsonBlock title="输入快照" value={selectedRun.input} />
                <JsonBlock title="输出摘要" value={selectedRun.output ?? null} />
                {selectedRun.permissionRequest ? <JsonBlock title="权限请求" value={selectedRun.permissionRequest} /> : null}
                {selectedRun.attention ? <JsonBlock title="处理记录" value={selectedRun.attention} /> : null}
                {selectedRun.attempts.length ? <JsonBlock title="尝试历史" value={selectedRun.attempts} /> : null}
              </div>
              <div className="flex min-h-[260px] flex-col p-4">
                <h4 className="mb-2 text-xs font-medium text-gray-600 dark:text-gray-300">结构化日志</h4>
                <pre className="min-h-0 flex-1 overflow-auto rounded-md bg-gray-950 p-3 font-mono text-xs leading-5 text-gray-200">{selectedRun.logs.length ? selectedRun.logs.join('\n') : '暂无日志'}</pre>
              </div>
            </div>
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center text-sm text-gray-400">选择一条自动化运行查看输入、输出和权限状态</div>
        )}
      </section>
    </div>
  );
}

function RunFact({ icon: Icon, label, value }: { icon: typeof Clock3; label: string; value: string }) {
  return (
    <div className="min-w-0 bg-white px-3 py-2 dark:bg-gray-900">
      <div className="flex items-center gap-1 text-[11px] text-gray-400"><Icon className="h-3 w-3" />{label}</div>
      <p className="mt-0.5 truncate text-xs text-gray-700 dark:text-gray-200" title={value}>{value}</p>
    </div>
  );
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  return (
    <div>
      <h4 className="mb-2 text-xs font-medium text-gray-600 dark:text-gray-300">{title}</h4>
      <pre className="max-h-56 overflow-auto rounded-md border border-gray-200 bg-white p-3 font-mono text-xs leading-5 text-gray-700 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200">{JSON.stringify(value, null, 2)}</pre>
    </div>
  );
}
