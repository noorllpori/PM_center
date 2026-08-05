import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  Boxes,
  CheckCircle2,
  LayoutTemplate,
  Layers3,
  Loader2,
  Package,
  Redo2,
  RefreshCw,
  Save,
  Undo2,
} from 'lucide-react';
import { getPlatformModuleRuntime } from '../../api/platformModules';
import {
  getWorkspaceProfileDocument,
  validateWorkspaceProfileDraft,
} from '../../api/workspaceProfiles';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import type {
  ModuleManifestV1,
  ProfileModuleSelection,
  WorkspaceProfileV1,
} from '../../types/platform';
import type { PlatformModuleDiagnostic } from '../../types/platformRuntime';
import type {
  WorkspaceProfileDraftValidation,
  WorkspaceProfileRuntimeCommandError,
} from '../../types/workspaceProfileRuntime';
import { removeModuleOwnedLayout } from '../../features/profileLayout';
import { Dialog } from '../Dialog';
import { WorkspaceProfileLayoutEditor } from './WorkspaceProfileLayoutEditor';

interface WorkspaceProfileEditorDialogProps {
  isOpen: boolean;
  profileId: string | null;
  onClose: () => void;
}

function cloneProfile(profile: WorkspaceProfileV1): WorkspaceProfileV1 {
  return JSON.parse(JSON.stringify(profile)) as WorkspaceProfileV1;
}

function formatRuntimeError(error: unknown) {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object') {
    const typed = error as WorkspaceProfileRuntimeCommandError;
    const prefix = typed.code ? `${typed.code}: ` : '';
    const details = typed.details?.length ? `\n${typed.details.join('\n')}` : '';
    return `${prefix}${typed.message || String(error)}${details}`;
  }
  return String(error);
}

function moduleSelection(manifest: ModuleManifestV1, requirement?: string): ProfileModuleSelection {
  return {
    id: manifest.id,
    versionRequirement: requirement || `^${manifest.version}`,
  };
}

function selectedModuleIds(profile: WorkspaceProfileV1 | null) {
  return new Set((profile?.enabledModules ?? []).map((selection) => selection.id));
}

