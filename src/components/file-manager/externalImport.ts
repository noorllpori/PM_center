import { exists, mkdir, remove, writeFile } from '@tauri-apps/plugin-fs';
import {
  buildRenamedFileName,
  getFileNameFromPath,
  getParentPath,
  joinPath,
  normalizePath,
} from './dragDrop';

interface ExternalDropImportResult {
  successCount: number;
  overwriteCount: number;
  renameCount: number;
  skippedCount: number;
  failedItems: string[];
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
      importIntoTargetPath: (targetPath: string) => Promise<void>;
    }
  | {
      kind: 'file';
      name: string;
      sourcePath?: string;
      importIntoTargetPath: (targetPath: string) => Promise<void>;
    };

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

async function writeDroppedFile(targetPath: string, file: File): Promise<void> {
  await ensureParentDirectory(targetPath);
  await writeFile(targetPath, new Uint8Array(await file.arrayBuffer()));
}

async function importEntry(entry: FileSystemEntry, targetPath: string): Promise<void> {
  if (entry.isDirectory) {
    await mkdir(targetPath, { recursive: true });

    const directoryEntry = entry as FileSystemDirectoryEntry;
    const children = await readAllDirectoryEntries(directoryEntry.createReader());

    for (const child of children) {
      await importEntry(child, joinPath(targetPath, child.name));
    }

    return;
  }

  const file = await readFileEntry(entry as FileSystemFileEntry);
  await writeDroppedFile(targetPath, file);
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
            importIntoTargetPath: async (targetPath: string) => {
              await writeDroppedFile(targetPath, file);
            },
          };
        }

        return {
          kind: 'entry' as const,
          name: entry.name,
          sourcePath,
          importIntoTargetPath: async (targetPath: string) => {
            await importEntry(entry, targetPath);
          },
        };
      }),
    );
  }

  return Array.from(dataTransfer.files || []).map((file) => ({
    kind: 'file' as const,
    name: file.name,
    sourcePath: getDroppedFileSourcePath(file),
    importIntoTargetPath: async (targetPath: string) => {
      await writeDroppedFile(targetPath, file);
    },
  }));
}

export async function importExternalDrop(
  dataTransfer: DataTransfer,
  targetDir: string,
  options: ExternalDropImportOptions = {},
): Promise<ExternalDropImportResult> {
  const roots = await getDroppedRoots(dataTransfer);
  const targetLabel = options.targetLabel || targetDir;

  let successCount = 0;
  let overwriteCount = 0;
  let renameCount = 0;
  let skippedCount = 0;
  const failedItems: string[] = [];

  for (const root of roots) {
    let targetPath = joinPath(targetDir, root.name);
    let appliedRename = false;
    let appliedOverwrite = false;

    try {
      while (await exists(targetPath)) {
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

      if (!targetPath) {
        continue;
      }

      await root.importIntoTargetPath(targetPath);
      successCount += 1;
      if (appliedOverwrite) {
        overwriteCount += 1;
      }
      if (appliedRename) {
        renameCount += 1;
      }
    } catch (error) {
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
