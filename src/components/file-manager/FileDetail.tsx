import { memo, useEffect, useMemo, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Box, FileIcon, FileText, Film, FolderIcon, Image } from 'lucide-react';
import { normalizeMdtReferenceKey } from '../../utils/mdt';
import type { FileInfo } from '../../types';
import { FileDetailsPanel } from './FileDetailsView';
import { useProjectStoreShallow } from '../../stores/projectStore';
import {
  isDirectPreviewImageExtension,
  isImageExtension,
} from '../image-viewer/imageViewerUtils';
import { useResolvedImageSource } from '../image-viewer/useResolvedImageSource';
import { isTextExtension, isVideoExtension } from '../workspace/fileOpeners';
import { cacheResolvedPreviewThumbnail } from './thumbnailCache';

function resolvePreviewSource(path: string | null) {
  if (!path) {
    return null;
  }

  if (/^(asset|https?|data|blob):/i.test(path)) {
    return path;
  }

  return convertFileSrc(path);
}

function getPreviewSource(file: FileInfo): { kind: 'image' | 'video'; src: string } | null {
  if (file.is_dir) {
    return null;
  }

  if (file.thumbnail) {
    const src = resolvePreviewSource(file.thumbnail);
    return src ? { kind: 'image', src } : null;
  }

  const extension = file.extension?.toLowerCase() || '';

  if (isImageExtension(extension) && isDirectPreviewImageExtension(extension)) {
    const src = resolvePreviewSource(file.path);
    return src ? { kind: 'image', src } : null;
  }

  if (extension === 'psd') {
    return { kind: 'image', src: file.path };
  }

  if (isVideoExtension(extension)) {
    const src = resolvePreviewSource(file.path);
    return src ? { kind: 'video', src } : null;
  }

  return null;
}

function getPreviewIcon(file: FileInfo) {
  if (file.is_dir) {
    return <FolderIcon className="h-8 w-8 text-yellow-500" />;
  }

  const extension = file.extension?.toLowerCase() || '';

  if (isImageExtension(extension)) {
    return <Image className="h-8 w-8 text-purple-500" />;
  }

  if (isVideoExtension(extension)) {
    return <Film className="h-8 w-8 text-red-500" />;
  }

  if (extension === 'blend') {
    return <Box className="h-8 w-8 text-orange-500" />;
  }

  if (isTextExtension(extension)) {
    return <FileText className="h-8 w-8 text-blue-500" />;
  }

  return <FileIcon className="h-8 w-8 text-gray-400" />;
}

const MultiSelectPreviewItem = memo(function MultiSelectPreviewItem({
  file,
  projectPath,
}: {
  file: FileInfo;
  projectPath: string | null;
}) {
  const preview = useMemo(() => getPreviewSource(file), [file]);
  const {
    resolvedSource,
    isLoading,
    errorMessage,
  } = useResolvedImageSource(preview?.kind === 'image' ? preview.src : '');
  const [hasPreviewError, setHasPreviewError] = useState(false);

  useEffect(() => {
    setHasPreviewError(false);
  }, [file.path, preview?.kind, preview?.src]);

  useEffect(() => {
    if (preview?.kind === 'image' && errorMessage) {
      setHasPreviewError(true);
    }
  }, [errorMessage, preview?.kind]);

  useEffect(() => {
    if (preview?.kind !== 'image' || !resolvedSource) {
      return;
    }

    void cacheResolvedPreviewThumbnail(projectPath, file, resolvedSource);
  }, [file, preview?.kind, projectPath, resolvedSource]);

  return (
    <div
      className="min-w-0 rounded-md border border-gray-200 bg-white p-1.5 shadow-sm dark:border-gray-700 dark:bg-gray-900"
      title={file.path}
    >
      <div className="aspect-square overflow-hidden rounded bg-gray-100 dark:bg-gray-800">
        {!preview || hasPreviewError ? (
          <div className="flex h-full w-full items-center justify-center">
            {getPreviewIcon(file)}
          </div>
        ) : preview.kind === 'image' ? (
          resolvedSource ? (
            <img
              src={resolvedSource}
              alt={file.name}
              className="h-full w-full object-cover"
              loading="lazy"
              onError={() => setHasPreviewError(true)}
            />
          ) : (
            <div className="flex h-full w-full items-center justify-center">
              {isLoading ? (
                <div className="h-8 w-8 animate-pulse rounded-lg bg-white/70 dark:bg-white/10" />
              ) : (
                getPreviewIcon(file)
              )}
            </div>
          )
        ) : (
          <video
            src={preview.src}
            className="h-full w-full object-cover"
            preload="metadata"
            muted
            playsInline
            onError={() => setHasPreviewError(true)}
          />
        )}
      </div>
      <div className="mt-1 truncate text-center text-xs text-gray-700 dark:text-gray-200">
        {file.name}
      </div>
    </div>
  );
});

