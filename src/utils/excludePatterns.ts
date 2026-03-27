export const BLENDER_BACKUP_EXCLUDE_PATTERN = '*.blend[0-9]*';

export const DEFAULT_EXCLUDE_PATTERNS = [
  '.pm_center',
  '.git',
  '*.tmp',
  '*.temp',
  'Thumbs.db',
  '.DS_Store',
  BLENDER_BACKUP_EXCLUDE_PATTERN,
];

export const PRESET_EXCLUDE_PATTERNS = [
  { value: '.git', label: 'Git 目录 (.git)', desc: '版本控制目录' },
  { value: 'node_modules', label: 'Node 模块 (node_modules)', desc: '依赖目录' },
  { value: '__pycache__', label: 'Python 缓存 (__pycache__)', desc: '编译缓存' },
  { value: '*.tmp', label: '临时文件 (*.tmp)', desc: '临时文件' },
  { value: '*.bak', label: '备份文件 (*.bak)', desc: '备份文件' },
  { value: BLENDER_BACKUP_EXCLUDE_PATTERN, label: 'Blender 备份 (*.blend1/.blend2/...)', desc: 'Blender 自动备份文件' },
  { value: '.DS_Store', label: 'Mac 索引 (.DS_Store)', desc: '系统文件' },
  { value: 'Thumbs.db', label: 'Windows 缩略图 (Thumbs.db)', desc: '系统文件' },
];

export function getExcludeStorageKey(projectPath: string) {
  return `project_exclude_${projectPath}`;
}

export function readProjectExcludePatterns(projectPath: string | null) {
  if (!projectPath) {
    return [];
  }

  const saved = localStorage.getItem(getExcludeStorageKey(projectPath));
  if (!saved) {
    return [];
  }

  try {
    const parsed = JSON.parse(saved);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function mergeExcludePatterns(globalPatterns: string[], projectPatterns: string[]) {
  return Array.from(new Set([...globalPatterns, ...projectPatterns]));
}

export function shouldExcludeFile(fileName: string, patterns: string[]) {
  return patterns.some((pattern) => matchesExcludePattern(fileName, pattern));
}

function matchesExcludePattern(fileName: string, pattern: string) {
  if (pattern.includes('*') || pattern.includes('?') || pattern.includes('[')) {
    return globToRegExp(pattern).test(fileName);
  }

  return fileName === pattern || fileName.startsWith(`${pattern}/`);
}

function globToRegExp(pattern: string) {
  let regex = '^';
  let index = 0;

  while (index < pattern.length) {
    const char = pattern[index];

    if (char === '*') {
      regex += '.*';
      index += 1;
      continue;
    }

    if (char === '?') {
      regex += '.';
      index += 1;
      continue;
    }

    if (char === '[') {
      const closingIndex = pattern.indexOf(']', index + 1);
      if (closingIndex > index + 1) {
        regex += pattern.slice(index, closingIndex + 1);
        index = closingIndex + 1;
        continue;
      }
    }

    regex += escapeRegexChar(char);
    index += 1;
  }

  regex += '$';
  return new RegExp(regex);
}

function escapeRegexChar(char: string) {
  return /[\\^$+?.()|{}]/.test(char) ? `\\${char}` : char;
}
