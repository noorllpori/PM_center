import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Bot,
  Boxes,
  CheckCircle2,
  FileCog,
  LayoutTemplate,
  Layers3,
  Loader2,
  Package,
  PanelsTopLeft,
  Redo2,
  RefreshCw,
  Save,
  Settings2,
  Undo2,
} from 'lucide-react';
import { getPlatformModuleRuntime } from '../../api/platformModules';
import { getComponentRuntimeOverview, validateInterfaceLayout } from '../../api/componentRuntime';
import {
  getWorkspaceProfileDocument,
  validateWorkspaceProfileDraft,
} from '../../api/workspaceProfiles';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import type {
  ModuleManifestV1,
  ProfileModuleSelection,
  ComponentCategory,
  JsonValue,
  WorkspaceProfileV1,
} from '../../types/platform';
import type { ComponentRuntimeOverview, InterfaceTemplateLayoutValidation } from '../../types/componentRuntime';
import type { PlatformModuleDiagnostic } from '../../types/platformRuntime';
import type {
  WorkspaceProfileDraftValidation,
  WorkspaceProfileRuntimeCommandError,
} from '../../types/workspaceProfileRuntime';
import { removeModuleOwnedLayout } from '../../features/profileLayout';
import { ConfirmDialog, Dialog } from '../Dialog';
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

const CATEGORY_LABELS: Record<ComponentCategory, string> = {
  workspace: '工作区',
  'file-handler': '文件处理',
  service: '服务',
  automation: '自动化',
  appearance: '外观',
  integration: '集成',
  data: '数据',
};

function inferredLegacyCategory(manifest: ModuleManifestV1): ComponentCategory {
  const value = `${manifest.id} ${manifest.name}`.toLowerCase();
  if (value.includes('file') || value.includes('文件') || value.includes('blender')) return 'file-handler';
  if (value.includes('automation') || value.includes('task') || value.includes('任务')) return 'automation';
  if (value.includes('theme') || value.includes('layout') || value.includes('外观')) return 'appearance';
  if (value.includes('lan') || value.includes('render') || value.includes('cache')) return 'service';
  return 'workspace';
}

function ProfileJsonEditor({
  value,
  onCommit,
}: {
  value: Record<string, JsonValue>;
  onCommit: (value: Record<string, JsonValue>) => void;
}) {
  const serialized = JSON.stringify(value, null, 2);
  const [text, setText] = useState(serialized);
  const [parseError, setParseError] = useState<string | null>(null);

  useEffect(() => setText(serialized), [serialized]);

  const commit = () => {
    try {
      const parsed = JSON.parse(text) as unknown;
      if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
        throw new Error('根值必须是 JSON 对象');
      }
      onCommit(parsed as Record<string, JsonValue>);
      setParseError(null);
    } catch (error) {
      setParseError(String(error));
    }
  };

  return (
    <div className="space-y-2">
      <textarea
        value={text}
        onChange={(event) => setText(event.target.value)}
        spellCheck={false}
        className="min-h-72 w-full resize-y rounded-md border border-gray-300 bg-white p-3 font-mono text-xs leading-5 outline-none focus:border-indigo-500 dark:border-gray-700 dark:bg-gray-900"
      />
      <div className="flex items-center justify-between gap-3">
        <p className={`text-xs ${parseError ? 'text-red-600 dark:text-red-300' : 'text-gray-500'}`}>
          {parseError || '按组件 ID 保存版本化配置；停用组件不会删除这些值。'}
        </p>
        <button type="button" onClick={commit} className="shrink-0 rounded-md bg-indigo-600 px-3 py-1.5 text-xs text-white hover:bg-indigo-700">
          应用到草稿
        </button>
      </div>
    </div>
  );
}