function MultiSelectPreviewPanel({
  files,
  selectedCount,
  projectPath,
}: {
  files: FileInfo[];
  selectedCount: number;
  projectPath: string | null;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col bg-white dark:bg-gray-900">
      <div className="border-b border-gray-200 px-3 py-2 dark:border-gray-700">
        <div className="text-sm font-semibold text-gray-900 dark:text-gray-100">
          已选择 {selectedCount} 个项目
        </div>
        <div className="mt-0.5 text-xs text-gray-500 dark:text-gray-400">
          {files.length} 个可预览项目
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-3">
        <div
          className="grid gap-2"
          style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(96px, 1fr))' }}
        >
          {files.map((file) => (
            <MultiSelectPreviewItem
              key={file.path}
              file={file}
              projectPath={projectPath}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

export function FileDetail() {
  const { selectedFiles, files, searchResults, searchQuery, fileTags, tags, mdtReferencesByFile, projectPath } = useProjectStoreShallow((state) => ({
    selectedFiles: state.selectedFiles,
    files: state.files,
    searchResults: state.searchResults,
    searchQuery: state.searchQuery,
    fileTags: state.fileTags,
    tags: state.tags,
    mdtReferencesByFile: state.mdtReferencesByFile,
    projectPath: state.projectPath,
  }));

  const selectedPaths = Array.from(selectedFiles);
  const displayFiles = searchQuery ? searchResults : files;
  const selectedFileInfos = useMemo(() => {
    const selectedPathSet = new Set(selectedPaths);
    const knownFileMap = new Map(
      [...displayFiles, ...files, ...searchResults].map((file) => [file.path, file] as const),
    );
    const orderedFiles = displayFiles.filter((file) => selectedPathSet.has(file.path));
    const orderedPathSet = new Set(orderedFiles.map((file) => file.path));
    const remainingFiles = selectedPaths
      .filter((path) => !orderedPathSet.has(path))
      .map((path) => knownFileMap.get(path))
      .filter((file): file is FileInfo => Boolean(file));

    return [...orderedFiles, ...remainingFiles];
  }, [displayFiles, files, searchResults, selectedPaths]);
  const selectedFile = selectedPaths.length === 1
    ? displayFiles.find((file) => file.path === selectedPaths[0]) || files.find((file) => file.path === selectedPaths[0]) || null
    : null;

  const fileTagIds = selectedFile ? (fileTags.get(selectedFile.path) || []) : [];
  const fileTagList = tags.filter((tag) => fileTagIds.includes(tag.id));
  const relatedMdtEntries = selectedFile
    ? (mdtReferencesByFile.get(normalizeMdtReferenceKey(selectedFile.path)) || [])
    : [];

  if (selectedPaths.length > 1) {
    return (
      <MultiSelectPreviewPanel
        files={selectedFileInfos}
        selectedCount={selectedPaths.length}
        projectPath={projectPath}
      />
    );
  }

  return (
    <FileDetailsPanel
      file={selectedFile}
      fileTagList={fileTagList}
      relatedMdtEntries={relatedMdtEntries}
      selectedCount={selectedPaths.length}
    />
  );
}
