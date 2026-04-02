import { isImageExtension } from '../image-viewer/imageViewerUtils';

export type WorkspaceOpenTarget = 'image' | 'text';

const TEXT_FILE_EXTENSIONS = new Set([
  'txt',
  'md',
  'markdown',
  'json',
  'js',
  'jsx',
  'ts',
  'tsx',
  'html',
  'htm',
  'css',
  'scss',
  'sass',
  'less',
  'py',
  'rs',
  'xml',
  'yml',
  'yaml',
  'toml',
  'ini',
  'log',
]);

export function getFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function getFileExtension(pathOrExtension?: string | null): string {
  if (!pathOrExtension) {
    return '';
  }

  const normalized = pathOrExtension.split(/[\\/]/).pop() || pathOrExtension;
  const dotIndex = normalized.lastIndexOf('.');
  if (dotIndex < 0) {
    return normalized.toLowerCase();
  }

  return normalized.slice(dotIndex + 1).toLowerCase();
}

export function isTextExtension(extension?: string | null): boolean {
  return !!extension && TEXT_FILE_EXTENSIONS.has(extension.toLowerCase());
}

export function getWorkspaceOpenTarget(pathOrExtension?: string | null): WorkspaceOpenTarget | null {
  const extension = getFileExtension(pathOrExtension);

  if (isImageExtension(extension)) {
    return 'image';
  }

  if (isTextExtension(extension)) {
    return 'text';
  }

  return null;
}