export function WorkspaceProfileEditorDialog({
  isOpen,
  profileId,
  onClose,
}: WorkspaceProfileEditorDialogProps) {
  const saveProfile = useWorkspaceProfileStore((state) => state.saveProfile);
  const isMutating = useWorkspaceProfileStore((state) => state.isMutating);
  const [draft, setDraft] = useState<WorkspaceProfileV1 | null>(null);
  const [modules, setModules] = useState<PlatformModuleDiagnostic[]>([]);
  const [validation, setValidation] = useState<WorkspaceProfileDraftValidation | null>(null);
  const [loading, setLoading] = useState(false);
  const [validating, setValidating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [activeEditorSection, setActiveEditorSection] = useState<'modules' | 'layout'>('modules');
  const originalDocumentRef = useRef('');
  const validationSequenceRef = useRef(0);
  const undoStackRef = useRef<WorkspaceProfileV1[]>([]);
  const redoStackRef = useRef<WorkspaceProfileV1[]>([]);

  const load = useCallback(async () => {
    if (!profileId) return;
    setLoading(true);
    setError(null);
    setSaveMessage(null);
    try {
      const [profile, overview] = await Promise.all([
        getWorkspaceProfileDocument(profileId),
        getPlatformModuleRuntime(),
      ]);
      const editable = cloneProfile(profile);
      setDraft(editable);
      setModules(
        overview.modules
          .filter((module) => !module.diagnostic)
          .sort((left, right) => left.manifest.name.localeCompare(right.manifest.name, 'zh-CN')),
      );
      originalDocumentRef.current = JSON.stringify(editable);
      undoStackRef.current = [];
      redoStackRef.current = [];
    } catch (loadError) {
      setDraft(null);
      setModules([]);
      setValidation(null);
      setError(formatRuntimeError(loadError));
    } finally {
      setLoading(false);
    }
  }, [profileId]);

  useEffect(() => {
    if (!isOpen) {
      setDraft(null);
      setValidation(null);
      setError(null);
      setSaveMessage(null);
      setActiveEditorSection('modules');
      undoStackRef.current = [];
      redoStackRef.current = [];
      return;
    }
    void load();
  }, [isOpen, load]);

  useEffect(() => {
    if (!isOpen || !draft || loading) return;
    const sequence = ++validationSequenceRef.current;
    setValidating(true);
    const timeoutId = window.setTimeout(() => {
      void validateWorkspaceProfileDraft(draft)
        .then((result) => {
          if (validationSequenceRef.current === sequence) {
            setValidation(result);
            setError(null);
          }
        })
        .catch((validationError) => {
          if (validationSequenceRef.current === sequence) {
            setValidation(null);
            setError(formatRuntimeError(validationError));
          }
        })
        .finally(() => {
          if (validationSequenceRef.current === sequence) {
            setValidating(false);
          }
        });
    }, 250);
    return () => window.clearTimeout(timeoutId);
  }, [draft, isOpen, loading]);

  const moduleById = useMemo(
    () => new Map(modules.map((module) => [module.manifest.id, module] as const)),
    [modules],
  );
  const selectedIds = useMemo(() => selectedModuleIds(draft), [draft]);
  const missingModuleSelections = useMemo(
    () => (draft?.enabledModules ?? []).filter((selection) => !moduleById.has(selection.id)),
    [draft, moduleById],
  );
  const knownComponentIds = useMemo(
    () => new Set((validation?.components ?? []).map((component) => component.id)),
    [validation],
  );
  const missingComponentSelections = useMemo(
    () => (draft?.enabledComponents ?? []).filter((selection) => !knownComponentIds.has(selection.id)),
    [draft, knownComponentIds],
  );
  const dirty = Boolean(draft) && JSON.stringify(draft) !== originalDocumentRef.current;

  const updateDraft = (updater: (current: WorkspaceProfileV1) => WorkspaceProfileV1) => {
    setDraft((current) => {
      if (!current) return current;
      const before = cloneProfile(current);
      const next = updater(cloneProfile(current));
      if (JSON.stringify(next) === JSON.stringify(current)) return current;
      undoStackRef.current = [...undoStackRef.current.slice(-49), before];
      redoStackRef.current = [];
      return next;
    });
    setSaveMessage(null);
  };

  const undoDraft = () => {
    setDraft((current) => {
      const previous = undoStackRef.current.pop();
      if (!current || !previous) return current;
      redoStackRef.current.push(cloneProfile(current));
      return cloneProfile(previous);
    });
    setSaveMessage(null);
  };

  const redoDraft = () => {
    setDraft((current) => {
      const next = redoStackRef.current.pop();
      if (!current || !next) return current;
      undoStackRef.current.push(cloneProfile(current));
      return cloneProfile(next);
    });
    setSaveMessage(null);
  };

  const addModuleWithDependencies = (moduleId: string) => {
    updateDraft((current) => {
      const selections = new Map(
        (current.enabledModules ?? []).map((selection) => [selection.id, selection] as const),
      );
      const visiting = new Set<string>();
      const add = (id: string, requirement?: string) => {
        if (visiting.has(id)) return;
        visiting.add(id);
        const diagnostic = moduleById.get(id);
        if (!selections.has(id)) {
          selections.set(
            id,
            diagnostic
              ? moduleSelection(diagnostic.manifest, requirement)
              : { id, versionRequirement: requirement || '*' },
          );
        }
        diagnostic?.manifest.requiresModules?.forEach((dependency) => {
          add(dependency.id, dependency.versionRequirement);
        });
      };
      add(moduleId);
      current.enabledModules = Array.from(selections.values()).sort((left, right) =>
        left.id.localeCompare(right.id),
      );
      return current;
    });
  };

  const removeModuleWithDependents = (moduleId: string) => {
    updateDraft((current) => {
      const selected = new Set((current.enabledModules ?? []).map((selection) => selection.id));
      const removed = new Set([moduleId]);
      let changed = true;
      while (changed) {
        changed = false;
        selected.forEach((candidateId) => {
          if (removed.has(candidateId)) return;
          const candidate = moduleById.get(candidateId)?.manifest;
          if (candidate?.requiresModules?.some((dependency) => removed.has(dependency.id))) {
            removed.add(candidateId);
            changed = true;
          }
        });
      }

      current.enabledModules = (current.enabledModules ?? []).filter(
        (selection) => !removed.has(selection.id),
      );
      const removedManifests = Array.from(removed).flatMap((id) => {
        const manifest = moduleById.get(id)?.manifest;
        return manifest ? [manifest] : [];
      });
      removeModuleOwnedLayout(current, removedManifests);
      return current;
    });
  };

  const toggleExplicitComponent = (componentId: string, enabled: boolean, version: string) => {
    updateDraft((current) => {
      const selections = new Map(
        (current.enabledComponents ?? []).map((selection) => [selection.id, selection] as const),
      );
      if (enabled) {
        selections.set(componentId, { id: componentId, versionRequirement: `^${version}` });
      } else {
        selections.delete(componentId);
      }
      current.enabledComponents = Array.from(selections.values()).sort((left, right) =>
        left.id.localeCompare(right.id),
      );
      return current;
    });
  };

  const handleClose = () => {
    if (dirty && !window.confirm('装配方案还有未保存修改，确定关闭吗？')) return;
    onClose();
  };

  const handleSave = async () => {
    if (!draft || !validation?.valid || isMutating || validating) return;
    setError(null);
    setSaveMessage(null);
    try {
      const result = await saveProfile({
        profile: draft,
        expectedRevision: draft.revision ?? 1,
      });
      const saved = cloneProfile(result.profile);
      setDraft(saved);
      setValidation(result.validation);
      originalDocumentRef.current = JSON.stringify(saved);
      undoStackRef.current = [];
      redoStackRef.current = [];
      setSaveMessage(`已保存修订 r${saved.revision ?? 1}`);
    } catch (saveError) {
      setError(formatRuntimeError(saveError));
    }
  };

  return (
    <Dialog
      isOpen={isOpen}
      onClose={handleClose}
      title="编辑装配方案"
      size="2xl"
      footer={
        <>
          <button
            type="button"
            onClick={handleClose}
            disabled={isMutating}
            className="rounded-md px-4 py-2 text-sm text-gray-600 hover:bg-gray-100 disabled:opacity-50 dark:text-gray-300 dark:hover:bg-gray-800"
          >
            关闭
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={!dirty || !validation?.valid || validating || loading || isMutating}
            className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-4 py-2 text-sm text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isMutating ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
            保存方案
          </button>
        </>
      }
    >
      {loading ? (
        <div className="flex min-h-80 items-center justify-center gap-2 text-sm text-gray-500">
          <Loader2 className="h-4 w-4 animate-spin" />
          正在读取装配方案与模块目录...
        </div>
      ) : !draft ? (
        <div className="flex min-h-64 flex-col items-center justify-center gap-3 text-center">
          <AlertTriangle className="h-8 w-8 text-amber-500" />
          <p className="max-w-lg whitespace-pre-wrap text-sm text-gray-600 dark:text-gray-300">
            {error || '无法读取装配方案。'}
          </p>
          <button
            type="button"
            onClick={() => void load()}
            className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-3 py-2 text-sm dark:border-gray-700"
          >
            <RefreshCw className="h-4 w-4" />
            重新读取
          </button>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1.6fr)]">
            <label className="block">
              <span className="mb-1.5 block text-xs font-medium text-gray-700 dark:text-gray-300">方案名称</span>
              <input
                value={draft.name}
                maxLength={80}
                onChange={(event) => updateDraft((current) => ({ ...current, name: event.target.value }))}
                className="h-9 w-full rounded-md border border-gray-300 bg-white px-3 text-sm outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 dark:border-gray-700 dark:bg-gray-800"
              />
            </label>
            <label className="block">
              <span className="mb-1.5 block text-xs font-medium text-gray-700 dark:text-gray-300">说明</span>
              <input
                value={draft.description ?? ''}
                maxLength={500}
                onChange={(event) => updateDraft((current) => ({ ...current, description: event.target.value }))}
                className="h-9 w-full rounded-md border border-gray-300 bg-white px-3 text-sm outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 dark:border-gray-700 dark:bg-gray-800"
              />
            </label>
          </div>

          {error ? (
            <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
              <span className="whitespace-pre-wrap break-all">{error}</span>
            </div>
          ) : null}
          {saveMessage ? (
            <div className="flex items-center gap-2 rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-xs text-emerald-700 dark:border-emerald-900/50 dark:bg-emerald-950/30 dark:text-emerald-300">
              <CheckCircle2 className="h-4 w-4" />
              {saveMessage}
            </div>
          ) : null}

          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-gray-200 pb-3 dark:border-gray-700">
            <div className="inline-flex overflow-hidden rounded-md border border-gray-200 dark:border-gray-700">
              <button
                type="button"
                onClick={() => setActiveEditorSection('modules')}
                className={`inline-flex h-9 items-center gap-1.5 border-r border-gray-200 px-3 text-sm dark:border-gray-700 ${
                  activeEditorSection === 'modules'
                    ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                }`}
              >
                <Layers3 className="h-4 w-4" />
                模块与组件
              </button>
              <button
                type="button"
                onClick={() => setActiveEditorSection('layout')}
                className={`inline-flex h-9 items-center gap-1.5 px-3 text-sm ${
                  activeEditorSection === 'layout'
                    ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                }`}
              >
                <LayoutTemplate className="h-4 w-4" />
                界面装配
              </button>
            </div>
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={undoDraft}
                disabled={undoStackRef.current.length === 0}
                className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-30 dark:hover:bg-gray-800"
                title="撤销"
              >
                <Undo2 className="h-4 w-4" />
              </button>
              <button
                type="button"
                onClick={redoDraft}
                disabled={redoStackRef.current.length === 0}
                className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-30 dark:hover:bg-gray-800"
                title="重做"
              >
                <Redo2 className="h-4 w-4" />
              </button>
            </div>
          </div>

          {activeEditorSection === 'modules' ? <>
          <div className="grid min-h-[390px] gap-4 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)]">
            <section className="min-w-0 rounded-md border border-gray-200 dark:border-gray-700">
              <div className="flex items-center justify-between border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
                <div className="flex items-center gap-2">
                  <Layers3 className="h-4 w-4 text-indigo-600" />
                  <h4 className="text-sm font-semibold">模块</h4>
                </div>
                <span className="text-xs text-gray-500">已选 {selectedIds.size}/{modules.length}</span>
              </div>
              <div className="max-h-[470px] divide-y divide-gray-100 overflow-auto dark:divide-gray-800">
                {missingModuleSelections.map((selection) => (
                  <label key={selection.id} className="flex cursor-pointer items-start gap-3 bg-red-50/60 px-3 py-3 dark:bg-red-950/15">
                    <input
                      type="checkbox"
                      checked
                      onChange={() => removeModuleWithDependents(selection.id)}
                      className="mt-0.5 h-4 w-4 rounded border-gray-300 text-red-600 focus:ring-red-500"
                    />
                    <div className="min-w-0 flex-1">
                      <p className="text-sm font-medium text-red-700 dark:text-red-300">未安装模块</p>
                      <p className="mt-1 break-all font-mono text-[11px] text-red-600 dark:text-red-400">{selection.id} · {selection.versionRequirement || '*'}</p>
                      <p className="mt-1 text-[11px] text-red-600 dark:text-red-400">取消勾选后可修复此阻塞方案。</p>
                    </div>
                  </label>
                ))}
                {modules.map((module) => {
                  const manifest = module.manifest;
                  const selected = selectedIds.has(manifest.id);
                  const requiredBy = modules.filter(
                    (candidate) => selectedIds.has(candidate.manifest.id)
                      && candidate.manifest.requiresModules?.some((dependency) => dependency.id === manifest.id),
                  );
                  return (
                    <label key={manifest.id} className="flex cursor-pointer items-start gap-3 px-3 py-3 hover:bg-gray-50 dark:hover:bg-gray-800/60">
                      <input
                        type="checkbox"
                        checked={selected}
                        onChange={(event) => {
                          if (event.target.checked) addModuleWithDependencies(manifest.id);
                          else removeModuleWithDependents(manifest.id);
                        }}
                        className="mt-0.5 h-4 w-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="text-sm font-medium text-gray-900 dark:text-gray-100">{manifest.name}</span>
                          <span className="font-mono text-[11px] text-gray-400">v{manifest.version}</span>
                        </div>
                        <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{manifest.description}</p>
                        <p className="mt-1 break-all font-mono text-[11px] text-gray-400">{manifest.id}</p>
                        {manifest.requiresModules?.length ? (
                          <p className="mt-1 text-[11px] text-blue-600 dark:text-blue-300">
                            必需模块：{manifest.requiresModules.map((dependency) => dependency.id).join('、')}
                          </p>
                        ) : null}
                        {manifest.requiresComponents?.length ? (
                          <p className="mt-1 text-[11px] text-violet-600 dark:text-violet-300">
                            自动组件：{manifest.requiresComponents.map((dependency) => dependency.id).join('、')}
                          </p>
                        ) : null}
                        {requiredBy.length ? (
                          <p className="mt-1 text-[11px] text-amber-600 dark:text-amber-300">
                            被 {requiredBy.map((candidate) => candidate.manifest.name).join('、')} 依赖
                          </p>
                        ) : null}
                      </div>
                    </label>
                  );
                })}
              </div>
            </section>

            <section className="min-w-0 rounded-md border border-gray-200 dark:border-gray-700">
              <div className="flex items-center justify-between border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
                <div className="flex items-center gap-2">
                  <Package className="h-4 w-4 text-violet-600" />
                  <h4 className="text-sm font-semibold">组件</h4>
                </div>
                <span className="text-xs text-gray-500">
                  有效 {validation?.effectiveComponentCount ?? 0}
                </span>
              </div>
              <div className="max-h-[470px] divide-y divide-gray-100 overflow-auto dark:divide-gray-800">
                {missingComponentSelections.map((selection) => (
                  <label key={selection.id} className="flex cursor-pointer items-start gap-3 bg-red-50/60 px-3 py-3 dark:bg-red-950/15">
                    <input
                      type="checkbox"
                      checked
                      onChange={() => toggleExplicitComponent(selection.id, false, '0.0.0')}
                      className="mt-0.5 h-4 w-4 rounded border-gray-300 text-red-600 focus:ring-red-500"
                    />
                    <div className="min-w-0 flex-1">
                      <p className="text-sm font-medium text-red-700 dark:text-red-300">未安装组件</p>
                      <p className="mt-1 break-all font-mono text-[11px] text-red-600 dark:text-red-400">{selection.id} · {selection.versionRequirement || '*'}</p>
                      <p className="mt-1 text-[11px] text-red-600 dark:text-red-400">取消显式选择后可继续校验。</p>
                    </div>
                  </label>
                ))}
                {(validation?.components ?? []).map((component) => {
                  const explicitlySelected = (draft.enabledComponents ?? []).some(
                    (selection) => selection.id === component.id,
                  );
                  return (
                    <label key={component.id} className="flex cursor-pointer items-start gap-3 px-3 py-3 hover:bg-gray-50 dark:hover:bg-gray-800/60">
                      <input
                        type="checkbox"
                        checked={explicitlySelected}
                        onChange={(event) => toggleExplicitComponent(component.id, event.target.checked, component.version)}
                        className="mt-0.5 h-4 w-4 rounded border-gray-300 text-violet-600 focus:ring-violet-500"
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="text-sm font-medium text-gray-900 dark:text-gray-100">{component.name}</span>
                          {component.effectiveEnabled ? (
                            <span className="rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300">生效</span>
                          ) : null}
                        </div>
                        <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{component.description}</p>
                        <p className="mt-1 break-all font-mono text-[11px] text-gray-400">{component.id} · {component.runtime}</p>
                        {component.requiredByModules.length ? (
                          <p className="mt-1 text-[11px] text-blue-600 dark:text-blue-300">
                            模块依赖：{component.requiredByModules.map((id) => moduleById.get(id)?.manifest.name || id).join('、')}
                          </p>
                        ) : null}
                        {component.requiredByComponents.length ? (
                          <p className="mt-1 text-[11px] text-violet-600 dark:text-violet-300">
                            组件依赖：{component.requiredByComponents.join('、')}
                          </p>
                        ) : null}
                      </div>
                    </label>
                  );
                })}
                {!validation?.components.length && !missingComponentSelections.length ? (
                  <div className="flex min-h-32 items-center justify-center gap-2 px-3 text-xs text-gray-500">
                    {validating ? <Loader2 className="h-4 w-4 animate-spin" /> : <Boxes className="h-4 w-4" />}
                    {validating ? '正在计算组件依赖...' : '没有登记可选组件'}
                  </div>
                ) : null}
              </div>
            </section>
          </div>

          <section className={`rounded-md border p-3 ${
            validation?.valid
              ? 'border-emerald-200 bg-emerald-50/60 dark:border-emerald-900/50 dark:bg-emerald-950/20'
              : 'border-amber-200 bg-amber-50/60 dark:border-amber-900/50 dark:bg-amber-950/20'
          }`}>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                {validating ? (
                  <Loader2 className="h-4 w-4 animate-spin text-gray-500" />
                ) : validation?.valid ? (
                  <CheckCircle2 className="h-4 w-4 text-emerald-600" />
                ) : (
                  <AlertTriangle className="h-4 w-4 text-amber-600" />
                )}
                <span className="text-sm font-medium text-gray-900 dark:text-gray-100">
                  {validating ? '正在校验依赖' : validation?.valid ? '依赖检查通过' : '保存前需要处理阻塞项'}
                </span>
              </div>
              <span className="text-xs text-gray-500">
                {validation?.selectedModuleCount ?? 0} 模块 · {validation?.explicitComponentCount ?? 0} 显式组件 · {validation?.effectiveComponentCount ?? 0} 有效组件
              </span>
            </div>
            {validation?.issues.length ? (
              <div className="mt-2 space-y-1">
                {validation.issues.map((issue) => (
                  <p key={`${issue.code}:${issue.moduleId ?? issue.message}`} className={`text-xs ${
                    issue.severity === 'error'
                      ? 'text-red-700 dark:text-red-300'
                      : issue.severity === 'warning'
                        ? 'text-amber-700 dark:text-amber-300'
                        : 'text-blue-700 dark:text-blue-300'
                  }`}>
                    {issue.message}
                  </p>
                ))}
              </div>
            ) : null}
          </section>
          </> : (
            <WorkspaceProfileLayoutEditor
              draft={draft}
              modules={modules.map((module) => module.manifest)}
              onChange={updateDraft}
            />
          )}
        </div>
      )}
    </Dialog>
  );
}
