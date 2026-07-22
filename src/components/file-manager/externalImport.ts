import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { exists, mkdir, remove, writeFile } from '@tauri-apps/plugin-fs';
import {
  buildRenamedFileName,
  getFileNameFromPath,
  getParentPath,
  joinPath,
  normalizePath,
} from './dragDrop';

export interface ExternalDropImportResult {
  successCount: number;
  overwriteCount: number;
  renameCount: number;
  skippedCount: number;
  failedItems: string[];
}

export interface ExternalImportProgress {
  itemIndex: number;
  itemCount: number;
  currentName: string;
  targetPath: string;
  bytesCopied: number;
  totalBytes: number;
  done: boolean;
}

export interface ConflictResolution {
  action: 'overwrite' | 'rename' | 'cancel';
  renameName?: string;
}

interface ExternalDropImportOptions {
  targetLabel?: string;
  requestConflictChoice?: (
    sourceName: string,
    targetLabel: string,
  ) => Promise<ConflictResolution>;
  onProgress?: (progress: ExternalImportProgress) => void;
  signal?: AbortSignal;
}

interface ImportProgressContext {
  itemIndex: number;
  itemCount: number;
  currentName: string;
  onProgress?: (progress: ExternalImportProgress) => void;
  signal?: AbortSignal;
}

interface FileCopyProgressEventPayload {
  progressId: string;
  source: string;
  target: string;
  bytesCopied: number;
  totalBytes: number;
  done: boolean;
}

type WebkitDataTransferItem = DataTransferItem & {
  webkitGetAsEntry: () => FileSystemEntry | null;
};

type DesktopFile = File & {
  path?: string;
};

type FileSystemEntryWithPath = FileSystemEntry & {
  path?: string;
  fullPath?: string;
};

type ExternalDropRoot =
  | {
      kind: 'entry';
      name: string;
      sourcePath?: string;
      importIntoTargetPath: (targetPath: string, progress: ImportProgressContext) => Promise<void>;
    }
  | {
      kind: 'file';
      name: string;
      sourcePath?: string;
      importIntoTargetPath: (targetPath: string, progress: ImportProgressContext) => Promise<void>;
    };

const FRONTEND_FILE_COPY_CHUNK_SIZE = 16 * 1024 * 1024;
const IMPORT_CANCELLED_MESSAGE = '导入已取消';

export class ExternalImportCancelledError extends Error {
  constructor() {
    super(IMPORT_CANCELLED_MESSAGE);
    this.name = 'ExternalImportCancelledError';
  }
}

export function isExternalImportCancelled(error: unknown): boolean {
  return error instanceof ExternalImportCancelledError
    || String(error).includes(IMPORT_CANCELLED_MESSAGE);
}

function throwIfImportCancelled(signal?: AbortSignal) {
  if (signal?.aborted) {
    throw new ExternalImportCancelledError();
  }
}

function looksLikeAbsolutePath(value: string): boolean {
  return /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\');
}

function getDroppedFileSourcePath(file: File): string | undefined {
  const candidate = (file as DesktopFile).path;
  return typeof candidate === 'string' && looksLikeAbsolutePath(candidate)
    ? candidate
    : undefined;
}

function getDroppedEntrySourcePath(entry: FileSystemEntry): string | undefined {
  const candidate = (entry as FileSystemEntryWithPath).path
    ?? (entry as FileSystemEntryWithPath).fullPath;
  return typeof candidate === 'string' && looksLikeAbsolutePath(candidate)
    ? candidate
    : undefined;
}

function arePathsEquivalent(left: string, right: string): boolean {
  return normalizePath(left).toLowerCase() === normalizePath(right).toLowerCase();
}

