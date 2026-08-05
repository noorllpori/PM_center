import { useMemo } from 'react';
import { resolveProfileHome } from '../../features/profileHome';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import { SHELL_SURFACE_RENDERERS } from '../workspace/contributionImplementationRegistry';
import { MinimalSafeHome } from './MinimalSafeHome';
import { ShellHomeHostProvider } from './ShellHomeHostContext';

const SHELL_HOME_RENDERER_IDS = new Set(Object.keys(SHELL_SURFACE_RENDERERS));

export function ProfileHomeSurface({
  onOpenProject,
  settingsLoaded,
  onOpenSettings,
}: {
  onOpenProject: (path: string) => Promise<void> | void;
  settingsLoaded: boolean;
  onOpenSettings: () => void;
}) {
  const profile = useWorkspaceProfileStore((state) => state.snapshot?.currentProfile ?? null);
  const runtimeError = useWorkspaceProfileStore((state) => state.error);
  const contributionRegistry = useContributionRegistryStore((state) => state.snapshot);
  const resolution = useMemo(
    () => resolveProfileHome(profile, contributionRegistry, SHELL_HOME_RENDERER_IDS),
    [contributionRegistry, profile],
  );

  if (resolution.kind === 'fallback') {
    return (
      <MinimalSafeHome
        resolution={resolution}
        runtimeError={runtimeError}
        onOpenSettings={onOpenSettings}
      />
    );
  }

  const Surface = SHELL_SURFACE_RENDERERS[resolution.contribution.id];
  return (
    <ShellHomeHostProvider value={{ onOpenProject, settingsLoaded }}>
      <Surface isActive />
    </ShellHomeHostProvider>
  );
}
