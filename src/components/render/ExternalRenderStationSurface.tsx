import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  createProjectStore,
  ProjectStoreProvider,
  type ProjectStoreApi,
} from '../../stores/projectStore';
import { RenderCenterSurface } from '../file-manager/RenderCenterSurface';

interface ExternalRenderStationContext {
  storagePath: string;
  defaultOutputRoot: string;
  displayName: string;
}

function createExternalStationStore(): ProjectStoreApi {
  return createProjectStore();
}

/**
 * Reuses the render-center UI with a private queue namespace.  Deliberately do
 * not call ProjectStore.setProject here: that would initialize a watcher,
 * project registry and file tree for a render station that is not a project.
 */
export function ExternalRenderStationSurface({ isActive }: { isActive: boolean }) {
  const [store] = useState(createExternalStationStore);
  const [context, setContext] = useState<ExternalRenderStationContext | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [requestKey, setRequestKey] = useState(0);

  useEffect(() => {
    let disposed = false;
    setContext(null);
    setError(null);
    void invoke<ExternalRenderStationContext>('get_external_render_station_context')
      .then((next) => {
        if (disposed) return;
        store.setState({
          projectPath: next.storagePath,
          projectName: next.displayName,
          isInitialized: true,
          currentPath: next.storagePath,
        });
        setContext(next);
      })
      .catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => { disposed = true; };
  }, [requestKey, store]);

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <p className="text-sm font-medium text-red-600 dark:text-red-400">外部渲染器初始化失败</p>
        <p className="max-w-xl break-words text-xs text-gray-500">{error}</p>
        <button
          type="button"
          onClick={() => setRequestKey((value) => value + 1)}
          className="h-8 rounded border border-gray-300 px-3 text-xs hover:bg-gray-100 dark:border-gray-700 dark:hover:bg-gray-800"
        >
          重试
        </button>
      </div>
    );
  }

  if (!context) {
    return <div className="flex h-full items-center justify-center text-sm text-gray-500">正在初始化外部渲染器...</div>;
  }

  return (
    <ProjectStoreProvider store={store}>
      <RenderCenterSurface
        isActive={isActive}
        stationKind="external"
        defaultOutputRoot={context.defaultOutputRoot}
      />
    </ProjectStoreProvider>
  );
}
