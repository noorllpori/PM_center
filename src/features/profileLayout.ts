import type {
  ContributionRegistrySnapshot,
  DataSourceContributionDefinition,
  ShellTabContributionDefinition,
  SurfaceContributionDefinition,
  WidgetContributionDefinition,
} from './contributionRegistry';
import {
  DATA_SOURCE_CONTRIBUTION_BY_ID,
  SHELL_TAB_CONTRIBUTIONS,
  SURFACE_CONTRIBUTION_BY_ID,
  SURFACE_CONTRIBUTIONS,
  WIDGET_CONTRIBUTION_BY_ID,
  getContributionUnavailableReason,
} from './contributionRegistry';
import type {
  DataSourceScope,
  ModuleManifestV1,
  ProfileDataSource,
  ProfileSurface,
  ProfileWidget,
  WorkspaceProfileV1,
} from '../types/platform';

export const PROFILE_LAYOUT_EDITOR_VERSION = 1;

const PROJECT_HOME_SURFACE_ID = 'pm-center-project-home';

interface ProjectHomeWidgetBlueprint {
  localId: string;
  widgetId: string;
  dataSourceLocalId: string;
  dataSourceId: string;
  dataSourceScope: DataSourceScope;
  region: 'sidebar' | 'content';
}

const PROJECT_HOME_WIDGET_BLUEPRINTS: ProjectHomeWidgetBlueprint[] = [
  {
    localId: 'project-directory',
    widgetId: 'builtin.project-manager.project-directory-widget',
    dataSourceLocalId: 'project-directory-state',
    dataSourceId: 'builtin.project-manager.project-directory-data-source',
    dataSourceScope: 'global',
    region: 'sidebar',
  },
  {
    localId: 'quick-actions',
    widgetId: 'builtin.project-manager.quick-actions-widget',
    dataSourceLocalId: 'project-quick-actions-state',
    dataSourceId: 'builtin.project-manager.quick-actions-data-source',
    dataSourceScope: 'surface',
    region: 'sidebar',
  },
  {
    localId: 'recent-projects',
    widgetId: 'builtin.project-manager.recent-projects-widget',
    dataSourceLocalId: 'recent-projects-state',
    dataSourceId: 'builtin.project-manager.recent-projects-data-source',
    dataSourceScope: 'global',
    region: 'content',
  },
  {
    localId: 'project-catalog',
    widgetId: 'builtin.project-manager.project-catalog-widget',
    dataSourceLocalId: 'project-catalog-state',
    dataSourceId: 'builtin.project-manager.project-catalog-data-source',
    dataSourceScope: 'surface',
    region: 'content',
  },
];

export interface ResolvedProfileNavigationItem {
  surfaceId: string;
  contributionId: string | null;
  title: string;
  tabContribution: ShellTabContributionDefinition | null;
  unavailableReason: string | null;
}

function selectedModuleIds(profile: WorkspaceProfileV1) {
  return new Set((profile.enabledModules ?? []).map((selection) => selection.id));
}

export function getSelectedModuleContributionIds(
  profile: WorkspaceProfileV1,
  modules: ModuleManifestV1[],
  kind: keyof NonNullable<ModuleManifestV1['contributes']>,
) {
  const selectedIds = selectedModuleIds(profile);
  const contributionIds = new Set<string>();
  modules.forEach((manifest) => {
    if (!selectedIds.has(manifest.id)) return;
    const values = manifest.contributes?.[kind];
    if (!Array.isArray(values)) return;
    values.forEach((value) => {
      if (typeof value === 'string') contributionIds.add(value);
    });
  });
  return contributionIds;
}

function surfaceLocalId(contributionId: string) {
  if (contributionId === SURFACE_CONTRIBUTIONS.projectHome.id) {
    return PROJECT_HOME_SURFACE_ID;
  }
  return contributionId
    .replace(/^(builtin|core)\./, '')
    .replace(/\./g, '-')
    .replace(/-surface$/, '')
    .slice(0, 80);
}

function uniqueLocalId(existingIds: Set<string>, preferred: string) {
  if (!existingIds.has(preferred)) return preferred;
  let suffix = 2;
  while (existingIds.has(`${preferred}-${suffix}`)) suffix += 1;
  return `${preferred}-${suffix}`;
}

function createSurface(definition: SurfaceContributionDefinition, id: string): ProfileSurface {
  return {
    id,
    title: definition.title,
    kind: definition.id === SURFACE_CONTRIBUTIONS.projectHome.id ? 'dashboard' : 'shell-page',
    layout: 'contribution-defined',
    contribution: definition.id,
    widgets: [],
    settings: {},
  };
}

