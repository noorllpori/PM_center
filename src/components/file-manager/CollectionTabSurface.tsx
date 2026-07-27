import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Layers, RefreshCw } from 'lucide-react';
import {
  createProjectStore,
  ProjectStoreProvider,
  useProjectStoreApi,
  useProjectStoreShallow,
  type ProjectStoreApi,
} from '../../stores/projectStore';
import { FILES_TAB_ID, useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import { useUiStore } from '../../stores/uiStore';
import { FileDetail } from './FileDetail';
import { FileList } from './FileList';

interface CollectionTabSurfaceProps {
  collectionId: string;
  projectPath: string;
  title: string;
}

function getProjectName(projectPath: string) {
  return projectPath.split(/[\\/]/).pop() || 'Project';
}

function createCollectionStore(projectPath: string, collectionPath: string): ProjectStoreApi {
  const store = createProjectStore();
  store.setState({
    projectPath,
    projectName: getProjectName(projectPath),
    isInitialized: true,
    currentPath: collectionPath,
    expandedKeys: new Set([projectPath]),
  });
  return store;
}

function CollectionDirectoryContent({
  collectionPath,
  collectionId,
  title,
}: {
  collectionPath: string;
  collectionId: string;
  title: string;
}) {
  const projectStore = useProjectStoreApi();
  const { currentPath, files } = useProjectStoreShallow((state) => ({
    currentPath: state.currentPath,
    files: state.files,
  }));
  const activateTab = useWorkspaceTabStore((state) => state.activateTab);
  const showToast = useUiStore((state) => state.showToast);
  const atCollectionRoot = currentPath === collectionPath;

  const handleBack = useCallback(() => {
    if (atCollectionRoot) {
      activateTab(FILES_TAB_ID);
      return;
    }
    void projectStore.getState().loadDirectory(collectionPath);
  }, [activateTab, atCollectionRoot, collectionPath, projectStore]);

  const handleRefresh = useCallback(() => {
    void projectStore.getState().refresh(true, true);
  }, [projectStore]);

  const handleRemoveFromCollection = useCallback(
    async (memberPaths: string[]) => {
      if (memberPaths.length === 0) {
        return;
      }

      const result = await invoke<{
        removed_count: number;
        not_found_count: number;
      }>('remove_collection_items', {
        projectPath: projectStore.getState().projectPath,
        collectionId,
        memberPaths,
      });
      await projectStore.getState().loadDirectory(collectionPath, true);

      if (result.removed_count > 0) {
        showToast({
          title: '已从集合中移除',
          message: result.removed_count === 1
            ? '已移除 1 个项目，磁盘文件未删除。'
            : `已移除 ${result.removed_count} 个项目，磁盘文件未删除。`,
          tone: 'success',
        });
      } else {
        showToast({
          title: '集合没有变化',
          message: '选中的项目已不在此集合中。',
          tone: 'warning',
        });
      }
    },
    [collectionId, collectionPath, projectStore, showToast],
  );

  return (
    <div className="flex h-full min-h-0 flex-col bg-white dark:bg-gray-900">
      <div className="flex shrink-0 items-center gap-2 border-b border-gray-200 px-3 py-2 dark:border-gray-700">
        <button
          type="button"
          onClick={handleBack}
          className="rounded-md p-1.5 text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
          title={atCollectionRoot ? '返回项目根目录' : '返回集合根目录'}
        >
          <ArrowLeft className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={handleRefresh}
          className="rounded-md p-1.5 text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
          title="刷新"
        >
          <RefreshCw className="h-4 w-4" />
        </button>
        <Layers className="h-4 w-4 shrink-0 text-violet-500" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-semibold text-gray-900 dark:text-gray-100">
            {atCollectionRoot ? title : currentPath?.split(/[\\/]/).pop() || title}
          </div>
          <div className="truncate text-xs text-gray-500 dark:text-gray-400">
            {atCollectionRoot ? `项目根目录 / ${title} · ${files.length} 项` : `${title} / 当前目录`}
          </div>
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
          <FileList onRemoveFromCollection={handleRemoveFromCollection} />
        </div>
        <div className="w-80 min-w-[260px] max-w-[45%] border-l border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
          <FileDetail />
        </div>
      </div>
    </div>
  );
}

export function CollectionTabSurface({
  collectionId,
  projectPath,
  title,
}: CollectionTabSurfaceProps) {
  const collectionPath = `pmc://collection/${collectionId}`;
  const [collectionStore, setCollectionStore] = useState<ProjectStoreApi>(() =>
    createCollectionStore(projectPath, collectionPath),
  );

  useEffect(() => {
    setCollectionStore(createCollectionStore(projectPath, collectionPath));
  }, [collectionPath, projectPath]);

  useEffect(() => {
    let cancelled = false;
    const loadCollection = async () => {
      await collectionStore.getState().loadDirectory(collectionPath, true);
      await Promise.all([
        collectionStore.getState().loadTags(),
        collectionStore.getState().refreshMdtIndex(),
      ]);
    };

    void loadCollection().catch((error) => {
      if (!cancelled) {
        console.error('Failed to load collection directory:', error);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [collectionPath, collectionStore]);

  return (
    <ProjectStoreProvider store={collectionStore}>
      <CollectionDirectoryContent
        collectionPath={collectionPath}
        collectionId={collectionId}
        title={title}
      />
    </ProjectStoreProvider>
  );
}
