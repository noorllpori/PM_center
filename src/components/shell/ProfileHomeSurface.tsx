import { useEffect, useMemo, useState } from 'react';
import { resolveProfileHome } from '../../features/profileHome';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import { useAutomationStore } from '../../stores/automationStore';
import { ScriptSurfaceFrame } from '../automation/ScriptSurfaceFrame';
import { SHELL_SURFACE_RENDERERS } from '../workspace/contributionImplementationRegistry';
import { MinimalSafeHome } from './MinimalSafeHome';
import { ShellHomeHostProvider } from './ShellHomeHostContext';

const SHELL_HOME_RENDERER_IDS = new Set(Object.keys(SHELL_SURFACE_RENDERERS));

export function ProfileHomeSurface({
  onOpenProject,
  settingsLoaded,
  onOpenRecovery,
}: {
  onOpenProject: (path: string) => Promise<void> | void;
  settingsLoaded: boolean;
  onOpenRecovery: () => void;
}) {
  const profile = useWorkspaceProfileStore((state) => state.snapshot?.currentProfile ?? null);
  const runtimeError = useWorkspaceProfileStore((state) => state.error);
  const contributionRegistry = useContributionRegistryStore((state) => state.snapshot);
  const automationSnapshot = useAutomationStore((state) => state.snapshot);
  const automationLoading = useAutomationStore((state) => state.loading);
  const automationError = useAutomationStore((state) => state.error);
  const initializeAutomation = useAutomationStore((state) => state.initialize);
  const refreshAutomation = useAutomationStore((state) => state.refresh);
  const profileRuntimeKey = profile ? `${profile.id}:${profile.revision ?? 0}` : null;
  const [resolvedAutomationProfileKey, setResolvedAutomationProfileKey] = useState<string | null>(null);
  const scriptSurfaceEntries = useMemo(() => (
    automationSnapshot?.running
      ? automationSnapshot.availableComponents.flatMap((component) => (
        component.surfaces.map((surface) => ({
          componentId: component.componentId,
          componentName: component.componentName,
          surface,
        }))
      ))
      : []
  ), [automationSnapshot]);

  useEffect(() => {
    let disposed = false;
    setResolvedAutomationProfileKey(null);
    void initializeAutomation()
      .then(() => refreshAutomation())
      .then(() => {
        if (!disposed) setResolvedAutomationProfileKey(profileRuntimeKey);
      });
    return () => {
      disposed = true;
    };
  }, [initializeAutomation, profileRuntimeKey, refreshAutomation]);

  const resolution = useMemo(
    () => resolveProfileHome(
      profile,
      contributionRegistry,
      SHELL_HOME_RENDERER_IDS,
      scriptSurfaceEntries,
      automationLoading
        || resolvedAutomationProfileKey !== profileRuntimeKey
        || (automationSnapshot === null && !automationError),
    ),
    [
      automationError,
      automationLoading,
      automationSnapshot,
      contributionRegistry,
      profile,
      profileRuntimeKey,
      resolvedAutomationProfileKey,
      scriptSurfaceEntries,
    ],
  );

  if (resolution.kind === 'fallback') {
    return (
      <MinimalSafeHome
        resolution={resolution}
        runtimeError={runtimeError}
        onOpenRecovery={onOpenRecovery}
      />
    );
  }

  if (resolution.kind === 'script-surface') {
    return (
      <div className="h-full min-h-0 w-full overflow-hidden bg-white dark:bg-gray-950">
        <ScriptSurfaceFrame
          componentId={resolution.componentId}
          surfaceId={resolution.scriptSurface.id}
        />
      </div>
    );
  }

  const Surface = SHELL_SURFACE_RENDERERS[resolution.contribution.id];
  return (
    <ShellHomeHostProvider value={{ onOpenProject, settingsLoaded }}>
      <Surface
        isActive
        profile={resolution.profile}
        profileSurface={resolution.surface}
      />
    </ShellHomeHostProvider>
  );
}