export function ensureProfileSurface(
  profile: WorkspaceProfileV1,
  contributionId: string,
) {
  const existing = (profile.surfaces ?? []).find(
    (surface) => surface.contribution === contributionId,
  );
  if (existing) return existing;

  const definition = SURFACE_CONTRIBUTION_BY_ID.get(contributionId);
  if (!definition) return null;
  const surfaces = profile.surfaces ?? [];
  const id = uniqueLocalId(new Set(surfaces.map((surface) => surface.id)), surfaceLocalId(contributionId));
  const surface = createSurface(definition, id);
  profile.surfaces = [...surfaces, surface];
  return surface;
}

export function setProfileHomeContribution(
  profile: WorkspaceProfileV1,
  contributionId: string | null,
) {
  const shellLayout = { ...(profile.shellLayout ?? {}) };
  if (!contributionId) {
    delete shellLayout.home;
    profile.shellLayout = shellLayout;
    return;
  }
  const surface = ensureProfileSurface(profile, contributionId);
  if (!surface) return;
  shellLayout.home = surface.id;
  profile.shellLayout = shellLayout;
}

export function setProfileNavigationContribution(
  profile: WorkspaceProfileV1,
  contributionId: string,
  enabled: boolean,
) {
  const surface = ensureProfileSurface(profile, contributionId);
  if (!surface) return;
  const current = profile.shellLayout?.navigation ?? [];
  profile.shellLayout = {
    ...(profile.shellLayout ?? {}),
    navigation: enabled
      ? current.includes(surface.id) ? current : [...current, surface.id]
      : current.filter((surfaceId) => surfaceId !== surface.id),
  };
}

export function reorderProfileNavigation(
  profile: WorkspaceProfileV1,
  surfaceId: string,
  beforeSurfaceId: string | null,
) {
  const current = profile.shellLayout?.navigation ?? [];
  if (!current.includes(surfaceId) || surfaceId === beforeSurfaceId) return;
  const next = current.filter((value) => value !== surfaceId);
  const index = beforeSurfaceId ? next.indexOf(beforeSurfaceId) : next.length;
  next.splice(index < 0 ? next.length : index, 0, surfaceId);
  profile.shellLayout = { ...(profile.shellLayout ?? {}), navigation: next };
}

export function reorderPinnedTools(
  profile: WorkspaceProfileV1,
  contributionId: string,
  beforeContributionId: string | null,
) {
  const current = profile.shellLayout?.pinnedTools ?? [];
  if (!current.includes(contributionId) || contributionId === beforeContributionId) return;
  const next = current.filter((value) => value !== contributionId);
  const index = beforeContributionId ? next.indexOf(beforeContributionId) : next.length;
  next.splice(index < 0 ? next.length : index, 0, contributionId);
  profile.shellLayout = { ...(profile.shellLayout ?? {}), pinnedTools: next };
}

export function setPinnedToolContribution(
  profile: WorkspaceProfileV1,
  contributionId: string,
  enabled: boolean,
) {
  const current = profile.shellLayout?.pinnedTools ?? [];
  profile.shellLayout = {
    ...(profile.shellLayout ?? {}),
    pinnedTools: enabled
      ? current.includes(contributionId) ? current : [...current, contributionId]
      : current.filter((value) => value !== contributionId),
  };
}

function blueprintForWidget(widgetId: string) {
  return PROJECT_HOME_WIDGET_BLUEPRINTS.find((blueprint) => blueprint.widgetId === widgetId);
}

function ensureDataSource(
  profile: WorkspaceProfileV1,
  blueprint: ProjectHomeWidgetBlueprint,
) {
  const existing = (profile.dataSources ?? []).find(
    (source) => source.source === blueprint.dataSourceId,
  );
  if (existing) return existing;
  const source: ProfileDataSource = {
    id: uniqueLocalId(
      new Set((profile.dataSources ?? []).map((candidate) => candidate.id)),
      blueprint.dataSourceLocalId,
    ),
    source: blueprint.dataSourceId,
    scope: blueprint.dataSourceScope,
    settings: {},
  };
  profile.dataSources = [...(profile.dataSources ?? []), source];
  return source;
}

