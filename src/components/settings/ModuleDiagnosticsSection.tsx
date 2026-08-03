import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  FlaskConical,
  Power,
  RefreshCw,
  RotateCcw,
  ShieldAlert,
  Square,
} from 'lucide-react';
import {
  configurePlatformModuleFailure,
  disablePlatformModule,
  enablePlatformModule,
  getPlatformModuleFailureInjections,
  getPlatformModuleRuntime,
  previewDisablePlatformModule,
  restartPlatformModule,
  runPlatformCycleLeakTest,
  runPlatformModuleHealthCheck,
} from '../../api/platformModules';
import type {
  PlatformDiagnosticResult,
  PlatformModuleCommandError,
  PlatformModuleRuntimeOverview,
  PlatformModuleState,
} from '../../types/platformRuntime';

const STATE_LABELS: Record<PlatformModuleState, string> = {
  disabled: '已停用',
  resolving: '解析依赖',
  starting: '启动中',
  running: '运行中',
  stopping: '停止中',
  blocked: '被阻止',
  error: '错误',
  'restart-required': '需要重启',
};

function stateTone(state: PlatformModuleState) {
  if (state === 'running') {
    return 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300';
  }
  if (state === 'error' || state === 'blocked') {
    return 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-300';
  }
  if (state === 'starting' || state === 'stopping' || state === 'resolving' || state === 'restart-required') {
    return 'bg-amber-100 text-amber-700 dark:bg-amber-950/50 dark:text-amber-300';
  }
  return 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300';
}

function formatCommandError(error: unknown) {
  if (typeof error === 'string') {
    return error;
  }
  if (error && typeof error === 'object') {
    const typed = error as PlatformModuleCommandError;
    const detail = typed.details?.length ? `\n${typed.details.join('\n')}` : '';
    return `${typed.code ? `${typed.code}: ` : ''}${typed.message || String(error)}${detail}`;
  }
  return String(error);
}

