import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FileInfo } from '../../types';
import { useProjectStoreApi, useProjectStoreShallow } from '../../stores/projectStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import { FileIcon, FolderIcon, Image, Film, FileText, Box } from 'lucide-react';
import { CurrentDirectoryContextMenu, FileContextMenu } from './FileContextMenu';
import { FileDetailsDialog } from './FileDetailsView';
import { InputDialog } from '../Dialog';
import { canMovePathsToDirectory, compactDraggedPaths, getPathLabel, joinPath } from './dragDrop';
import { useFileDropMove } from './useFileDropMove';
import { useInternalFileDrag } from './useInternalFileDrag';
import { getWorkspaceOpenTarget, isTextExtension, isVideoExtension } from '../workspace/fileOpeners';
import { isImageExtension } from '../image-viewer/imageViewerUtils';
import {
  mergeExcludePatterns,
  readProjectExcludePatterns,
  shouldExcludeFile,
} from '../../utils/excludePatterns';

// 格式化文件大小
function formatSize(bytes: number): string {
  if (bytes === 0) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }
  return `${size.toFixed(1)} ${units[unitIndex]}`;
}

// 格式化日期
function formatDate(dateStr: string | null): string {
  if (!dateStr) return '-';
  const date = new Date(dateStr);
  return date.toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

// 获取文件图标
function getFileIcon(file: FileInfo) {
  if (file.is_dir) {
    return <FolderIcon className="w-5 h-5 text-yellow-500" />;
  }
  
  const ext = file.extension?.toLowerCase();
  
  // 图片
  if (isImageExtension(ext)) {
    return <Image className="w-5 h-5 text-purple-500" />;
  }
  
  // 视频
  if (isVideoExtension(ext)) {
    return <Film className="w-5 h-5 text-red-500" />;
  }
  
  // Blender
  if (ext === 'blend') {
    return <Box className="w-5 h-5 text-orange-500" />;
  }
  
  // 文本
  if (isTextExtension(ext)) {
    return <FileText className="w-5 h-5 text-blue-500" />;
  }
  
  return <FileIcon className="w-5 h-5 text-gray-400" />;
}

const MIN_COLUMN_WIDTHS: Record<string, number> = {
  name: 220,
  size: 90,
  modified: 150,
  type: 100,
  tags: 120,
};

function clampColumnWidth(key: string, width: number) {
  return Math.max(MIN_COLUMN_WIDTHS[key] ?? 80, Math.round(width));
}

// 列表视图
function ListView({
  files,
  selectedFiles,
  onSelect,
  onDoubleClick,
  onContextMenu,
  onBackgroundContextMenu,
  onDragStart,
  onDragEnd,
  onDropToDirectory,
  onHoverDirectory,
  canDropToDirectory,
  getDraggedPathsFromDataTransfer,
  suppressInteraction,
  dropTargetPath,
  currentPath,
  columns,
  resizingColumnKey,
  onStartColumnResize,
  tags,
  fileTags,
  isExcluded,
  showExcludedFiles,
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
  onBackgroundContextMenu: (x: number, y: number) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (dataTransfer: DataTransfer | null) => string[];
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  dropTargetPath: string | null;
  currentPath: string;
  columns: any[];
  resizingColumnKey: string | null;
  onStartColumnResize: (key: string, width: number, event: React.MouseEvent<HTMLDivElement>) => void;
  tags: any[];
  fileTags: Map<string, string[]>;
  isExcluded: (file: FileInfo) => boolean;
  showExcludedFiles: boolean;
}) {
  const visibleColumns = columns.filter(c => c.visible);
  const headerScrollRef = useRef<HTMLDivElement>(null);
  const bodyScrollRef = useRef<HTMLDivElement>(null);

  const syncHeaderScroll = useCallback(() => {
    if (!headerScrollRef.current || !bodyScrollRef.current) {
      return;
    }

    headerScrollRef.current.scrollLeft = bodyScrollRef.current.scrollLeft;
  }, []);
  
  return (
    <div className="flex flex-col h-full">
      {/* 表头 */}
      <div
        ref={headerScrollRef}
        className="overflow-hidden border-b border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-800"
      >
        <div className="flex min-w-max">
          {visibleColumns.map((col, index) => {
            const isLastVisibleColumn = index === visibleColumns.length - 1;
            const isResizing = resizingColumnKey === col.key;

            return (
              <div
                key={col.key}
                className="relative shrink-0 px-3 py-2 text-xs font-medium uppercase tracking-wider text-gray-500"
                style={{ width: col.width, textAlign: col.align || 'left' }}
              >
                {col.title}

                {!isLastVisibleColumn && (
                  <div
                    onMouseDown={(event) => onStartColumnResize(col.key, col.width, event)}
                    className="absolute inset-y-0 -right-1 z-10 flex w-2 cursor-col-resize items-center justify-center"
                    title="拖动调整列宽"
                  >
                    <div
                      className={`h-5 w-px transition-colors ${
                        isResizing
                          ? 'bg-blue-500'
                          : 'bg-gray-200 dark:bg-gray-700 hover:bg-blue-400 dark:hover:bg-blue-500'
                      }`}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
      
      {/* 文件列表 */}
      <div
        ref={bodyScrollRef}
        className={`flex-1 overflow-auto ${dropTargetPath === currentPath ? 'bg-blue-50/60' : ''}`}
        onScroll={syncHeaderScroll}
        onContextMenu={(e) => {
          if (e.target !== e.currentTarget) return;
          e.preventDefault();
          onBackgroundContextMenu(e.clientX, e.clientY);
        }}
        onDragOver={(e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
          if (!canDropToDirectory(currentPath, internalDragPaths)) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = 'move';
          onHoverDirectory(currentPath);
        }}
        onDrop={async (e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
          if (!canDropToDirectory(currentPath, internalDragPaths)) return;
          e.preventDefault();
          await onDropToDirectory(currentPath, internalDragPaths);
        }}
      >
        {files.map(file => {
          const isSelected = selectedFiles.has(file.path);
          const fileTagIds = fileTags.get(file.path) || [];
          const fileTagList = tags.filter(t => fileTagIds.includes(t.id));
          const isDropTarget = file.is_dir && dropTargetPath === file.path;
          const excluded = isExcluded(file);
          
          return (
            <div
              key={file.path}
              draggable
              className={`
                relative flex min-w-max items-center border-b border-gray-100 dark:border-gray-800
                cursor-pointer select-none transition-colors
                ${isSelected 
                  ? 'bg-blue-100/90 dark:bg-blue-950/45 text-blue-950 dark:text-blue-50 ring-1 ring-inset ring-blue-500/70 shadow-[inset_4px_0_0_0_#2563eb] dark:shadow-[inset_4px_0_0_0_#60a5fa]' 
                  : 'hover:bg-gray-50 dark:hover:bg-gray-800/50'
                }
                ${isDropTarget ? 'ring-2 ring-inset ring-blue-500 bg-blue-50' : ''}
                ${showExcludedFiles && excluded ? 'opacity-70' : ''}
              `}
              onClick={(e) => {
                if (suppressInteraction(e)) return;
                onSelect(file.path, e.ctrlKey || e.metaKey);
              }}
              onDoubleClick={(e) => {
                if (suppressInteraction(e)) return;
                onDoubleClick(file, e.ctrlKey || e.metaKey);
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                onContextMenu(file, e.clientX, e.clientY);
              }}
              onDragStart={(e) => onDragStart(file, e)}
              onDragEnd={onDragEnd}
              onDragOver={(e) => {
                const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
                if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths)) return;
                e.preventDefault();
                e.dataTransfer.dropEffect = 'move';
                onHoverDirectory(file.path);
              }}
              onDragEnter={(e) => {
                const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
                if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths)) return;
                e.preventDefault();
                onHoverDirectory(file.path);
              }}
              onDrop={async (e) => {
                const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
                if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths)) return;
                e.preventDefault();
                e.stopPropagation();
                await onDropToDirectory(file.path, internalDragPaths);
              }}
            >
              {isSelected && (
                <div className="absolute left-0 top-1/2 h-6 w-1 -translate-y-1/2 rounded-r-full bg-blue-600 dark:bg-blue-400" />
              )}

              {visibleColumns.map(col => (
                <div
                  key={col.key}
                  className={`shrink-0 px-3 py-2 text-sm truncate ${
                    isSelected ? 'text-blue-950 dark:text-blue-50' : 'text-gray-700 dark:text-gray-200'
                  }`}
                  style={{ width: col.width, textAlign: col.align || 'left' }}
                >
                  {col.key === 'name' && (
                    <div className="flex items-center gap-2">
                      {getFileIcon(file)}
                      <span className={`truncate ${isSelected ? 'font-semibold' : ''}`}>{file.name}</span>
                      {showExcludedFiles && excluded && (
                        <span className="shrink-0 rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">
                          已排除
                        </span>
                      )}
                    </div>
                  )}
                  {col.key === 'size' && formatSize(file.size)}
                  {col.key === 'modified' && formatDate(file.modified)}
                  {col.key === 'type' && (file.is_dir ? '文件夹' : file.extension?.toUpperCase() || '文件')}
                  {col.key === 'tags' && (
                    <div className="flex gap-1 flex-wrap">
                      {fileTagList.map(tag => (
                        <span
                          key={tag.id}
                          className="px-1.5 py-0.5 text-xs rounded"
                          style={{ backgroundColor: tag.color + '20', color: tag.color }}
                        >
                          {tag.name}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// 网格视图
function GridView({
  files,
  selectedFiles,
  onSelect,
  onDoubleClick,
  onContextMenu,
  onBackgroundContextMenu,
  onDragStart,
  onDragEnd,
  onDropToDirectory,
  onHoverDirectory,
  canDropToDirectory,
  getDraggedPathsFromDataTransfer,
  suppressInteraction,
  dropTargetPath,
  currentPath,
  tags,
  fileTags,
  isExcluded,
  showExcludedFiles,
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
  onBackgroundContextMenu: (x: number, y: number) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (dataTransfer: DataTransfer | null) => string[];
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  dropTargetPath: string | null;
  currentPath: string;
  tags: any[];
  fileTags: Map<string, string[]>;
  isExcluded: (file: FileInfo) => boolean;
  showExcludedFiles: boolean;
}) {
  return (
    <div
      className={`p-4 grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4 overflow-auto ${dropTargetPath === currentPath ? 'bg-blue-50/60' : ''}`}
      onContextMenu={(e) => {
        if (e.target !== e.currentTarget) return;
        e.preventDefault();
        onBackgroundContextMenu(e.clientX, e.clientY);
      }}
      onDragOver={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
        if (!canDropToDirectory(currentPath, internalDragPaths)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        onHoverDirectory(currentPath);
      }}
      onDrop={async (e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
        if (!canDropToDirectory(currentPath, internalDragPaths)) return;
        e.preventDefault();
        await onDropToDirectory(currentPath, internalDragPaths);
      }}
    >
      {files.map(file => {
        const isSelected = selectedFiles.has(file.path);
        const fileTagIds = fileTags.get(file.path) || [];
        const fileTagList = tags.filter(t => fileTagIds.includes(t.id));
        const isDropTarget = file.is_dir && dropTargetPath === file.path;
        const excluded = isExcluded(file);
        
        return (
          <div
            key={file.path}
            draggable
            className={`
              group relative p-3 rounded-lg cursor-pointer select-none transition-all
              ${isSelected 
                ? 'bg-blue-100 dark:bg-blue-950/40 ring-2 ring-blue-500 shadow-lg shadow-blue-500/10 dark:shadow-blue-950/30' 
                : 'hover:bg-gray-50 dark:hover:bg-gray-800'
              }
              ${isDropTarget ? 'ring-2 ring-blue-500 bg-blue-50' : ''}
              ${showExcludedFiles && excluded ? 'opacity-70' : ''}
            `}
            onClick={(e) => {
              if (suppressInteraction(e)) return;
              onSelect(file.path, e.ctrlKey || e.metaKey);
            }}
            onDoubleClick={(e) => {
              if (suppressInteraction(e)) return;
              onDoubleClick(file, e.ctrlKey || e.metaKey);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              onContextMenu(file, e.clientX, e.clientY);
            }}
            onDragStart={(e) => onDragStart(file, e)}
            onDragEnd={onDragEnd}
            onDragOver={(e) => {
              const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
              if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths)) return;
              e.preventDefault();
              e.dataTransfer.dropEffect = 'move';
              onHoverDirectory(file.path);
            }}
            onDragEnter={(e) => {
              const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
              if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths)) return;
              e.preventDefault();
              onHoverDirectory(file.path);
            }}
            onDrop={async (e) => {
              const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
              if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths)) return;
              e.preventDefault();
              e.stopPropagation();
              await onDropToDirectory(file.path, internalDragPaths);
            }}
          >
            {isSelected && (
              <div className="absolute right-2 top-2 rounded-full bg-blue-600 px-1.5 py-0.5 text-[10px] font-medium text-white dark:bg-blue-500">
                已选中
              </div>
            )}

            {/* 图标 */}
            <div className={`mb-2 aspect-square flex items-center justify-center rounded-xl transition-colors ${
              isSelected ? 'bg-blue-200/70 dark:bg-blue-900/50' : ''
            }`}>
              {getFileIcon(file)}
            </div>
            
            {/* 名称 */}
            <div
              className={`text-sm text-center truncate ${
                isSelected ? 'font-semibold text-blue-950 dark:text-blue-50' : ''
              }`}
              title={file.name}
            >
              {file.name}
            </div>
            {showExcludedFiles && excluded && (
              <div className="mt-1 text-center">
                <span className="rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">
                  已排除
                </span>
              </div>
            )}
            
            {/* 标签 */}
            {fileTagList.length > 0 && (
              <div className="flex justify-center gap-1 mt-1 flex-wrap">
                {fileTagList.slice(0, 2).map(tag => (
                  <span
                    key={tag.id}
                    className="w-2 h-2 rounded-full"
                    style={{ backgroundColor: tag.color }}
                    title={tag.name}
                  />
                ))}
                {fileTagList.length > 2 && (
                  <span className="text-xs text-gray-400">+{fileTagList.length - 2}</span>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

export function FileList() {
  const projectStore = useProjectStoreApi();
  const {
    files,
    selectedFiles,
    viewMode,
    columns,
    updateColumn,
    tags,
    fileTags,
    selectFile,
    clearSelection,
    loadDirectory,
    refresh,
    currentPath,
    searchResults,
    isSearching,
    searchQuery,
    projectPath,
    showExcludedFiles,
  } = useProjectStoreShallow((state) => ({
    files: state.files,
    selectedFiles: state.selectedFiles,
    viewMode: state.viewMode,
    columns: state.columns,
    updateColumn: state.updateColumn,
    tags: state.tags,
    fileTags: state.fileTags,
    selectFile: state.selectFile,
    clearSelection: state.clearSelection,
    loadDirectory: state.loadDirectory,
    refresh: state.refresh,
    currentPath: state.currentPath,
    searchResults: state.searchResults,
    isSearching: state.isSearching,
    searchQuery: state.searchQuery,
    projectPath: state.projectPath,
    showExcludedFiles: state.showExcludedFiles,
  }));
  const showToast = useUiStore((state) => state.showToast);
  const globalExcludePatterns = useSettingsStore((state) => state.globalExcludePatterns);
  const openFileInTab = useWorkspaceTabStore((state) => state.openFileInTab);
  const openFileInStandaloneWindow = useWorkspaceTabStore((state) => state.openFileInStandaloneWindow);
  const {
    draggedPaths,
    startInternalDrag,
    finishInternalDrag,
    suppressInteraction,
    getDraggedPathsFromDataTransfer,
  } = useInternalFileDrag();
  const [dropTargetPath, setDropTargetPath] = useState<string | null>(null);
  const { movePathsToDirectory, conflictDialog } = useFileDropMove(async () => {
    await refresh();
  });

  // 右键菜单状态
  const [contextMenu, setContextMenu] = useState<
    | { kind: 'file'; file: FileInfo; x: number; y: number }
    | { kind: 'directory'; x: number; y: number }
    | null
  >(null);
  const [detailsDialogFile, setDetailsDialogFile] = useState<FileInfo | null>(null);
  const [createFolderDialog, setCreateFolderDialog] = useState({
    isOpen: false,
    suggestedName: '',
    folderName: '',
  });
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [resizingColumnKey, setResizingColumnKey] = useState<string | null>(null);
  const columnResizeStateRef = useRef<{ key: string; startX: number; startWidth: number } | null>(null);

  const displayFiles = searchQuery ? searchResults : files;
  const excludePatterns = projectPath
    ? mergeExcludePatterns(globalExcludePatterns, readProjectExcludePatterns(projectPath))
    : [];
  const isExcluded = useCallback((file: FileInfo) => {
    return excludePatterns.length > 0 && shouldExcludeFile(file.name, excludePatterns);
  }, [excludePatterns]);
  const detailsDialogTagIds = detailsDialogFile ? (fileTags.get(detailsDialogFile.path) || []) : [];
  const detailsDialogTagList = detailsDialogFile
    ? tags.filter((tag) => detailsDialogTagIds.includes(tag.id))
    : [];

  const handleSystemOpenFile = useCallback(async (file: FileInfo) => {
    try {
      await invoke('open_file', { path: file.path });
      showToast({
        title: '已打开',
        message: file.name,
        tone: 'success',
      });
    } catch (error) {
      console.error('Failed to open file:', error);
      showToast({
        title: '打开失败',
        message: String(error),
        tone: 'error',
      });
    }
  }, [showToast]);

  const handleDoubleClick = useCallback(async (file: FileInfo, openInStandalone: boolean) => {
    if (file.is_dir) {
      await loadDirectory(file.path);
      return;
    }

    const openTarget = getWorkspaceOpenTarget(file.path);
    if (!openTarget) {
      await handleSystemOpenFile(file);
      return;
    }

    try {
      if (openInStandalone) {
        const opened = await openFileInStandaloneWindow(file.path, {
          projectPath: projectPath || undefined,
        });
        if (!opened) {
          await handleSystemOpenFile(file);
        }
        return;
      }

      const tabId = await openFileInTab(file.path);
      if (!tabId) {
        await handleSystemOpenFile(file);
      }
    } catch (error) {
      console.error('Failed to open in workspace:', error);
      showToast({
        title: '打开失败',
        message: String(error),
        tone: 'error',
      });
    }
  }, [handleSystemOpenFile, loadDirectory, openFileInStandaloneWindow, openFileInTab, projectPath, showToast]);

  const handleContextMenu = useCallback((file: FileInfo, x: number, y: number) => {
    if (!selectedFiles.has(file.path)) {
      projectStore.setState({
        selectedFiles: new Set([file.path]),
      });
    }
    setContextMenu({ kind: 'file', file, x, y });
  }, [projectStore, selectedFiles]);

  const handleBackgroundContextMenu = useCallback((x: number, y: number) => {
    clearSelection();
    setContextMenu({ kind: 'directory', x, y });
  }, [clearSelection]);

  const handleCloseContextMenu = () => {
    setContextMenu(null);
  };

  const handleShowDetails = useCallback((file: FileInfo) => {
    setDetailsDialogFile(file);
  }, []);

  const handleCloseDetailsDialog = useCallback(() => {
    setDetailsDialogFile(null);
  }, []);

  const handleRefresh = () => {
    refresh();
  };

  const stopColumnResize = useCallback(() => {
    columnResizeStateRef.current = null;
    setResizingColumnKey(null);
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }, []);

  useEffect(() => {
    if (!resizingColumnKey) {
      return;
    }

    const handleMouseMove = (event: MouseEvent) => {
      const resizeState = columnResizeStateRef.current;
      if (!resizeState) {
        return;
      }

      const nextWidth = resizeState.startWidth + (event.clientX - resizeState.startX);
      updateColumn(resizeState.key, { width: clampColumnWidth(resizeState.key, nextWidth) });
    };

    const handleMouseUp = () => {
      stopColumnResize();
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [resizingColumnKey, stopColumnResize, updateColumn]);

  useEffect(() => {
    return () => {
      stopColumnResize();
    };
  }, [stopColumnResize]);

  const handleStartColumnResize = useCallback((key: string, width: number, event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.stopPropagation();
    columnResizeStateRef.current = {
      key,
      startX: event.clientX,
      startWidth: width,
    };
    setResizingColumnKey(key);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, []);

  const getSuggestedFolderName = useCallback(async (targetDir: string) => {
    const baseName = '新建文件夹';
    let candidate = baseName;
    let index = 2;

    while (await invoke<boolean>('path_exists', { path: joinPath(targetDir, candidate) })) {
      candidate = `${baseName} ${index}`;
      index += 1;
    }

    return candidate;
  }, []);

  const handleCreateFolder = useCallback(async () => {
    if (!currentPath) {
      return;
    }

    try {
      const suggestedName = await getSuggestedFolderName(currentPath);
      setCreateFolderDialog({
        isOpen: true,
        suggestedName,
        folderName: suggestedName,
      });
    } catch (error) {
      console.error('Failed to open create folder dialog:', error);
      showToast({
        title: '创建失败',
        message: String(error),
        tone: 'error',
      });
    }
  }, [currentPath, getSuggestedFolderName, showToast]);

  const handleCloseCreateFolderDialog = useCallback(() => {
    if (isCreatingFolder) {
      return;
    }

    setCreateFolderDialog((state) => ({
      ...state,
      isOpen: false,
    }));
  }, [isCreatingFolder]);

  const handleCreateFolderNameChange = useCallback((folderName: string) => {
    setCreateFolderDialog((state) => ({
      ...state,
      folderName,
    }));
  }, []);

  const handleConfirmCreateFolder = useCallback(async (rawFolderName: string) => {
    if (!currentPath) {
      return;
    }

    const folderName = rawFolderName.trim();

    if (!folderName) {
      showToast({
        title: '创建失败',
        message: '请输入文件夹名称。',
        tone: 'error',
      });
      return;
    }

    if (/[\\/]/.test(folderName)) {
      showToast({
        title: '创建失败',
        message: '文件夹名称不能包含路径分隔符。',
        tone: 'error',
      });
      return;
    }

    if (folderName === '.' || folderName === '..') {
      showToast({
        title: '创建失败',
        message: '请输入有效的文件夹名称。',
        tone: 'error',
      });
      return;
    }

    setIsCreatingFolder(true);
    try {
      const targetPath = joinPath(currentPath, folderName);
      const exists = await invoke<boolean>('path_exists', { path: targetPath });

      if (exists) {
        showToast({
          title: '创建失败',
          message: '当前目录已存在同名文件夹。',
          tone: 'error',
        });
        return;
      }

      await invoke('create_directory', { path: targetPath });
      await refresh();
      setCreateFolderDialog((state) => ({
        ...state,
        isOpen: false,
      }));
      showToast({
        title: '文件夹已创建',
        message: folderName,
        tone: 'success',
      });
    } catch (error) {
      console.error('Failed to create folder:', error);
      showToast({
        title: '创建失败',
        message: String(error),
        tone: 'error',
      });
    } finally {
      setIsCreatingFolder(false);
    }
  }, [currentPath, refresh, showToast]);

  const handleDelete = useCallback(async (targetPaths: string[]) => {
    const paths = compactDraggedPaths(targetPaths);
    if (paths.length === 0) {
      return;
    }

    try {
      const deletedCount = await invoke<number>('delete_paths', { paths });
      await refresh();

      if (deletedCount === 0) {
        showToast({
          title: '未删除任何项目',
          message: '选中的文件可能已经不存在，列表已刷新。',
          tone: 'warning',
        });
        return;
      }

      showToast({
        title: deletedCount > 1 ? '已移到回收站' : '文件已移到回收站',
        message: deletedCount > 1
          ? `已将 ${deletedCount} 个项目移到回收站。`
          : `已将 ${paths[0].split(/[\\/]/).pop() || '该项目'} 移到回收站。`,
        tone: 'success',
      });
    } catch (error) {
      console.error('Failed to delete:', error);
      showToast({
        title: '删除失败',
        message: String(error),
        tone: 'error',
      });
    }
  }, [refresh, showToast]);

  const handleDeleteFromContextMenu = useCallback(async (file: FileInfo) => {
    const targetPaths = selectedFiles.has(file.path)
      ? Array.from(selectedFiles)
      : [file.path];
    await handleDelete(targetPaths);
  }, [handleDelete, selectedFiles]);

  const getDraggedItems = useCallback((file: FileInfo) => {
    if (selectedFiles.has(file.path) && selectedFiles.size > 1) {
      return Array.from(selectedFiles);
    }
    return [file.path];
  }, [selectedFiles]);

  const canDropToDirectory = useCallback((targetDir: string, dragPaths = draggedPaths) => {
    return canMovePathsToDirectory(targetDir, dragPaths);
  }, [draggedPaths]);

  const handleDragStart = useCallback((file: FileInfo, event: React.DragEvent<HTMLDivElement>) => {
    startInternalDrag(event, getDraggedItems(file));
  }, [getDraggedItems, startInternalDrag]);

  const handleDragEnd = useCallback(() => {
    finishInternalDrag();
    setDropTargetPath(null);
  }, [finishInternalDrag]);

  const handleDropToDirectory = useCallback(async (targetDir: string, dragPaths?: string[]) => {
    const currentDraggedPaths = dragPaths && dragPaths.length > 0 ? dragPaths : draggedPaths;
    if (currentDraggedPaths.length === 0) {
      return;
    }

    setDropTargetPath(null);
    await movePathsToDirectory(
      currentDraggedPaths,
      targetDir,
      getPathLabel(
        targetDir,
        projectStore.getState().projectPath,
        projectStore.getState().projectName,
      ),
    );
  }, [draggedPaths, movePathsToDirectory, projectStore]);

  const handleHoverDirectory = useCallback((targetDir: string) => {
    setDropTargetPath(targetDir);
  }, []);

  if (isSearching) {
    return (
      <div className="h-full flex items-center justify-center text-gray-400">
        搜索中...
      </div>
    );
  }

  return (
    <div className="h-full">
      {viewMode === 'list' ? (
        <ListView
          files={displayFiles}
          selectedFiles={selectedFiles}
          onSelect={selectFile}
          onDoubleClick={handleDoubleClick}
          onContextMenu={handleContextMenu}
          onBackgroundContextMenu={handleBackgroundContextMenu}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          onDropToDirectory={handleDropToDirectory}
          onHoverDirectory={handleHoverDirectory}
          canDropToDirectory={canDropToDirectory}
          getDraggedPathsFromDataTransfer={getDraggedPathsFromDataTransfer}
          suppressInteraction={suppressInteraction}
          dropTargetPath={dropTargetPath}
          currentPath={currentPath || ''}
          columns={columns}
          resizingColumnKey={resizingColumnKey}
          onStartColumnResize={handleStartColumnResize}
          tags={tags}
          fileTags={fileTags}
          isExcluded={isExcluded}
          showExcludedFiles={showExcludedFiles}
        />
      ) : (
        <GridView
          files={displayFiles}
          selectedFiles={selectedFiles}
          onSelect={selectFile}
          onDoubleClick={handleDoubleClick}
          onContextMenu={handleContextMenu}
          onBackgroundContextMenu={handleBackgroundContextMenu}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          onDropToDirectory={handleDropToDirectory}
          onHoverDirectory={handleHoverDirectory}
          canDropToDirectory={canDropToDirectory}
          getDraggedPathsFromDataTransfer={getDraggedPathsFromDataTransfer}
          suppressInteraction={suppressInteraction}
          dropTargetPath={dropTargetPath}
          currentPath={currentPath || ''}
          tags={tags}
          fileTags={fileTags}
          isExcluded={isExcluded}
          showExcludedFiles={showExcludedFiles}
        />
      )}

      {/* 右键菜单 */}
      {contextMenu?.kind === 'file' && (
        <FileContextMenu
          file={contextMenu.file}
          x={contextMenu.x}
          y={contextMenu.y}
          currentPath={currentPath || ''}
          projectPath={projectPath || ''}
          onClose={handleCloseContextMenu}
          onRefresh={handleRefresh}
          onShowDetails={handleShowDetails}
          onDelete={handleDeleteFromContextMenu}
          onCreateFolder={handleCreateFolder}
          onOpenFile={handleSystemOpenFile}
        />
      )}

      {contextMenu?.kind === 'directory' && (
        <CurrentDirectoryContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          currentPath={currentPath || ''}
          projectPath={projectPath || ''}
          onClose={handleCloseContextMenu}
          onRefresh={handleRefresh}
          onCreateFolder={handleCreateFolder}
        />
      )}

      <FileDetailsDialog
        file={detailsDialogFile}
        fileTagList={detailsDialogTagList}
        isOpen={!!detailsDialogFile}
        onClose={handleCloseDetailsDialog}
      />

      <InputDialog
        isOpen={createFolderDialog.isOpen}
        onClose={handleCloseCreateFolderDialog}
        onConfirm={handleConfirmCreateFolder}
        title="新建文件夹"
        label="文件夹名称"
        value={createFolderDialog.folderName}
        onChange={handleCreateFolderNameChange}
        confirmText={isCreatingFolder ? '创建中...' : '创建'}
        disabled={isCreatingFolder}
        description={
          createFolderDialog.suggestedName
            ? `默认名称：${createFolderDialog.suggestedName}`
            : undefined
        }
        selectOnOpen
      />

      {conflictDialog}
    </div>
  );
}
