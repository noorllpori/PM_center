import { useEffect, useMemo, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { Box, ExternalLink, FileIcon, FileText, Film, FolderIcon, Hash, Image, Music4, RefreshCw, Tag as TagIcon } from 'lucide-react';
import { Dialog } from '../Dialog';
import { FileDetailsResponse, FileInfo, Tag } from '../../types';
import { useFileDetails } from './useFileDetails';

interface FileDetailsContentProps {
  file: FileInfo | null;
  fileTagList: Tag[];
  view: 'panel' | 'dialog';
  selectedCount?: number;
}

interface FileDetailsDialogProps {
  file: FileInfo | null;
  fileTagList: Tag[];
  isOpen: boolean;
  onClose: () => void;
}

const IMAGE_EXTENSIONS = new Set(['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'exr', 'hdr', 'tif', 'tiff', 'svg']);
const AUDIO_EXTENSIONS = new Set(['mp3', 'flac', 'wav', 'ogg', 'opus', 'm4a', 'aac']);
const VIDEO_EXTENSIONS = new Set(['mp4', 'mov', 'avi', 'mkv', 'webm', 'm4v']);
const TEXT_EXTENSIONS = new Set(['txt', 'md', 'json', 'xml', 'py', 'js', 'ts', 'tsx', 'jsx', 'html', 'css']);

function getFileExtension(file: FileInfo | null) {
  return file?.extension?.toLowerCase() || '';
}

function getFileIcon(file: FileInfo | null) {
  if (!file) {
    return <FileIcon className="w-16 h-16 text-gray-400" />;
  }

  if (file.is_dir) {
    return <FolderIcon className="w-16 h-16 text-yellow-500" />;
  }

  const ext = getFileExtension(file);

  if (IMAGE_EXTENSIONS.has(ext)) {
    return <Image className="w-16 h-16 text-purple-500" />;
  }

  if (AUDIO_EXTENSIONS.has(ext)) {
    return <Music4 className="w-16 h-16 text-emerald-500" />;
  }

  if (VIDEO_EXTENSIONS.has(ext)) {
    return <Film className="w-16 h-16 text-red-500" />;
  }

  if (ext === 'blend') {
    return <Box className="w-16 h-16 text-orange-500" />;
  }

  if (TEXT_EXTENSIONS.has(ext)) {
    return <FileText className="w-16 h-16 text-blue-500" />;
  }

  return <FileIcon className="w-16 h-16 text-gray-400" />;
}

function resolvePreviewSource(path: string | null) {
  if (!path) {
    return null;
  }

  if (/^(asset|https?|data|blob):/i.test(path)) {
    return path;
  }

  return convertFileSrc(path);
}

function getFilePreview(file: FileInfo | null): { kind: 'image' | 'video'; src: string } | null {
  if (!file || file.is_dir) {
    return null;
  }

  if (file.thumbnail) {
    const src = resolvePreviewSource(file.thumbnail);
    return src ? { kind: 'image', src } : null;
  }

  const ext = getFileExtension(file);
  const src = resolvePreviewSource(file.path);
  if (!src) {
    return null;
  }

  if (IMAGE_EXTENSIONS.has(ext)) {
    return { kind: 'image', src };
  }

  if (VIDEO_EXTENSIONS.has(ext)) {
    return { kind: 'video', src };
  }

  return null;
}

function FilePreviewHeader({ file }: { file: FileInfo | null }) {
  const preview = useMemo(() => getFilePreview(file), [file]);
  const [hasPreviewError, setHasPreviewError] = useState(false);

  useEffect(() => {
    setHasPreviewError(false);
  }, [preview?.kind, preview?.src, file?.path]);

  if (!preview || hasPreviewError) {
    return (
      <div className="flex justify-center py-8 bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">
        {getFileIcon(file)}
      </div>
    );
  }

  return (
    <div className="border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
      <div className="flex justify-center px-4 py-4">
        <div className="flex w-full items-center justify-center overflow-hidden rounded-xl border border-white/70 bg-gradient-to-br from-white to-gray-100 shadow-sm dark:border-gray-700 dark:from-gray-900 dark:to-gray-800">
          {preview.kind === 'image' ? (
            <img
              src={preview.src}
              alt={file?.name || '文件预览'}
              className="max-h-[260px] w-full object-contain"
              onError={() => setHasPreviewError(true)}
            />
          ) : (
            <video
              src={preview.src}
              className="max-h-[260px] w-full bg-black object-contain"
              controls
              preload="metadata"
              muted
              onError={() => setHasPreviewError(true)}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function SectionBlock({ title, items }: { title: string; items: FileDetailsResponse['sections'][number]['items'] }) {
  if (items.length === 0) {
    return null;
  }

  return (
    <div className="pt-4 border-t border-gray-200 dark:border-gray-700 first:pt-0 first:border-t-0">
      <div className="flex items-center gap-2 mb-3">
        <Hash className="w-4 h-4 text-gray-400" />
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">{title}</span>
      </div>
      <div className="space-y-3">
        {items.map((entry, index) => (
          <div key={`${entry.label}-${index}`} className="flex items-start gap-3 text-sm">
            <span className="min-w-[72px] text-gray-500">{entry.label}</span>
            <span className="flex-1 text-right text-gray-900 dark:text-gray-100 break-all">{entry.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function FileDetailsContent({ file, fileTagList, view, selectedCount = 0 }: FileDetailsContentProps) {
  const { details, isLoading, isRefreshing, errorMessage, refresh } = useFileDetails(file, view);

  const displayPath = details?.basic.path || file?.path || '';
  const displayName = details?.basic.name || file?.name || '';
  const sections = details?.sections || [];
  const hasDetails = details !== null;

  const actionButton = useMemo(() => {
    if (!file) {
      return null;
    }

    return (
      <button
        className="w-full px-3 py-2 text-sm text-gray-700 bg-gray-100 hover:bg-gray-200
                   dark:text-gray-300 dark:bg-gray-800 dark:hover:bg-gray-700
                   rounded transition-colors flex items-center justify-center gap-2"
        onClick={() => {
          invoke('show_in_folder', { path: file.path }).catch(() => {});
        }}
      >
        <ExternalLink className="w-4 h-4" />
        在文件夹中显示
      </button>
    );
  }, [file]);

  if (!file) {
    return (
      <div className="h-full flex flex-col bg-white dark:bg-gray-900">
        <div className="flex-1 flex items-center justify-center text-gray-400 text-sm p-4 text-center">
          <div>
            <FileIcon className="w-12 h-12 mx-auto mb-3 opacity-50" />
            <p>选择一个文件查看详情</p>
            <p className="text-xs mt-1 text-gray-300">
              {selectedCount > 1 ? `已选择 ${selectedCount} 个文件` : '没有选择文件'}
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={`h-full flex flex-col bg-white dark:bg-gray-900 ${view === 'panel' ? 'overflow-auto' : ''}`}>
      <FilePreviewHeader file={file} />

      <div className="p-4 space-y-4">
        <div>
          <h3 className="font-medium text-sm text-gray-900 dark:text-gray-100 break-all">{displayName}</h3>
          <p className="text-xs text-gray-500 mt-1 break-all">{displayPath}</p>
        </div>

        {isLoading && (
          <div className="rounded-lg border border-blue-200 bg-blue-50/80 px-3 py-2 text-sm text-blue-700">
            正在分析文件信息...
          </div>
        )}

        {isRefreshing && hasDetails && (
          <div className="rounded-lg border border-blue-200 bg-blue-50/80 px-3 py-2 text-sm text-blue-700">
            正在刷新文件信息...
          </div>
        )}

        {errorMessage && (
          <div
            className={`rounded-lg px-3 py-2 text-sm break-all ${
              hasDetails
                ? 'border border-yellow-200 bg-yellow-50/80 text-yellow-800'
                : 'border border-red-200 bg-red-50/80 text-red-700'
            }`}
          >
            {hasDetails ? errorMessage : `无法读取详细信息：${errorMessage}`}
          </div>
        )}

        {!isLoading && sections.map((section) => (
          <SectionBlock key={section.id} title={section.title} items={section.items} />
        ))}

        <div className="pt-4 border-t border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 mb-3">
            <TagIcon className="w-4 h-4 text-gray-400" />
            <span className="text-sm font-medium text-gray-700 dark:text-gray-300">标签</span>
          </div>

          {fileTagList.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {fileTagList.map((tag) => (
                <span
                  key={tag.id}
                  className="px-2 py-1 text-xs rounded"
                  style={{
                    backgroundColor: `${tag.color}20`,
                    color: tag.color,
                    border: `1px solid ${tag.color}40`,
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

        <div className="pt-4 border-t border-gray-200 dark:border-gray-700 space-y-2">
          <button
            className="w-full px-3 py-2 text-sm text-gray-700 bg-gray-100 hover:bg-gray-200
                       dark:text-gray-300 dark:bg-gray-800 dark:hover:bg-gray-700
                       rounded transition-colors flex items-center justify-center gap-2 disabled:opacity-60 disabled:cursor-not-allowed"
            onClick={() => {
              void refresh();
            }}
            disabled={isLoading || isRefreshing}
          >
            <RefreshCw className={`w-4 h-4 ${isRefreshing ? 'animate-spin' : ''}`} />
            {isRefreshing ? '刷新中...' : '刷新'}
          </button>
          {actionButton}
        </div>
      </div>
    </div>
  );
}

export function FileDetailsPanel({
  file,
  fileTagList,
  selectedCount = 0,
}: {
  file: FileInfo | null;
  fileTagList: Tag[];
  selectedCount?: number;
}) {
  return (
    <FileDetailsContent
      file={file}
      fileTagList={fileTagList}
      view="panel"
      selectedCount={selectedCount}
    />
  );
}

export function FileDetailsDialog({ file, fileTagList, isOpen, onClose }: FileDetailsDialogProps) {
  return (
    <Dialog
      isOpen={isOpen && !!file}
      onClose={onClose}
      title="详细信息"
      size="lg"
    >
      <FileDetailsContent
        file={file}
        fileTagList={fileTagList}
        view="dialog"
      />
    </Dialog>
  );
}