export function ModuleDiagnosticsSection() {
  const [overview, setOverview] = useState<PlatformModuleRuntimeOverview | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<PlatformDiagnosticResult | null>(null);
  const [injections, setInjections] = useState<Record<string, boolean>>({});

  const load = useCallback(async () => {
    try {
      setError(null);
      const [nextOverview, nextInjections] = await Promise.all([
        getPlatformModuleRuntime(),
        getPlatformModuleFailureInjections(),
      ]);
      setOverview(nextOverview);
      setInjections(nextInjections);
    } catch (nextError) {
      setError(formatCommandError(nextError));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const runAction = useCallback(
    async (key: string, action: () => Promise<unknown>) => {
      setPending(key);
      setError(null);
      try {
        await action();
      } catch (nextError) {
        setError(formatCommandError(nextError));
      } finally {
        await load();
        setPending(null);
      }
    },
    [load],
  );

  const disable = useCallback(
    async (moduleId: string, force = false) => {
      await runAction(`${moduleId}:disable`, async () => {
        if (force) {
          await disablePlatformModule(moduleId, 'force');
          return;
        }
        const preview = await previewDisablePlatformModule(moduleId);
        if (
          !preview.canDisableGracefully
          && !confirm(`${preview.message}\n\n将按依赖关系级联停用：\n${preview.runningDependents.join('\n')}`)
        ) {
          return;
        }
        await disablePlatformModule(
          moduleId,
          preview.canDisableGracefully ? 'graceful' : 'cascade',
        );
      });
    },
    [runAction],
  );

  const toggleInjection = useCallback(
    async (moduleId: string, point: string) => {
      const key = `${moduleId}:${point}`;
      const enabled = !injections[key];
      await runAction(key, async () => {
        await configurePlatformModuleFailure(moduleId, point, enabled);
        setInjections((current) => ({ ...current, [key]: enabled }));
      });
    },
    [injections, runAction],
  );

  const modules = useMemo(
    () =>
      [...(overview?.modules || [])].sort(
        (left, right) => Number(left.diagnostic) - Number(right.diagnostic),
      ),
    [overview],
  );
  const managedModuleCount = modules.filter((module) => !module.diagnostic).length;
  const diagnosticModuleCount = modules.length - managedModuleCount;

  return (
    <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Activity className="h-4 w-4 text-blue-500" />
            <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">后台模块</h4>
            <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[11px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">
              R4
            </span>
          </div>
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            智能剪贴板已由模块生命周期托管；停用会释放监听线程、原生窗口和快捷键，但保留历史数据。
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void runAction('health', () => runPlatformModuleHealthCheck())}
            disabled={pending !== null}
            className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-2.5 py-1.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            <CheckCircle2 className="h-3.5 w-3.5" />
            健康检查
          </button>
          <button
            type="button"
            onClick={() => void load()}
            disabled={pending !== null}
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
            title="刷新模块状态"
          >
            <RefreshCw className={`h-4 w-4 ${pending ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {overview && (
        <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1 border-y border-gray-100 py-2 text-xs text-gray-500 dark:border-gray-800 dark:text-gray-400">
          <span>{managedModuleCount} 个已接入模块</span>
          <span>{diagnosticModuleCount} 个诊断模块</span>
          <span>{overview.resourceCount} 个已登记资源</span>
          <span className="min-w-0 truncate" title={overview.persistencePath}>状态：{overview.persistencePath}</span>
        </div>
      )}

      {(error || overview?.startupNotice) && (
        <div className="mt-3 flex items-start gap-2 rounded-md bg-red-50 px-3 py-2 text-xs text-red-700 dark:bg-red-950/30 dark:text-red-300">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span className="whitespace-pre-wrap break-all">
            {error || `${overview?.startupNotice?.code}: ${overview?.startupNotice?.message}`}
          </span>
        </div>
      )}

      <div className="mt-3 divide-y divide-gray-100 border-y border-gray-100 dark:divide-gray-800 dark:border-gray-800">
        {modules.map((module, index) => {
          const moduleId = module.manifest.id;
          const busy = pending?.startsWith(moduleId) || false;
          const running = module.state === 'running';
          return (
            <div key={moduleId}>
              {(index === 0 || modules[index - 1]?.diagnostic !== module.diagnostic) && (
                <div className="border-b border-gray-100 py-2 text-xs font-medium text-gray-500 dark:border-gray-800 dark:text-gray-400">
                  {module.diagnostic ? '隔离诊断模块' : '已接入后台模块'}
                </div>
              )}
              <div className="py-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium text-gray-900 dark:text-gray-100">
                      {module.manifest.name}
                    </span>
                    <span className={`rounded px-1.5 py-0.5 text-[11px] font-medium ${stateTone(module.state)}`}>
                      {STATE_LABELS[module.state]}
                    </span>
                    {module.desiredEnabled && (
                      <span className="text-[11px] text-blue-600 dark:text-blue-300">已保存启用</span>
                    )}
                  </div>
                  <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                    {module.manifest.description}
                  </p>
                  <p className="mt-1 font-mono text-[11px] text-gray-400">{moduleId}</p>
                </div>

                <div className="flex flex-wrap items-center justify-end gap-1.5">
                  {!running ? (
                    <button
                      type="button"
                      onClick={() => void runAction(`${moduleId}:enable`, () => enablePlatformModule(moduleId))}
                      disabled={busy || pending !== null}
                      className="inline-flex items-center gap-1 rounded-md bg-blue-600 px-2.5 py-1.5 text-xs text-white hover:bg-blue-700 disabled:opacity-50"
                    >
                      <Power className="h-3.5 w-3.5" />
                      启用
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => void disable(moduleId)}
                      disabled={busy || pending !== null}
                      className="inline-flex items-center gap-1 rounded-md border border-gray-200 px-2.5 py-1.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
                    >
                      <Square className="h-3.5 w-3.5" />
                      停用
                    </button>
                  )}
                  {running && (
                    <button
                      type="button"
                      onClick={() => void runAction(`${moduleId}:restart`, () => restartPlatformModule(moduleId))}
                      disabled={busy || pending !== null}
                      className="flex h-7 w-7 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
                      title="重启模块"
                    >
                      <RotateCcw className="h-3.5 w-3.5" />
                    </button>
                  )}
                  {module.state === 'error' && (
                    <button
                      type="button"
                      onClick={() => void disable(moduleId, true)}
                      disabled={busy || pending !== null}
                      className="inline-flex items-center gap-1 rounded-md px-2 py-1.5 text-xs text-red-600 hover:bg-red-50 disabled:opacity-50 dark:text-red-300 dark:hover:bg-red-950/30"
                    >
                      <ShieldAlert className="h-3.5 w-3.5" />
                      强制释放
                    </button>
                  )}
                </div>
              </div>

              <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-gray-500 dark:text-gray-400">
                <span>健康：{module.health.message}</span>
                <span>资源：{module.resources.length}</span>
                {module.dependencies.map((dependency) => (
                  <span key={`${moduleId}:${dependency.id}`}>
                    {dependency.required ? '依赖' : '可选'}：{dependency.id} · {dependency.installed ? dependency.state : '未安装'}
                  </span>
                ))}
              </div>

              {module.resources.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {module.resources.map((resource) => (
                    <span
                      key={resource.id}
                      className="rounded bg-gray-100 px-2 py-1 text-[11px] text-gray-600 dark:bg-gray-800 dark:text-gray-300"
                      title={resource.id}
                    >
                      {resource.kind} · {resource.label}
                    </span>
                  ))}
                </div>
              )}

              {module.lastError && (
                <div className="mt-2 text-xs text-red-600 dark:text-red-300">
                  {module.lastError.code}：{module.lastError.message}
                </div>
              )}

              {moduleId === 'diagnostic.runtime-failing' && (
                <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
                  <span className="text-gray-500">故障注入</span>
                  {['start', 'health'].map((point) => {
                    const key = `${moduleId}:${point}`;
                    return (
                      <button
                        key={point}
                        type="button"
                        onClick={() => void toggleInjection(moduleId, point)}
                        disabled={pending !== null}
                        className={`rounded-md px-2 py-1 transition-colors disabled:opacity-50 ${
                          injections[key]
                            ? 'bg-red-100 text-red-700 dark:bg-red-950/40 dark:text-red-300'
                            : 'bg-gray-100 text-gray-600 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700'
                        }`}
                      >
                        {point === 'start' ? '启动失败' : '健康失败'}
                      </button>
                    );
                  })}
                </div>
              )}

              {moduleId === 'diagnostic.runtime-slow-stop' && (
                <div className="mt-2 flex items-center gap-2 text-xs">
                  <span className="text-gray-500">故障注入</span>
                  <button
                    type="button"
                    onClick={() => void toggleInjection(moduleId, 'stop-timeout')}
                    disabled={pending !== null}
                    className={`rounded-md px-2 py-1 transition-colors disabled:opacity-50 ${
                      injections[`${moduleId}:stop-timeout`]
                        ? 'bg-red-100 text-red-700 dark:bg-red-950/40 dark:text-red-300'
                        : 'bg-gray-100 text-gray-600 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700'
                    }`}
                  >
                    停止超时
                  </button>
                </div>
              )}
              </div>
            </div>
          );
        })}
      </div>

      <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 text-xs text-gray-500 dark:text-gray-400">
          {testResult ? (
            <span className={testResult.success ? 'text-emerald-600 dark:text-emerald-300' : 'text-red-600 dark:text-red-300'}>
              {testResult.message}
            </span>
          ) : (
            '模块停用只释放运行资源，不删除模块数据；100 次检查仅针对隔离诊断模块。'
          )}
        </div>
        <button
          type="button"
          onClick={() => void runAction('cycle-test', async () => setTestResult(await runPlatformCycleLeakTest()))}
          disabled={pending !== null}
          className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-2.5 py-1.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
        >
          <FlaskConical className="h-3.5 w-3.5" />
          运行 100 次泄漏检查
        </button>
      </div>
    </section>
  );
}
