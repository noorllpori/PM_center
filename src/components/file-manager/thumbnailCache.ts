import { invoke } from '@tauri-apps/api/core';
import { FileInfo } from '../../types';

const pendingPreviewThumbnailWrites = new Set<string>();

function buildPreviewThumbnailKey(file: FileInfo) {
  return `${file.path}:${file.size}:${file.modified || ''}`;
}

export async function cacheResolvedPreviewThumbnail(
  projectPath: string | null,
  file: FileInfo | null,
  resolvedSource: string | null,
) {
  if (!projectPath || !file || file.is_dir || file.thumbnail || !resolvedSource) {
    return;
  }

  if (file.extension?.toLowerCase() !== 'psd') {
    return;
  }

  const cacheKey = buildPreviewThumbnailKey(file);
  if (pendingPreviewThumbnailWrites.has(cacheKey)) {
    return;
  }

  pendingPreviewThumbnailWrites.add(cacheKey);

  try {
    const response = await fetch(resolvedSource);
    const buffer = await response.arrayBuffer();
    await invoke('store_cached_thumbnail', {
      projectPath,
      sourcePath: file.path,
      pngBytes: Array.from(new Uint8Array(buffer)),
    });
  } catch (error) {
    pendingPreviewThumbnailWrites.delete(cacheKey);
    console.warn('Failed to cache rendered preview thumbnail:', file.path, error);
  }
}
