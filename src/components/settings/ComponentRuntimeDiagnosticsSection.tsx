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
  Power,
  PowerOff,
  RefreshCw,
  Square,
  Trash2,
} from 'lucide-react';
import {
  COMPONENT_OPERATION_EVENT,
  cancelComponentOperation,
  deleteComponent,
  disableComponent,
  enableComponent,
  getComponentRuntimeOverview,
  inspectComponentPackage,
  installComponentFromPackage,
  installComponentFromDirectory,
  getPresentationTemplatePreview,
  invokeComponentCommand,
  trustComponentPackagePublisher,
} from '../../api/componentRuntime';
import type {
  ComponentPackageInspection,
  ComponentInvocationResult,
  ComponentOperationSummary,
  ComponentRuntimeCommandError,
  ComponentRuntimeOverview,
  InstalledComponentSummary,
  PresentationTemplatePreview,
} from '../../types/componentRuntime';
import { PLATFORM_MODULE_RUNTIME_CHANGED_EVENT } from '../../api/platformModules';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import { ConfirmDialog, Dialog } from '../Dialog';

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

function previewDocument(preview: PresentationTemplatePreview) {
  const body = preview.baseHtml || '<main><h1>主题模板</h1><p>该模板只提供视觉令牌，没有独立 HTML 结构。</p></main>';
  const styles = preview.styles || '';
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src 'none'; font-src 'none'; connect-src 'none'; media-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"><style>html,body{min-height:100%;margin:0;background:#f8fafc;color:#0f172a;font:14px system-ui,sans-serif}*{box-sizing:border-box}pm-surface-host,pm-overlay-host,pm-window-controls,pm-window-drag-region,pm-recovery-entry,pm-navigation,pm-toolbar,pm-tabs{display:block;min-height:30px;margin:8px;padding:8px;border:1px dashed #94a3b8;border-radius:4px;background:rgba(255,255,255,.78)}pm-surface-host::before{content:'Nexora Surface Host';color:#475569;font-size:12px}pm-overlay-host::before{content:'Overlay Host';color:#475569;font-size:12px}pm-window-controls::before{content:'Window Controls';color:#475569;font-size:12px}pm-window-drag-region::before{content:'Drag Region';color:#475569;font-size:12px}pm-recovery-entry::before{content:'Recovery Entry';color:#475569;font-size:12px}pm-navigation::before{content:'Navigation';color:#475569;font-size:12px}pm-toolbar::before{content:'Toolbar';color:#475569;font-size:12px}pm-tabs::before{content:'Tabs';color:#475569;font-size:12px}</style><style>${styles}</style></head><body>${body}</body></html>`;
}

