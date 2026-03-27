import { exists, mkdir, writeFile } from '@tauri-apps/plugin-fs';
import { appendRelativePath, getParentPath } from './dragDrop';

interface ExternalDropImportResult {
  successCount: number;
  failedItems: string[];
}

type WebkitDataTransferItem = DataTransferItem & {
  webkitGetAsEntry: () => FileSystemEntry | null;
};

type ExternalDropRoot =
  | {
      kind: 'entry';
      name: string;
      importInto: (targetDir: string) => Promise<void>;
    }
  | {
      kind: 'file';
      name: string;
      importInto: (targetDir: string) => Promise<void>;
    };

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

async function importEntry(
  entry: FileSystemEntry,
  targetDir: string,
  relativeParentPath = '',
): Promise<void> {
  const relativePath = relativeParentPath ? `${relativeParentPath}/${entry.name}` : entry.name;
  const targetPath = appendRelativePath(targetDir, relativePath);

  if (entry.isDirectory) {
    await mkdir(targetPath, { recursive: true });

    const directoryEntry = entry as FileSystemDirectoryEntry;
    const children = await readAllDirectoryEntries(directoryEntry.createReader());

    for (const child of children) {
      await importEntry(child, targetDir, relativePath);
    }

    return;
  }

  const file = await readFileEntry(entry as FileSystemFileEntry);
  await writeDroppedFile(targetPath, file);
}

async function getDroppedRoots(dataTransfer: DataTransfer): Promise<ExternalDropRoot[]> {
  const entryRoots = Array.from(dataTransfer.items || [])
    .filter((item) => item.kind === 'file')
    .map((item) => (item as WebkitDataTransferItem).webkitGetAsEntry?.())
    .filter((entry): entry is FileSystemEntry => Boolean(entry));

  if (entryRoots.length > 0) {
    return entryRoots.map((entry) => ({
      kind: 'entry' as const,
      name: entry.name,
      importInto: async (targetDir: string) => {
        await importEntry(entry, targetDir);
      },
    }));
  }

  return Array.from(dataTransfer.files || []).map((file) => ({
    kind: 'file' as const,
    name: file.name,
    importInto: async (targetDir: string) => {
      const targetPath = appendRelativePath(targetDir, file.webkitRelativePath || file.name);
      await writeDroppedFile(targetPath, file);
    },
  }));
}

export async function importExternalDrop(dataTransfer: DataTransfer, targetDir: string): Promise<ExternalDropImportResult> {
  const roots = await getDroppedRoots(dataTransfer);

  let successCount = 0;
  const failedItems: string[] = [];

  for (const root of roots) {
    const targetPath = appendRelativePath(targetDir, root.name);

    try {
      if (await exists(targetPath)) {
        failedItems.push(`${root.name}: 目标位置已存在同名文件或文件夹`);
        continue;
      }

      await root.importInto(targetDir);
      successCount += 1;
    } catch (error) {
      failedItems.push(`${root.name}: ${String(error)}`);
    }
  }

  return {
    successCount,
    failedItems,
  };
}
