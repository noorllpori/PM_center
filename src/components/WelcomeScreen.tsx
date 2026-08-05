import { ProjectHomeComposition } from './project-home/ProjectHomeComposition';

interface WelcomeScreenProps {
  onOpenProject: (path: string) => Promise<void> | void;
  settingsLoaded: boolean;
}

// Compatibility facade for legacy callers. The actual home is now composed by contributions.
export function WelcomeScreen(props: WelcomeScreenProps) {
  return <ProjectHomeComposition {...props} />;
}
