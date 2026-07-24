import { useCallback, useEffect, useRef, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import {
  ArrowLeft,
  Box,
  FileIcon,
  Film,
  FolderIcon,
  Grid,
  Image,
  List,
  RefreshCw,
} from 'lucide-react';
import type { FileInfo } from '../../types';
import { FILES_TAB_ID, useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import {
  getWorkspaceOpenTarget,
  isTextExtension,
  isVideoExtension,
} from '../workspace/fileOpeners';
import {
  isDirectPreviewImageExtension,
  isImageExtension,
} from '../image-viewer/imageViewerUtils';

interface CollectionTabSurfaceProps {
  collectionId: string;
  projectPath: string;
  title: string;
}

function formatSize(bytes: number) {
  if (!bytes) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

function formatDate(value: string | null) {
  if (!value) return '-';
  return new Date(value).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function getCollectionMemberIcon(file: FileInfo) {
  if (file.is_dir) {
    return <FolderIcon className="h-5 w-5 text-yellow-500" />;
  }

  const extension = file.extension?.toLowerCase();
  if (extension === 'blend') {
    return <Box className="h-5 w-5 text-orange-500" />;
  }
  if (extension && isImageExtension(extension)) {
    return <Image className="h-5 w-5 text-purple-500" />;
  }
  if (extension && isVideoExtension(extension)) {
    return <Film className="h-5 w-5 text-rose-500" />;
  }
  return <FileIcon className="h-5 w-5 text-gray-400" />;
}

function getMemberType(file: FileInfo) {
  if (file.is_dir) return '文件夹';
  return file.extension?.toUpperCase() || '文件';
}

function getMemberPreview(file: FileInfo) {
  const extension = file.extension?.toLowerCase() || '';
  if (!file.is_dir && isImageExtension(extension) && isDirectPreviewImageExtension(extension)) {
    return convertFileSrc(file.path);
  }
  return null;
}

export function CollectionTabSurface({
  collectionId,
  projectPath,
  title,
}: CollectionTabSurfaceProps) {
  const openFileInTab = useWorkspaceTabStore((state) => state.openFileInTab);
  const openDirectoryInTab = useWorkspaceTabStore((state) => state.openDirectoryInTab);
  const activateTab = useWorkspaceTabStore((state) => state.activateTab);
  const [items, setItems] = useState<FileInfo[]>([]);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [viewMode, setViewMode] = useState<'list' | 'grid'>('list');
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const selectionAnchorRef = useRef<string | null>(null);

  const loadItems = useCallback(async () => {
    setIsLoading(true);
    setErrorMessage(null);
    try {
      const nextItems = await invoke<FileInfo[]>('get_collection_items', {
        projectPath,
        collectionId,
      });
      setItems(nextItems);
      setSelectedPaths((previous) => {
        const visiblePaths = new Set(nextItems.map((item) => item.path));
        return new Set([...previous].filter((path) => visiblePaths.has(path)));
      });
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

  const handleSelectItem = useCallback(
    (path: string, multi: boolean, range: boolean) => {
      if (range && selectionAnchorRef.current) {
        const start = items.findIndex((item) => item.path === selectionAnchorRef.current);
        const end = items.findIndex((item) => item.path === path);
        if (start >= 0 && end >= 0) {
          const [from, to] = start <= end ? [start, end] : [end, start];
          setSelectedPaths(new Set(items.slice(from, to + 1).map((item) => item.path)));
          return;
        }
      }

      selectionAnchorRef.current = path;
      setSelectedPaths((previous) => {
        if (!multi) return new Set([path]);
        const next = new Set(previous);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
    },
    [items],
  );

  const handleOpenItem = useCallback(
    async (item: FileInfo) => {
      if (item.is_dir) {
        openDirectoryInTab(item.path);
        return;
      }

      const target = getWorkspaceOpenTarget(item.path);
      if (!target || (target === 'text' && !isTextExtension(item.extension)) || (target === 'video' && !isVideoExtension(item.extension))) {
        await invoke('open_file', { path: item.path });
        return;
      }

      await openFileInTab(item.path);
    },
    [openDirectoryInTab, openFileInTab],
  );

  const handleReturnToProject = useCallback(() => {
    activateTab(FILES_TAB_ID);
  }, [activateTab]);

  return (
    <div className="flex h-full min-h-0 flex-col bg-white text-gray-900 dark:bg-gray-900 dark:text-gray-100">
      <div className="flex shrink-0 items-center gap-2 border-b border-gray-200 px-3 py-2 dark:border-gray-700">
        <button
          type="button"
          onClick={handleReturnToProject}
          className="rounded-md p-1.5 text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
          title="返回项目根目录"
        >
          <ArrowLeft className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={() => void loadItems()}
          className="rounded-md p-1.5 text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
          title="刷新集合"
        >
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </button>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-semibold">{title}</div>
          <div className="truncate text-xs text-gray-500 dark:text-gray-400" title={projectPath}>
            项目根目录 / {title} · {items.length} 项
          </div>
        </div>
        <div className="flex items-center rounded-md bg-gray-100 p-0.5 dark:bg-gray-800">
          <button
            type="button"
            onClick={() => setViewMode('list')}
            className={`rounded p-1.5 transition-colors ${
              viewMode === 'list'
                ? 'bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100'
                : 'text-gray-500 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100'
            }`}
            title="列表视图"
          >
            <List className="h-4 w-4" />
          </button>
          <button
            type="button"
            onClick={() => setViewMode('grid')}
            className={`rounded p-1.5 transition-colors ${
              viewMode === 'grid'
                ? 'bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100'
                : 'text-gray-500 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100'
            }`}
            title="网格视图"
          >
            <Grid className="h-4 w-4" />
          </button>
        </div>
      </div>

      {errorMessage ? (
        <div className="m-3 shrink-0 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-200">
          {errorMessage}
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-auto">
        {!isLoading && items.length === 0 ? (
          <div className="flex h-full min-h-[240px] items-center justify-center text-sm text-gray-400">
            集合中没有可用成员。
          </div>
        ) : viewMode === 'list' ? (
          <div className="min-w-[680px]">
            <div className="grid grid-cols-[minmax(260px,1fr)_110px_130px_150px] border-b border-gray-200 bg-gray-50 px-3 py-2 text-xs font-medium uppercase text-gray-500 dark:border-gray-700 dark:bg-gray-800">
              <span>名称</span>
              <span>类型</span>
              <span>大小</span>
              <span>修改时间</span>
            </div>
            {items.map((item) => {
              const isSelected = selectedPaths.has(item.path);
              return (
                <button
                  key={item.path}
                  type="button"
                  onClick={(event) => handleSelectItem(item.path, event.ctrlKey || event.metaKey, event.shiftKey)}
                  onDoubleClick={() => void handleOpenItem(item)}
                  className={`grid w-full grid-cols-[minmax(260px,1fr)_110px_130px_150px] items-center border-b border-gray-100 px-3 py-2 text-left text-sm transition-colors dark:border-gray-800 ${
                    isSelected
                      ? 'bg-blue-100 text-blue-950 shadow-[inset_4px_0_0_0_#2563eb] dark:bg-blue-950/45 dark:text-blue-50 dark:shadow-[inset_4px_0_0_0_#60a5fa]'
                      : 'hover:bg-gray-50 dark:hover:bg-gray-800/60'
                  }`}
                  title={item.path}
                >
                  <span className="flex min-w-0 items-center gap-2 truncate">
                    {getCollectionMemberIcon(item)}
                    <span className="truncate font-medium">{item.name}</span>
                  </span>
                  <span className="truncate text-gray-500 dark:text-gray-400">{getMemberType(item)}</span>
                  <span className="truncate text-gray-500 dark:text-gray-400">{item.is_dir ? '-' : formatSize(item.size)}</span>
                  <span className="truncate text-gray-500 dark:text-gray-400">{formatDate(item.modified)}</span>
                </button>
              );
            })}
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(176px,1fr))] gap-3 p-4">
            {items.map((item) => {
              const isSelected = selectedPaths.has(item.path);
              const preview = getMemberPreview(item);
              return (
                <button
                  key={item.path}
                  type="button"
                  onClick={(event) => handleSelectItem(item.path, event.ctrlKey || event.metaKey, event.shiftKey)}
                  onDoubleClick={() => void handleOpenItem(item)}
                  className={`group min-w-0 rounded-md p-3 text-left transition-colors ${
                    isSelected
                      ? 'bg-blue-100 ring-2 ring-blue-500 dark:bg-blue-950/45'
                      : 'hover:bg-gray-50 dark:hover:bg-gray-800/60'
                  }`}
                  title={item.path}
                >
                  <div className="mb-2 flex h-28 items-center justify-center overflow-hidden rounded-md bg-gray-100 dark:bg-gray-800">
                    {preview ? (
                      <img src={preview} alt="" className="h-full w-full object-cover" draggable={false} />
                    ) : (
                      <span className="scale-[1.7]">{getCollectionMemberIcon(item)}</span>
                    )}
                  </div>
                  <div className={`truncate text-sm ${isSelected ? 'font-semibold text-blue-950 dark:text-blue-50' : 'font-medium'}`}>
                    {item.name}
                  </div>
                  <div className="mt-1 truncate text-xs text-gray-500 dark:text-gray-400">
                    {getMemberType(item)}{item.is_dir ? '' : ` · ${formatSize(item.size)}`}
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
