import { writeFile } from '@tauri-apps/plugin-fs';

type DesktopFile = File & { path?: string };

export interface PreparedTransferFile {
  path: string;
  staged: boolean;
  name: string;
}

const WRITE_CHUNK_SIZE = 16 * 1024 * 1024;

function isAbsoluteDesktopPath(path: unknown): path is string {
  return typeof path === 'string' && (/^[a-zA-Z]:[\\/]/.test(path) || path.startsWith('\\\\'));
}

function clipboardFileName(file: File, index: number) {
  if (file.name.trim()) return file.name;
  const extension = file.type.split('/')[1]?.replace(/[^a-zA-Z0-9]/g, '') || 'png';
  const stamp = new Date().toISOString().replace(/\D/g, '').slice(0, 14);
  return `剪贴板图像-${stamp}-${index + 1}.${extension}`;
}

async function writeBrowserFile(path: string, file: File) {
  if (file.size === 0) {
    await writeFile(path, new Uint8Array());
    return;
  }
  for (let offset = 0; offset < file.size; offset += WRITE_CHUNK_SIZE) {
    const bytes = new Uint8Array(await file.slice(offset, offset + WRITE_CHUNK_SIZE).arrayBuffer());
    await writeFile(path, bytes, {
      append: offset > 0,
      create: true,
    });
  }
}

export async function prepareBrowserFiles(
  files: File[],
  createStagingPath: (fileName: string) => Promise<string>,
  discardStagingFile: (path: string) => Promise<void>,
  fromClipboard = false,
): Promise<PreparedTransferFile[]> {
  const prepared: PreparedTransferFile[] = [];
  for (const [index, file] of files.entries()) {
    const desktopPath = (file as DesktopFile).path;
    if (isAbsoluteDesktopPath(desktopPath)) {
      prepared.push({ path: desktopPath, staged: false, name: file.name });
      continue;
    }
    const name = fromClipboard ? clipboardFileName(file, index) : file.name;
    if (!name.trim()) continue;
    const stagingPath = await createStagingPath(name);
    try {
      await writeBrowserFile(stagingPath, file);
      prepared.push({ path: stagingPath, staged: true, name });
    } catch (error) {
      await discardStagingFile(stagingPath).catch(() => {});
      throw error;
    }
  }
  return prepared;
}

export function clipboardImageFiles(dataTransfer: DataTransfer) {
  const itemFiles = Array.from(dataTransfer.items || [])
    .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
    .map((item) => item.getAsFile())
    .filter((file): file is File => Boolean(file));
  return itemFiles.length > 0
    ? itemFiles
    : Array.from(dataTransfer.files || []).filter((file) => file.type.startsWith('image/'));
}
