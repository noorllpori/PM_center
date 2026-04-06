export const INTERNAL_FILE_DRAG_MIME = 'application/x-pmcenter-files';

export function normalizePath(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '');
}

export function getFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function getParentPath(path: string): string {
  const parts = path.split(/[\\/]/);
  parts.pop();
  return parts.join(path.includes('\\') ? '\\' : '/');
}

export function joinPath(dir: string, name: string): string {
  if (!dir) {
    return name;
  }

  const separator = dir.includes('\\') ? '\\' : '/';
  return `${dir.replace(/[\\/]+$/, '')}${separator}${name}`;
}

export function appendRelativePath(basePath: string, relativePath: string): string {
  if (!relativePath) {
    return basePath;
  }

  const separator = basePath.includes('\\') ? '\\' : '/';
  const cleanedRelativePath = relativePath
    .split(/[\\/]+/)
    .filter(Boolean)
    .join(separator);

  return joinPath(basePath, cleanedRelativePath);
}

export function getPathLabel(targetPath: string | null, projectPath: string | null, projectName: string | null): string {
  if (!targetPath) return '当前目录';
  if (!projectPath || !projectName) return targetPath;

  const normalizedProjectPath = normalizePath(projectPath);
  const normalizedTargetPath = normalizePath(targetPath);

  if (normalizedTargetPath === normalizedProjectPath) {
    return projectName;
  }

  if (normalizedTargetPath.startsWith(normalizedProjectPath + '/')) {
    return normalizedTargetPath.replace(normalizedProjectPath, projectName);
  }

  return targetPath;
}

export function isSameOrDescendantPath(path: string, parentPath: string): boolean {
  const normalizedPath = normalizePath(path);
  const normalizedParent = normalizePath(parentPath);

  return normalizedPath === normalizedParent || normalizedPath.startsWith(`${normalizedParent}/`);
}

export function compactDraggedPaths(paths: string[]): string[] {
  const uniquePaths = Array.from(new Set(paths));
  const sortedPaths = uniquePaths.sort((a, b) => normalizePath(a).length - normalizePath(b).length);

  return sortedPaths.filter((candidate, index) => {
    return !sortedPaths.slice(0, index).some((existing) => isSameOrDescendantPath(candidate, existing));
  });
}

export function setFileDragData(dataTransfer: DataTransfer, paths: string[]): void {
  const compactPaths = compactDraggedPaths(paths);
  dataTransfer.effectAllowed = 'move';
  dataTransfer.setData(INTERNAL_FILE_DRAG_MIME, JSON.stringify(compactPaths));
  dataTransfer.setData('text/plain', compactPaths.map(getFileNameFromPath).join('\n'));
}

export function canMovePathsToDirectory(targetDir: string, dragPaths: string[]): boolean {
  if (!targetDir || dragPaths.length === 0) {
    return false;
  }

  return dragPaths.some((path) => {
    if (getParentPath(path) === targetDir) {
      return false;
    }

    return !isSameOrDescendantPath(targetDir, path);
  });
}

export function getInternalDragPaths(dataTransfer: DataTransfer | null): string[] {
  if (!dataTransfer) {
    return [];
  }

  try {
    const raw = dataTransfer.getData(INTERNAL_FILE_DRAG_MIME);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : [];
  } catch {
    return [];
  }
}

export function hasInternalDragData(dataTransfer: DataTransfer | null): boolean {
  if (!dataTransfer) {
    return false;
  }

  return Array.from(dataTransfer.types || []).includes(INTERNAL_FILE_DRAG_MIME);
}

export function isExternalFileDrag(dataTransfer: DataTransfer | null, hasActiveInternalDrag = false): boolean {
  if (!dataTransfer || hasActiveInternalDrag || hasInternalDragData(dataTransfer)) {
    return false;
  }

  return Array.from(dataTransfer.types || []).includes('Files');
}

export function resolveInternalDragPaths(dataTransfer: DataTransfer | null, fallbackPaths: string[]): string[] {
  const internalPaths = getInternalDragPaths(dataTransfer);
  if (internalPaths.length > 0) {
    return compactDraggedPaths(internalPaths);
  }

  const compactFallbackPaths = compactDraggedPaths(fallbackPaths);
  if (compactFallbackPaths.length === 0) {
    return [];
  }

  if (!dataTransfer) {
    return compactFallbackPaths;
  }

  return compactFallbackPaths;
}
