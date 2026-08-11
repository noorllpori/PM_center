import type {
  ProfileSurface,
  ScriptSurfaceContribution,
  WorkspaceProfileV1,
} from '../types/platform';
import {
  SURFACE_CONTRIBUTION_BY_ID,
  SURFACE_CONTRIBUTIONS,
  getContributionUnavailableReason,
  type ContributionRegistrySnapshot,
  type SurfaceContributionDefinition,
} from './contributionRegistry';

export type ProfileHomeFallbackCode =
  | 'PROFILE_LOADING'
  | 'HOME_NOT_CONFIGURED'
  | 'HOME_SURFACE_MISSING'
  | 'HOME_SURFACE_KIND_UNSUPPORTED'
  | 'HOME_CONTRIBUTION_MISSING'
  | 'HOME_CONTRIBUTION_UNKNOWN'
  | 'HOME_CONTRIBUTION_HOST_UNSUPPORTED'
  | 'HOME_CONTRIBUTION_UNAVAILABLE'
  | 'HOME_RENDERER_MISSING';

export interface ResolvedProfileHome {
  kind: 'resolved';
  profile: WorkspaceProfileV1;
  surface: ProfileSurface;
  contribution: SurfaceContributionDefinition;
}

export interface ProfileHomeScriptSurfaceEntry {
  componentId: string;
  componentName: string;
  surface: ScriptSurfaceContribution;
}

export interface ResolvedProfileScriptHome {
  kind: 'script-surface';
  profile: WorkspaceProfileV1;
  surface: ProfileSurface;
  componentId: string;
  scriptSurface: ScriptSurfaceContribution;
}

export interface FallbackProfileHome {
  kind: 'fallback';
  profile: WorkspaceProfileV1 | null;
  code: ProfileHomeFallbackCode;
  message: string;
  requestedSurfaceId?: string;
  contributionId?: string;
}

export type ProfileHomeResolution = ResolvedProfileHome | ResolvedProfileScriptHome | FallbackProfileHome;

const SUPPORTED_HOME_SURFACE_KINDS = new Set<ProfileSurface['kind']>([
  'dashboard',
  'shell-page',
]);

export function resolveProfileHome(
  profile: WorkspaceProfileV1 | null,
  contributionRegistry: ContributionRegistrySnapshot,
  shellRendererIds: ReadonlySet<string>,
  scriptSurfaceEntries: readonly ProfileHomeScriptSurfaceEntry[] = [],
  scriptSurfaceCatalogLoading = false,
): ProfileHomeResolution {
  if (!profile) {
    return fallback(null, 'PROFILE_LOADING', '正在读取当前装配方案。');
  }

  const requestedSurfaceId = profile.shellLayout?.home?.trim();
  if (!requestedSurfaceId) {
    const contribution = SURFACE_CONTRIBUTIONS.nexoraWelcome;
    const unavailableReason = getContributionUnavailableReason(
      contributionRegistry,
      contribution,
    );
    if (!unavailableReason && shellRendererIds.has(contribution.id)) {
      return {
        kind: 'resolved',
        profile,
        surface: {
          id: 'nexora-default-welcome',
          title: contribution.title,
          kind: 'shell-page',
          layout: 'contribution-defined',
          contribution: contribution.id,
          widgets: [],
          settings: {},
        },
        contribution,
      };
    }
    return fallback(
      profile,
      'HOME_NOT_CONFIGURED',
      '当前装配方案没有指定默认主页。',
    );
  }

  const surface = (profile.surfaces ?? []).find((item) => item.id === requestedSurfaceId);
  if (!surface) {
    return fallback(
      profile,
      'HOME_SURFACE_MISSING',
      `默认主页引用的 Surface 不存在：${requestedSurfaceId}`,
      requestedSurfaceId,
    );
  }

  if (!SUPPORTED_HOME_SURFACE_KINDS.has(surface.kind)) {
    return fallback(
      profile,
      'HOME_SURFACE_KIND_UNSUPPORTED',
      `Surface“${surface.title || surface.id}”不是可作为主页的类型。`,
      requestedSurfaceId,
    );
  }

  const contributionId = surface.contribution?.trim();
  if (!contributionId) {
    return fallback(
      profile,
      'HOME_CONTRIBUTION_MISSING',
      `Surface“${surface.title || surface.id}”尚未绑定可渲染的主页贡献。`,
      requestedSurfaceId,
    );
  }

  const contribution = SURFACE_CONTRIBUTION_BY_ID.get(contributionId);
  if (!contribution) {
    const configuredComponentId = typeof surface.settings?.componentId === 'string'
      ? surface.settings.componentId
      : null;
    const configuredSurfaceId = typeof surface.settings?.scriptSurfaceId === 'string'
      ? surface.settings.scriptSurfaceId
      : contributionId;
    const scriptEntry = scriptSurfaceEntries.find((entry) => (
      entry.surface.id === configuredSurfaceId
      && (!configuredComponentId || entry.componentId === configuredComponentId)
    ));
    if (!scriptEntry) {
      if (scriptSurfaceCatalogLoading) {
        return fallback(
          profile,
          'PROFILE_LOADING',
          '正在读取组件主页目录。',
          requestedSurfaceId,
          contributionId,
        );
      }
      return fallback(
        profile,
        'HOME_CONTRIBUTION_UNKNOWN',
        `主页组件页面未安装、未启用或未进入当前方案：${contributionId}`,
        requestedSurfaceId,
        contributionId,
      );
    }
    if (!scriptEntry.surface.placements.includes('shell')) {
      return fallback(
        profile,
        'HOME_CONTRIBUTION_HOST_UNSUPPORTED',
        `组件页面“${scriptEntry.surface.name}”未声明 shell 放置，不能作为主页。`,
        requestedSurfaceId,
        contributionId,
      );
    }
    return {
      kind: 'script-surface',
      profile,
      surface,
      componentId: scriptEntry.componentId,
      scriptSurface: scriptEntry.surface,
    };
  }

  if (contribution.host !== 'shell') {
    return fallback(
      profile,
      'HOME_CONTRIBUTION_HOST_UNSUPPORTED',
      `贡献“${contribution.title}”不能在主 Shell 中作为主页显示。`,
      requestedSurfaceId,
      contributionId,
    );
  }

  const unavailableReason = getContributionUnavailableReason(
    contributionRegistry,
    contribution,
  );
  if (unavailableReason) {
    return fallback(
      profile,
      'HOME_CONTRIBUTION_UNAVAILABLE',
      `主页贡献“${contribution.title}”当前不可用：${unavailableReason}`,
      requestedSurfaceId,
      contributionId,
    );
  }

  if (!shellRendererIds.has(contributionId)) {
    return fallback(
      profile,
      'HOME_RENDERER_MISSING',
      `主页贡献“${contribution.title}”缺少前端渲染器。`,
      requestedSurfaceId,
      contributionId,
    );
  }

  return {
    kind: 'resolved',
    profile,
    surface,
    contribution,
  };
}

function fallback(
  profile: WorkspaceProfileV1 | null,
  code: ProfileHomeFallbackCode,
  message: string,
  requestedSurfaceId?: string,
  contributionId?: string,
): FallbackProfileHome {
  return {
    kind: 'fallback',
    profile,
    code,
    message,
    requestedSurfaceId,
    contributionId,
  };
}
