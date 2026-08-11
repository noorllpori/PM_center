import { useEffect, useMemo, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import {
  ArrowDown,
  ArrowUp,
  FileCode2,
  GripVertical,
  LayoutDashboard,
  Menu,
  PanelLeft,
  PanelTop,
  Pin,
  Plus,
  RefreshCw,
  X,
} from 'lucide-react';
import { BUILTIN_TOOLS } from '../../features/builtinTools';
import {
  SHELL_TAB_CONTRIBUTIONS,
  SURFACE_CONTRIBUTION_BY_ID,
  SURFACE_CONTRIBUTIONS,
  WIDGET_CONTRIBUTION_BY_ID,
  type SurfaceContributionDefinition,
  type WidgetContributionDefinition,
} from '../../features/contributionRegistry';
import {
  getSelectedModuleContributionIds,
  getWidgetDefinition,
  reorderPinnedTools,
  reorderProfileNavigation,
  reorderProfileWidgets,
  setPinnedToolContribution,
  setProfileHomeContribution,
  setProfileHomeScriptSurface,
  setProfileNavigationContribution,
  setProfileWidgetContribution,
  updateProfileWidgetRegion,
} from '../../features/profileLayout';
import {
  duplicateInterfaceTemplateForDevelopment,
  getComponentRuntimeOverview,
  getPresentationTemplatePreview,
  installComponentFromDirectory,
  reloadDevelopmentInterfaceTemplate,
} from '../../api/componentRuntime';
import {
  getDevelopmentComponentSnapshot,
  openScriptDevelopmentDirectoryInVSCode,
  trustScriptDevelopmentDirectory,
} from '../../api/scriptAutomation';
import type { DevelopmentComponentSnapshot } from '../../types/automation';
import type {
  ModuleManifestV1,
  ProfileComponentSelection,
  ProfileTemplateSlotBinding,
  ShellNavigationKind,
  TemplateSlotDefinition,
  WorkspaceProfileV1,
} from '../../types/platform';
import type { ComponentRuntimeOverview, PresentationTemplatePreview } from '../../types/componentRuntime';

interface WorkspaceProfileLayoutEditorProps {
  draft: WorkspaceProfileV1;
  modules: ModuleManifestV1[];
  onChange: (updater: (profile: WorkspaceProfileV1) => WorkspaceProfileV1) => void;
}

type DragKind = 'navigation' | 'tool' | 'widget';

interface DragState {
  kind: DragKind;
  id: string;
}

const SHELL_TEMPLATE_OPTIONS: Array<{
  value: ShellNavigationKind;
  templateId: string;
  label: string;
  description: string;
  icon: typeof PanelTop;
}> = [
  {
    value: 'top-bar',
    templateId: 'nexora.shell.top-bar',
    label: '顶部',
    description: '导航和工具位于窗口顶部',
    icon: PanelTop,
  },
  {
    value: 'side-bar',
    templateId: 'nexora.shell.side-bar',
    label: '侧边',
    description: '主导航位于左侧',
    icon: PanelLeft,
  },
  {
    value: 'minimal',
    templateId: 'nexora.shell.minimal',
    label: '紧凑',
    description: '减少固定导航占用',
    icon: Menu,
  },
];

const LEGACY_SHELL_TEMPLATE_ALIASES: Record<string, string> = {
  'builtin.shell.top-bar': 'nexora.shell.top-bar',
  'builtin.shell.side-bar': 'nexora.shell.side-bar',
  'builtin.shell.compact': 'nexora.shell.minimal',
};

const BUILTIN_TEMPLATE_SLOTS: TemplateSlotDefinition[] = [
  { id: 'tabs', name: '标签', accepts: ['tabs'], multiplicity: 'one', layout: 'single' },
  { id: 'navigation', name: '导航', accepts: ['navigation'], multiplicity: 'one', layout: 'single', collapseWhenEmpty: true },
  { id: 'toolbar', name: '项目工具', accepts: ['toolbar'], multiplicity: 'one', layout: 'single', collapseWhenEmpty: true },
  { id: 'primary', name: '主内容', accepts: ['active-surface', 'component-surface'], multiplicity: 'many', layout: 'stack', required: true },
  { id: 'status', name: '状态', accepts: ['status'], multiplicity: 'one', layout: 'single', collapseWhenEmpty: true },
];

function canonicalShellTemplateId(id: string | undefined) {
  return id ? LEGACY_SHELL_TEMPLATE_ALIASES[id] ?? id : '';
}

function selectComponent(profile: WorkspaceProfileV1, componentId: string, version: string) {
  const enabled = new Map((profile.enabledComponents ?? []).map((item) => [item.id, item] as const));
  const selection: ProfileComponentSelection = {
    id: componentId,
    versionRequirement: version === '*' ? '*' : `^${version}`,
  };
  enabled.set(componentId, selection);
  profile.enabledComponents = Array.from(enabled.values()).sort((left, right) => left.id.localeCompare(right.id));
}

function resolveEffectiveComponentIds(
  profile: WorkspaceProfileV1,
  modules: ModuleManifestV1[],
  runtime: ComponentRuntimeOverview | null,
) {
  const manifests = new Map(
    (runtime?.installedComponents ?? []).map((item) => [item.manifest.id, item.manifest] as const),
  );
  const enabledModules = new Set((profile.enabledModules ?? []).map((item) => item.id));
  const roots = [
    ...(profile.enabledComponents ?? []).map((item) => item.id),
    ...modules
      .filter((module) => enabledModules.has(module.id))
      .flatMap((module) => [
        ...(module.requiresComponents ?? []).map((item) => item.id),
        ...(module.optionalComponents ?? []).map((item) => item.id),
      ]),
  ];
  const effective = new Set<string>();
  const add = (componentId: string) => {
    if (effective.has(componentId)) return;
    const manifest = manifests.get(componentId);
    if (!manifest) return;
    effective.add(componentId);
    [...(manifest.requiresComponents ?? []), ...(manifest.optionalComponents ?? [])]
      .forEach((dependency) => add(dependency.id));
  };
  roots.forEach(add);
  return effective;
}

function contributionTitle(id: string) {
  return SURFACE_CONTRIBUTION_BY_ID.get(id)?.title
    || BUILTIN_TOOLS.find((tool) => tool.contribution.id === id)?.title
    || WIDGET_CONTRIBUTION_BY_ID.get(id)?.title
    || id;
}

function orderedWidgets(profile: WorkspaceProfileV1, surfaceId: string) {
  const surface = (profile.surfaces ?? []).find((candidate) => candidate.id === surfaceId);
  return [...(surface?.widgets ?? [])].sort((left, right) => {
    const leftRegion = left.region === 'sidebar' ? 0 : 1;
    const rightRegion = right.region === 'sidebar' ? 0 : 1;
    return leftRegion - rightRegion
      || (left.order ?? 0) - (right.order ?? 0)
      || left.id.localeCompare(right.id);
  });
}

function slotAcceptsLabel(slot: TemplateSlotDefinition) {
  const labels: Record<TemplateSlotDefinition['accepts'][number], string> = {
    'active-surface': '活动页面',
    'component-surface': '组件页面',
    widget: 'Widget',
    navigation: '导航',
    tabs: '标签',
    toolbar: '项目工具',
    status: '状态',
  };
  return slot.accepts.map((item) => labels[item]).join('、');
}

function localBindingId(...parts: string[]) {
  return parts
    .join('-')
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 104) || 'slot-binding';
}