function normalizeDroppedPath(value: string): string {
  if (value.startsWith('/') && /^[a-zA-Z]:\//.test(value.slice(1))) {
    return value.slice(1).replace(/\//g, '\\');
  }

  return value;
}

function getPathName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function createProgressId(): string {
  const randomPart = Math.random().toString(36).slice(2);
  return `external-import-${Date.now()}-${randomPart}`;
}

function emitImportProgress(
  context: ImportProgressContext,
  targetPath: string,
  bytesCopied: number,
  totalBytes: number,
  done = false,
) {
  context.onProgress?.({
    itemIndex: context.itemIndex,
    itemCount: context.itemCount,
    currentName: context.currentName,
    targetPath,
    bytesCopied,
    totalBytes,
    done,
  });
}

async function readFileEntry(entry: FileSystemFileEntry): Promise<File> {
  return new Promise((resolve, reject) => {
    entry.file(resolve, reject);
  });
}

async function readAllDirectoryEntries(reader: FileSystemDirectoryReader): Promise<FileSystemEntry[]> {
  const entries: FileSystemEntry[] = [];

  while (true) {
    const batch = await new Promise<FileSystemEntry[]>((resolve, reject) => {
      reader.readEntries(resolve, reject);
    });

    if (batch.length === 0) {
      return entries;
    }

    entries.push(...batch);
  }
}

async function ensureParentDirectory(filePath: string): Promise<void> {
  const parentPath = getParentPath(filePath);
  if (!parentPath) {
    return;
  }

  await mkdir(parentPath, { recursive: true });
}

async function writeDroppedFile(
  targetPath: string,
  file: File,
  progress: ImportProgressContext,
): Promise<void> {
  throwIfImportCancelled(progress.signal);
  await ensureParentDirectory(targetPath);
  let bytesCopied = 0;
  let lastProgressAt = 0;

  const reportProgress = (done = false) => {
    const now = Date.now();
    if (!done && now - lastProgressAt < 100) {
      return;
    }

    lastProgressAt = now;
    emitImportProgress(progress, targetPath, bytesCopied, file.size, done);
  };

  reportProgress(false);

  if (file.size === 0) {
    await writeFile(targetPath, new Uint8Array());
  }

  for (let offset = 0; offset < file.size; offset += FRONTEND_FILE_COPY_CHUNK_SIZE) {
    throwIfImportCancelled(progress.signal);
    const chunk = file.slice(offset, offset + FRONTEND_FILE_COPY_CHUNK_SIZE);
    const bytes = new Uint8Array(await chunk.arrayBuffer());
    throwIfImportCancelled(progress.signal);

    // writeFile sends Uint8Array as the IPC request body. FileHandle.write wraps
    // the same bytes in JSON command arguments, which is much slower for large drops.
    await writeFile(targetPath, bytes, {
      append: offset > 0,
      create: true,
    });
    bytesCopied += bytes.byteLength;
    reportProgress(false);
  }

  throwIfImportCancelled(progress.signal);
  reportProgress(true);
}

async function copySourcePathToTarget(
  sourcePath: string,
  targetPath: string,
  progress: ImportProgressContext,
): Promise<void> {
  throwIfImportCancelled(progress.signal);
  const progressId = createProgressId();
  let unlisten: (() => void) | null = null;
  const cancelRustCopy = () => {
    void invoke('cancel_file_copy', { progressId }).catch(() => {});
  };

  try {
    unlisten = await listen<FileCopyProgressEventPayload>('pm-center:file-copy-progress', (event) => {
      const payload = event.payload;
      if (!payload || payload.progressId !== progressId) {
        return;
      }

      emitImportProgress(
        progress,
        payload.target || targetPath,
        payload.bytesCopied,
        payload.totalBytes,
        payload.done,
      );
    });

    progress.signal?.addEventListener('abort', cancelRustCopy, { once: true });

    try {
      await invoke('copy_path_to_target', {
        source: normalizeDroppedPath(sourcePath),
        target: targetPath,
        progressId,
      });
    } catch (error) {
      if (isExternalImportCancelled(error)) {
        throw new ExternalImportCancelledError();
      }
      throw error;
    }

    throwIfImportCancelled(progress.signal);
  } finally {
    progress.signal?.removeEventListener('abort', cancelRustCopy);
    unlisten?.();
  }
}

async function importEntry(
  entry: FileSystemEntry,
  targetPath: string,
  progress: ImportProgressContext,
): Promise<void> {
  throwIfImportCancelled(progress.signal);
  if (entry.isDirectory) {
    await mkdir(targetPath, { recursive: true });

    const directoryEntry = entry as FileSystemDirectoryEntry;
    const children = await readAllDirectoryEntries(directoryEntry.createReader());

    for (const child of children) {
      throwIfImportCancelled(progress.signal);
      await importEntry(child, joinPath(targetPath, child.name), progress);
    }

    return;
  }

  const file = await readFileEntry(entry as FileSystemFileEntry);
  await writeDroppedFile(targetPath, file, progress);
}

async function buildRenamedPath(path: string): Promise<string> {
  const parentPath = getParentPath(path);
  const fileName = getFileNameFromPath(path);

  for (let index = 1; ; index += 1) {
    const candidateName = buildRenamedFileName(fileName, index);
    const candidatePath = joinPath(parentPath, candidateName);
    if (!(await exists(candidatePath))) {
      return candidatePath;
    }
  }
}

async function getDroppedRoots(dataTransfer: DataTransfer): Promise<ExternalDropRoot[]> {
  const entryRoots = Array.from(dataTransfer.items || [])
    .filter((item) => item.kind === 'file')
    .map((item) => (item as WebkitDataTransferItem).webkitGetAsEntry?.())
    .filter((entry): entry is FileSystemEntry => Boolean(entry));

  if (entryRoots.length > 0) {
    return Promise.all(
      entryRoots.map(async (entry) => {
        let sourcePath: string | undefined = getDroppedEntrySourcePath(entry);
        if (entry.isFile) {
          const file = await readFileEntry(entry as FileSystemFileEntry);
          sourcePath = getDroppedFileSourcePath(file) ?? sourcePath;
          return {
            kind: 'entry' as const,
            name: entry.name,
            sourcePath,
            importIntoTargetPath: async (targetPath: string, progress: ImportProgressContext) => {
              if (sourcePath) {
                await copySourcePathToTarget(sourcePath, targetPath, progress);
              } else {
                await writeDroppedFile(targetPath, file, progress);
              }
            },
          };
        }

        return {
          kind: 'entry' as const,
          name: entry.name,
          sourcePath,
          importIntoTargetPath: async (targetPath: string, progress: ImportProgressContext) => {
            if (sourcePath) {
              await copySourcePathToTarget(sourcePath, targetPath, progress);
            } else {
              await importEntry(entry, targetPath, progress);
            }
          },
        };
      }),
    );
  }

  return Array.from(dataTransfer.files || []).map((file) => ({
    kind: 'file' as const,
    name: file.name,
    sourcePath: getDroppedFileSourcePath(file),
    importIntoTargetPath: async (targetPath: string, progress: ImportProgressContext) => {
      const sourcePath = getDroppedFileSourcePath(file);
      if (sourcePath) {
        await copySourcePathToTarget(sourcePath, targetPath, progress);
      } else {
        await writeDroppedFile(targetPath, file, progress);
      }
    },
  }));
}

export async function importExternalPaths(
  sourcePaths: string[],
  targetDir: string,
  options: ExternalDropImportOptions = {},
): Promise<ExternalDropImportResult> {
  const roots: ExternalDropRoot[] = sourcePaths
    .map(normalizeDroppedPath)
    .filter(Boolean)
    .map((sourcePath) => ({
      kind: 'file' as const,
      name: getPathName(sourcePath),
      sourcePath,
      importIntoTargetPath: async (targetPath: string, progress: ImportProgressContext) => {
        await copySourcePathToTarget(sourcePath, targetPath, progress);
      },
    }));

  return importExternalRoots(roots, targetDir, options);
}

export async function importExternalDrop(
  dataTransfer: DataTransfer,
  targetDir: string,
  options: ExternalDropImportOptions = {},
): Promise<ExternalDropImportResult> {
  const roots = await getDroppedRoots(dataTransfer);
  throwIfImportCancelled(options.signal);
  return importExternalRoots(roots, targetDir, options);
}

async function importExternalRoots(
  roots: ExternalDropRoot[],
  targetDir: string,
  options: ExternalDropImportOptions = {},
): Promise<ExternalDropImportResult> {
  const targetLabel = options.targetLabel || targetDir;

  let successCount = 0;
  let overwriteCount = 0;
  let renameCount = 0;
  let skippedCount = 0;
  const failedItems: string[] = [];

  for (const [rootIndex, root] of roots.entries()) {
    throwIfImportCancelled(options.signal);
    let targetPath = joinPath(targetDir, root.name);
    let appliedRename = false;
    let appliedOverwrite = false;
    let importStarted = false;

    try {
      while (await exists(targetPath)) {
        throwIfImportCancelled(options.signal);
        if (
          root.sourcePath &&
          arePathsEquivalent(root.sourcePath, targetPath)
        ) {
          skippedCount += 1;
          targetPath = '';
          break;
        }

        const resolution = options.requestConflictChoice
          ? await options.requestConflictChoice(root.name, targetLabel)
          : { action: 'cancel' as const };

        if (resolution.action === 'cancel') {
          skippedCount += 1;
          targetPath = '';
          break;
        }

        if (resolution.action === 'overwrite') {
          await remove(targetPath, { recursive: true });
          appliedOverwrite = true;
          break;
        } else {
          const requestedName = resolution.renameName?.trim();
          targetPath = requestedName
            ? joinPath(targetDir, requestedName)
            : await buildRenamedPath(targetPath);
          appliedRename = true;
        }
      }

      throwIfImportCancelled(options.signal);

      if (!targetPath) {
        continue;
      }

      throwIfImportCancelled(options.signal);
      importStarted = true;
      await root.importIntoTargetPath(targetPath, {
        itemIndex: rootIndex + 1,
        itemCount: roots.length,
        currentName: root.name,
        onProgress: options.onProgress,
        signal: options.signal,
      });
      successCount += 1;
      if (appliedOverwrite) {
        overwriteCount += 1;
      }
      if (appliedRename) {
        renameCount += 1;
      }
    } catch (error) {
      if (importStarted && targetPath) {
        try {
          if (await exists(targetPath)) {
            await remove(targetPath, { recursive: true });
          }
        } catch {
          // Keep the original import error visible; cleanup is best-effort.
        }
      }
      if (isExternalImportCancelled(error)) {
        throw new ExternalImportCancelledError();
      }
      failedItems.push(`${root.name}: ${String(error)}`);
    }
  }

  return {
    successCount,
    overwriteCount,
    renameCount,
    skippedCount,
    failedItems,
  };
}
