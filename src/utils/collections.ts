import type { FileInfo, ImageSequenceInfo } from '../types';

export interface ProjectCollectionInfo {
  id: string;
  directory_path: string;
  name: string;
  created_at: number;
  updated_at: number;
  item_count: number;
  member_paths: string[];
}

const DIRECT_SEQUENCE_IMAGE_EXTENSIONS = new Set([
  'png',
  'jpg',
  'jpeg',
  'webp',
  'bmp',
]);

function getBasenameWithoutExtension(name: string, extension: string) {
  const suffix = `.${extension}`;
  return name.toLowerCase().endsWith(suffix)
    ? name.slice(0, -suffix.length)
    : name.replace(/\.[^/.]+$/, '');
}

function makeSequenceId(key: string) {
  let hash = 2166136261;
  for (let index = 0; index < key.length; index += 1) {
    hash ^= key.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
}

function getNewestTimestamp(files: FileInfo[]) {
  const timestamps = files
    .map((file) => file.modified)
    .filter((value): value is string => Boolean(value))
    .sort();
  return timestamps[timestamps.length - 1] ?? null;
}

export function detectImageSequences(directoryPath: string, files: FileInfo[]): FileInfo[] {
  const groups = new Map<string, Array<FileInfo & { frame: number }>>();

  for (const file of files) {
    if (file.is_dir) {
      continue;
    }

    const extension = file.extension?.toLowerCase();
    if (!extension || !DIRECT_SEQUENCE_IMAGE_EXTENSIONS.has(extension)) {
      continue;
    }

    const basename = getBasenameWithoutExtension(file.name, extension);
    const match = basename.match(/^(.*?)(\d+)$/);
    if (!match) {
      continue;
    }

    const [, prefix, frameToken] = match;
    const frame = Number.parseInt(frameToken, 10);
    if (!Number.isFinite(frame)) {
      continue;
    }

    const key = `${directoryPath}\u0000${prefix}\u0000${frameToken.length}\u0000${extension}`;
    const group = groups.get(key) ?? [];
    group.push({ ...file, frame });
    groups.set(key, group);
  }

  const sequenceEntries: FileInfo[] = [];

  for (const [key, group] of groups.entries()) {
    if (group.length < 3) {
      continue;
    }

    group.sort((left, right) => left.frame - right.frame || left.path.localeCompare(right.path));
    const first = group[0];
    const last = group[group.length - 1];
    const extension = first.extension?.toLowerCase() || '';
    const basename = getBasenameWithoutExtension(first.name, extension);
    const match = basename.match(/^(.*?)(\d+)$/);
    if (!match) {
      continue;
    }

    const [, prefix, frameToken] = match;
    const startFrame = first.frame;
    const endFrame = last.frame;
    const frameSet = new Set(group.map((file) => file.frame));
    let missingCount = 0;
    for (let frame = startFrame; frame <= endFrame; frame += 1) {
      if (!frameSet.has(frame)) {
        missingCount += 1;
      }
    }

    const id = makeSequenceId(key);
    const virtualPath = `pmc://sequence/${id}`;
    const sequence: ImageSequenceInfo = {
      id,
      virtual_path: virtualPath,
      directory_path: directoryPath,
      prefix,
      extension,
      padding: frameToken.length,
      start_frame: startFrame,
      end_frame: endFrame,
      frame_count: group.length,
      missing_count: missingCount,
      frames: group.map((file) => ({
        frame: file.frame,
        path: file.path,
        name: file.name,
      })),
    };

    sequenceEntries.push({
      name: `${prefix}${String(startFrame).padStart(frameToken.length, '0')}-${String(endFrame).padStart(frameToken.length, '0')}.${extension}`,
      path: virtualPath,
      is_dir: true,
      size: group.reduce((sum, file) => sum + file.size, 0),
      modified: getNewestTimestamp(group),
      created: first.created,
      extension: null,
      thumbnail: first.thumbnail,
      entry_kind: 'image_sequence',
      virtual_path: virtualPath,
      item_count: group.length,
      sequence,
    });
  }

  sequenceEntries.sort((left, right) => left.name.localeCompare(right.name, 'zh-CN'));
  return sequenceEntries;
}

export function createManualCollectionFileInfo(collection: ProjectCollectionInfo): FileInfo {
  const virtualPath = `pmc://collection/${collection.id}`;
  return {
    name: collection.name,
    path: virtualPath,
    is_dir: true,
    size: 0,
    modified: null,
    created: null,
    extension: null,
    thumbnail: null,
    entry_kind: 'manual_collection',
    virtual_path: virtualPath,
    collection_id: collection.id,
    item_count: collection.item_count,
    collection_member_paths: collection.member_paths,
    directory_path: collection.directory_path,
    created_at: collection.created_at,
    updated_at: collection.updated_at,
  };
}

export function isVirtualFile(file: FileInfo) {
  return file.entry_kind === 'manual_collection' || file.entry_kind === 'image_sequence';
}

