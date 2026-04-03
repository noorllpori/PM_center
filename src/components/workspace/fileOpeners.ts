import { isImageExtension } from '../image-viewer/imageViewerUtils';

export type WorkspaceOpenTarget = 'image' | 'text' | 'video';

const TEXT_FILE_EXTENSIONS = new Set([
  'txt',
  'md',
  'markdown',
  'mdx',
  'csv',
  'tsv',
  'json',
  'jsonc',
  'js',
  'mjs',
  'cjs',
  'jsx',
  'ts',
  'mts',
  'cts',
  'tsx',
  'html',
  'htm',
  'vue',
  'svelte',
  'astro',
  'css',
  'scss',
  'sass',
  'less',
  'py',
  'pyi',
  'pyw',
  'rs',
  'c',
  'h',
  'cc',
  'cpp',
  'cxx',
  'hpp',
  'hxx',
  'cs',
  'java',
  'kt',
  'kts',
  'go',
  'php',
  'rb',
  'swift',
  'sh',
  'bash',
  'zsh',
  'bat',
  'cmd',
  'ps1',
  'psm1',
  'psd1',
  'xml',
  'yml',
  'yaml',
  'toml',
  'ini',
  'conf',
  'cfg',
  'config',
  'properties',
  'env',
  'gitignore',
  'gitattributes',
  'editorconfig',
  'dockerfile',
  'makefile',
  'mk',
  'gradle',
  'sql',
  'prisma',
  'graphql',
  'gql',
  'lua',
  'log',
]);

const VIDEO_FILE_EXTENSIONS = new Set([
  'mp4',
  'm4v',
  'mov',
  'avi',
  'mkv',
  'webm',
  'wmv',
  'flv',
  'mpeg',
  'mpg',
  'm2ts',
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

export function isVideoExtension(extension?: string | null): boolean {
  return !!extension && VIDEO_FILE_EXTENSIONS.has(extension.toLowerCase());
}

export function getWorkspaceOpenTarget(pathOrExtension?: string | null): WorkspaceOpenTarget | null {
  const extension = getFileExtension(pathOrExtension);

  if (isImageExtension(extension)) {
    return 'image';
  }

  if (isVideoExtension(extension)) {
    return 'video';
  }

  if (isTextExtension(extension)) {
    return 'text';
  }

  return null;
}
