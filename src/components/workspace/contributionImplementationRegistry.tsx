import type { ComponentType } from 'react';
import type { ProfileSurface, WorkspaceProfileV1 } from '../../types/platform';
import type { ContributionImplementationInventory } from '../../features/contributionCatalogDiagnostics';
import { getContributionCommandHandlerIds } from '../../features/contributionCommands';
import { getContributionDataSourceReaderIds } from '../../features/contributionDataSources';
import { SURFACE_CONTRIBUTIONS } from '../../features/contributionRegistry';
import { CacheManagerSurface } from '../file-manager/CacheManagerSurface';
import { LanProjectPlaceholderSurface } from '../file-manager/LanProjectPlaceholderSurface';
import { RenderCenterSurface } from '../file-manager/RenderCenterSurface';
import { LanCollaborationSurface } from '../lan/LanCollaborationSurface';
import { ProjectHomeSurface } from '../shell/ProjectHomeSurface';
import { ProjectWorkspace } from '../file-manager/ProjectWorkspace';
import { ContributionDiagnosticsSurface } from './ContributionDiagnosticsSurface';
import { getContributedWidgetRendererIds } from './ContributedWidget';
import { getSettingsSectionImplementationIds } from '../settings/settingsContributionImplementationRegistry';

export interface ContributionSurfaceRendererProps {
  isActive: boolean;
  profile?: WorkspaceProfileV1;
  profileSurface?: ProfileSurface;
}

export const WORKSPACE_SURFACE_RENDERERS: Record<
  string,
  ComponentType<ContributionSurfaceRendererProps>
> = {
  [SURFACE_CONTRIBUTIONS.cacheManager.id]: CacheManagerSurface,
  [SURFACE_CONTRIBUTIONS.renderCenter.id]: RenderCenterSurface,
  [SURFACE_CONTRIBUTIONS.lanProject.id]: () => <LanProjectPlaceholderSurface />,
  [SURFACE_CONTRIBUTIONS.diagnosticSample.id]: ({ isActive }) => (
    <ContributionDiagnosticsSurface
      isActive={isActive}
      implementationInventory={getContributionImplementationInventory()}
    />
  ),
};

export const SHELL_SURFACE_RENDERERS: Record<
  string,
  ComponentType<ContributionSurfaceRendererProps>
> = {
  [SURFACE_CONTRIBUTIONS.projectHome.id]: ProjectHomeSurface,
  [SURFACE_CONTRIBUTIONS.projectWorkspace.id]: ProjectWorkspace,
  [SURFACE_CONTRIBUTIONS.lanMain.id]: LanCollaborationSurface,
};

export function getContributionImplementationInventory(): ContributionImplementationInventory {
  return {
    workspaceSurfaceRendererIds: Object.keys(WORKSPACE_SURFACE_RENDERERS),
    shellSurfaceRendererIds: Object.keys(SHELL_SURFACE_RENDERERS),
    widgetRendererIds: getContributedWidgetRendererIds(),
    dataSourceReaderIds: getContributionDataSourceReaderIds(),
    commandHandlerIds: getContributionCommandHandlerIds(),
    settingsSectionRendererIds: getSettingsSectionImplementationIds(),
  };
}