export function ComponentRuntimeDiagnosticsSection() {
  const [overview, setOverview] = useState<ComponentRuntimeOverview | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [disableTarget, setDisableTarget] = useState<InstalledComponentSummary | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<InstalledComponentSummary | null>(null);
  const [packageInspection, setPackageInspection] = useState<ComponentPackageInspection | null>(null);
  const [templatePreview, setTemplatePreview] = useState<PresentationTemplatePreview | null>(null);
  const [nativeHealthResult, setNativeHealthResult] = useState<ComponentInvocationResult | null>(null);

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
      await useWorkspaceProfileStore.getState().refresh();
      window.dispatchEvent(new Event(PLATFORM_MODULE_RUNTIME_CHANGED_EVENT));
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
      if (!inspection.trust.installable) {
        if (inspection.trust.status === 'signed-untrusted' && inspection.publisher) {
          const shouldTrust = window.confirm(`发布者签名有效，但尚未在本机受信任。\n\n发布者：${inspection.publisher.displayName}\n组件：${label || inspection.componentId || '未知组件'}\n\n确认信任该发布者并继续安装吗？`);
          if (!shouldTrust) return;
          await trustComponentPackagePublisher(packagePath);
        } else {
          setError(inspection.trust.message);
          return;
        }
      }
      if (!window.confirm(`组件包检查完成：${label || inspection.componentId || '未知组件'}\n文件 ${inspection.fileCount} 个，解压后 ${Math.ceil(inspection.totalBytes / 1024)} KiB。\n信任状态：${inspection.trust.message}\n\n确认安装并替换同 ID 的旧版本吗？`)) {
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

  const openTemplatePreview = async (componentId: string, templateId: string) => {
    setPending(`preview:${componentId}:${templateId}`);
    setError(null);
    try {
      setTemplatePreview(await getPresentationTemplatePreview(componentId, templateId));
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setPending(null);
    }
  };

  const runNativeHealthCheck = async (component: InstalledComponentSummary) => {
    const healthCommand = (component.manifest as Record<string, unknown>).healthCommand;
    if (typeof healthCommand !== 'string' || !healthCommand.trim()) return;
    const key = `health:${component.manifest.id}`;
    setPending(key);
    setError(null);
    setNotice(null);
    try {
      const result = await invokeComponentCommand({
        componentId: component.manifest.id,
        moduleId: 'core.recovery-settings',
        command: healthCommand,
        input: { source: 'component-runtime-diagnostics' },
        timeoutMs: 10_000,
      });
      setNativeHealthResult(result);
      setNotice(`“${component.manifest.name}”已通过隔离宿主健康调用。`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      await load();
      setPending(null);
    }
  };

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
                <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">组件安装与运行</h4>
                <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[11px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">R10</span>
              </div>
              <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                所有组件使用同一安装目录、依赖校验、进程监督和操作日志；归档包会先做路径、大小、摘要、入口及表现模板安全检查。
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
              title="刷新组件状态"
              className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
            >
              <RefreshCw className="h-4 w-4" />
            </button>
          </div>
        </div>

        {overview ? (
          <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1 border-y border-gray-100 py-2 text-xs text-gray-500 dark:border-gray-800 dark:text-gray-400">
            <span>{overview.installedComponents.length} 个已启用</span>
            <span>{overview.disabledComponents.length} 个已禁用</span>
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
            <span className={packageInspection.trust.installable ? 'mt-1 block text-emerald-700 dark:text-emerald-300' : 'mt-1 block text-red-700 dark:text-red-300'}>{packageInspection.trust.message}</span>
            {packageInspection.publisher ? <span className="mt-1 block text-gray-500 dark:text-gray-400">发布者：{packageInspection.publisher.displayName}{packageInspection.license ? ` · ${packageInspection.license}` : ''}</span> : null}
            {packageInspection.warnings.length ? (
              <span className="mt-1 block text-amber-700 dark:text-amber-300">{packageInspection.warnings.join('；')}</span>
            ) : null}
          </div>
        ) : null}

        <div className="mt-3 divide-y divide-gray-100 border-y border-gray-100 dark:divide-gray-800 dark:border-gray-800">
          {overview?.installedComponents.map((component) => {
            const id = component.manifest.id;
            const healthCommand = (component.manifest as Record<string, unknown>).healthCommand;
            const templates = [
              ...(component.manifest.contributes?.shellTemplates ?? []).map((template) => ({ id: template.id, name: template.name, kind: 'Shell' })),
              ...(component.manifest.contributes?.pageTemplates ?? []).map((template) => ({ id: template.id, name: template.name, kind: '页面' })),
              ...(component.manifest.contributes?.themePresets ?? []).map((template) => ({ id: template.id, name: template.name, kind: '主题' })),
            ];
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
                    {templates.length ? (
                      <div className="mt-2 flex flex-wrap gap-1.5">
                        {templates.map((template) => (
                          <button
                            key={template.id}
                            type="button"
                            disabled={!component.packagePath || pending !== null}
                            onClick={() => void openTemplatePreview(id, template.id)}
                            title={component.packagePath ? `在隔离预览中查看${template.name}` : '随程序提供的兼容模板由宿主直接渲染'}
                            className="inline-flex items-center gap-1 rounded border border-violet-200 px-1.5 py-1 text-[11px] text-violet-700 hover:bg-violet-50 disabled:cursor-not-allowed disabled:opacity-45 dark:border-violet-900/60 dark:text-violet-300 dark:hover:bg-violet-950/30"
                          >
                            {pending === `preview:${id}:${template.id}` ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
                            {template.kind} · {template.name}
                          </button>
                        ))}
                      </div>
                    ) : null}
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    {component.manifest.runtime === 'native-library' && typeof healthCommand === 'string' ? (
                      <button
                        type="button"
                        onClick={() => void runNativeHealthCheck(component)}
                        disabled={pending !== null}
                        title="通过隔离宿主执行组件健康调用"
                        className="inline-flex h-8 items-center gap-1.5 rounded-md border border-emerald-200 px-2.5 text-xs text-emerald-700 hover:bg-emerald-50 disabled:opacity-40 dark:border-emerald-900/60 dark:text-emerald-300 dark:hover:bg-emerald-950/30"
                      >
                        {pending === `health:${id}` ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <CheckCircle2 className="h-3.5 w-3.5" />}
                        健康调用
                      </button>
                    ) : null}
                    <button
                      type="button"
                      onClick={() => setDisableTarget(component)}
                      disabled={!component.removable || component.activeOperationCount > 0 || pending !== null}
                      title={component.activeOperationCount > 0 ? '组件仍有运行中的操作' : '禁用组件并保留安装文件'}
                      className="inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs text-amber-700 hover:bg-amber-50 disabled:opacity-40 dark:text-amber-300 dark:hover:bg-amber-950/30"
                    >
                      {pending === `disable:${id}` ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <PowerOff className="h-3.5 w-3.5" />}
                      禁用
                    </button>
                    {component.source !== 'bundled' ? (
                      <button
                        type="button"
                        onClick={() => setDeleteTarget(component)}
                        disabled={!component.removable || component.activeOperationCount > 0 || pending !== null}
                        title="删除第三方组件安装副本"
                        className="inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs text-red-600 hover:bg-red-50 disabled:opacity-40 dark:text-red-300 dark:hover:bg-red-950/30"
                      >
                        {pending === `delete:${id}` ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Trash2 className="h-3.5 w-3.5" />}
                        删除
                      </button>
                    ) : null}
                  </div>
                </div>
              </div>
            );
          })}
          {!overview?.installedComponents.length ? (
            <p className="py-4 text-xs text-gray-500 dark:text-gray-400">当前没有已启用组件。</p>
          ) : null}
        </div>

        {overview?.disabledComponents.length ? (
          <div className="mt-4">
            <p className="text-xs font-medium text-gray-700 dark:text-gray-300">已禁用组件</p>
            <div className="mt-2 divide-y divide-gray-100 rounded-md border border-gray-200 dark:divide-gray-800 dark:border-gray-700">
              {overview.disabledComponents.map((component) => (
                <div key={component.manifest.id} className="flex items-center justify-between gap-3 px-3 py-2.5">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="truncate text-xs font-medium text-gray-800 dark:text-gray-200">{component.manifest.name}</p>
                      <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">{SOURCE_LABEL[component.source]}</span>
                    </div>
                    <p className="mt-0.5 truncate font-mono text-[10px] text-gray-400">{component.manifest.id} · {component.manifest.version} · {component.manifest.runtime}</p>
                    {component.packagePath ? <p className="mt-0.5 truncate text-[10px] text-gray-400">{component.packagePath}</p> : null}
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                      onClick={() => void runAction(`enable:${component.manifest.id}`, () => enableComponent(component.manifest.id), `已启用“${component.manifest.name}”。`)}
                      disabled={pending !== null}
                      className="inline-flex h-8 items-center gap-1.5 rounded-md bg-blue-600 px-2.5 text-xs text-white hover:bg-blue-700 disabled:opacity-50"
                    >
                      {pending === `enable:${component.manifest.id}` ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Power className="h-3.5 w-3.5" />}
                      启用
                    </button>
                    {component.source !== 'bundled' ? (
                      <button
                        type="button"
                        onClick={() => setDeleteTarget(component)}
                        disabled={pending !== null}
                        title="删除第三方组件安装副本"
                        className="inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs text-red-600 hover:bg-red-50 disabled:opacity-40 dark:text-red-300 dark:hover:bg-red-950/30"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        删除
                      </button>
                    ) : null}
                  </div>
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
          <div className="mt-3 flex items-center gap-2 py-4 text-xs text-gray-500"><Loader2 className="h-4 w-4 animate-spin" />正在读取组件状态...</div>
        )}
      </section>

      <ConfirmDialog
        isOpen={Boolean(disableTarget)}
        onClose={() => setDisableTarget(null)}
        onConfirm={() => {
          const target = disableTarget;
          setDisableTarget(null);
          if (target) {
            void runAction(
              `disable:${target.manifest.id}`,
              () => disableComponent(target.manifest.id),
              `已禁用“${target.manifest.name}”。安装文件和持久状态均已保留。`,
            );
          }
        }}
        title="禁用组件"
        message={disableTarget
          ? `确定禁用“${disableTarget.manifest.name}”吗？\n\n组件会停止运行并撤下页面、菜单和文件处理入口，但安装文件、装配引用和持久状态都会保留，可随时重新启用。`
          : ''}
        confirmText="确认禁用"
        cancelText="取消"
        type="warning"
      />

      <ConfirmDialog
        isOpen={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => {
          const target = deleteTarget;
          setDeleteTarget(null);
          if (target) {
            void runAction(
              `delete:${target.manifest.id}`,
              () => deleteComponent(target.manifest.id),
              `已删除“${target.manifest.name}”的安装副本，并清理相关装配入口。持久状态仍保留。`,
            );
          }
        }}
        title="删除组件"
        message={deleteTarget
          ? `确定删除“${deleteTarget.manifest.name}”吗？\n\n将删除 Nexora 中的组件安装副本和已缓存安装包，并从所有装配方案中清理该组件的页面、快捷栏、自动化与文件处理引用。组件持久状态仍会保留；开发源目录和你保存的原始 .pmc-pack 不受影响。`
          : ''}
        confirmText="确认删除"
        cancelText="取消"
        type="danger"
      />

      <Dialog
        isOpen={Boolean(templatePreview)}
        onClose={() => setTemplatePreview(null)}
        title={templatePreview ? `${templatePreview.name} · 安全预览` : '安全预览'}
        size="2xl"
        contentClassName="min-h-0 overflow-hidden p-0"
      >
        {templatePreview ? (
          <div className="flex min-h-[56vh] flex-col">
            <div className="border-b border-gray-200 px-4 py-2 text-xs text-gray-500 dark:border-gray-800 dark:text-gray-400">
              <span>{templatePreview.componentName}</span><span className="mx-2">·</span><span>{templatePreview.kind}</span><span className="mx-2">·</span><span>{templatePreview.version}</span>
            </div>
            <iframe
              title={`${templatePreview.name} 安全预览`}
              sandbox=""
              srcDoc={previewDocument(templatePreview)}
              className="min-h-0 flex-1 border-0 bg-white"
            />
            <p className="border-t border-gray-200 px-4 py-2 text-[11px] text-gray-400 dark:border-gray-800">该预览不执行脚本、不开放网络或 Tauri API；插槽显示为宿主占位。</p>
          </div>
        ) : null}
      </Dialog>

      <Dialog
        isOpen={Boolean(nativeHealthResult)}
        onClose={() => setNativeHealthResult(null)}
        title="隔离原生组件健康调用"
        size="lg"
      >
        {nativeHealthResult ? (
          <div className="space-y-3 text-xs">
            <p className="text-gray-600 dark:text-gray-300">{nativeHealthResult.componentId} · {nativeHealthResult.durationMs} ms · 宿主进程已完成并退出。</p>
            <pre className="max-h-72 overflow-auto rounded-md bg-gray-950 p-3 font-mono text-[11px] text-gray-100">{JSON.stringify(nativeHealthResult.output, null, 2)}</pre>
            {nativeHealthResult.logs.length ? <p className="break-all text-[11px] text-gray-500">{nativeHealthResult.logs.join('\n')}</p> : null}
          </div>
        ) : null}
      </Dialog>
    </>
  );
}
