import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FileIcon, FolderIcon, Image, RefreshCw } from 'lucide-react';
import type { FileInfo } from '../../types';
import { useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import {
  getWorkspaceOpenTarget,
  isTextExtension,
  isVideoExtension,
} from '../workspace/fileOpeners';

interface CollectionTabSurfaceProps {
  collectionId: string;
  projectPath: string;
  title: string;
}

function getCollectionMemberIcon(file: FileInfo) {
  if (file.is_dir) {
    return <FolderIcon className="h-5 w-5 text-yellow-500" />;
  }

  const extension = file.extension?.toLowerCase();
  if (extension && ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'gif'].includes(extension)) {
    return <Image className="h-5 w-5 text-purple-500" />;
  }

  return <FileIcon className="h-5 w-5 text-gray-400" />;
}

export function CollectionTabSurface({
  collectionId,
  projectPath,
  title,
}: CollectionTabSurfaceProps) {
  const openFileInTab = useWorkspaceTabStore((state) => state.openFileInTab);
  const openDirectoryInTab = useWorkspaceTabStore((state) => state.openDirectoryInTab);
  const [items, setItems] = useState<FileInfo[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const loadItems = useCallback(async () => {
    setIsLoading(true);
    setErrorMessage(null);
    try {
      const nextItems = await invoke<FileInfo[]>('get_collection_items', {
        projectPath,
        collectionId,
      });
      setItems(nextItems);
    } catch (error) {
      console.error('Failed to load collection items:', error);
      setErrorMessage(String(error));
    } finally {
      setIsLoading(false);
    }
  }, [collectionId, projectPath]);

  useEffect(() => {
    void loadItems();
  }, [loadItems]);

  const handleOpenItem = useCallback(
    async (item: FileInfo) => {
      if (item.is_dir) {
        openDirectoryInTab(item.path);
        return;
      }

      const target = getWorkspaceOpenTarget(item.path);
      if (!target) {
        await invoke('open_file', { path: item.path });
        return;
      }

      if (target === 'text' && !isTextExtension(item.extension)) {
        await invoke('open_file', { path: item.path });
        return;
      }

      if (target === 'video' && !isVideoExtension(item.extension)) {
        await invoke('open_file', { path: item.path });
        return;
      }

      await openFileInTab(item.path);
    },
    [openDirectoryInTab, openFileInTab],
  );

  return (
    <div className="flex h-full min-h-0 flex-col bg-white text-gray-900 dark:bg-gray-900 dark:text-gray-100">
      <div className="flex shrink-0 items-center justify-between border-b border-gray-200 px-4 py-3 dark:border-gray-700">
        <div className="min-w-0">
          <h2 className="truncate text-base font-semibold">{title}</h2>
          <p className="text-xs text-gray-500 dark:text-gray-400">
            {items.length} 个可用成员
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadItems()}
          className="inline-flex items-center gap-2 rounded-md border border-gray-200 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
        >
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          刷新
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {errorMessage ? (
          <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-200">
            {errorMessage}
          </div>
        ) : null}

        {!isLoading && items.length === 0 ? (
          <div className="flex h-full min-h-[240px] items-center justify-center text-sm text-gray-400">
            集合成员不存在或已被移动。
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3">
            {items.map((item) => (
              <button
                key={item.path}
                type="button"
                onDoubleClick={() => void handleOpenItem(item)}
                className="flex min-w-0 items-center gap-3 rounded-md border border-gray-200 bg-white px-3 py-3 text-left transition hover:border-blue-300 hover:bg-blue-50/50 dark:border-gray-700 dark:bg-gray-900 dark:hover:border-blue-700 dark:hover:bg-blue-950/20"
                title={item.path}
              >
                <span className="shrink-0">{getCollectionMemberIcon(item)}</span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{item.name}</span>
                  <span className="block truncate text-xs text-gray-500 dark:text-gray-400">
                    {item.is_dir ? '文件夹' : item.extension?.toUpperCase() || '文件'}
                  </span>
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

