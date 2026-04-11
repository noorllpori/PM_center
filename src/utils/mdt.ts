const MDT_EXTENSION = 'mdt';
const MDT_FRONTMATTER_COMMENT = '#此文件由PmCenter项目管理器创建';

const MEDIA_EXTENSIONS = new Set([
  'png',
  'jpg',
  'jpeg',
  'gif',
  'webp',
  'bmp',
  'svg',
  'mp4',
  'webm',
  'mov',
  'avi',
  'mkv',
  'mp3',
  'wav',
  'ogg',
  'flac',
  'm4a',
  'aac',
]);

export interface MdtTaskItem {
  text: string;
  checked: boolean;
}

export interface MdtReferenceEntry {
  mdtPath: string;
  mdtRelativePath: string;
  mdtTitle: string;
  createdAt: string | null;
  summary: string;
  openTaskCount: number;
  completedTaskCount: number;
}

export interface MdtDocumentSummary {
  filePath: string;
  fileName: string;
  relativePath: string;
  title: string;
  createdAt: string | null;
  summary: string;
  relatedFiles: string[];
  mediaFiles: string[];
  tasks: MdtTaskItem[];
  openTaskCount: number;
  completedTaskCount: number;
  logEntries: string[];
  excerpt: string;
  parseError: string | null;
}

export type MdtInlineReferenceMode = 'link' | 'image';

interface FrontmatterBlock {
  content: string | null;
  body: string;
  hasFrontmatter: boolean;
}

type FrontmatterValue = string | string[];

type FrontmatterMap = Record<string, FrontmatterValue>;

function getFileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function getBaseName(path: string) {
  const fileName = getFileNameFromPath(path);
  const dotIndex = fileName.lastIndexOf('.');
  return dotIndex > 0 ? fileName.slice(0, dotIndex) : fileName;
}

function getParentPath(path: string) {
  const parts = path.split(/[\\/]/);
  parts.pop();
  return parts.join(path.includes('\\') ? '\\' : '/');
}

function getPathSeparator(path: string) {
  return path.includes('\\') ? '\\' : '/';
}

function normalizePathKey(path: string) {
  return path.replace(/\\/g, '/').replace(/\/+$/, '');
}

function joinPath(basePath: string, relativePath: string) {
  const separator = getPathSeparator(basePath);
  const baseParts = basePath.split(/[\\/]+/).filter(Boolean);
  const relativeParts = relativePath.split(/[\\/]+/).filter(Boolean);
  const absolutePrefix = /^[A-Za-z]:$/.test(baseParts[0] || '')
    ? `${baseParts[0]}${separator}`
    : basePath.startsWith('\\')
      ? `${separator}${separator}`
      : basePath.startsWith('/')
        ? separator
        : '';

  const stack = absolutePrefix ? baseParts.slice(1) : [...baseParts];

  for (const part of relativeParts) {
    if (part === '.') {
      continue;
    }
    if (part === '..') {
      stack.pop();
      continue;
    }
    stack.push(part);
  }

  return absolutePrefix
    ? `${absolutePrefix}${stack.join(separator)}`
    : stack.join(separator);
}

function isAbsolutePath(path: string) {
  return /^[A-Za-z]:[\\/]/.test(path) || path.startsWith('\\\\') || path.startsWith('/');
}

function toProjectRelativePath(projectPath: string, targetPath: string) {
  const normalizedProjectPath = normalizePathKey(projectPath);
  const normalizedTargetPath = normalizePathKey(targetPath);
  if (normalizedTargetPath === normalizedProjectPath) {
    return '.';
  }

  if (!normalizedTargetPath.startsWith(`${normalizedProjectPath}/`)) {
    return targetPath;
  }

  return normalizedTargetPath.slice(normalizedProjectPath.length + 1);
}

