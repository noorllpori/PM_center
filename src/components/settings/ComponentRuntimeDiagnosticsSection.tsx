import { useCallback, useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import {
  AlertTriangle,
  Boxes,
  CheckCircle2,
  FolderPlus,
  Loader2,
  PackageCheck,
  PackagePlus,
  RefreshCw,
  Square,
  Trash2,
} from 'lucide-react';
import {
  COMPONENT_OPERATION_EVENT,
  cancelComponentOperation,
  getComponentRuntimeOverview,
  inspectComponentPackage,
  installComponentFromPackage,
  installComponentFromDirectory,
  reinstallBundledComponent,
  uninstallComponent,
} from '../../api/componentRuntime';
import type {
  ComponentPackageInspection,
  ComponentOperationSummary,
  ComponentRuntimeCommandError,
  ComponentRuntimeOverview,
  InstalledComponentSummary,
} from '../../types/componentRuntime';
import { ConfirmDialog } from '../Dialog';

const SOURCE_LABEL = {
  bundled: '随安装包',
  local: '本地目录',
  marketplace: '组件商城',
} as const;

const STATUS_LABEL = {
  starting: '启动中',
  running: '运行中',
  completed: '已完成',
  failed: '失败',
  cancelled: '已取消',
  'timed-out': '超时',
} as const;

function errorMessage(error: unknown) {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object') {
    const typed = error as ComponentRuntimeCommandError;
    return [typed.code, typed.message, ...(typed.details ?? [])].filter(Boolean).join('\n');
  }
  return String(error);
}

function formatTime(timestamp?: number | null) {
  return timestamp ? new Date(timestamp).toLocaleString('zh-CN', { hour12: false }) : '-';
}

function contributionCount(component: InstalledComponentSummary) {
  const contributes = component.manifest.contributes;
  return [
    contributes?.workflowNodes,
    contributes?.toolActions,
    contributes?.widgets,
    contributes?.dataSources,
    contributes?.settingsSections,
    contributes?.shellTemplates,
    contributes?.pageTemplates,
    contributes?.themePresets,
    contributes?.fileHandlers,
  ].reduce<number>((total, values) => total + (Array.isArray(values) ? values.length : 0), 0);
}

function operationTone(status: ComponentOperationSummary['status']) {
  if (status === 'completed') return 'text-emerald-700 dark:text-emerald-300';
  if (status === 'failed' || status === 'timed-out') return 'text-red-700 dark:text-red-300';
  if (status === 'running' || status === 'starting') return 'text-blue-700 dark:text-blue-300';
  return 'text-gray-500 dark:text-gray-400';
}

