import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FileInfo } from '../../types';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { FileIcon, FolderIcon, Image, Film, FileText, Box } from 'lucide-react';
import { FileContextMenu } from './FileContextMenu';
import { FileDetailsDialog } from './FileDetailsView';
import { canMovePathsToDirectory, getPathLabel } from './dragDrop';
import { useFileDropMove } from './useFileDropMove';
import { useInternalFileDrag } from './useInternalFileDrag';

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
  if (['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'exr', 'hdr'].includes(ext || '')) {
    return <Image className="w-5 h-5 text-purple-500" />;
  }
  
  // 视频
  if (['mp4', 'mov', 'avi', 'mkv', 'webm'].includes(ext || '')) {
    return <Film className="w-5 h-5 text-red-500" />;
  }
  
  // Blender
  if (ext === 'blend') {
    return <Box className="w-5 h-5 text-orange-500" />;
  }
  
  // 文本
  if (['txt', 'md', 'json', 'xml', 'py'].includes(ext || '')) {
    return <FileText className="w-5 h-5 text-blue-500" />;
  }
  
  return <FileIcon className="w-5 h-5 text-gray-400" />;
}

// 列表视图
function ListView({
  files,
  selectedFiles,
  onSelect,
  onDoubleClick,
  onContextMenu,
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
  tags,
  fileTags,
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
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
  tags: any[];
  fileTags: Map<string, string[]>;
}) {
  const visibleColumns = columns.filter(c => c.visible);
  
  return (
    <div className="flex flex-col h-full">
      {/* 表头 */}
      <div className="flex border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
        {visibleColumns.map(col => (
          <div
            key={col.key}
            className="px-3 py-2 text-xs font-medium text-gray-500 uppercase tracking-wider"
            style={{ width: col.width, textAlign: col.align || 'left' }}
          >
            {col.title}
          </div>
        ))}
      </div>
      
      {/* 文件列表 */}
      <div
        className={`flex-1 overflow-auto ${dropTargetPath === currentPath ? 'bg-blue-50/60' : ''}`}
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
          
          return (
            <div
              key={file.path}
              draggable
              className={`
                flex items-center border-b border-gray-100 dark:border-gray-800
                cursor-pointer select-none
                ${isSelected 
                  ? 'bg-blue-50 dark:bg-blue-900/20' 
                  : 'hover:bg-gray-50 dark:hover:bg-gray-800/50'
                }
                ${isDropTarget ? 'ring-2 ring-inset ring-blue-500 bg-blue-50' : ''}
              `}
              onClick={(e) => {
                if (suppressInteraction(e)) return;
                onSelect(file.path, e.ctrlKey || e.metaKey);
              }}
              onDoubleClick={(e) => {
                if (suppressInteraction(e)) return;
                onDoubleClick(file);
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
              {visibleColumns.map(col => (
                <div
                  key={col.key}
                  className="px-3 py-2 text-sm truncate"
                  style={{ width: col.width, textAlign: col.align || 'left' }}
                >
                  {col.key === 'name' && (
                    <div className="flex items-center gap-2">
                      {getFileIcon(file)}
                      <span className="truncate">{file.name}</span>
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
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
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
}) {
  return (
    <div
      className={`p-4 grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4 overflow-auto ${dropTargetPath === currentPath ? 'bg-blue-50/60' : ''}`}
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
        
        return (
          <div
            key={file.path}
            draggable
            className={`
              group relative p-3 rounded-lg cursor-pointer select-none
              ${isSelected 
                ? 'bg-blue-50 dark:bg-blue-900/20 ring-2 ring-blue-500' 
                : 'hover:bg-gray-50 dark:hover:bg-gray-800'
              }
              ${isDropTarget ? 'ring-2 ring-blue-500 bg-blue-50' : ''}
            `}
            onClick={(e) => {
              if (suppressInteraction(e)) return;
              onSelect(file.path, e.ctrlKey || e.metaKey);
            }}
            onDoubleClick={(e) => {
              if (suppressInteraction(e)) return;
              onDoubleClick(file);
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
            {/* 图标 */}
            <div className="aspect-square flex items-center justify-center mb-2">
              {getFileIcon(file)}
            </div>
            
            {/* 名称 */}
            <div className="text-sm text-center truncate" title={file.name}>
              {file.name}
            </div>
            
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
  const {
    files,
    selectedFiles,
    viewMode,
    columns,
    tags,
    fileTags,
    selectFile,
    loadDirectory,
    refresh,
    currentPath,
    searchResults,
    isSearching,
    searchQuery,
  } = useProjectStore();
  const showToast = useUiStore((state) => state.showToast);
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
  const [contextMenu, setContextMenu] = useState<{
    file: FileInfo;
    x: number;
    y: number;
  } | null>(null);
  const [detailsDialogFile, setDetailsDialogFile] = useState<FileInfo | null>(null);

  const displayFiles = searchQuery ? searchResults : files;
  const detailsDialogTagIds = detailsDialogFile ? (fileTags.get(detailsDialogFile.path) || []) : [];
  const detailsDialogTagList = detailsDialogFile
    ? tags.filter((tag) => detailsDialogTagIds.includes(tag.id))
    : [];

  const handleDoubleClick = useCallback(async (file: FileInfo) => {
    if (file.is_dir) {
      await loadDirectory(file.path);
      return;
    }

    try {
      await invoke('open_file', { path: file.path });
      showToast({
        title: '已打开',
        message: file.name,
        tone: 'success',
      });
    } catch (error) {
      console.error('Failed to open file:', error);
    }
  }, [loadDirectory, showToast]);

  const handleContextMenu = (file: FileInfo, x: number, y: number) => {
    setContextMenu({ file, x, y });
  };

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
      getPathLabel(targetDir, useProjectStore.getState().projectPath, useProjectStore.getState().projectName),
    );
  }, [draggedPaths, movePathsToDirectory]);

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
          tags={tags}
          fileTags={fileTags}
        />
      ) : (
        <GridView
          files={displayFiles}
          selectedFiles={selectedFiles}
          onSelect={selectFile}
          onDoubleClick={handleDoubleClick}
          onContextMenu={handleContextMenu}
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
        />
      )}

      {/* 右键菜单 */}
      {contextMenu && (
        <FileContextMenu
          file={contextMenu.file}
          x={contextMenu.x}
          y={contextMenu.y}
          currentPath={currentPath || ''}
          onClose={handleCloseContextMenu}
          onRefresh={handleRefresh}
          onShowDetails={handleShowDetails}
        />
      )}

      <FileDetailsDialog
        file={detailsDialogFile}
        fileTagList={detailsDialogTagList}
        isOpen={!!detailsDialogFile}
        onClose={handleCloseDetailsDialog}
      />

      {conflictDialog}
    </div>
  );
}
