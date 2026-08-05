import { WelcomeScreen } from '../WelcomeScreen';
import { useShellHomeHost } from './ShellHomeHostContext';

export function ProjectHomeSurface() {
  const { onOpenProject, settingsLoaded } = useShellHomeHost();
  return (
    <WelcomeScreen
      onOpenProject={onOpenProject}
      settingsLoaded={settingsLoaded}
    />
  );
}
