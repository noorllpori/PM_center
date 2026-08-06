import { useCallback, useEffect, useMemo, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  FileCog,
  Loader2,
  RefreshCw,
  Route,
  Trash2,
} from 'lucide-react';
import {
  getFileRoutingSnapshotForContext,
  routeFileIntent,
  removeFileAssociationBinding,
  setFileAssociationBinding,
  type FileAssociationBinding,
  type FileIntent,
  type FileRoutePlan,
  type FileRoutingScopeContext,
  type FileRoutingSnapshot,
} from '../../api/fileRouter';
import { getComponentRuntimeOverview } from '../../api/componentRuntime';
import { useProjectStore } from '../../stores/projectStore';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import type { ComponentRuntimeOverview } from '../../types/componentRuntime';

const INTENTS: Array<{ value: FileIntent; label: string }> = [
  { value: 'open', label: '默认打开' },
  { value: 'open-internal', label: '仅 Nexora 内部打开' },
  { value: 'preview', label: '预览' },
  { value: 'edit', label: '编辑' },
  { value: 'inspect', label: '结构化解析' },
];

const SCOPE_LABEL: Record<string, string> = {
  global: '全局',
  profile: '当前装配方案',
  project: '当前项目',
};

function errorMessage(error: unknown) {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object') {
    const value = error as { code?: string; message?: string; details?: string[] };
    return [value.code, value.message, ...(value.details ?? [])].filter(Boolean).join('\n');
  }
  return String(error);
}

