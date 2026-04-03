import type { ReactNode } from 'react';
import { ProjectStoreApi, ProjectStoreProvider } from '../../stores/projectStore';
import { WorkspaceTabStoreApi, WorkspaceTabStoreProvider } from '../../stores/workspaceTabStore';

interface ProjectSessionProviderProps {
  projectStore: ProjectStoreApi;
  workspaceTabStore: WorkspaceTabStoreApi;
  children: ReactNode;
}

export function ProjectSessionProvider({
  projectStore,
  workspaceTabStore,
  children,
}: ProjectSessionProviderProps) {
  return (
    <ProjectStoreProvider store={projectStore}>
      <WorkspaceTabStoreProvider store={workspaceTabStore}>
        {children}
      </WorkspaceTabStoreProvider>
    </ProjectStoreProvider>
  );
}