function extractFrontmatterBlock(markdown: string): FrontmatterBlock {
  const normalized = markdown.replace(/\r\n/g, '\n');
  const match = /^---\n([\s\S]*?)\n---(?:\n|$)/.exec(normalized);
  if (!match) {
    return {
      content: null,
      body: markdown,
      hasFrontmatter: false,
    };
  }

  return {
    content: match[1],
    body: normalized.slice(match[0].length),
    hasFrontmatter: true,
  };
}

function parseInlineArray(value: string) {
  const inner = value.slice(1, -1).trim();
  if (!inner) {
    return [];
  }

  return inner
    .split(',')
    .map((item) => parseYamlScalar(item.trim()))
    .filter(Boolean);
}

function parseYamlScalar(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return '';
  }

  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"'))
    || (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1);
  }

  return trimmed;
}

function parseFrontmatter(frontmatter: string | null): FrontmatterMap {
  if (!frontmatter) {
    return {};
  }

  const lines = frontmatter.replace(/\r\n/g, '\n').split('\n');
  const metadata: FrontmatterMap = {};

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const trimmed = line.trim();

    if (!trimmed || trimmed.startsWith('#')) {
      continue;
    }

    const blockMatch = /^([A-Za-z0-9_-]+):\s*$/.exec(trimmed);
    if (blockMatch) {
      const key = blockMatch[1];
      const values: string[] = [];
      while (index + 1 < lines.length) {
        const nextLine = lines[index + 1];
        const nextTrimmed = nextLine.trim();
        if (!nextTrimmed) {
          index += 1;
          continue;
        }
        if (!/^\s*-\s+/.test(nextLine)) {
          break;
        }
        values.push(parseYamlScalar(nextTrimmed.replace(/^-+\s*/, '')));
        index += 1;
      }
      metadata[key] = values;
      continue;
    }

    const multilineMatch = /^([A-Za-z0-9_-]+):\s*[|>]-?\s*$/.exec(trimmed);
    if (multilineMatch) {
      const key = multilineMatch[1];
      const values: string[] = [];
      while (index + 1 < lines.length) {
        const nextLine = lines[index + 1];
        if (nextLine.trim() && !/^\s+/.test(nextLine)) {
          break;
        }
        values.push(nextLine.replace(/^\s{2}/, ''));
        index += 1;
      }
      metadata[key] = values.join('\n').trim();
      continue;
    }

    const scalarMatch = /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(trimmed);
    if (!scalarMatch) {
      continue;
    }

    const key = scalarMatch[1];
    const value = scalarMatch[2].trim();
    if (value.startsWith('[') && value.endsWith(']')) {
      metadata[key] = parseInlineArray(value);
      continue;
    }

    metadata[key] = parseYamlScalar(value);
  }

  return metadata;
}