function createBindingId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return `file-association-${crypto.randomUUID()}`;
  }
  return `file-association-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function FileRoutingSettingsSection() {
  const projectPath = useProjectStore((state) => state.projectPath);
  const profile = useWorkspaceProfileStore((state) => state.snapshot?.currentProfile ?? null);
  const context = useMemo<FileRoutingScopeContext>(() => ({
    projectPath: projectPath || undefined,
    profileId: profile?.id,
  }), [profile?.id, projectPath]);
  const [snapshot, setSnapshot] = useState<FileRoutingSnapshot | null>(null);
  const [runtime, setRuntime] = useState<ComponentRuntimeOverview | null>(null);
  const [extension, setExtension] = useState('txt');
  const [scope, setScope] = useState<'global' | 'profile' | 'project'>('global');
  const [intent, setIntent] = useState<FileIntent>('open');
  const [handler, setHandler] = useState('system');
  const [strict, setStrict] = useState(false);
  const [routePlan, setRoutePlan] = useState<FileRoutePlan | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const handlers = useMemo(() => (runtime?.installedComponents ?? [])
    .flatMap((component) => (component.manifest.contributes?.fileHandlers ?? []).map((item) => ({
      ...item,
      componentId: component.manifest.id,
      componentName: component.manifest.name,
    })))
    .sort((left, right) => left.name.localeCompare(right.name, 'zh-CN')),
  [runtime]);

  const load = useCallback(async () => {
    const [nextSnapshot, nextRuntime] = await Promise.all([
      getFileRoutingSnapshotForContext(context),
      getComponentRuntimeOverview(),
    ]);
    setSnapshot(nextSnapshot);
    setRuntime(nextRuntime);
  }, [context]);

  useEffect(() => {
    let cancelled = false;
    void load().catch((nextError) => {
      if (!cancelled) setError(errorMessage(nextError));
    });
    return () => { cancelled = true; };
  }, [load]);

  useEffect(() => {
    if (scope === 'project' && !projectPath) setScope(profile?.id ? 'profile' : 'global');
    if (scope === 'profile' && !profile?.id) setScope('global');
  }, [profile?.id, projectPath, scope]);

  const saveBinding = async () => {
    const normalizedExtension = extension.trim().replace(/^\.+/, '').toLowerCase();
    if (!normalizedExtension || /[.\\/]/.test(normalizedExtension)) {
      setError('请输入不带点的单个文件后缀，例如 txt、blend 或 png。');
      return;
    }
    if (scope === 'project' && !projectPath) {
      setError('项目级关联需要先打开项目。');
      return;
    }
    if (scope === 'profile' && !profile?.id) {
      setError('装配方案级关联需要当前装配方案已经加载。');
      return;
    }
    setPending('save');
    setError(null);
    setNotice(null);
    try {
      const binding: FileAssociationBinding = {
        id: createBindingId(),
        scope,
        extension: normalizedExtension,
        intent,
        handler,
        behavior: strict ? 'strict' : 'fallback',
        projectPath: scope === 'project' ? projectPath : undefined,
        profileId: scope === 'profile' ? profile?.id : undefined,
      };
      await setFileAssociationBinding(binding);
      await load();
      setNotice(`已保存 ${SCOPE_LABEL[scope]}关联：.${normalizedExtension}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setPending(null);
    }
  };

  const removeBinding = async (binding: FileAssociationBinding) => {
    setPending(`remove:${binding.id}`);
    setError(null);
    setNotice(null);
    try {
      await removeFileAssociationBinding(binding.id, context);
      await load();
      setNotice(`已移除 .${binding.extension || '*'} 的${SCOPE_LABEL[binding.scope] || binding.scope}关联。`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setPending(null);
    }
  };

  const inspectFile = async () => {
    const selected = await open({ title: '选择要检查打开方式的文件或目录', directory: false, multiple: false });
    if (!selected || Array.isArray(selected)) return;
    setPending('inspect');
    setError(null);
    try {
      setRoutePlan(await routeFileIntent(selected, 'open', context));
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setPending(null);
    }
  };

  return (
    <section className="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-indigo-100 text-indigo-700 dark:bg-indigo-950/50 dark:text-indigo-300">
            <FileCog className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">文件打开方式</h4>
            <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">组件提供内部处理器；项目、装配方案和全局设置按顺序覆盖。普通双击没有可用处理器时仍交给 Windows。</p>
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => void inspectFile()}
            disabled={pending !== null}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            {pending === 'inspect' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Route className="h-3.5 w-3.5" />}
            检查文件
          </button>
          <button type="button" onClick={() => void load()} disabled={pending !== null} title="刷新关联" className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800">
            <RefreshCw className="h-4 w-4" />
          </button>
        </div>
      </div>

      {error ? <div className="mt-3 flex items-start gap-2 rounded-md bg-red-50 px-3 py-2 text-xs text-red-700 dark:bg-red-950/30 dark:text-red-300"><AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" /><span className="whitespace-pre-wrap break-all">{error}</span></div> : null}
      {notice ? <div className="mt-3 flex items-start gap-2 rounded-md bg-emerald-50 px-3 py-2 text-xs text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300"><CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" /><span>{notice}</span></div> : null}

      <div className="mt-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-[minmax(0,1fr)_150px_160px]">
        <label className="block">
          <span className="mb-1 block text-[11px] font-medium text-gray-500 dark:text-gray-400">文件后缀</span>
          <div className="flex items-center rounded-md border border-gray-200 bg-white px-2 dark:border-gray-700 dark:bg-gray-950">
            <span className="text-sm text-gray-400">.</span>
            <input value={extension} onChange={(event) => setExtension(event.target.value)} className="min-w-0 flex-1 bg-transparent px-1 py-2 text-sm text-gray-900 outline-none dark:text-gray-100" placeholder="txt" />
          </div>
        </label>
        <label className="block">
          <span className="mb-1 block text-[11px] font-medium text-gray-500 dark:text-gray-400">保存范围</span>
          <select value={scope} onChange={(event) => setScope(event.target.value as typeof scope)} className="h-9 w-full rounded-md border border-gray-200 bg-white px-2 text-sm text-gray-800 outline-none dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100">
            <option value="global">全局</option>
            <option value="profile" disabled={!profile?.id}>当前装配方案</option>
            <option value="project" disabled={!projectPath}>当前项目</option>
          </select>
        </label>
        <label className="block">
          <span className="mb-1 block text-[11px] font-medium text-gray-500 dark:text-gray-400">意图</span>
          <select value={intent} onChange={(event) => setIntent(event.target.value as FileIntent)} className="h-9 w-full rounded-md border border-gray-200 bg-white px-2 text-sm text-gray-800 outline-none dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100">
            {INTENTS.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}
          </select>
        </label>
      </div>
      <div className="mt-2 flex flex-col gap-2 sm:flex-row sm:items-end">
        <label className="min-w-0 flex-1">
          <span className="mb-1 block text-[11px] font-medium text-gray-500 dark:text-gray-400">默认处理器</span>
          <select value={handler} onChange={(event) => setHandler(event.target.value)} className="h-9 w-full rounded-md border border-gray-200 bg-white px-2 text-sm text-gray-800 outline-none dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100">
            <option value="system">Windows 系统默认程序</option>
            {handlers.map((item) => <option key={item.id} value={item.id}>{item.name} · {item.componentName}</option>)}
          </select>
        </label>
        <label className="flex h-9 shrink-0 items-center gap-2 px-1 text-xs text-gray-600 dark:text-gray-300" title="严格模式下目标组件缺失会阻止本次意图，不会回退系统程序。">
          <input type="checkbox" checked={strict} onChange={(event) => setStrict(event.target.checked)} className="h-4 w-4 rounded border-gray-300 text-blue-600" />
          严格绑定
        </label>
        <button type="button" onClick={() => void saveBinding()} disabled={pending !== null} className="inline-flex h-9 shrink-0 items-center justify-center gap-1.5 rounded-md bg-blue-600 px-3 text-xs font-medium text-white hover:bg-blue-700 disabled:opacity-50">
          {pending === 'save' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ChevronDown className="h-3.5 w-3.5" />}
          保存关联
        </button>
      </div>

      <div className="mt-4 border-t border-gray-100 pt-3 dark:border-gray-800">
        <p className="text-xs font-medium text-gray-700 dark:text-gray-300">当前生效范围</p>
        <div className="mt-2 divide-y divide-gray-100 rounded-md border border-gray-200 dark:divide-gray-800 dark:border-gray-700">
          {snapshot?.bindings.map((binding) => {
            const matchingHandler = handlers.find((item) => item.id === binding.handler);
            return (
              <div key={`${binding.scope}:${binding.id}`} className="flex items-center gap-3 px-3 py-2 text-xs">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-gray-800 dark:text-gray-200">.{binding.extension || '*'} · {INTENTS.find((item) => item.value === binding.intent)?.label || binding.intent}</p>
                  <p className="mt-0.5 truncate text-[11px] text-gray-400">{SCOPE_LABEL[binding.scope] || binding.scope} · {binding.handler === 'system' ? 'Windows 系统默认程序' : matchingHandler ? matchingHandler.name : `缺失组件：${binding.handler}`} · {binding.behavior || 'fallback'}</p>
                </div>
                <button type="button" onClick={() => void removeBinding(binding)} disabled={pending !== null} title="移除此关联" className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-red-600 hover:bg-red-50 disabled:opacity-50 dark:text-red-300 dark:hover:bg-red-950/30">
                  {pending === `remove:${binding.id}` ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Trash2 className="h-3.5 w-3.5" />}
                </button>
              </div>
            );
          })}
          {!snapshot?.bindings.length ? <p className="px-3 py-4 text-xs text-gray-500 dark:text-gray-400">尚未设置默认处理器，将按已安装组件优先级选择，普通打开可回退系统程序。</p> : null}
        </div>
        {snapshot?.storagePaths.map((item) => <p key={item.scope} className="mt-1 break-all text-[10px] text-gray-400">{SCOPE_LABEL[item.scope] || item.scope}：{item.path}</p>)}
      </div>

      {routePlan ? (
        <div className="mt-4 border-t border-gray-100 pt-3 dark:border-gray-800">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-xs font-medium text-gray-700 dark:text-gray-300">路由追踪</p>
            <span className={routePlan.accepted ? 'text-xs text-emerald-600 dark:text-emerald-300' : 'text-xs text-red-600 dark:text-red-300'}>{routePlan.handlerName || '未找到内部处理器'}</span>
          </div>
          <p className="mt-1 break-all text-[11px] text-gray-400">{routePlan.path}</p>
          <div className="mt-2 divide-y divide-gray-100 rounded-md border border-gray-200 dark:divide-gray-800 dark:border-gray-700">
            {routePlan.candidates.map((candidate) => (
              <div key={`${candidate.componentId}:${candidate.handlerId}`} className="px-3 py-2 text-xs">
                <div className="flex flex-wrap items-center gap-2"><span className={candidate.selected ? 'font-medium text-blue-700 dark:text-blue-300' : candidate.eligible ? 'text-gray-800 dark:text-gray-200' : 'text-gray-400'}>{candidate.handlerName}</span><span className="font-mono text-[10px] text-gray-400">{candidate.componentId}</span>{candidate.selected ? <span className="rounded bg-blue-50 px-1 py-0.5 text-[10px] text-blue-700 dark:bg-blue-950/40 dark:text-blue-300">已选择</span> : null}</div>
                {candidate.reasons.map((reason) => <p key={reason.code} className="mt-0.5 text-[11px] text-gray-400">{reason.code} · {reason.message}</p>)}
              </div>
            ))}
            {!routePlan.candidates.length ? <p className="px-3 py-3 text-xs text-gray-500">当前没有已安装组件声明文件处理器。</p> : null}
          </div>
          {routePlan.diagnostics.map((diagnostic) => <p key={diagnostic.code} className="mt-1 text-[11px] text-gray-400">{diagnostic.code} · {diagnostic.message}</p>)}
        </div>
      ) : null}
    </section>
  );
}