export function WorkspaceProfileEditorDialog({
  isOpen,
  profileId,
  onClose,
}: WorkspaceProfileEditorDialogProps) {
  const saveProfile = useWorkspaceProfileStore((state) => state.saveProfile);
  const saveCurrentProfile = useWorkspaceProfileStore((state) => state.saveCurrentProfile);
  const currentProfileId = useWorkspaceProfileStore(
    (state) => state.snapshot?.currentProfile.id ?? null,
  );
  const isMutating = useWorkspaceProfileStore((state) => state.isMutating);
  const [draft, setDraft] = useState<WorkspaceProfileV1 | null>(null);
  const [modules, setModules] = useState<PlatformModuleDiagnostic[]>([]);
  const [componentRuntime, setComponentRuntime] = useState<ComponentRuntimeOverview | null>(null);
  const [validation, setValidation] = useState<WorkspaceProfileDraftValidation | null>(null);
  const [interfaceValidation, setInterfaceValidation] = useState<InterfaceTemplateLayoutValidation | null>(null);
  const [loading, setLoading] = useState(false);
  const [validating, setValidating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [activeEditorSection, setActiveEditorSection] = useState<
  'components' | 'layout' | 'file-handlers' | 'automation' | 'settings'
    | 'ui-extensions'
  >('components');
  const [closeConfirmationOpen, setCloseConfirmationOpen] = useState(false);
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
      const [profile, overview, runtimeOverview] = await Promise.all([
        getWorkspaceProfileDocument(profileId),
        getPlatformModuleRuntime(),
        getComponentRuntimeOverview(),
      ]);
      const editable = cloneProfile(profile);
      setDraft(editable);
      setModules(
        overview.modules
          .filter((module) => !module.diagnostic)
          .sort((left, right) => left.manifest.name.localeCompare(right.manifest.name, 'zh-CN')),
      );
      setComponentRuntime(runtimeOverview);
      originalDocumentRef.current = JSON.stringify(editable);
      undoStackRef.current = [];
      redoStackRef.current = [];
    } catch (loadError) {
      setDraft(null);
      setModules([]);
      setComponentRuntime(null);
      setValidation(null);
      setInterfaceValidation(null);
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
      setActiveEditorSection('components');
      setCloseConfirmationOpen(false);
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
      void Promise.all([validateWorkspaceProfileDraft(draft), validateInterfaceLayout(draft)])
        .then(([result, templateResult]) => {
          if (validationSequenceRef.current === sequence) {
            setValidation(result);
            setInterfaceValidation(templateResult);
            setError(null);
          }
        })
        .catch((validationError) => {
          if (validationSequenceRef.current === sequence) {
            setValidation(null);
            setInterfaceValidation(null);
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
    () => validation
      ? (draft?.enabledComponents ?? []).filter((selection) => !knownComponentIds.has(selection.id))
      : [],
    [draft, knownComponentIds, validation],
  );
  const installedComponentManifests = useMemo(
    () => new Map(
      (componentRuntime?.installedComponents ?? []).map((component) => [component.manifest.id, component.manifest] as const),
    ),
    [componentRuntime],
  );
  const effectiveFileHandlers = useMemo(
    () => (validation?.components ?? [])
      .filter((component) => component.effectiveEnabled)
      .flatMap((component) => (installedComponentManifests.get(component.id)?.contributes?.fileHandlers ?? []).map((handler) => ({
        ...handler,
        componentId: component.id,
        componentName: component.name,
      }))),
    [installedComponentManifests, validation?.components],
  );
  const effectiveUiExtensions = useMemo(() => (
    (componentRuntime?.installedComponents ?? [])
      .filter((component) => (validation?.components ?? []).some(
        (summary) => summary.id === component.manifest.id && summary.effectiveEnabled,
      ))
      .flatMap((component) => (component.manifest.contributes?.uiExtensions ?? []).map((extension) => ({
        componentId: component.manifest.id,
        componentName: component.manifest.name,
        extension,
        surface: component.manifest.contributes?.scriptSurfaces?.find(
          (surface) => surface.id === extension.surfaceId,
        ),
        target: (componentRuntime?.installedComponents ?? [])
          .find((candidate) => candidate.manifest.id === extension.targetComponentId)?.manifest
          .contributes?.uiExtensionPoints?.find((point) => point.id === extension.targetPointId),
      })))
      .sort((left, right) => left.componentName.localeCompare(right.componentName, 'zh-CN')
        || (left.extension.order ?? 0) - (right.extension.order ?? 0)
        || left.extension.id.localeCompare(right.extension.id))
  ), [componentRuntime, validation?.components]);
  const dirty = Boolean(draft) && JSON.stringify(draft) !== originalDocumentRef.current;
  const editingCurrentProfile = Boolean(profileId) && profileId === currentProfileId;
  const layoutValid = interfaceValidation?.valid !== false;

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
    if (isMutating || closeConfirmationOpen) return;
    if (dirty) {
      setCloseConfirmationOpen(true);
      return;
    }
    onClose();
  };

  const confirmDiscardAndClose = () => {
    setCloseConfirmationOpen(false);
    onClose();
  };

  const handleSave = async () => {
    if (!draft || !validation?.valid || !layoutValid || isMutating || validating) return;
    setError(null);
    setSaveMessage(null);
    try {
      const save = editingCurrentProfile ? saveCurrentProfile : saveProfile;
      const result = await save({
        profile: draft,
        expectedRevision: draft.revision ?? 1,
      });
      const saved = cloneProfile(result.profile);
      setDraft(saved);
      setValidation(result.validation);
      originalDocumentRef.current = JSON.stringify(saved);
      undoStackRef.current = [];
      redoStackRef.current = [];
      setSaveMessage(
        editingCurrentProfile
          ? `已保存并应用修订 r${saved.revision ?? 1}`
          : `已保存修订 r${saved.revision ?? 1}`,
      );
    } catch (saveError) {
      setError(formatRuntimeError(saveError));
    }
  };

  return (
    <>
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
            disabled={!dirty || !validation?.valid || !layoutValid || validating || loading || isMutating}
            className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-4 py-2 text-sm text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isMutating ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
            {editingCurrentProfile ? '保存并应用' : '保存方案'}
          </button>
          </>
        }
      >
      {loading ? (
        <div className="flex min-h-80 items-center justify-center gap-2 text-sm text-gray-500">
          <Loader2 className="h-4 w-4 animate-spin" />
          正在读取装配方案与组件目录...
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
          {editingCurrentProfile ? (
            <div className="flex items-start gap-2 rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-xs text-blue-700 dark:border-blue-900/50 dark:bg-blue-950/30 dark:text-blue-300">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
              <span>正在编辑当前装配方案。点击“保存并应用”后，组件、界面、自动化和组件设置会作为同一份草稿原子更新。</span>
            </div>
          ) : null}
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
                onClick={() => setActiveEditorSection('components')}
                className={`inline-flex h-9 items-center gap-1.5 border-r border-gray-200 px-3 text-sm dark:border-gray-700 ${
                  activeEditorSection === 'components'
                    ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                }`}
              >
                <Layers3 className="h-4 w-4" />
                组件
              </button>
              <button
                type="button"
                onClick={() => setActiveEditorSection('layout')}
                className={`inline-flex h-9 items-center gap-1.5 border-r border-gray-200 px-3 text-sm dark:border-gray-700 ${
                  activeEditorSection === 'layout'
                    ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                }`}
              >
                <LayoutTemplate className="h-4 w-4" />
                界面装配
              </button>
              <button
                type="button"
                onClick={() => setActiveEditorSection('ui-extensions')}
                className={`inline-flex h-9 items-center gap-1.5 border-r border-gray-200 px-3 text-sm dark:border-gray-700 ${
                  activeEditorSection === 'ui-extensions'
                    ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                }`}
              >
                <PanelsTopLeft className="h-4 w-4" />
                界面扩展
              </button>
              <button
                type="button"
                onClick={() => setActiveEditorSection('file-handlers')}
                className={`inline-flex h-9 items-center gap-1.5 border-r border-gray-200 px-3 text-sm dark:border-gray-700 ${
                  activeEditorSection === 'file-handlers'
                    ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                }`}
              >
                <FileCog className="h-4 w-4" />
                文件处理
              </button>
              <button
                type="button"
                onClick={() => setActiveEditorSection('automation')}
                className={`inline-flex h-9 items-center gap-1.5 border-r border-gray-200 px-3 text-sm dark:border-gray-700 ${
                  activeEditorSection === 'automation'
                    ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                }`}
              >
                <Bot className="h-4 w-4" />
                自动化
              </button>
              <button
                type="button"
                onClick={() => setActiveEditorSection('settings')}
                className={`inline-flex h-9 items-center gap-1.5 px-3 text-sm ${
                  activeEditorSection === 'settings'
                    ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-gray-600 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                }`}
              >
                <Settings2 className="h-4 w-4" />
                组件设置
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

          {activeEditorSection === 'components' ? <>
          <div className="grid min-h-[390px] gap-4 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)]">
            <section className="min-w-0 rounded-md border border-gray-200 dark:border-gray-700">
              <div className="flex items-center justify-between border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
                <div className="flex items-center gap-2">
                  <Layers3 className="h-4 w-4 text-indigo-600" />
                  <h4 className="text-sm font-semibold">内置组件</h4>
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
                      <p className="text-sm font-medium text-red-700 dark:text-red-300">缺失的内置组件</p>
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
                          <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-600 dark:bg-gray-800 dark:text-gray-300">
                            {CATEGORY_LABELS[inferredLegacyCategory(manifest)]}
                          </span>
                          <span className="font-mono text-[11px] text-gray-400">v{manifest.version}</span>
                        </div>
                        <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{manifest.description}</p>
                        <p className="mt-1 break-all font-mono text-[11px] text-gray-400">{manifest.id}</p>
                        {manifest.requiresModules?.length ? (
                          <p className="mt-1 text-[11px] text-blue-600 dark:text-blue-300">
                            必需组件：{manifest.requiresModules.map((dependency) => dependency.id).join('、')}
                          </p>
                        ) : null}
                        {manifest.requiresComponents?.length ? (
                          <p className="mt-1 text-[11px] text-violet-600 dark:text-violet-300">
                            附加组件：{manifest.requiresComponents.map((dependency) => dependency.id).join('、')}
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
                  <h4 className="text-sm font-semibold">可安装组件</h4>
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
                          <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-600 dark:bg-gray-800 dark:text-gray-300">
                            {CATEGORY_LABELS[component.category ?? 'integration']}
                          </span>
                          {component.effectiveEnabled ? (
                            <span className="rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300">生效</span>
                          ) : null}
                        </div>
                        <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{component.description}</p>
                        <p className="mt-1 break-all font-mono text-[11px] text-gray-400">{component.id} · {component.runtime}</p>
                        {component.requiredByModules.length ? (
                          <p className="mt-1 text-[11px] text-blue-600 dark:text-blue-300">
                            内置组件依赖：{component.requiredByModules.map((id) => moduleById.get(id)?.manifest.name || id).join('、')}
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
                {(validation?.selectedModuleCount ?? 0) + (validation?.explicitComponentCount ?? 0)} 个已选组件 · {validation?.effectiveComponentCount ?? 0} 个有效依赖组件
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
            {interfaceValidation?.diagnostics.length ? (
              <div className="mt-2 space-y-1 border-t border-current/10 pt-2">
                {interfaceValidation.diagnostics.map((diagnostic) => (
                  <p key={`${diagnostic.code}:${diagnostic.path}`} className={`text-xs ${
                    diagnostic.severity === 'error'
                      ? 'text-red-700 dark:text-red-300'
                      : diagnostic.severity === 'warning'
                        ? 'text-amber-700 dark:text-amber-300'
                        : 'text-blue-700 dark:text-blue-300'
                  }`}>
                    界面模板：{diagnostic.message}
                  </p>
                ))}
              </div>
            ) : null}
          </section>
          </> : activeEditorSection === 'layout' ? (
            <WorkspaceProfileLayoutEditor
              draft={draft}
              modules={modules.map((module) => module.manifest)}
              onChange={updateDraft}
            />
          ) : activeEditorSection === 'ui-extensions' ? (
            <section className="rounded-md border border-gray-200 dark:border-gray-700">
              <div className="border-b border-gray-200 px-4 py-3 dark:border-gray-700">
                <h4 className="text-sm font-semibold">组件界面扩展</h4>
                <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-gray-400">
                  扩展由组件声明目标和隔离页面，方案只决定是否启用及顺序。目标组件或扩展停用时，保存前会显示阻塞原因。
                </p>
              </div>
              <div className="max-h-[470px] divide-y divide-gray-100 overflow-auto dark:divide-gray-800">
                {effectiveUiExtensions.map((item) => {
                  const binding = (draft.uiExtensionBindings ?? []).find(
                    (candidate) => candidate.extensionId === item.extension.id,
                  );
                  const enabled = binding?.enabled === true;
                  return (
                    <div key={item.extension.id} className="flex items-start gap-3 px-4 py-3">
                      <input
                        type="checkbox"
                        checked={enabled}
                        onChange={(event) => updateDraft((current) => {
                          const bindings = (current.uiExtensionBindings ?? []).filter(
                            (candidate) => candidate.extensionId !== item.extension.id,
                          );
                          if (event.target.checked) {
                            bindings.push({
                              id: `ui-${item.extension.id.replace(/\./g, '-').replace(/[^a-z0-9-]/g, '')}`,
                              extensionId: item.extension.id,
                              enabled: true,
                              order: item.extension.order ?? 0,
                            });
                          }
                          return { ...current, uiExtensionBindings: bindings };
                        })}
                        className="mt-0.5 h-4 w-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="text-sm font-medium text-gray-900 dark:text-gray-100">{item.extension.id}</span>
                          <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-600 dark:bg-gray-800 dark:text-gray-300">
                            {item.extension.mode === 'replace' ? '整页替换' : '插入'}
                          </span>
                        </div>
                        <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                          {item.componentName} · {item.target?.name || item.extension.targetPointId} · {item.surface?.name || item.extension.surfaceId}
                        </p>
                        <p className="mt-1 break-all font-mono text-[11px] text-gray-400">
                          {item.componentId} → {item.extension.targetComponentId}
                        </p>
                      </div>
                      {enabled ? <div className="flex shrink-0 items-center gap-1">
                        <button
                          type="button"
                          title="向前排序"
                          onClick={() => updateDraft((current) => ({
                            ...current,
                            uiExtensionBindings: (current.uiExtensionBindings ?? []).map((candidate) => (
                              candidate.extensionId === item.extension.id
                                ? { ...candidate, order: (candidate.order ?? 0) - 10 }
                                : candidate
                            )),
                          }))}
                          className="h-7 w-7 rounded-md text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"
                        ><ArrowUp className="mx-auto h-3.5 w-3.5" /></button>
                        <button
                          type="button"
                          title="向后排序"
                          onClick={() => updateDraft((current) => ({
                            ...current,
                            uiExtensionBindings: (current.uiExtensionBindings ?? []).map((candidate) => (
                              candidate.extensionId === item.extension.id
                                ? { ...candidate, order: (candidate.order ?? 0) + 10 }
                                : candidate
                            )),
                          }))}
                          className="h-7 w-7 rounded-md text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"
                        ><ArrowDown className="mx-auto h-3.5 w-3.5" /></button>
                      </div> : null}
                    </div>
                  );
                })}
                {!effectiveUiExtensions.length ? (
                  <div className="flex min-h-40 items-center justify-center gap-2 px-4 text-sm text-gray-500">
                    <PanelsTopLeft className="h-4 w-4" />当前方案没有组件声明可用界面扩展
                  </div>
                ) : null}
              </div>
            </section>
          ) : activeEditorSection === 'file-handlers' ? (
            <section className="rounded-md border border-gray-200 dark:border-gray-700">
              <div className="border-b border-gray-200 px-4 py-3 dark:border-gray-700">
                <h4 className="text-sm font-semibold">当前方案的文件处理能力</h4>
                <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-gray-400">
                  这里显示由已选组件注入的处理器。停用对应组件后，双击和右键入口会一起撤下并回退到系统打开。
                </p>
              </div>
              <div className="max-h-[470px] divide-y divide-gray-100 overflow-auto dark:divide-gray-800">
                {effectiveFileHandlers.map((handler) => (
                  <div key={`${handler.componentId}:${handler.id}`} className="px-4 py-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium text-gray-900 dark:text-gray-100">{handler.name}</span>
                      <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-600 dark:bg-gray-800 dark:text-gray-300">{handler.componentName}</span>
                    </div>
                    <p className="mt-1 font-mono text-[11px] text-gray-400">{handler.id}</p>
                    <p className="mt-1 text-xs text-gray-500">
                      后缀 {(handler.extensions ?? []).map((extension) => `.${extension}`).join('、') || '不限'} · 意图 {handler.intents.join('、') || 'open'}
                    </p>
                  </div>
                ))}
                {!effectiveFileHandlers.length ? (
                  <div className="flex min-h-40 items-center justify-center gap-2 px-4 text-sm text-gray-500">
                    <FileCog className="h-4 w-4" />当前方案没有启用文件处理组件
                  </div>
                ) : null}
              </div>
            </section>
          ) : activeEditorSection === 'automation' ? (
            <section className="rounded-md border border-gray-200 dark:border-gray-700">
              <div className="border-b border-gray-200 px-4 py-3 dark:border-gray-700">
                <h4 className="text-sm font-semibold">自动化绑定</h4>
                <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-gray-400">
                  绑定属于当前方案草稿；切换、停用或删除会在“保存并应用”后统一生效。
                </p>
              </div>
              <div className="max-h-[470px] divide-y divide-gray-100 overflow-auto dark:divide-gray-800">
                {(draft.automationBindings ?? []).map((binding) => (
                  <div key={binding.id} className="flex items-start gap-3 px-4 py-3">
                    <input
                      type="checkbox"
                      checked={binding.enabled !== false}
                      onChange={(event) => updateDraft((current) => ({
                        ...current,
                        automationBindings: (current.automationBindings ?? []).map((candidate) => (
                          candidate.id === binding.id ? { ...candidate, enabled: event.target.checked } : candidate
                        )),
                      }))}
                      className="mt-0.5 h-4 w-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
                    />
                    <div className="min-w-0 flex-1">
                      <p className="text-sm font-medium text-gray-900 dark:text-gray-100">{binding.componentId} / {binding.command}</p>
                      <p className="mt-1 font-mono text-[11px] text-gray-400">{binding.id} · {binding.trigger.kind}</p>
                    </div>
                    <button
                      type="button"
                      onClick={() => updateDraft((current) => ({
                        ...current,
                        automationBindings: (current.automationBindings ?? []).filter((candidate) => candidate.id !== binding.id),
                      }))}
                      className="rounded-md px-2 py-1 text-xs text-red-600 hover:bg-red-50 dark:text-red-300 dark:hover:bg-red-950/30"
                    >
                      移除
                    </button>
                  </div>
                ))}
                {!draft.automationBindings?.length ? (
                  <div className="flex min-h-40 items-center justify-center gap-2 px-4 text-sm text-gray-500">
                    <Bot className="h-4 w-4" />当前方案没有自动化绑定，可在脚本开发者工作台创建
                  </div>
                ) : null}
              </div>
            </section>
          ) : (
            <section className="rounded-md border border-gray-200 p-4 dark:border-gray-700">
              <div className="mb-3">
                <h4 className="text-sm font-semibold">方案级组件设置</h4>
                <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-gray-400">
                  此处编辑方案文档中的 `componentSettings`。设备路径、凭据、信任和权限不属于方案，仍保留在设备级设置。
                </p>
              </div>
              <ProfileJsonEditor
                value={draft.componentSettings ?? {}}
                onCommit={(value) => updateDraft((current) => ({ ...current, componentSettings: value }))}
              />
            </section>
          )}
        </div>
      )}
      </Dialog>
      <ConfirmDialog
        isOpen={closeConfirmationOpen}
        onClose={() => setCloseConfirmationOpen(false)}
        onConfirm={confirmDiscardAndClose}
        title="关闭装配方案编辑器"
        message="当前装配方案还有未保存修改。关闭后这些修改将丢失，是否继续？"
        confirmText="放弃修改并关闭"
        cancelText="继续编辑"
        type="warning"
      />
    </>
  );
}