export function WorkspaceProfileLayoutEditor({
  draft,
  modules,
  onChange,
}: WorkspaceProfileLayoutEditorProps) {
  const [dragged, setDragged] = useState<DragState | null>(null);
  const [componentRuntime, setComponentRuntime] = useState<ComponentRuntimeOverview | null>(null);
  const [templateLoadError, setTemplateLoadError] = useState<string | null>(null);
  const [templatePreview, setTemplatePreview] = useState<PresentationTemplatePreview | null>(null);
  const [templateDeveloperMessage, setTemplateDeveloperMessage] = useState<string | null>(null);
  const [developmentComponents, setDevelopmentComponents] = useState<DevelopmentComponentSnapshot[]>([]);

  useEffect(() => {
    let disposed = false;
    void getComponentRuntimeOverview()
      .then((overview) => {
        if (!disposed) setComponentRuntime(overview);
      })
      .catch((error) => {
        if (!disposed) setTemplateLoadError(String(error));
      });
    return () => { disposed = true; };
  }, []);
  const refreshDevelopmentComponents = async () => {
    setDevelopmentComponents(await getDevelopmentComponentSnapshot());
  };
  useEffect(() => {
    void refreshDevelopmentComponents().catch(() => setDevelopmentComponents([]));
  }, []);
  const selectedSurfaceIds = useMemo(
    () => getSelectedModuleContributionIds(draft, modules, 'surfaces'),
    [draft, modules],
  );
  const selectedShellTabIds = useMemo(
    () => getSelectedModuleContributionIds(draft, modules, 'shellTabs'),
    [draft, modules],
  );
  const selectedToolIds = useMemo(
    () => getSelectedModuleContributionIds(draft, modules, 'tools'),
    [draft, modules],
  );
  const selectedWidgetIds = useMemo(
    () => getSelectedModuleContributionIds(draft, modules, 'widgets'),
    [draft, modules],
  );

  const availableHomeSurfaces = useMemo(() => (
    Object.values(SURFACE_CONTRIBUTIONS)
      .filter((definition) => selectedSurfaceIds.has(definition.id))
      .filter((definition) => definition.host === 'shell')
      .filter((definition) => definition.id !== SURFACE_CONTRIBUTIONS.projectWorkspace.id)
  ), [selectedSurfaceIds]);

  const availableNavigationSurfaces = useMemo(() => {
    const definitions: SurfaceContributionDefinition[] = Object.values(SHELL_TAB_CONTRIBUTIONS)
      .filter((definition) => definition.instanceMode === 'singleton')
      .filter((definition) => selectedShellTabIds.has(definition.id))
      .flatMap((definition) => {
        const surface = SURFACE_CONTRIBUTION_BY_ID.get(definition.surfaceId);
        return surface ? [surface] : [];
      });
    if (selectedSurfaceIds.has(SURFACE_CONTRIBUTIONS.projectHome.id)) {
      definitions.unshift(SURFACE_CONTRIBUTIONS.projectHome);
    }
    return definitions;
  }, [selectedShellTabIds, selectedSurfaceIds]);

  const availableTools = useMemo(() => (
    BUILTIN_TOOLS.filter((tool) => tool.pinnable && selectedToolIds.has(tool.contribution.id))
  ), [selectedToolIds]);
  const effectiveComponentIds = useMemo(
    () => resolveEffectiveComponentIds(draft, modules, componentRuntime),
    [componentRuntime, draft, modules],
  );
  const componentSurfaces = useMemo(() => (
    (componentRuntime?.installedComponents ?? [])
      .filter((item) => effectiveComponentIds.has(item.manifest.id))
      .flatMap((item) => (item.manifest.contributes?.scriptSurfaces ?? []).map((surface) => ({
        componentId: item.manifest.id,
        componentName: item.manifest.name,
        surfaceId: surface.id,
        title: surface.name,
        placements: surface.placements,
        instanceMode: surface.instanceMode ?? 'singleton',
        sizeHints: surface.sizeHints,
        pinnable: surface.placements.includes('shell'),
        homeEligible: surface.placements.includes('shell'),
      })))
      .sort((left, right) => left.title.localeCompare(right.title, 'zh-CN'))
  ), [componentRuntime, effectiveComponentIds]);
  const componentSurfaceById = useMemo(
    () => new Map(componentSurfaces.map((surface) => [surface.surfaceId, surface] as const)),
    [componentSurfaces],
  );
  const availableScriptHomeSurfaces = useMemo(
    () => componentSurfaces.filter((surface) => surface.homeEligible),
    [componentSurfaces],
  );

  const currentHomeSurface = (draft.surfaces ?? []).find(
    (surface) => surface.id === draft.shellLayout?.home,
  );
  const currentHomeContributionId = currentHomeSurface?.contribution ?? '';
  const navigationSurfaceIds = draft.shellLayout?.navigation ?? [];
  const navigationSurfaces = navigationSurfaceIds.map((surfaceId) => (
    (draft.surfaces ?? []).find((surface) => surface.id === surfaceId)
  )).filter((surface): surface is NonNullable<WorkspaceProfileV1['surfaces']>[number] => Boolean(surface));
  const pinnedToolContributionIds = draft.shellLayout?.pinnedTools ?? [];
  const projectHomeSurface = (draft.surfaces ?? []).find(
    (surface) => surface.contribution === SURFACE_CONTRIBUTIONS.projectHome.id,
  );
  const availableWidgets = useMemo(() => (
    Array.from(selectedWidgetIds)
      .map((widgetId) => WIDGET_CONTRIBUTION_BY_ID.get(widgetId))
      .filter((definition): definition is WidgetContributionDefinition => Boolean(definition))
      .sort((left, right) => left.title.localeCompare(right.title, 'zh-CN'))
  ), [selectedWidgetIds]);
  const widgets = projectHomeSurface ? orderedWidgets(draft, projectHomeSurface.id) : [];
  const externalShellTemplates = useMemo(() => (componentRuntime?.templates.shellTemplates ?? [])
    .filter((item) => !SHELL_TEMPLATE_OPTIONS.some((builtin) => builtin.templateId === item.template.id))
    .sort((left, right) => left.template.name.localeCompare(right.template.name, 'zh-CN')),
  [componentRuntime]);
  const selectedShellTemplateId = draft.shellLayout?.shellTemplate?.id ?? '';
  const canonicalSelectedShellTemplateId = canonicalShellTemplateId(selectedShellTemplateId);
  const selectedBuiltinShellTemplate = SHELL_TEMPLATE_OPTIONS.some(
    (option) => option.templateId === canonicalSelectedShellTemplateId,
  );
  const selectedExternalShellTemplate = externalShellTemplates.find(
    (item) => item.template.id === selectedShellTemplateId,
  );
  const selectedTemplateForDevelopment = selectedExternalShellTemplate?.template.id
    || (selectedBuiltinShellTemplate ? canonicalSelectedShellTemplateId : '');
  const selectedDevelopmentTemplate = selectedExternalShellTemplate
    ? developmentComponents.find((item) => item.componentId === selectedExternalShellTemplate.owner.componentId) ?? null
    : null;
  const activeTemplateSlots = selectedBuiltinShellTemplate || !selectedShellTemplateId
    ? BUILTIN_TEMPLATE_SLOTS
    : (templatePreview?.slots ?? []);
  const activeTemplateState = (draft.shellLayout?.interfaceTemplateStates ?? []).find(
    (state) => state.templateId === (selectedShellTemplateId
      || SHELL_TEMPLATE_OPTIONS.find((option) => option.value === (draft.shellLayout?.navigationKind ?? 'top-bar'))?.templateId),
  ) ?? null;
  const activeTemplateBindings = activeTemplateState?.slotBindings ?? [];
  const activeTemplateId = selectedShellTemplateId
    || SHELL_TEMPLATE_OPTIONS.find((option) => option.value === (draft.shellLayout?.navigationKind ?? 'top-bar'))?.templateId
    || 'nexora.shell.top-bar';

  const isSurfaceBound = (componentId: string, surfaceId: string) => activeTemplateBindings.some(
    (binding) => binding.enabled !== false
      && binding.kind === 'component-surface'
      && binding.componentId === componentId
      && binding.surfaceId === surfaceId,
  );

  const addComponentSurfaceToSlot = (
    slot: TemplateSlotDefinition,
    surface: (typeof componentSurfaces)[number],
  ) => {
    mutate((profile) => mutateTemplateState(profile, (bindings) => {
      const slotBindings = bindings.filter((binding) => binding.slotId === slot.id && binding.enabled !== false);
      if (slot.multiplicity === 'one' && slotBindings.length > 0) return;
      if (surface.instanceMode === 'singleton' && bindings.some(
        (binding) => binding.enabled !== false
          && binding.kind === 'component-surface'
          && binding.componentId === surface.componentId
          && binding.surfaceId === surface.surfaceId,
      )) return;
      const sequence = bindings.filter((binding) => binding.slotId === slot.id).length + 1;
      const instanceNumber = bindings.filter(
        (binding) => binding.componentId === surface.componentId && binding.surfaceId === surface.surfaceId,
      ).length + 1;
      const baseId = localBindingId('slot', slot.id, surface.componentId, surface.surfaceId, String(instanceNumber));
      let id = baseId;
      let duplicate = 2;
      while (bindings.some((binding) => binding.id === id)) {
        id = localBindingId(baseId, String(duplicate));
        duplicate += 1;
      }
      bindings.push({
        id,
        slotId: slot.id,
        kind: 'component-surface',
        componentId: surface.componentId,
        surfaceId: surface.surfaceId,
        instanceId: surface.instanceMode === 'multiple' ? localBindingId('instance', surface.surfaceId, String(instanceNumber)) : undefined,
        enabled: true,
        order: sequence * 10,
      });
    }));
  };

  const removeTemplateBinding = (bindingId: string) => {
    mutate((profile) => mutateTemplateState(profile, (bindings) => {
      const index = bindings.findIndex((binding) => binding.id === bindingId);
      if (index >= 0) bindings.splice(index, 1);
    }));
  };

  const moveTemplateBinding = (bindingId: string, direction: -1 | 1) => {
    mutate((profile) => mutateTemplateState(profile, (bindings) => {
      const binding = bindings.find((candidate) => candidate.id === bindingId);
      if (!binding) return;
      const peers = bindings
        .filter((candidate) => candidate.slotId === binding.slotId)
        .sort((left, right) => (left.order ?? 0) - (right.order ?? 0) || left.id.localeCompare(right.id));
      const index = peers.findIndex((candidate) => candidate.id === bindingId);
      const neighbor = peers[index + direction];
      if (!neighbor) return;
      const order = binding.order ?? 0;
      binding.order = neighbor.order ?? 0;
      neighbor.order = order;
    }));
  };

  const duplicateSelectedTemplate = async () => {
    if (!selectedTemplateForDevelopment) return;
    setTemplateDeveloperMessage(null);
    try {
      const selectedDirectory = await open({
        directory: true,
        multiple: false,
        title: '选择界面模板开发目录',
      });
      if (!selectedDirectory || Array.isArray(selectedDirectory)) return;
      const sourcePath = await duplicateInterfaceTemplateForDevelopment(
        selectedTemplateForDevelopment,
        selectedDirectory,
      );
      await trustScriptDevelopmentDirectory(sourcePath);
      const manifest = await installComponentFromDirectory(sourcePath);
      const copiedTemplate = manifest.contributes?.shellTemplates?.[0];
      if (!copiedTemplate) throw new Error('开发副本没有声明界面模板');
      mutate((profile) => {
        profile.shellLayout = {
          ...(profile.shellLayout ?? {}),
          shellTemplate: { id: copiedTemplate.id, versionRequirement: `^${copiedTemplate.version}` },
        };
        selectComponent(profile, manifest.id, manifest.version);
      });
      setComponentRuntime(await getComponentRuntimeOverview());
      await refreshDevelopmentComponents();
      setTemplateDeveloperMessage(`已创建、信任并安装开发副本：${sourcePath}`);
    } catch (error) {
      setTemplateDeveloperMessage(`创建开发副本失败：${String(error)}`);
    }
  };

  const reloadSelectedDevelopmentTemplate = async () => {
    if (!selectedDevelopmentTemplate?.componentId) return;
    try {
      await reloadDevelopmentInterfaceTemplate(selectedDevelopmentTemplate.componentId);
      setComponentRuntime(await getComponentRuntimeOverview());
      await refreshDevelopmentComponents();
      setTemplateDeveloperMessage(`已重载开发模板：${selectedDevelopmentTemplate.componentId}`);
    } catch (error) {
      setTemplateDeveloperMessage(`重载开发模板失败：${String(error)}`);
    }
  };

  useEffect(() => {
    let disposed = false;
    setTemplatePreview(null);
    if (!selectedExternalShellTemplate) return () => { disposed = true; };
    void getPresentationTemplatePreview(
      selectedExternalShellTemplate.owner.componentId,
      selectedExternalShellTemplate.template.id,
    ).then((preview) => {
      if (!disposed) setTemplatePreview(preview);
    }).catch((error) => {
      if (!disposed) setTemplateLoadError(String(error));
    });
    return () => { disposed = true; };
  }, [selectedExternalShellTemplate]);

  const mutate = (updater: (profile: WorkspaceProfileV1) => void) => {
    onChange((profile) => {
      updater(profile);
      return profile;
    });
  };

  const mutateTemplateState = (
    profile: WorkspaceProfileV1,
    updater: (bindings: ProfileTemplateSlotBinding[]) => void,
  ) => {
    const templateId = profile.shellLayout?.shellTemplate?.id
      || SHELL_TEMPLATE_OPTIONS.find((option) => option.value === (profile.shellLayout?.navigationKind ?? 'top-bar'))?.templateId
      || 'nexora.shell.top-bar';
    profile.shellLayout = { ...(profile.shellLayout ?? {}) };
    const states = [...(profile.shellLayout.interfaceTemplateStates ?? [])];
    const existingIndex = states.findIndex((state) => state.templateId === templateId);
    const bindings = existingIndex >= 0 ? [...(states[existingIndex].slotBindings ?? [])] : [];
    updater(bindings);
    const nextState = existingIndex >= 0
      ? { ...states[existingIndex], slotBindings: bindings }
      : { templateId, slotBindings: bindings };
    if (existingIndex >= 0) states[existingIndex] = nextState;
    else states.push(nextState);
    profile.shellLayout.interfaceTemplateStates = states;
  };

  const handleDrop = (
    kind: DragKind,
    beforeId: string | null,
    action: (profile: WorkspaceProfileV1, draggedId: string, beforeId: string | null) => void,
  ) => {
    if (!dragged || dragged.kind !== kind || dragged.id === beforeId) return;
    mutate((profile) => action(profile, dragged.id, beforeId));
    setDragged(null);
  };

  const toggleNavigation = (definition: SurfaceContributionDefinition, enabled: boolean) => {
    mutate((profile) => setProfileNavigationContribution(profile, definition.id, enabled));
  };

  const selectHome = (definition: SurfaceContributionDefinition | null) => {
    mutate((profile) => setProfileHomeContribution(profile, definition?.id ?? null));
  };

  const selectScriptHome = (surface: (typeof componentSurfaces)[number]) => {
    mutate((profile) => setProfileHomeScriptSurface(profile, {
      componentId: surface.componentId,
      surfaceId: surface.surfaceId,
      title: surface.title,
    }));
  };

  return (
    <div className="space-y-4">
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.1fr)_minmax(320px,0.9fr)]">
        <div className="space-y-4">
          <section className="rounded-md border border-gray-200 dark:border-gray-700">
            <div className="flex items-center gap-2 border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
              <LayoutDashboard className="h-4 w-4 text-sky-600" />
              <h4 className="text-sm font-semibold">启动主页</h4>
            </div>
            <div className="p-3">
              <select
                value={currentHomeContributionId}
                onChange={(event) => {
                  const definition = availableHomeSurfaces.find(
                    (candidate) => candidate.id === event.target.value,
                  );
                  if (definition) {
                    selectHome(definition);
                    return;
                  }
                  const scriptSurface = availableScriptHomeSurfaces.find(
                    (candidate) => candidate.surfaceId === event.target.value,
                  );
                  if (scriptSurface) {
                    selectScriptHome(scriptSurface);
                    return;
                  }
                  selectHome(null);
                }}
                className="h-9 w-full rounded-md border border-gray-300 bg-white px-3 text-sm dark:border-gray-700 dark:bg-gray-800"
              >
                <option value="">最小安全主页</option>
                {availableHomeSurfaces.map((definition) => (
                  <option key={definition.id} value={definition.id}>{definition.title}</option>
                ))}
                {availableScriptHomeSurfaces.length > 0 ? (
                  <optgroup label="组件页面">
                    {availableScriptHomeSurfaces.map((surface) => (
                      <option key={`${surface.componentId}:${surface.surfaceId}`} value={surface.surfaceId}>
                        {surface.title} · {surface.componentName}
                      </option>
                    ))}
                  </optgroup>
                ) : null}
                {currentHomeContributionId
                  && !availableHomeSurfaces.some((definition) => definition.id === currentHomeContributionId)
                  && !availableScriptHomeSurfaces.some((surface) => surface.surfaceId === currentHomeContributionId) ? (
                    <option value={currentHomeContributionId} disabled>
                      {componentSurfaceById.get(currentHomeContributionId)?.title
                        || contributionTitle(currentHomeContributionId)}（所属组件未选择）
                    </option>
                  ) : null}
              </select>
              <p className="mt-2 text-xs text-gray-500 dark:text-gray-400">
                主页只决定启动时激活的页面；同一页面可另外加入导航或固定到快捷栏，并不互斥。未选择主页时始终进入恢复安全页，不会出现空白窗口。
              </p>
            </div>
          </section>

          <section className="rounded-md border border-gray-200 dark:border-gray-700">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
              <div className="flex items-center gap-2">
                <LayoutDashboard className="h-4 w-4 text-indigo-600" />
                <div>
                  <h4 className="text-sm font-semibold">模板插槽</h4>
                  <p className="text-[11px] text-gray-500 dark:text-gray-400">选择区域并装配当前方案中有效组件公开的页面。切换模板时，各模板的装配会分别保留。</p>
                </div>
              </div>
              <span className="font-mono text-[10px] text-gray-400">{activeTemplateId}</span>
            </div>
            <div className="divide-y divide-gray-100 dark:divide-gray-800">
              {activeTemplateSlots.map((slot) => {
                const bindings = activeTemplateBindings
                  .filter((binding) => binding.slotId === slot.id)
                  .sort((left, right) => (left.order ?? 0) - (right.order ?? 0) || left.id.localeCompare(right.id));
                const acceptsComponentSurface = slot.accepts.includes('component-surface');
                const hostKinds = slot.accepts.filter((kind) => kind !== 'component-surface' && kind !== 'widget');
                const eligibleSurfaces = acceptsComponentSurface
                  ? componentSurfaces.filter((surface) => (
                    surface.placements.includes('shell') || surface.placements.includes('workspace')
                  )).filter((surface) => (
                    surface.instanceMode === 'multiple' || !isSurfaceBound(surface.componentId, surface.surfaceId)
                  ))
                  : [];
                const missingRequired = slot.required && bindings.length === 0 && !slot.accepts.some(
                  (kind) => ['active-surface', 'navigation', 'tabs', 'toolbar', 'status'].includes(kind),
                );
                return (
                  <div key={slot.id} className="p-3">
                    <div className="flex flex-wrap items-start justify-between gap-2">
                      <div>
                        <div className="flex flex-wrap items-center gap-1.5">
                          <p className="text-sm font-medium text-gray-900 dark:text-gray-100">{slot.name || slot.id}</p>
                          {slot.required ? <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-950/50 dark:text-amber-300">必需</span> : null}
                          {slot.collapseWhenEmpty ? <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-800">空时折叠</span> : null}
                          <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-800">{slot.layout || 'flow'} · {slot.multiplicity || 'many'}</span>
                        </div>
                        <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">接受：{slotAcceptsLabel(slot)}{slot.minWidth || slot.minHeight ? ` · 最小 ${slot.minWidth ?? '自动'} x ${slot.minHeight ?? '自动'}` : ''}</p>
                      </div>
                      {hostKinds.length ? <span className="rounded border border-emerald-200 bg-emerald-50 px-2 py-1 text-[10px] text-emerald-700 dark:border-emerald-900/60 dark:bg-emerald-950/30 dark:text-emerald-300">宿主提供：{hostKinds.map((kind) => slotAcceptsLabel({ ...slot, accepts: [kind] })).join('、')}</span> : null}
                    </div>

                    {bindings.length ? (
                      <div className="mt-2 space-y-1.5">
                        {bindings.map((binding, index) => {
                          const surface = binding.componentId && binding.surfaceId
                            ? componentSurfaces.find((candidate) => candidate.componentId === binding.componentId && candidate.surfaceId === binding.surfaceId)
                            : null;
                          return (
                            <div key={binding.id} className={`flex min-h-9 items-center gap-2 rounded-md border px-2 ${surface ? 'border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800' : 'border-amber-200 bg-amber-50 dark:border-amber-900/60 dark:bg-amber-950/20'}`}>
                              <FileCode2 className={`h-3.5 w-3.5 shrink-0 ${surface ? 'text-violet-500' : 'text-amber-600'}`} />
                              <span className="min-w-0 flex-1 truncate text-xs">{surface ? `${surface.title} · ${surface.componentName}` : `${binding.surfaceId || binding.contributionId || binding.kind}（当前不可用）`}</span>
                              {binding.instanceId ? <span className="font-mono text-[10px] text-gray-400">{binding.instanceId}</span> : null}
                              <button type="button" disabled={index === 0} onClick={() => moveTemplateBinding(binding.id, -1)} className="flex h-7 w-6 items-center justify-center rounded text-gray-400 hover:bg-gray-100 disabled:opacity-30 dark:hover:bg-gray-700" title="向前排序"><ArrowUp className="h-3.5 w-3.5" /></button>
                              <button type="button" disabled={index === bindings.length - 1} onClick={() => moveTemplateBinding(binding.id, 1)} className="flex h-7 w-6 items-center justify-center rounded text-gray-400 hover:bg-gray-100 disabled:opacity-30 dark:hover:bg-gray-700" title="向后排序"><ArrowDown className="h-3.5 w-3.5" /></button>
                              <button type="button" onClick={() => removeTemplateBinding(binding.id)} className="flex h-7 w-7 items-center justify-center rounded text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-700" title="从插槽移除"><X className="h-3.5 w-3.5" /></button>
                            </div>
                          );
                        })}
                      </div>
                    ) : null}
                    {missingRequired ? <p className="mt-2 text-xs text-red-600 dark:text-red-300">此必需插槽没有可用内容，保存并应用会被阻止。</p> : null}
                    {acceptsComponentSurface ? (
                      <div className="mt-2 grid gap-1 sm:grid-cols-2">
                        {eligibleSurfaces.map((surface) => {
                          const minTooLarge = (slot.maxWidth && surface.sizeHints?.minWidth && surface.sizeHints.minWidth > slot.maxWidth)
                            || (slot.maxHeight && surface.sizeHints?.minHeight && surface.sizeHints.minHeight > slot.maxHeight);
                          const full = slot.multiplicity === 'one' && bindings.length > 0;
                          return (
                            <button
                              key={`${slot.id}:${surface.componentId}:${surface.surfaceId}`}
                              type="button"
                              disabled={Boolean(minTooLarge || full)}
                              onClick={() => addComponentSurfaceToSlot(slot, surface)}
                              className="flex min-h-9 items-center gap-2 rounded-md px-2 text-left text-xs hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-gray-800"
                              title={minTooLarge ? '组件最小尺寸超过模板插槽限制' : full ? '此插槽只允许一个内容' : `加入 ${slot.name || slot.id}`}
                            >
                              <Plus className="h-3.5 w-3.5 shrink-0 text-gray-400" />
                              <span className="min-w-0 flex-1 truncate">{surface.title}</span>
                              <span className="max-w-28 truncate text-[10px] text-gray-400">{surface.instanceMode === 'multiple' ? '可多开' : '单例'} · {surface.componentName}</span>
                            </button>
                          );
                        })}
                        {eligibleSurfaces.length === 0 ? <p className="py-1 text-xs text-gray-400">没有可加入此插槽的组件页面。</p> : null}
                      </div>
                    ) : null}
                  </div>
                );
              })}
              {activeTemplateSlots.length === 0 ? <p className="p-4 text-center text-xs text-amber-600 dark:text-amber-300">该模板没有声明可装配插槽，不能应用为主界面。</p> : null}
            </div>
          </section>

          <section className="rounded-md border border-gray-200 dark:border-gray-700">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
              <div className="flex items-center gap-2">
                <Menu className="h-4 w-4 text-emerald-600" />
                <div>
                  <h4 className="text-sm font-semibold">界面模板</h4>
                  <p className="text-[11px] text-gray-500 dark:text-gray-400">控制宿主工具带以下的全部布局，可引用内置或已安装模板包</p>
                </div>
              </div>
              <div className="inline-flex overflow-hidden rounded-md border border-gray-200 dark:border-gray-700">
                {SHELL_TEMPLATE_OPTIONS.map((option) => {
                  const Icon = option.icon;
                  const active = selectedShellTemplateId
                    ? canonicalSelectedShellTemplateId === option.templateId
                    : (draft.shellLayout?.navigationKind ?? 'top-bar') === option.value;
                  return (
                    <button
                      key={option.value}
                      type="button"
                      onClick={() => mutate((profile) => {
                        profile.shellLayout = {
                          ...(profile.shellLayout ?? {}),
                          navigationKind: option.value,
                          shellTemplate: {
                            id: option.templateId,
                            versionRequirement: '*',
                          },
                        };
                        selectComponent(profile, 'nexora.presentation.templates', '*');
                      })}
                      className={`inline-flex h-8 items-center gap-1 border-r border-gray-200 px-2 text-xs last:border-r-0 dark:border-gray-700 ${
                        active
                          ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300'
                          : 'text-gray-500 hover:bg-gray-50 dark:hover:bg-gray-800'
                      }`}
                      title={option.description}
                    >
                      <Icon className="h-3.5 w-3.5" />
                      {option.label}
                    </button>
                  );
                })}
              </div>
            </div>
            <div className="grid gap-3 p-3 md:grid-cols-2">
              <div className="md:col-span-2 flex flex-wrap items-center justify-between gap-2 rounded-md border border-gray-200 bg-gray-50 px-3 py-2.5 dark:border-gray-700 dark:bg-gray-800/50">
                <div>
                  <p className="text-xs font-medium text-gray-700 dark:text-gray-200">宿主工具带</p>
                  <p className="mt-0.5 text-[11px] text-gray-500 dark:text-gray-400">快捷栏、DEV、维护中心和功能中心始终由 Nexora 管理。</p>
                </div>
                <div className="inline-flex overflow-hidden rounded-md border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
                  {([
                    ['fixed', '固定显示'],
                    ['auto-hide', '移到顶部展开'],
                  ] as const).map(([mode, label]) => (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => mutate((profile) => {
                        profile.shellLayout = {
                          ...(profile.shellLayout ?? {}),
                          hostToolbar: { ...(profile.shellLayout?.hostToolbar ?? {}), mode },
                        };
                      })}
                      className={`h-8 px-2.5 text-xs transition-colors ${
                        (draft.shellLayout?.hostToolbar?.mode ?? 'fixed') === mode
                          ? 'bg-sky-50 font-medium text-sky-700 dark:bg-sky-950/50 dark:text-sky-300'
                          : 'text-gray-500 hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-gray-800'
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>
              <label className="md:col-span-2">
                <span className="mb-1 block text-xs font-medium text-gray-600 dark:text-gray-300">安装的界面模板</span>
                <select
                  value={selectedBuiltinShellTemplate ? '' : selectedShellTemplateId}
                  onChange={(event) => {
                    const selected = externalShellTemplates.find((item) => item.template.id === event.target.value);
                    if (!selected) return;
                    mutate((profile) => {
                      profile.shellLayout = {
                        ...(profile.shellLayout ?? {}),
                        shellTemplate: {
                          id: selected.template.id,
                          versionRequirement: `^${selected.template.version}`,
                        },
                      };
                      selectComponent(profile, selected.owner.componentId, selected.owner.componentVersion);
                    });
                  }}
                  className="h-9 w-full rounded-md border border-gray-300 bg-white px-3 text-sm dark:border-gray-700 dark:bg-gray-800"
                >
                  <option value="">使用下方内置界面模板</option>
                  {externalShellTemplates.map((item) => (
                    <option key={item.template.id} value={item.template.id}>{item.template.name} · {item.owner.componentName}</option>
                  ))}
                  {selectedShellTemplateId
                    && !selectedBuiltinShellTemplate
                    && !externalShellTemplates.some((item) => item.template.id === selectedShellTemplateId) ? (
                      <option value={selectedShellTemplateId} disabled>{selectedShellTemplateId}（模板缺失）</option>
                    ) : null}
                </select>
                {selectedShellTemplateId !== canonicalSelectedShellTemplateId ? (
                  <p className="mt-1 text-[11px] text-gray-400">旧版模板标识已兼容为“{SHELL_TEMPLATE_OPTIONS.find((item) => item.templateId === canonicalSelectedShellTemplateId)?.label}”。下次选择布局并保存后会写入新标识。</p>
                ) : null}
                {externalShellTemplates.length ? (
                  <p className="mt-1 text-[11px] text-gray-400">模板包先经静态净化；模板只控制工具带以下的布局，缺失时自动回退内置布局。</p>
                ) : null}
                {templateLoadError ? <p className="mt-1 text-[11px] text-amber-600 dark:text-amber-300">无法读取模板目录：{templateLoadError}</p> : null}
               </label>
              {selectedTemplateForDevelopment ? (
                <div className="md:col-span-2 flex flex-wrap items-center justify-between gap-2 rounded-md border border-dashed border-gray-300 px-3 py-2 dark:border-gray-700">
                  <p className="text-[11px] text-gray-500 dark:text-gray-400">已安装模板保持只读。复制后会创建受信任的本地 data-pack，供 VS Code 修改与 DEV 重载。</p>
                  <div className="flex items-center gap-1.5">
                    {selectedDevelopmentTemplate ? <>
                      <button type="button" onClick={() => void openScriptDevelopmentDirectoryInVSCode(selectedDevelopmentTemplate.sourcePath).catch((error) => setTemplateDeveloperMessage(`无法打开 VS Code：${String(error)}`))} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-300 px-2.5 text-xs text-gray-700 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800">
                        <FileCode2 className="h-3.5 w-3.5" />VS Code
                      </button>
                      <button type="button" disabled={!selectedDevelopmentTemplate.valid || !selectedDevelopmentTemplate.trusted} onClick={() => void reloadSelectedDevelopmentTemplate()} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-sky-200 px-2.5 text-xs text-sky-700 hover:bg-sky-50 disabled:opacity-40 dark:border-sky-900/60 dark:text-sky-300 dark:hover:bg-sky-950/30">
                        <RefreshCw className="h-3.5 w-3.5" />重载
                      </button>
                    </> : null}
                    <button type="button" onClick={() => void duplicateSelectedTemplate()} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-indigo-200 px-2.5 text-xs text-indigo-700 hover:bg-indigo-50 dark:border-indigo-900/60 dark:text-indigo-300 dark:hover:bg-indigo-950/30">
                      <FileCode2 className="h-3.5 w-3.5" />复制为开发模板
                    </button>
                  </div>
                </div>
              ) : null}
              {templateDeveloperMessage ? <p className="md:col-span-2 text-[11px] text-gray-500 dark:text-gray-400">{templateDeveloperMessage}</p> : null}
              <div>
                <p className="mb-2 text-xs font-medium text-gray-600 dark:text-gray-300">已加入导航</p>
                <div
                  className="min-h-20 space-y-1.5"
                  onDragOver={(event) => event.preventDefault()}
                  onDrop={() => handleDrop('navigation', null, reorderProfileNavigation)}
                >
                  {navigationSurfaces.map((surface) => (
                    <div
                      key={surface.id}
                      draggable
                      onDragStart={() => setDragged({ kind: 'navigation', id: surface.id })}
                      onDragOver={(event) => event.preventDefault()}
                      onDrop={(event) => {
                        event.stopPropagation();
                        handleDrop('navigation', surface.id, reorderProfileNavigation);
                      }}
                      className="flex h-9 items-center gap-2 rounded-md border border-gray-200 bg-white px-2 text-sm dark:border-gray-700 dark:bg-gray-800"
                    >
                      <GripVertical className="h-4 w-4 cursor-grab text-gray-400" />
                      <span className="min-w-0 flex-1 truncate">
                        {surface.contribution ? contributionTitle(surface.contribution) : surface.title || surface.id}
                      </span>
                      <button
                        type="button"
                        onClick={() => surface.contribution && mutate((profile) => (
                          setProfileNavigationContribution(profile, surface.contribution!, false)
                        ))}
                        className="flex h-7 w-7 items-center justify-center rounded text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-700"
                        title="从导航移除"
                      >
                        <X className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  ))}
                  {navigationSurfaces.length === 0 ? (
                    <p className="py-5 text-center text-xs text-gray-400">尚未配置主导航</p>
                  ) : null}
                </div>
              </div>
              <div>
                <p className="mb-2 text-xs font-medium text-gray-600 dark:text-gray-300">可用页面</p>
                <div className="space-y-1.5">
                  {availableNavigationSurfaces.map((definition) => {
                    const selected = navigationSurfaces.some(
                      (surface) => surface.contribution === definition.id,
                    );
                    return (
                      <label key={definition.id} className="flex min-h-9 cursor-pointer items-center gap-2 rounded-md px-2 text-sm hover:bg-gray-50 dark:hover:bg-gray-800">
                        <input
                          type="checkbox"
                          checked={selected}
                          onChange={(event) => toggleNavigation(definition, event.target.checked)}
                          className="h-4 w-4 rounded border-gray-300 text-emerald-600 focus:ring-emerald-500"
                        />
                        <span>{definition.title}</span>
                      </label>
                    );
                  })}
                  {availableNavigationSurfaces.length === 0 ? (
                    <p className="py-5 text-center text-xs text-gray-400">所选组件没有单例导航页面</p>
                  ) : null}
                </div>
              </div>
            </div>
          </section>

          <section className="rounded-md border border-gray-200 dark:border-gray-700">
            <div className="flex items-center gap-2 border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
              <Pin className="h-4 w-4 text-violet-600" />
              <h4 className="text-sm font-semibold">快捷栏</h4>
              <span className="ml-auto text-xs text-gray-500">{pinnedToolContributionIds.length} 项</span>
            </div>
            <div className="grid gap-3 p-3 md:grid-cols-2">
              <div
                className="min-h-20 space-y-1.5"
                onDragOver={(event) => event.preventDefault()}
                onDrop={() => handleDrop('tool', null, reorderPinnedTools)}
              >
                {pinnedToolContributionIds.map((contributionId) => {
                  const tool = BUILTIN_TOOLS.find((candidate) => candidate.contribution.id === contributionId);
                  const componentSurface = componentSurfaceById.get(contributionId);
                  const Icon = tool?.icon ?? (componentSurface ? FileCode2 : Pin);
                  return (
                    <div
                      key={contributionId}
                      draggable
                      onDragStart={() => setDragged({ kind: 'tool', id: contributionId })}
                      onDragOver={(event) => event.preventDefault()}
                      onDrop={(event) => {
                        event.stopPropagation();
                        handleDrop('tool', contributionId, reorderPinnedTools);
                      }}
                      className="flex h-9 items-center gap-2 rounded-md border border-gray-200 bg-white px-2 text-sm dark:border-gray-700 dark:bg-gray-800"
                    >
                      <GripVertical className="h-4 w-4 cursor-grab text-gray-400" />
                      <Icon className="h-4 w-4 text-violet-500" />
                      <span className="min-w-0 flex-1 truncate">{tool?.title || componentSurface?.title || contributionId}</span>
                      <button
                        type="button"
                        onClick={() => mutate((profile) => setPinnedToolContribution(profile, contributionId, false))}
                        className="flex h-7 w-7 items-center justify-center rounded text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-700"
                        title="取消固定"
                      >
                        <X className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  );
                })}
                {pinnedToolContributionIds.length === 0 ? (
                  <p className="py-5 text-center text-xs text-gray-400">快捷栏为空</p>
                ) : null}
              </div>
              <div className="space-y-1.5">
                {availableTools
                  .filter((tool) => !pinnedToolContributionIds.includes(tool.contribution.id))
                  .map((tool) => {
                    const Icon = tool.icon;
                    return (
                      <button
                        key={tool.id}
                        type="button"
                        onClick={() => mutate((profile) => setPinnedToolContribution(profile, tool.contribution.id, true))}
                        className="flex h-9 w-full items-center gap-2 rounded-md px-2 text-left text-sm hover:bg-gray-50 dark:hover:bg-gray-800"
                      >
                        <Plus className="h-4 w-4 text-gray-400" />
                        <Icon className="h-4 w-4 text-gray-500" />
                        <span className="truncate">{tool.title}</span>
                      </button>
                    );
                  })}
                {componentSurfaces
                  .filter((surface) => surface.pinnable && !pinnedToolContributionIds.includes(surface.surfaceId))
                  .map((surface) => (
                    <button
                      key={surface.surfaceId}
                      type="button"
                      onClick={() => mutate((profile) => setPinnedToolContribution(profile, surface.surfaceId, true))}
                      className="flex h-9 w-full items-center gap-2 rounded-md px-2 text-left text-sm hover:bg-gray-50 dark:hover:bg-gray-800"
                    >
                      <Plus className="h-4 w-4 text-gray-400" />
                      <FileCode2 className="h-4 w-4 text-violet-500" />
                      <span className="min-w-0 flex-1 truncate">{surface.title}</span>
                      <span className="max-w-32 truncate text-[10px] text-gray-400">{surface.componentName}</span>
                    </button>
                  ))}
                {availableTools.every((tool) => pinnedToolContributionIds.includes(tool.contribution.id))
                  && componentSurfaces.filter((surface) => surface.pinnable).every(
                    (surface) => pinnedToolContributionIds.includes(surface.surfaceId),
                  ) ? (
                  <p className="py-5 text-center text-xs text-gray-400">没有更多可固定工具</p>
                ) : null}
              </div>
            </div>
          </section>
        </div>

        <div className="space-y-4">
          <section className="rounded-md border border-gray-200 dark:border-gray-700">
            <div className="flex items-center gap-2 border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
              <FileCode2 className="h-4 w-4 text-violet-600" />
              <h4 className="text-sm font-semibold">组件页面</h4>
              <span className="ml-auto text-xs text-gray-500">{componentSurfaces.length} 项</span>
            </div>
            <div className="max-h-52 divide-y divide-gray-100 overflow-auto dark:divide-gray-800">
              {componentSurfaces.map((surface) => (
                <div key={`${surface.componentId}:${surface.surfaceId}`} className="flex items-center gap-3 px-3 py-2.5">
                  <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-violet-100 text-violet-700 dark:bg-violet-950/50 dark:text-violet-300">
                    <FileCode2 className="h-4 w-4" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium text-gray-900 dark:text-gray-100">{surface.title}</p>
                    <p className="truncate text-[11px] text-gray-500">{surface.componentName} · {surface.placements.join(' / ')}</p>
                  </div>
                  {surface.homeEligible ? (
                    <button
                      type="button"
                      onClick={() => selectScriptHome(surface)}
                      className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-md ${currentHomeContributionId === surface.surfaceId ? 'bg-sky-100 text-sky-700 dark:bg-sky-950/50 dark:text-sky-300' : 'text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800'}`}
                      title={currentHomeContributionId === surface.surfaceId ? '当前启动主页' : '设为启动主页'}
                    >
                      <LayoutDashboard className="h-4 w-4" />
                    </button>
                  ) : null}
                  {surface.pinnable ? (
                    <button
                      type="button"
                      onClick={() => mutate((profile) => setPinnedToolContribution(
                        profile,
                        surface.surfaceId,
                        !pinnedToolContributionIds.includes(surface.surfaceId),
                      ))}
                      className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-md ${pinnedToolContributionIds.includes(surface.surfaceId) ? 'bg-violet-100 text-violet-700 dark:bg-violet-950/50 dark:text-violet-300' : 'text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800'}`}
                      title={pinnedToolContributionIds.includes(surface.surfaceId) ? '从快捷栏移除' : '固定到快捷栏'}
                    >
                      <Pin className="h-4 w-4" />
                    </button>
                  ) : (
                    <span className="text-[10px] text-gray-400">未声明 shell</span>
                  )}
                </div>
              ))}
              {componentSurfaces.length === 0 ? (
                <p className="px-3 py-6 text-center text-xs text-gray-400">当前方案没有启用带页面的外部组件</p>
              ) : null}
            </div>
          </section>

          <section className="rounded-md border border-gray-200 dark:border-gray-700">
            <div className="flex items-center gap-2 border-b border-gray-200 px-3 py-2.5 dark:border-gray-700">
              <LayoutDashboard className="h-4 w-4 text-orange-500" />
              <h4 className="text-sm font-semibold">项目主页组件</h4>
            </div>
            {!projectHomeSurface ? (
              <div className="p-6 text-center text-xs text-gray-500">
                选择“项目主页”作为主页，或启用项目管理器后再配置组件。
              </div>
            ) : (
              <div className="p-3">
                <div
                  className="space-y-1.5"
                  onDragOver={(event) => event.preventDefault()}
                  onDrop={() => handleDrop(
                    'widget',
                    null,
                    (profile, id, beforeId) => reorderProfileWidgets(
                      profile,
                      projectHomeSurface.id,
                      id,
                      beforeId,
                    ),
                  )}
                >
                  {widgets.map((widget) => {
                    const definition = getWidgetDefinition(widget.widget);
                    return (
                      <div
                        key={widget.id}
                        draggable
                        onDragStart={() => setDragged({ kind: 'widget', id: widget.id })}
                        onDragOver={(event) => event.preventDefault()}
                        onDrop={(event) => {
                          event.stopPropagation();
                          handleDrop(
                            'widget',
                            widget.id,
                            (profile, id, beforeId) => reorderProfileWidgets(
                              profile,
                              projectHomeSurface.id,
                              id,
                              beforeId,
                            ),
                          );
                        }}
                        className="flex min-h-10 items-center gap-2 rounded-md border border-gray-200 bg-white px-2 dark:border-gray-700 dark:bg-gray-800"
                      >
                        <GripVertical className="h-4 w-4 cursor-grab text-gray-400" />
                        <span className="min-w-0 flex-1 truncate text-sm">{definition?.title || widget.widget}</span>
                        <select
                          value={widget.region === 'sidebar' ? 'sidebar' : 'content'}
                          onChange={(event) => mutate((profile) => updateProfileWidgetRegion(
                            profile,
                            projectHomeSurface.id,
                            widget.id,
                            event.target.value as 'sidebar' | 'content',
                          ))}
                          className="h-7 rounded border border-gray-200 bg-white px-1.5 text-xs dark:border-gray-700 dark:bg-gray-900"
                          title="组件区域"
                        >
                          <option value="sidebar">侧栏</option>
                          <option value="content">内容</option>
                        </select>
                        <button
                          type="button"
                          onClick={() => definition && mutate((profile) => setProfileWidgetContribution(
                            profile,
                            projectHomeSurface.id,
                            definition,
                            false,
                          ))}
                          className="flex h-7 w-7 items-center justify-center rounded text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-700"
                          title="移除组件"
                        >
                          <X className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    );
                  })}
                  {widgets.length === 0 ? (
                    <p className="py-5 text-center text-xs text-gray-400">主页没有组件</p>
                  ) : null}
                </div>
                <div className="mt-3 border-t border-gray-200 pt-3 dark:border-gray-700">
                  <p className="mb-1.5 text-xs font-medium text-gray-600 dark:text-gray-300">添加组件</p>
                  <div className="grid gap-1 sm:grid-cols-2">
                    {availableWidgets
                      .filter((definition) => !widgets.some((widget) => widget.widget === definition.id))
                      .map((definition) => (
                        <button
                          key={definition.id}
                          type="button"
                          onClick={() => mutate((profile) => setProfileWidgetContribution(
                            profile,
                            projectHomeSurface.id,
                            definition,
                            true,
                          ))}
                          className="flex min-h-9 items-center gap-2 rounded-md px-2 text-left text-xs hover:bg-gray-50 dark:hover:bg-gray-800"
                        >
                          <Plus className="h-3.5 w-3.5 text-gray-400" />
                          <span>{definition.title}</span>
                        </button>
                      ))}
                  </div>
                </div>
              </div>
            )}
          </section>

          <section className="rounded-md border border-gray-200 bg-gray-50 p-3 dark:border-gray-700 dark:bg-gray-800/40">
            <p className="text-xs font-semibold text-gray-700 dark:text-gray-200">布局预览</p>
            <div className="mt-3 overflow-hidden rounded-md border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
              <div className="flex h-8 items-center gap-1 border-b border-gray-200 px-2 dark:border-gray-700">
                {(draft.shellLayout?.navigationKind ?? 'top-bar') !== 'side-bar'
                  ? navigationSurfaces.map((surface) => (
                      <span key={surface.id} className="rounded bg-gray-100 px-2 py-1 text-[10px] dark:bg-gray-800">
                        {surface.title || surface.id}
                      </span>
                    ))
                  : <span className="text-[10px] text-gray-400">侧边导航</span>}
                <div className="ml-auto flex gap-1">
                  {pinnedToolContributionIds.slice(0, 5).map((id) => (
                    <span key={id} className="h-4 w-4 rounded bg-violet-100 dark:bg-violet-950/60" title={contributionTitle(id)} />
                  ))}
                </div>
              </div>
              <div className="flex min-h-36">
                {(draft.shellLayout?.navigationKind ?? 'top-bar') === 'side-bar' ? (
                  <div className="w-20 border-r border-gray-200 p-2 dark:border-gray-700">
                    {navigationSurfaces.map((surface) => (
                      <div key={surface.id} className="mb-1 h-4 rounded bg-emerald-100 dark:bg-emerald-950/50" />
                    ))}
                  </div>
                ) : null}
                <div className="grid min-w-0 flex-1 grid-cols-3 gap-2 p-3">
                  <div className="space-y-2">
                    {widgets.filter((widget) => widget.region === 'sidebar').map((widget) => (
                      <div key={widget.id} className="h-8 rounded bg-sky-100 dark:bg-sky-950/50" title={getWidgetDefinition(widget.widget)?.title} />
                    ))}
                  </div>
                  <div className="col-span-2 space-y-2">
                    {widgets.filter((widget) => widget.region !== 'sidebar').map((widget) => (
                      <div key={widget.id} className="h-10 rounded bg-orange-100 dark:bg-orange-950/50" title={getWidgetDefinition(widget.widget)?.title} />
                    ))}
                    {widgets.length === 0 ? (
                      <div className="flex h-full items-center justify-center text-[10px] text-gray-400">
                        {currentHomeContributionId ? contributionTitle(currentHomeContributionId) : '最小安全主页'}
                      </div>
                    ) : null}
                  </div>
                </div>
              </div>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
