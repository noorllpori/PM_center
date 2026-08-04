import type { ComponentType } from 'react';
import {
  WORKSPACE_TAB_CONTRIBUTION_BY_ID,
  isWorkspaceTabContributionAvailable,
} from '../../features/contributionRegistry';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';
import type { WorkspaceTab } from '../../stores/workspaceTabStore';
import { CacheManagerSurface } from '../file-manager/CacheManagerSurface';
import { LanProjectPlaceholderSurface } from '../file-manager/LanProjectPlaceholderSurface';
import { RenderCenterSurface } from '../file-manager/RenderCenterSurface';
import { ContributionDiagnosticsSurface } from './ContributionDiagnosticsSurface';

interface WorkspaceContributionSurfaceProps {
  isActive: boolean;
}

const SURFACE_RENDERERS: Record<string, ComponentType<WorkspaceContributionSurfaceProps>> = {
  'builtin.project-resources.cache-surface': CacheManagerSurface,
  'builtin.render-center.surface': RenderCenterSurface,
  'builtin.lan-collaboration.project-surface': () => <LanProjectPlaceholderSurface />,
  'diagnostic.contribution-sample.surface': () => <ContributionDiagnosticsSurface />,
};

export function ContributedWorkspaceSurface({
  tab,
  isActive,
}: {
  tab: WorkspaceTab;
  isActive: boolean;
}) {
  const snapshot = useContributionRegistryStore((state) => state.snapshot);
  if (!tab.contributionId) {
    return null;
  }

  const definition = WORKSPACE_TAB_CONTRIBUTION_BY_ID.get(tab.contributionId);
  if (!definition || !isWorkspaceTabContributionAvailable(snapshot, definition)) {
    return null;
  }

  const Surface = SURFACE_RENDERERS[definition.surfaceId];
  return Surface ? <Surface isActive={isActive} /> : null;
}