function getFrontmatterString(metadata: FrontmatterMap, keys: string[]) {
  for (const key of keys) {
    const value = metadata[key];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return '';
}

function getFrontmatterArray(metadata: FrontmatterMap, keys: string[]) {
  for (const key of keys) {
    const value = metadata[key];
    if (Array.isArray(value)) {
      return value.filter((entry): entry is string => typeof entry === 'string' && entry.trim().length > 0);
    }
    if (typeof value === 'string' && value.trim()) {
      return [value.trim()];
    }
  }
  return [];
}

function stripCodeFences(body: string) {
  return body.replace(/```[\s\S]*?```/g, '');
}

function extractFirstHeading(body: string) {
  const match = body.match(/^\s*#\s+(.+)$/m);
  return match?.[1]?.trim() || '';
}

function extractSummary(body: string) {
  const sanitized = stripCodeFences(body);
  const paragraphs = sanitized
    .split(/\n\s*\n/)
    .map((paragraph) => paragraph.trim())
    .filter(Boolean)
    .filter((paragraph) => !/^(#{1,6}\s|[-*+]\s|\d+\.\s|>\s|!\[)/.test(paragraph));

  return paragraphs[0] || '';
}

function extractTaskItems(body: string): MdtTaskItem[] {
  return body
    .split(/\r?\n/)
    .map((line) => line.match(/^\s*[-*+]\s+\[( |x|X)\]\s+(.*)$/))
    .filter((match): match is RegExpMatchArray => Boolean(match))
    .map((match) => ({
      checked: match[1].toLowerCase() === 'x',
      text: match[2].trim(),
    }));
}

function normalizeHeadingText(heading: string) {
  return heading.trim().toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '');
}

function extractSectionLines(body: string, targetHeadings: string[]) {
  const headingKeys = new Set(targetHeadings.map((heading) => normalizeHeadingText(heading)));
  const lines = body.replace(/\r\n/g, '\n').split('\n');
  const collected: string[] = [];
  let isInsideTargetSection = false;
  let targetLevel = 0;

  for (const line of lines) {
    const headingMatch = /^(#{1,6})\s+(.+)$/.exec(line.trim());
    if (headingMatch) {
      const headingLevel = headingMatch[1].length;
      const headingKey = normalizeHeadingText(headingMatch[2]);
      if (headingKeys.has(headingKey)) {
        isInsideTargetSection = true;
        targetLevel = headingLevel;
        continue;
      }

      if (isInsideTargetSection && headingLevel <= targetLevel) {
        break;
      }
    }

    if (isInsideTargetSection && line.trim()) {
      collected.push(line.trim());
    }
  }

  return collected;
}

function sanitizeLinkTarget(rawTarget: string) {
  const trimmed = rawTarget.trim();
  if (!trimmed) {
    return '';
  }

  const angleMatch = /^<(.+)>$/.exec(trimmed);
  const withoutWrapper = angleMatch ? angleMatch[1] : trimmed.split(/\s+/)[0];
  const cleaned = withoutWrapper.replace(/\\([()])/g, '$1');
  const withoutFragment = cleaned.split('#')[0]?.split('?')[0] || '';

  try {
    return decodeURIComponent(withoutFragment);
  } catch {
    return withoutFragment;
  }
}

function resolveReferencePath(reference: string, filePath: string, projectPath: string) {
  const normalizedReference = reference.trim();
  if (!normalizedReference) {
    return null;
  }

  if (/^(https?:|data:|blob:|mailto:|tel:)/i.test(normalizedReference)) {
    return null;
  }

  const projectSeparator = getPathSeparator(projectPath);
  const normalizedSeparators = normalizedReference.replace(/[\\/]+/g, projectSeparator);
  const resolved = isAbsolutePath(normalizedSeparators)
    ? normalizedSeparators
    : normalizedSeparators.startsWith(projectSeparator)
      ? joinPath(projectPath, normalizedSeparators.replace(/^[\\/]+/, ''))
      : joinPath(getParentPath(filePath), normalizedSeparators);

  const normalizedKey = normalizePathKey(resolved);
  const projectRootKey = normalizePathKey(projectPath);
  if (
    normalizedKey !== projectRootKey
    && !normalizedKey.startsWith(`${projectRootKey}/`)
  ) {
    return null;
  }

  return resolved;
}

function extractBodyReferences(body: string, filePath: string, projectPath: string) {
  const references: string[] = [];
  const markdownLinkPattern = /!?\[[^\]]*]\(([^)]+)\)/g;
  const htmlLinkPattern = /\b(?:src|href)=["']([^"']+)["']/gi;

  for (const match of body.matchAll(markdownLinkPattern)) {
    const target = sanitizeLinkTarget(match[1] || '');
    const resolved = resolveReferencePath(target, filePath, projectPath);
    if (resolved) {
      references.push(resolved);
    }
  }

  for (const match of body.matchAll(htmlLinkPattern)) {
    const target = sanitizeLinkTarget(match[1] || '');
    const resolved = resolveReferencePath(target, filePath, projectPath);
    if (resolved) {
      references.push(resolved);
    }
  }

  return references;
}

function dedupePaths(paths: string[]) {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const path of paths) {
    const key = normalizePathKey(path);
    if (!key || seen.has(key)) {
      continue;
    }
    seen.add(key);
    result.push(path);
  }
  return result;
}

function isMediaFile(path: string) {
  const extension = path.split('.').pop()?.toLowerCase() || '';
  return MEDIA_EXTENSIONS.has(extension);
}

function escapeFrontmatterValue(value: string) {
  return JSON.stringify(value);
}

function buildMdtTemplate(filePath: string, createdAt: string) {
  const title = getBaseName(filePath);
  return [
    '---',
    MDT_FRONTMATTER_COMMENT,
    `title: ${escapeFrontmatterValue(title)}`,
    `createdAt: ${escapeFrontmatterValue(createdAt)}`,
    'summary: ""',
    'relatedFiles: []',
    '---',
    '',
    `# ${title}`,
    '',
    '## Summary',
    '',
    '',
    '## Todo',
    '',
    '- [ ] First task',
    '',
    '## Logs',
    '',
    `- ${createdAt} Created`,
    '',
    '## Media',
    '',
    '',
  ].join('\n');
}

export function isMdtPath(pathOrFileName?: string | null) {
  if (!pathOrFileName) {
    return false;
  }

  const extension = pathOrFileName.split('.').pop()?.toLowerCase();
  return extension === MDT_EXTENSION;
}

export function normalizeMdtReferenceKey(path: string) {
  return normalizePathKey(path);
}

export function getMdtRelativePath(projectPath: string, targetPath: string) {
  return toProjectRelativePath(projectPath, targetPath);
}

export function ensureMdtContent(
  markdown: string,
  options: {
    filePath: string;
    defaultCreatedAt: string;
  },
) {
  const normalizedMarkdown = markdown.replace(/\r\n/g, '\n');
  const createdAtValue = options.defaultCreatedAt;

  if (!normalizedMarkdown.trim()) {
    return {
      content: buildMdtTemplate(options.filePath, createdAtValue),
      changed: true,
    };
  }

  const frontmatter = extractFrontmatterBlock(normalizedMarkdown);
  if (!frontmatter.hasFrontmatter || !frontmatter.content) {
    const title = getBaseName(options.filePath);
    const prefix = [
      '---',
      MDT_FRONTMATTER_COMMENT,
      `title: ${escapeFrontmatterValue(title)}`,
      `createdAt: ${escapeFrontmatterValue(createdAtValue)}`,
      'summary: ""',
      'relatedFiles: []',
      '---',
      '',
    ].join('\n');

    return {
      content: `${prefix}${normalizedMarkdown.replace(/^\n+/, '')}`,
      changed: true,
    };
  }

  const metadata = parseFrontmatter(frontmatter.content);
  const existingCreatedAt = getFrontmatterString(metadata, ['createdAt', 'created_at', 'time', 'created']);
  const frontmatterLines = frontmatter.content.split('\n');
  let changed = false;

  if (!frontmatterLines.some((line) => line.trim() === MDT_FRONTMATTER_COMMENT)) {
    frontmatterLines.unshift(MDT_FRONTMATTER_COMMENT);
    changed = true;
  }

  if (existingCreatedAt) {
    if (!changed) {
      return {
        content: normalizedMarkdown,
        changed: false,
      };
    }

    return {
      content: `---\n${frontmatterLines.join('\n')}\n---\n${frontmatter.body}`,
      changed: true,
    };
  }

  const titleLineIndex = frontmatterLines.findIndex((line) => /^\s*title\s*:/.test(line));
  const insertIndex = titleLineIndex >= 0 ? titleLineIndex + 1 : 0;
  frontmatterLines.splice(insertIndex, 0, `createdAt: ${escapeFrontmatterValue(createdAtValue)}`);
  changed = true;

  return {
    content: `---\n${frontmatterLines.join('\n')}\n---\n${frontmatter.body}`,
    changed,
  };
}

function getComparablePathParts(path: string) {
  return normalizePathKey(path)
    .split('/')
    .filter(Boolean)
    .map((part, index) => (index === 0 && /^[A-Za-z]:$/.test(part) ? part.toLowerCase() : part));
}

function getRelativeMarkdownPath(fromFilePath: string, targetPath: string) {
  const fromDirectoryParts = getComparablePathParts(getParentPath(fromFilePath));
  const targetParts = getComparablePathParts(targetPath);

  if (fromDirectoryParts.length === 0 || targetParts.length === 0) {
    return targetPath.replace(/\\/g, '/');
  }

  if (fromDirectoryParts[0] !== targetParts[0]) {
    return targetPath.replace(/\\/g, '/');
  }

  let sharedIndex = 0;
  while (
    sharedIndex < fromDirectoryParts.length
    && sharedIndex < targetParts.length
    && fromDirectoryParts[sharedIndex] === targetParts[sharedIndex]
  ) {
    sharedIndex += 1;
  }

  const relativeParts = [
    ...new Array(fromDirectoryParts.length - sharedIndex).fill('..'),
    ...targetParts.slice(sharedIndex),
  ];

  return relativeParts.join('/') || '.';
}

function isImageFile(path: string) {
  const extension = path.split('.').pop()?.toLowerCase() || '';
  return new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg']).has(extension);
}

function findSectionRange(lines: string[], targetHeadings: string[]) {
  const headingKeys = new Set(targetHeadings.map((heading) => normalizeHeadingText(heading)));

  for (let index = 0; index < lines.length; index += 1) {
    const headingMatch = /^(#{1,6})\s+(.+)$/.exec(lines[index].trim());
    if (!headingMatch) {
      continue;
    }

    if (!headingKeys.has(normalizeHeadingText(headingMatch[2]))) {
      continue;
    }

    const headingLevel = headingMatch[1].length;
    let endIndex = lines.length;
    for (let nextIndex = index + 1; nextIndex < lines.length; nextIndex += 1) {
      const nextHeadingMatch = /^(#{1,6})\s+(.+)$/.exec(lines[nextIndex].trim());
      if (nextHeadingMatch && nextHeadingMatch[1].length <= headingLevel) {
        endIndex = nextIndex;
        break;
      }
    }

    return {
      start: index,
      end: endIndex,
    };
  }

  return null;
}

function insertLineIntoBodySection(
  body: string,
  options: {
    targetHeadings: string[];
    createHeading: string;
    anchorHeadings?: string[];
    line: string;
  },
) {
  const normalizedBody = body.replace(/\r\n/g, '\n').replace(/^\n+/, '');
  const lines = normalizedBody ? normalizedBody.split('\n') : [];
  const existingSection = findSectionRange(lines, options.targetHeadings);

  if (existingSection) {
    const rawSectionLines = lines.slice(existingSection.start, existingSection.end);
    const sectionLines = rawSectionLines.join('\n').replace(/\n+$/, '');
    const hasBodyContent = rawSectionLines.slice(1).some((line) => line.trim().length > 0);
    const nextSectionLines = hasBodyContent
      ? `${sectionLines}\n${options.line}\n`.split('\n')
      : `${rawSectionLines[0]}\n\n${options.line}\n`.split('\n');
    lines.splice(existingSection.start, existingSection.end - existingSection.start, ...nextSectionLines);
    return lines.join('\n').replace(/^\n+/, '');
  }

  let insertIndex = lines.length;
  if (options.anchorHeadings && options.anchorHeadings.length > 0) {
    const anchorSection = findSectionRange(lines, options.anchorHeadings);
    if (anchorSection) {
      insertIndex = anchorSection.start;
    }
  }

  const nextSection = [
    options.createHeading,
    '',
    options.line,
    '',
  ];

  if (insertIndex > 0 && lines[insertIndex - 1].trim()) {
    nextSection.unshift('');
  }

  lines.splice(insertIndex, 0, ...nextSection);
  return lines.join('\n').replace(/^\n+/, '');
}

export function addReferenceToMdtContent(
  markdown: string,
  options: {
    filePath: string;
    targetPath: string;
    projectPath: string;
  },
) {
  const normalizedMarkdown = markdown.replace(/\r\n/g, '\n');
  const normalizedTargetPath = normalizePathKey(options.targetPath);

  if (normalizedTargetPath === normalizePathKey(options.filePath)) {
    return {
      content: normalizedMarkdown,
      added: false,
      reference: getRelativeMarkdownPath(options.filePath, options.targetPath),
    };
  }

  const frontmatterBlock = extractFrontmatterBlock(normalizedMarkdown);
  const frontmatterMetadata = parseFrontmatter(frontmatterBlock.content);
  const existingFrontmatterReferences = getFrontmatterArray(frontmatterMetadata, ['relatedFiles', 'related_files', 'files'])
    .map((reference) => resolveReferencePath(reference, options.filePath, options.projectPath))
    .filter((path): path is string => Boolean(path));
  const existingBodyReferences = extractBodyReferences(
    frontmatterBlock.body,
    options.filePath,
    options.projectPath,
  );
  const existingReferences = dedupePaths([
    ...existingFrontmatterReferences,
    ...existingBodyReferences,
  ]);

  if (existingReferences.some((path) => normalizePathKey(path) === normalizedTargetPath)) {
    return {
      content: normalizedMarkdown,
      added: false,
      reference: getRelativeMarkdownPath(options.filePath, options.targetPath),
    };
  }

  const inlineReference = buildMdtInlineReference(options.filePath, options.targetPath, 'link');
  const imageReference = buildMdtInlineReference(options.filePath, options.targetPath, 'image');
  const line = isMediaFile(options.targetPath) && isImageFile(options.targetPath)
    ? imageReference.markdown
    : `- ${inlineReference.markdown}`;
  const nextBody = isMediaFile(options.targetPath)
    ? insertLineIntoBodySection(frontmatterBlock.body, {
      targetHeadings: ['media', '附件', '多媒体'],
      createHeading: '## Media',
      line,
    })
    : insertLineIntoBodySection(frontmatterBlock.body, {
      targetHeadings: ['related files', 'relatedfiles', 'files', '关联文件'],
      createHeading: '## Related Files',
      anchorHeadings: ['logs', 'log', '日志', 'media', '附件', '多媒体'],
      line,
    });

  if (!frontmatterBlock.hasFrontmatter || frontmatterBlock.content === null) {
    return {
      content: nextBody,
      added: true,
      reference: inlineReference.reference,
    };
  }

  return {
    content: `---\n${frontmatterBlock.content}\n---\n${nextBody}`,
    added: true,
    reference: inlineReference.reference,
  };
}

export function buildMdtInlineReference(
  filePath: string,
  targetPath: string,
  mode: MdtInlineReferenceMode = 'link',
) {
  const reference = getRelativeMarkdownPath(filePath, targetPath);
  const label = getFileNameFromPath(targetPath);

  return {
    label,
    reference,
    markdown: mode === 'image'
      ? `![${label}](${reference})`
      : `[${label}](${reference})`,
  };
}

export function parseMdtDocument(
  markdown: string,
  options: {
    filePath: string;
    fileName?: string;
    projectPath: string;
    defaultCreatedAt?: string | null;
    parseError?: string | null;
  },
): MdtDocumentSummary {
  const normalizedMarkdown = markdown.replace(/\r\n/g, '\n');
  const frontmatterBlock = extractFrontmatterBlock(normalizedMarkdown);
  const metadata = parseFrontmatter(frontmatterBlock.content);
  const body = frontmatterBlock.body.trim();
  const title = getFrontmatterString(metadata, ['title', 'name'])
    || extractFirstHeading(body)
    || getBaseName(options.filePath);
  const createdAt = getFrontmatterString(metadata, ['createdAt', 'created_at', 'time', 'created'])
    || options.defaultCreatedAt
    || null;
  const summary = getFrontmatterString(metadata, ['summary', 'description'])
    || extractSummary(body)
    || 'No summary yet.';
  const frontmatterRelatedFiles = getFrontmatterArray(metadata, ['relatedFiles', 'related_files', 'files']);
  const resolvedFrontmatterReferences = frontmatterRelatedFiles
    .map((reference) => resolveReferencePath(reference, options.filePath, options.projectPath))
    .filter((path): path is string => Boolean(path));
  const bodyReferences = extractBodyReferences(body, options.filePath, options.projectPath);
  const relatedFiles = dedupePaths([
    ...resolvedFrontmatterReferences,
    ...bodyReferences,
  ]).filter((path) => normalizePathKey(path) !== normalizePathKey(options.filePath));
  const mediaFiles = relatedFiles.filter(isMediaFile);
  const tasks = extractTaskItems(body);
  const logEntries = extractSectionLines(body, ['logs', 'log', '日志']);
  const openTaskCount = tasks.filter((task) => !task.checked).length;
  const completedTaskCount = tasks.length - openTaskCount;

  return {
    filePath: options.filePath,
    fileName: options.fileName || getFileNameFromPath(options.filePath),
    relativePath: toProjectRelativePath(options.projectPath, options.filePath),
    title,
    createdAt,
    summary,
    relatedFiles,
    mediaFiles,
    tasks,
    openTaskCount,
    completedTaskCount,
    logEntries,
    excerpt: summary,
    parseError: options.parseError || null,
  };
}

export function buildMdtReferenceIndex(documents: MdtDocumentSummary[]) {
  const referencesByFile = new Map<string, MdtReferenceEntry[]>();

  for (const document of documents) {
    const reference: MdtReferenceEntry = {
      mdtPath: document.filePath,
      mdtRelativePath: document.relativePath,
      mdtTitle: document.title,
      createdAt: document.createdAt,
      summary: document.summary,
      openTaskCount: document.openTaskCount,
      completedTaskCount: document.completedTaskCount,
    };

    for (const relatedFile of document.relatedFiles) {
      const key = normalizePathKey(relatedFile);
      const existing = referencesByFile.get(key) || [];
      if (existing.some((entry) => normalizePathKey(entry.mdtPath) === normalizePathKey(document.filePath))) {
        continue;
      }
      referencesByFile.set(key, [...existing, reference]);
    }
  }

  for (const [key, entries] of referencesByFile.entries()) {
    entries.sort((left, right) => {
      if (left.openTaskCount !== right.openTaskCount) {
        return right.openTaskCount - left.openTaskCount;
      }
      return left.mdtTitle.localeCompare(right.mdtTitle, 'zh-CN');
    });
    referencesByFile.set(key, entries);
  }

  return referencesByFile;
}

export function sortMdtDocuments(documents: MdtDocumentSummary[]) {
  return [...documents].sort((left, right) => {
    if (left.openTaskCount !== right.openTaskCount) {
      return right.openTaskCount - left.openTaskCount;
    }

    const leftCreatedAt = left.createdAt ? Date.parse(left.createdAt) : Number.NaN;
    const rightCreatedAt = right.createdAt ? Date.parse(right.createdAt) : Number.NaN;
    const hasComparableDates = Number.isFinite(leftCreatedAt) && Number.isFinite(rightCreatedAt);
    if (hasComparableDates && leftCreatedAt !== rightCreatedAt) {
      return rightCreatedAt - leftCreatedAt;
    }

    return left.title.localeCompare(right.title, 'zh-CN');
  });
}