function normalizeWidgetOrders(surface: ProfileSurface) {
  const regions = ['sidebar', 'content'] as const;
  regions.forEach((region) => {
    (surface.widgets ?? [])
      ?.filter((widget) => (widget.region === 'sidebar' ? 'sidebar' : 'content') === region)
      .sort((left, right) => (left.order ?? 0) - (right.order ?? 0) || left.id.localeCompare(right.id))
      .forEach((widget, index) => {
        widget.order = index;
      });
  });
}

export function setProfileWidgetContribution(
  profile: WorkspaceProfileV1,
  surfaceId: string,
  widgetDefinition: WidgetContributionDefinition,
  enabled: boolean,
) {
  const surface = (profile.surfaces ?? []).find((candidate) => candidate.id === surfaceId);
  if (!surface) return;
  surface.settings = {
    ...(surface.settings ?? {}),
    widgetsConfigured: true,
    layoutEditorVersion: PROFILE_LAYOUT_EDITOR_VERSION,
  };
  const widgets = surface.widgets ?? [];
  if (!enabled) {
    surface.widgets = widgets.filter((widget) => widget.widget !== widgetDefinition.id);
    normalizeWidgetOrders(surface);
    return;
  }
  if (widgets.some((widget) => widget.widget === widgetDefinition.id)) return;

  const blueprint = blueprintForWidget(widgetDefinition.id);
  const dataSourceDefinition = widgetDefinition.dataSourceId
    ? DATA_SOURCE_CONTRIBUTION_BY_ID.get(widgetDefinition.dataSourceId)
    : null;
  let dataSourceLocalId: string | undefined;
  if (blueprint) {
    dataSourceLocalId = ensureDataSource(profile, blueprint).id;
  } else if (dataSourceDefinition) {
    dataSourceLocalId = ensureGenericDataSource(profile, dataSourceDefinition).id;
  }
  const region = blueprint?.region ?? 'content';
  const widget: ProfileWidget = {
    id: uniqueLocalId(
      new Set(widgets.map((candidate) => candidate.id)),
      blueprint?.localId ?? widgetDefinition.id.replace(/\./g, '-').replace(/-widget$/, ''),
    ),
    widget: widgetDefinition.id,
    dataSource: dataSourceLocalId,
    region,
    order: widgets.filter((candidate) => candidate.region === region).length,
    settings: {},
  };
  surface.widgets = [...widgets, widget];
  normalizeWidgetOrders(surface);
}

function ensureGenericDataSource(
  profile: WorkspaceProfileV1,
  definition: DataSourceContributionDefinition,
) {
  const existing = (profile.dataSources ?? []).find((source) => source.source === definition.id);
  if (existing) return existing;
  const source: ProfileDataSource = {
    id: uniqueLocalId(
      new Set((profile.dataSources ?? []).map((candidate) => candidate.id)),
      definition.id.replace(/\./g, '-').replace(/-data-source$/, '-state'),
    ),
    source: definition.id,
    scope: definition.scope,
    settings: {},
  };
  profile.dataSources = [...(profile.dataSources ?? []), source];
  return source;
}

export function updateProfileWidgetRegion(
  profile: WorkspaceProfileV1,
  surfaceId: string,
  widgetLocalId: string,
  region: 'sidebar' | 'content',
) {
  const surface = (profile.surfaces ?? []).find((candidate) => candidate.id === surfaceId);
  const widget = surface?.widgets?.find((candidate) => candidate.id === widgetLocalId);
  if (!surface || !widget) return;
  widget.region = region;
  widget.order = (surface.widgets ?? []).filter((candidate) => candidate.region === region).length;
  surface.settings = {
    ...(surface.settings ?? {}),
    widgetsConfigured: true,
    layoutEditorVersion: PROFILE_LAYOUT_EDITOR_VERSION,
  };
  normalizeWidgetOrders(surface);
}

export function reorderProfileWidgets(
  profile: WorkspaceProfileV1,
  surfaceId: string,
  widgetLocalId: string,
  beforeWidgetLocalId: string | null,
) {
  const surface = (profile.surfaces ?? []).find((candidate) => candidate.id === surfaceId);
  if (!surface?.widgets?.some((widget) => widget.id === widgetLocalId)) return;
  const moved = surface.widgets.find((widget) => widget.id === widgetLocalId)!;
  const sameRegion = surface.widgets
    .filter((widget) => widget.region === moved.region)
    .sort((left, right) => (left.order ?? 0) - (right.order ?? 0));
  const next = sameRegion.filter((widget) => widget.id !== widgetLocalId);
  const index = beforeWidgetLocalId
    ? next.findIndex((widget) => widget.id === beforeWidgetLocalId)
    : next.length;
  next.splice(index < 0 ? next.length : index, 0, moved);
  next.forEach((widget, order) => {
    widget.order = order;
  });
  surface.settings = {
    ...(surface.settings ?? {}),
    widgetsConfigured: true,
    layoutEditorVersion: PROFILE_LAYOUT_EDITOR_VERSION,
  };
}

