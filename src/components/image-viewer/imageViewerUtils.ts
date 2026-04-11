export const IMAGE_FILE_EXTENSIONS = new Set([
  'avif',
  'bmp',
  'gif',
  'hdr',
  'heic',
  'heif',
  'jpeg',
  'jpg',
  'png',
  'psd',
  'svg',
  'tif',
  'tiff',
  'webp',
  'exr',
]);

const DIRECT_PREVIEW_IMAGE_EXTENSIONS = new Set([
  'avif',
  'bmp',
  'gif',
  'jpeg',
  'jpg',
  'png',
  'svg',
  'webp',
]);

const IMAGE_MIME_TYPES: Record<string, string> = {
  avif: 'image/avif',
  bmp: 'image/bmp',
  gif: 'image/gif',
  hdr: 'image/vnd.radiance',
  heic: 'image/heic',
  heif: 'image/heif',
  jpeg: 'image/jpeg',
  jpg: 'image/jpeg',
  png: 'image/png',
  svg: 'image/svg+xml',
  tif: 'image/tiff',
  tiff: 'image/tiff',
  webp: 'image/webp',
  exr: 'image/x-exr',
};

export function isImageExtension(extension?: string | null): boolean {
  return !!extension && IMAGE_FILE_EXTENSIONS.has(extension.toLowerCase());
}

export function isPsdExtension(extension?: string | null): boolean {
  return !!extension && extension.toLowerCase() === 'psd';
}

export function isDirectPreviewImageExtension(extension?: string | null): boolean {
  return !!extension && DIRECT_PREVIEW_IMAGE_EXTENSIONS.has(extension.toLowerCase());
}

export function getImageExtension(pathOrExtension: string): string {
  return pathOrExtension
    .split(/[\\/]/)
    .pop()
    ?.split('.')
    .pop()
    ?.toLowerCase() || '';
}

export function getImageMimeType(pathOrExtension: string): string {
  const normalized = getImageExtension(pathOrExtension);

  return IMAGE_MIME_TYPES[normalized] || 'application/octet-stream';
}