export function ComponentRuntimeDiagnosticsSection() {
  const [overview, setOverview] = useState<ComponentRuntimeOverview | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [uninstallTarget, setUninstallTarget] = useState<InstalledComponentSummary | null>(null);
  const [packageInspection, setPackageInspection] = useState<ComponentPackageInspection | null>(null);

  const load = useCallback(async () => {
    try {
      setError(null);
      setOverview(await getComponentRuntimeOverview());
    } catch (nextError) {
      setError(errorMessage(nextError));
    }
  }, []);

  useEffect(() => {
    void load();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ComponentOperationSummary>(COMPONENT_OPERATION_EVENT, () => {
      if (!disposed) void load();
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [load]);

  const runAction = useCallback(async (key: string, action: () => Promise<unknown>, message: string) => {
    setPending(key);
    setError(null);
    setNotice(null);
    try {
      await action();
      setNotice(message);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      await load();
      setPending(null);
    }
  }, [load]);

  const installLocal = async () => {
    const sourcePath = await open({
      title: '选择包含 component.json 的组件目录',
      directory: true,
      multiple: false,
    });
    if (!sourcePath || Array.isArray(sourcePath)) return;
    await runAction('install', () => installComponentFromDirectory(sourcePath), '组件已安装并同步到装配方案目录。');
  };

  const installPackage = async () => {
    const packagePath = await open({
      title: '选择 .pmc-pack 组件包',
      multiple: false,
      directory: false,
      filters: [{ name: 'Nexora 组件包', extensions: ['pmc-pack'] }],
    });
    if (!packagePath || Array.isArray(packagePath)) return;
    try {
      setError(null);
      const inspection = await inspectComponentPackage(packagePath);
      setPackageInspection(inspection);
      const label = [inspection.componentName, inspection.componentVersion].filter(Boolean).join(' ');
      if (!window.confirm(`已通过安全检查：${label || inspection.componentId || '未知组件'}\n文件 ${inspection.fileCount} 个，解压后 ${Math.ceil(inspection.totalBytes / 1024)} KiB。\n\n确认安装并替换同 ID 的旧版本吗？`)) {
        return;
      }
      await runAction('install-package', () => installComponentFromPackage(packagePath), '组件包已校验并安装。');
    } catch (nextError) {
      setError(errorMessage(nextError));
    }
  };

  const templateCount = useMemo(() => overview
    ? overview.templates.shellTemplates.length
      + overview.templates.pageTemplates.length
      + overview.templates.themePresets.length
    : 0, [overview]);

  return (
    <>
      <section className="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex min-w-0 items-start gap-2.5">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-cyan-100 text-cyan-700 dark:bg-cyan-950/50 dark:text-cyan-300">
              <Boxes className="h-4 w-4" />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">组件运行时</h4>
                <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[11px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">R10-1</span>
              </div>
              <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                所有组件使用同一安装目录、依赖校验、进程监督和操作日志；归档包会先做路径、大小、摘要和入口检查。
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => void installLocal()}
              disabled={pending !== null}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
            >
              {pending === 'install' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <FolderPlus className="h-3.5 w-3.5" />}
              从目录安装
            </button>
            <button
              type="button"
              onClick={() => void installPackage()}
              disabled={pending !== null}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
              title="检查并安装 .pmc-pack"
            >
              {pending === 'install-package' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <PackageCheck className="h-3.5 w-3.5" />}
              安装组件包
            </button>
            <button
              type="button"
              onClick={() => void load()}
              disabled={pending !== null}
              title="刷新组件运行时"
              className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
            >
              <RefreshCw className="h-4 w-4" />
            </button>
          </div>
        </div>

        {overview ? (
          <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1 border-y border-gray-100 py-2 text-xs text-gray-500 dark:border-gray-800 dark:text-gray-400">
            <span>{overview.installedComponents.length} 个已安装</span>
            <span>{overview.availableBundledComponents.length} 个可重新安装</span>
            <span>{overview.activeOperations.length} 个活动操作</span>
            <span>{templateCount} 个表现模板</span>
            <span>{overview.legacyPythonActionCompatible ? '兼容旧 Python 动作' : '旧 Python 动作不可用'}</span>
            <span className={overview.componentHostAvailable ? 'text-emerald-600 dark:text-emerald-300' : 'text-amber-600 dark:text-amber-300'}>
              {overview.componentHostAvailable ? '原生隔离宿主已就绪' : '原生隔离宿主未安装'}
            </span>
          </div>
        ) : null}

        {error ? (
          <div className="mt-3 flex items-start gap-2 rounded-md bg-red-50 px-3 py-2 text-xs text-red-700 dark:bg-red-950/30 dark:text-red-300">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span className="whitespace-pre-wrap break-all">{error}</span>
          </div>
        ) : null}
        {notice ? (
          <div className="mt-3 flex items-start gap-2 rounded-md bg-emerald-50 px-3 py-2 text-xs text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300">
            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{notice}</span>
          </div>
        ) : null}
        {packageInspection ? (
          <div className="mt-3 rounded-md border border-blue-100 bg-blue-50 px-3 py-2 text-xs text-blue-800 dark:border-blue-900/50 dark:bg-blue-950/30 dark:text-blue-200">
            最近检查：<span className="font-medium">{packageInspection.componentName || packageInspection.componentId || '组件包'}</span>
            {' · '}{packageInspection.componentVersion || '-'}{' · '}{packageInspection.fileCount} 个文件{' · '}{Math.ceil(packageInspection.totalBytes / 1024)} KiB
            {packageInspection.warnings.length ? (
              <span className="mt-1 block text-amber-700 dark:text-amber-300">{packageInspection.warnings.join('；')}</span>
            ) : null}
          </div>
        ) : null}

        <div className="mt-3 divide-y divide-gray-100 border-y border-gray-100 dark:divide-gray-800 dark:border-gray-800">
          {overview?.installedComponents.map((component) => {
            const id = component.manifest.id;
            const busy = pending === `uninstall:${id}`;
            return (
              <div key={id} className="py-3">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium text-gray-900 dark:text-gray-100">{component.manifest.name}</span>
                      <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[11px] text-gray-600 dark:bg-gray-800 dark:text-gray-300">{SOURCE_LABEL[component.source]}</span>
                      <span className="rounded bg-cyan-50 px-1.5 py-0.5 font-mono text-[11px] text-cyan-700 dark:bg-cyan-950/40 dark:text-cyan-300">{component.manifest.runtime}</span>
                      {component.worker ? (
                        <span className="rounded bg-blue-50 px-1.5 py-0.5 text-[11px] text-blue-700 dark:bg-blue-950/40 dark:text-blue-300">Worker {component.worker.pid} · {component.worker.status}</span>
                      ) : null}
                    </div>
                    <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{component.manifest.description}</p>
                    <p className="mt-1 break-all font-mono text-[11px] text-gray-400">{id} · {component.manifest.version} · {contributionCount(component)} 项贡献</p>
                    {component.packagePath ? <p className="mt-1 break-all text-[11px] text-gray-400">{component.packagePath}</p> : null}
                  </div>
                  <button
                    type="button"
                    onClick={() => setUninstallTarget(component)}
                    disabled={!component.removable || component.activeOperationCount > 0 || pending !== null}
                    title={component.activeOperationCount > 0 ? '组件仍有运行中的操作' : '卸载组件'}
                    className="inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs text-red-600 hover:bg-red-50 disabled:opacity-40 dark:text-red-300 dark:hover:bg-red-950/30"
                  >
                    {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Trash2 className="h-3.5 w-3.5" />}
                    卸载
                  </button>
                </div>
              </div>
            );
          })}
          {!overview?.installedComponents.length ? (
            <p className="py-4 text-xs text-gray-500 dark:text-gray-400">当前没有已安装组件。</p>
          ) : null}
        </div>

        {overview?.availableBundledComponents.length ? (
          <div className="mt-4">
            <p className="text-xs font-medium text-gray-700 dark:text-gray-300">安装器随附但已卸载</p>
            <div className="mt-2 divide-y divide-gray-100 rounded-md border border-gray-200 dark:divide-gray-800 dark:border-gray-700">
              {overview.availableBundledComponents.map((manifest) => (
                <div key={manifest.id} className="flex items-center justify-between gap-3 px-3 py-2.5">
                  <div className="min-w-0">
                    <p className="truncate text-xs font-medium text-gray-800 dark:text-gray-200">{manifest.name}</p>
                    <p className="truncate font-mono text-[10px] text-gray-400">{manifest.id} · {manifest.version}</p>
                  </div>
                  <button
                    type="button"
                    onClick={() => void runAction(`reinstall:${manifest.id}`, () => reinstallBundledComponent(manifest.id), `已重新安装“${manifest.name}”。`)}
                    disabled={pending !== null}
                    className="inline-flex shrink-0 items-center gap-1 rounded-md bg-blue-600 px-2.5 py-1.5 text-xs text-white hover:bg-blue-700 disabled:opacity-50"
                  >
                    {pending === `reinstall:${manifest.id}` ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <PackagePlus className="h-3.5 w-3.5" />}
                    重新安装
                  </button>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        {(overview?.activeOperations.length || overview?.recentOperations.length) ? (
          <div className="mt-4">
            <p className="text-xs font-medium text-gray-700 dark:text-gray-300">组件操作</p>
            <div className="mt-2 divide-y divide-gray-100 border-y border-gray-100 dark:divide-gray-800 dark:border-gray-800">
              {[...(overview?.activeOperations ?? []), ...(overview?.recentOperations ?? []).slice(0, 8)].map((operation) => {
                const active = operation.status === 'running' || operation.status === 'starting';
                return (
                  <div key={`${operation.operationId}:${operation.status}`} className="flex items-start justify-between gap-3 py-2.5 text-xs">
                    <div className="min-w-0">
                      <p className="truncate text-gray-800 dark:text-gray-200">{operation.componentId} · {operation.command}</p>
                      <p className="mt-0.5 truncate text-[11px] text-gray-400">{formatTime(operation.startedAt)} · {operation.message}</p>
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      <span className={operationTone(operation.status)}>{STATUS_LABEL[operation.status]}</span>
                      {active ? (
                        <button
                          type="button"
                          onClick={() => void runAction(`cancel:${operation.operationId}`, () => cancelComponentOperation(operation.operationId), '已发送取消请求。')}
                          disabled={pending !== null}
                          title="取消组件操作"
                          className="flex h-7 w-7 items-center justify-center rounded-md text-red-600 hover:bg-red-50 dark:text-red-300 dark:hover:bg-red-950/30"
                        >
                          <Square className="h-3.5 w-3.5" />
                        </button>
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        {overview ? (
          <div className="mt-3 space-y-1 text-[11px] text-gray-400">
            <p className="break-all">组件目录：{overview.rootPath}</p>
            <p className="break-all">安装状态：{overview.statePath}</p>
          </div>
        ) : (
          <div className="mt-3 flex items-center gap-2 py-4 text-xs text-gray-500"><Loader2 className="h-4 w-4 animate-spin" />正在读取组件运行时...</div>
        )}
      </section>

      <ConfirmDialog
        isOpen={Boolean(uninstallTarget)}
        onClose={() => setUninstallTarget(null)}
        onConfirm={() => {
          const target = uninstallTarget;
          setUninstallTarget(null);
          if (target) {
            void runAction(
              `uninstall:${target.manifest.id}`,
              () => uninstallComponent(target.manifest.id),
              `已卸载“${target.manifest.name}”。依赖它的 Profile 会显示为缺失，重新安装后可恢复。`,
            );
          }
        }}
        title="卸载组件"
        message={uninstallTarget
          ? `确定卸载“${uninstallTarget.manifest.name}”吗？\n\n组件数据目录会按来源撤下；依赖它的模块和装配方案不会被删除，但在重新安装前无法运行。`
          : ''}
        confirmText="确认卸载"
        cancelText="取消"
        type="danger"
      />
    </>
  );
}
