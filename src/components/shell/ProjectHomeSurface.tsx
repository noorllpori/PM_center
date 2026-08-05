import type { ProfileSurface, WorkspaceProfileV1 } from '../../types/platform';
import { ProjectHomeComposition } from '../project-home/ProjectHomeComposition';
import { useShellHomeHost } from './ShellHomeHostContext';

export function ProjectHomeSurface({
  profile,
  profileSurface,
}: {
  isActive?: boolean;
  profile?: WorkspaceProfileV1;
  profileSurface?: ProfileSurface;
}) {
  const { onOpenProject, settingsLoaded } = useShellHomeHost();
  return (
    <ProjectHomeComposition
      onOpenProject={onOpenProject}
      settingsLoaded={settingsLoaded}
      profile={profile}
      profileSurface={profileSurface}
    />
  );
}
