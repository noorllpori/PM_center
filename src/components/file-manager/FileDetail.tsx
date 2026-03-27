import { useProjectStore } from '../../stores/projectStore';
import { FileIcon, FolderIcon, Image, Film, FileText, Box, Calendar, HardDrive, Hash, Tag } from 'lucide-react';
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

// 格式化文件大小
function formatSize(bytes: number): string {
  if (bytes === 0) return '-';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
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
function getFileIcon(file: { is_dir: boolean; extension?: string | null }) {
  if (file.is_dir) {
    return <FolderIcon className="w-16 h-16 text-yellow-500" />;
  }
  
  const ext = file.extension?.toLowerCase();
  
  if (['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'exr', 'hdr'].includes(ext || '')) {
    return <Image className="w-16 h-16 text-purple-500" />;
  }
  
  if (['mp4', 'mov', 'avi', 'mkv', 'webm'].includes(ext || '')) {
    return <Film className="w-16 h-16 text-red-500" />;
  }
  
  if (ext === 'blend') {
    return <Box className="w-16 h-16 text-orange-500" />;
  }
  
  if (['txt', 'md', 'json', 'xml', 'py'].includes(ext || '')) {
    return <FileText className="w-16 h-16 text-blue-500" />;
  }
  
  return <FileIcon className="w-16 h-16 text-gray-400" />;
}

export function FileDetail() {
  const { selectedFiles, files, fileTags, tags } = useProjectStore();
  const [fileInfo, setFileInfo] = useState<any>(null);

  // 获取选中的文件
  const selectedPaths = Array.from(selectedFiles);
  const selectedFile = selectedPaths.length === 1
    ? files.find(f => f.path === selectedPaths[0])
    : null;

  // 加载文件详情
  useEffect(() => {
    if (selectedFile) {
      invoke('get_file_info', { path: selectedFile.path })
        .then(info => setFileInfo(info))
        .catch(() => setFileInfo(null));
    } else {
      setFileInfo(null);
    }
  }, [selectedFile]);

  // 如果没有选中单个文件
  if (!selectedFile) {
    return (
      <div className="h-full flex flex-col bg-white dark:bg-gray-900">
        <div className="flex-1 flex items-center justify-center text-gray-400 text-sm p-4 text-center">
          <div>
            <FileIcon className="w-12 h-12 mx-auto mb-3 opacity-50" />
            <p>选择一个文件查看详情</p>
            <p className="text-xs mt-1 text-gray-300">
              {selectedPaths.length > 1 ? `已选择 ${selectedPaths.length} 个文件` : '没有选择文件'}
            </p>
          </div>
        </div>
      </div>
    );
  }

  const fileTagIds = fileTags.get(selectedFile.path) || [];
  const fileTagList = tags.filter(t => fileTagIds.includes(t.id));

  return (
    <div className="h-full flex flex-col bg-white dark:bg-gray-900 overflow-auto">
      {/* 文件图标 */}
      <div className="flex justify-center py-8 bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">
        {getFileIcon(selectedFile)}
      </div>

      {/* 文件信息 */}
      <div className="p-4 space-y-4">
        {/* 文件名 */}
        <div>
          <h3 className="font-medium text-sm text-gray-900 dark:text-gray-100 break-all">
            {selectedFile.name}
          </h3>
          <p className="text-xs text-gray-500 mt-1 break-all">
            {selectedFile.path}
          </p>
        </div>

        {/* 基本信息 */}
        <div className="space-y-3">
          <div className="flex items-center gap-3 text-sm">
            <HardDrive className="w-4 h-4 text-gray-400" />
            <span className="text-gray-500">大小</span>
            <span className="flex-1 text-right text-gray-900 dark:text-gray-100">
              {formatSize(selectedFile.size)}
            </span>
          </div>

          <div className="flex items-center gap-3 text-sm">
            <Hash className="w-4 h-4 text-gray-400" />
            <span className="text-gray-500">类型</span>
            <span className="flex-1 text-right text-gray-900 dark:text-gray-100">
              {selectedFile.is_dir ? '文件夹' : selectedFile.extension?.toUpperCase() || '文件'}
            </span>
          </div>

          <div className="flex items-center gap-3 text-sm">
            <Calendar className="w-4 h-4 text-gray-400" />
            <span className="text-gray-500">修改时间</span>
            <span className="flex-1 text-right text-gray-900 dark:text-gray-100">
              {formatDate(selectedFile.modified)}
            </span>
          </div>

          {selectedFile.created && (
            <div className="flex items-center gap-3 text-sm">
              <Calendar className="w-4 h-4 text-gray-400" />
              <span className="text-gray-500">创建时间</span>
              <span className="flex-1 text-right text-gray-900 dark:text-gray-100">
                {formatDate(selectedFile.created)}
              </span>
            </div>
          )}
        </div>

        {/* 标签 */}
        <div className="pt-4 border-t border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 mb-3">
            <Tag className="w-4 h-4 text-gray-400" />
            <span className="text-sm font-medium text-gray-700 dark:text-gray-300">标签</span>
          </div>
          
          {fileTagList.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {fileTagList.map(tag => (
                <span
                  key={tag.id}
                  className="px-2 py-1 text-xs rounded"
                  style={{ 
                    backgroundColor: tag.color + '20', 
                    color: tag.color,
                    border: `1px solid ${tag.color}40`
                  }}
                >
                  {tag.name}
                </span>
              ))}
            </div>
          ) : (
            <p className="text-xs text-gray-400">暂无标签</p>
          )}
        </div>

        {/* 操作按钮 */}
        <div className="pt-4 border-t border-gray-200 dark:border-gray-700 space-y-2">
          <button
            className="w-full px-3 py-2 text-sm text-gray-700 bg-gray-100 hover:bg-gray-200 
                       dark:text-gray-300 dark:bg-gray-800 dark:hover:bg-gray-700
                       rounded transition-colors"
            onClick={() => {
              // 在资源管理器中显示
              invoke('show_in_folder', { path: selectedFile.path }).catch(() => {});
            }}
          >
            在文件夹中显示
          </button>
        </div>
      </div>
    </div>
  );
}