export function removeModuleOwnedLayout(
  profile: WorkspaceProfileV1,
  removedManifests: ModuleManifestV1[],
) {
  const removedSurfaceContributions = new Set(
    removedManifests.flatMap((manifest) => manifest.contributes?.surfaces ?? []),
  );
  const removedWidgetContributions = new Set(
    removedManifests.flatMap((manifest) => manifest.contributes?.widgets ?? []),
  );
  const removedDataSourceContributions = new Set(
    removedManifests.flatMap((manifest) => manifest.contributes?.dataSources ?? []),
  );
  const removedCommandContributions = new Set(
    removedManifests.flatMap((manifest) => manifest.contributes?.commands ?? []),
  );
  const removedToolContributions = new Set(
    removedManifests.flatMap((manifest) => manifest.contributes?.tools ?? []),
  );

  const removedSurfaceIds = new Set(
    (profile.surfaces ?? [])
      .filter((surface) => surface.contribution && removedSurfaceContributions.has(surface.contribution))
      .map((surface) => surface.id),
  );
  profile.surfaces = (profile.surfaces ?? [])
    .filter((surface) => !removedSurfaceIds.has(surface.id))
    .map((surface) => ({
      ...surface,
      widgets: (surface.widgets ?? []).filter(
        (widget) => !removedWidgetContributions.has(widget.widget),
      ),
    }));
  profile.dataSources = (profile.dataSources ?? []).filter(
    (source) => !removedDataSourceContributions.has(source.source),
  );
  profile.commandBindings = (profile.commandBindings ?? []).filter(
    (binding) => !removedCommandContributions.has(binding.command)
      && (!binding.surface || !removedSurfaceIds.has(binding.surface)),
  );
  profile.shellLayout = {
    ...(profile.shellLayout ?? {}),
    home: profile.shellLayout?.home && !removedSurfaceIds.has(profile.shellLayout.home)
      ? profile.shellLayout.home
      : undefined,
    navigation: (profile.shellLayout?.navigation ?? []).filter(
      (surfaceId) => !removedSurfaceIds.has(surfaceId),
    ),
    pinnedTools: (profile.shellLayout?.pinnedTools ?? []).filter(
      (contributionId) => !removedToolContributions.has(contributionId),
    ),
  };
}

export function resolveProfileNavigation(
  profile: WorkspaceProfileV1 | null,
  registry: ContributionRegistrySnapshot,
): ResolvedProfileNavigationItem[] {
  if (!profile) return [];
  const homeSurfaceId = profile.shellLayout?.home;
  const surfaceById = new Map((profile.surfaces ?? []).map((surface) => [surface.id, surface]));
  return (profile.shellLayout?.navigation ?? []).map((surfaceId) => {
    const surface = surfaceById.get(surfaceId);
    const contributionId = surface?.contribution ?? null;
    const definition = contributionId ? SURFACE_CONTRIBUTION_BY_ID.get(contributionId) : null;
    const tabContribution = contributionId
      ? Object.values(SHELL_TAB_CONTRIBUTIONS).find(
          (candidate) => candidate.surfaceId === contributionId && candidate.instanceMode === 'singleton',
        ) ?? null
      : null;
    let unavailableReason: string | null = null;
    if (!surface) {
      unavailableReason = `导航页面不存在：${surfaceId}`;
    } else if (!contributionId) {
      unavailableReason = `导航页面“${surface.title || surface.id}”没有绑定贡献`;
    } else if (!definition) {
      unavailableReason = `导航贡献未安装：${contributionId}`;
    } else if (surfaceId === homeSurfaceId) {
      unavailableReason = getContributionUnavailableReason(registry, definition);
    } else if (!tabContribution) {
      unavailableReason = `导航页面“${definition.title}”没有单例 Shell 标签入口`;
    } else {
      unavailableReason = getContributionUnavailableReason(registry, tabContribution);
    }
    return {
      surfaceId,
      contributionId,
      title: surface?.title || definition?.title || surfaceId,
      tabContribution,
      unavailableReason,
    };
  });
}

export function getWidgetDefinition(widgetId: string) {
  return WIDGET_CONTRIBUTION_BY_ID.get(widgetId) ?? null;
}
