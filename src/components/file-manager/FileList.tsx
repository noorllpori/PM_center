import { useState } from 'react';
import { FileInfo } from '../../types';
import { useProjectStore } from '../../stores/projectStore';
import { FileIcon, FolderIcon, Image, Film, FileText, Box } from 'lucide-react';
import { FileContextMenu } from './FileContextMenu';

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
      <div className="flex-1 overflow-auto">
        {files.map(file => {
          const isSelected = selectedFiles.has(file.path);
          const fileTagIds = fileTags.get(file.path) || [];
          const fileTagList = tags.filter(t => fileTagIds.includes(t.id));
          
          return (
            <div
              key={file.path}
              className={`
                flex items-center border-b border-gray-100 dark:border-gray-800
                cursor-pointer select-none
                ${isSelected 
                  ? 'bg-blue-50 dark:bg-blue-900/20' 
                  : 'hover:bg-gray-50 dark:hover:bg-gray-800/50'
                }
              `}
              onClick={(e) => onSelect(file.path, e.ctrlKey || e.metaKey)}
              onDoubleClick={() => onDoubleClick(file)}
              onContextMenu={(e) => {
                e.preventDefault();
                onContextMenu(file, e.clientX, e.clientY);
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
  currentPath,
  tags,
  fileTags,
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
  currentPath: string;
  tags: any[];
  fileTags: Map<string, string[]>;
}) {
  return (
    <div className="p-4 grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4 overflow-auto">
      {files.map(file => {
        const isSelected = selectedFiles.has(file.path);
        const fileTagIds = fileTags.get(file.path) || [];
        const fileTagList = tags.filter(t => fileTagIds.includes(t.id));
        
        return (
          <div
            key={file.path}
            className={`
              group relative p-3 rounded-lg cursor-pointer select-none
              ${isSelected 
                ? 'bg-blue-50 dark:bg-blue-900/20 ring-2 ring-blue-500' 
                : 'hover:bg-gray-50 dark:hover:bg-gray-800'
              }
            `}
            onClick={(e) => onSelect(file.path, e.ctrlKey || e.metaKey)}
            onDoubleClick={() => onDoubleClick(file)}
            onContextMenu={(e) => {
              e.preventDefault();
              onContextMenu(file, e.clientX, e.clientY);
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

  // 右键菜单状态
  const [contextMenu, setContextMenu] = useState<{
    file: FileInfo;
    x: number;
    y: number;
  } | null>(null);

  const displayFiles = searchQuery ? searchResults : files;

  const handleDoubleClick = (file: FileInfo) => {
    if (file.is_dir) {
      loadDirectory(file.path);
    }
  };

  const handleContextMenu = (file: FileInfo, x: number, y: number) => {
    setContextMenu({ file, x, y });
  };

  const handleCloseContextMenu = () => {
    setContextMenu(null);
  };

  const handleRefresh = () => {
    refresh();
  };

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
        />
      )}
    </div>
  );
}
