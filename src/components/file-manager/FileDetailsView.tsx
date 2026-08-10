import { useCallback, useEffect, useMemo, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { Box, ChevronDown, ChevronRight, ExternalLink, FileIcon, FileText, Film, FolderIcon, Hash, Image, Layers, Link2, Maximize2, Music4, RefreshCw, Tag as TagIcon } from 'lucide-react';
import { Dialog } from '../Dialog';
import {
  BlenderSceneRenderEdit,
  BlenderWriteReport,
  FileDetailsResponse,
  FileInfo,
  Tag,
} from '../../types';
import { useFileDetails } from './useFileDetails';
import {
  isDirectPreviewImageExtension,
  isImageExtension,
} from '../image-viewer/imageViewerUtils';
import { useResolvedImageSource } from '../image-viewer/useResolvedImageSource';
import { useOptionalProjectStore } from '../../stores/projectStore';
import { useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import { getMdtRelativePath, type MdtReferenceEntry } from '../../utils/mdt';
import { cacheResolvedPreviewThumbnail } from './thumbnailCache';
import { isVirtualFile } from '../../utils/collections';
import { UiExtensionSlot } from '../automation/UiExtensionSlot';

interface FileDetailsContentProps {
  file: FileInfo | null;
  fileTagList: Tag[];
  relatedMdtEntries: MdtReferenceEntry[];
  view: 'panel' | 'dialog';
  selectedCount?: number;
}

interface FileDetailsDialogProps {
  file: FileInfo | null;
  fileTagList: Tag[];
  relatedMdtEntries: MdtReferenceEntry[];
  isOpen: boolean;
  onClose: () => void;
}

const AUDIO_EXTENSIONS = new Set(['mp3', 'flac', 'wav', 'ogg', 'opus', 'm4a', 'aac']);
const VIDEO_EXTENSIONS = new Set(['mp4', 'm4v', 'mov', 'avi', 'mkv', 'webm', 'wmv', 'flv', 'mpeg', 'mpg', 'm2ts']);
const TEXT_EXTENSIONS = new Set([
  'txt', 'md', 'markdown', 'mdx', 'mdt', 'csv', 'tsv', 'json', 'jsonc', 'xml',
  'py', 'pyi', 'pyw', 'js', 'mjs', 'cjs', 'ts', 'mts', 'cts', 'tsx', 'jsx',
  'html', 'htm', 'css', 'scss', 'sass', 'less', 'vue', 'svelte', 'astro',
  'rs', 'c', 'h', 'cc', 'cpp', 'cxx', 'hpp', 'hxx', 'cs', 'java', 'kt', 'kts',
  'go', 'php', 'rb', 'swift', 'sh', 'bash', 'zsh', 'bat', 'cmd', 'ps1', 'psm1',
  'psd1', 'yml', 'yaml', 'toml', 'ini', 'conf', 'cfg', 'config', 'properties',
  'env', 'gitignore', 'gitattributes', 'editorconfig', 'dockerfile', 'makefile',
  'mk', 'gradle', 'sql', 'prisma', 'graphql', 'gql', 'lua', 'log',
]);

const BLENDER_PARAMETER_LABELS = new Set([
  '压缩方式',
  '字节序',
  '块头类型',
  '指针大小',
  '块数量',
  'ID 数量',
  '场景数',
  '对象数',
  '集合数',
  '网格数',
  '材质数',
  '相机数',
  '灯光数',
  '动作数',
  '图片数',
]);

function getFileExtension(file: FileInfo | null) {
  return file?.extension?.toLowerCase() || '';
}

function getFileIcon(file: FileInfo | null) {
  if (!file) {
    return <FileIcon className="w-16 h-16 text-gray-400" />;
  }

  if (file.is_dir) {
    return <FolderIcon className="w-16 h-16 text-yellow-500" />;
  }

  const ext = getFileExtension(file);

  if (isImageExtension(ext)) {
    return <Image className="w-16 h-16 text-purple-500" />;
  }

  if (AUDIO_EXTENSIONS.has(ext)) {
    return <Music4 className="w-16 h-16 text-emerald-500" />;
  }

  if (VIDEO_EXTENSIONS.has(ext)) {
    return <Film className="w-16 h-16 text-red-500" />;
  }

  if (ext === 'blend') {
    return <Box className="w-16 h-16 text-orange-500" />;
  }

  if (TEXT_EXTENSIONS.has(ext)) {
    return <FileText className="w-16 h-16 text-blue-500" />;
  }

  return <FileIcon className="w-16 h-16 text-gray-400" />;
}

function resolvePreviewSource(path: string | null) {
  if (!path) {
    return null;
  }

  if (/^(asset|https?|data|blob):/i.test(path)) {
    return path;
  }

  return convertFileSrc(path);
}

function getFilePreview(file: FileInfo | null): { kind: 'image' | 'video'; src: string } | null {
  if (!file || file.is_dir) {
    return null;
  }

  if (file.thumbnail) {
    const src = resolvePreviewSource(file.thumbnail);
    return src ? { kind: 'image', src } : null;
  }

  const ext = getFileExtension(file);
  const src = resolvePreviewSource(file.path);
  if (!src) {
    return null;
  }

  if (isImageExtension(ext) && isDirectPreviewImageExtension(ext)) {
    return { kind: 'image', src };
  }

  if (ext === 'psd') {
    return { kind: 'image', src: file.path };
  }

  if (ext === 'blend') {
    return { kind: 'image', src: file.path };
  }

  if (VIDEO_EXTENSIONS.has(ext)) {
    return { kind: 'video', src };
  }

  return null;
}

function FilePreviewHeader({
  file,
  projectPath,
}: {
  file: FileInfo | null;
  projectPath: string | null;
}) {
  const preview = useMemo(() => getFilePreview(file), [file]);
  const {
    resolvedSource,
    isLoading: isImageLoading,
    errorMessage: imageErrorMessage,
  } = useResolvedImageSource(preview?.kind === 'image' ? preview.src : '');
  const [hasPreviewError, setHasPreviewError] = useState(false);

  useEffect(() => {
    setHasPreviewError(false);
  }, [preview?.kind, preview?.src, file?.path]);

  useEffect(() => {
    if (preview?.kind === 'image' && imageErrorMessage) {
      setHasPreviewError(true);
    }
  }, [imageErrorMessage, preview?.kind]);

  useEffect(() => {
    if (preview?.kind !== 'image' || !resolvedSource) {
      return;
    }

    void cacheResolvedPreviewThumbnail(projectPath, file, resolvedSource);
  }, [file, preview?.kind, projectPath, resolvedSource]);

  if (!preview || hasPreviewError) {
    return (
      <div className="flex justify-center py-8 bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">
        {getFileIcon(file)}
      </div>
    );
  }

  return (
    <div className="border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
      <div className="flex justify-center px-4 py-4">
        <div className="flex w-full items-center justify-center overflow-hidden rounded-xl border border-white/70 bg-gradient-to-br from-white to-gray-100 shadow-sm dark:border-gray-700 dark:from-gray-900 dark:to-gray-800">
          {preview.kind === 'image' ? (
            resolvedSource ? (
              <img
                src={resolvedSource}
                alt={file?.name || '文件预览'}
                className="max-h-[260px] w-full object-contain"
                onError={() => setHasPreviewError(true)}
              />
            ) : (
              <div className="flex min-h-[180px] w-full items-center justify-center text-sm text-gray-500 dark:text-gray-400">
                {isImageLoading ? '正在读取预览...' : '正在准备预览...'}
              </div>
            )
          ) : (
            <video
              src={preview.src}
              className="max-h-[260px] w-full bg-black object-contain"
              controls
              preload="metadata"
              muted
              onError={() => setHasPreviewError(true)}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function formatDetailValue(value: unknown): string {
  if (value === null || value === undefined || value === '') {
    return '-';
  }

  if (typeof value === 'boolean') {
    return value ? '是' : '否';
  }

  if (Array.isArray(value)) {
    return value.map(formatDetailValue).join('、');
  }

  return String(value);
}

function isBlenderParameterItem(item: FileDetailsResponse['sections'][number]['items'][number]) {
  return BLENDER_PARAMETER_LABELS.has(item.label);
}

function getItemValue(
  items: FileDetailsResponse['sections'][number]['items'],
  label: string,
) {
  return items.find((item) => item.label === label)?.value;
}

function buildBlenderParameterSummary(items: FileDetailsResponse['sections'][number]['items']) {
  const parts = [
    ['场景数', '场景'],
    ['对象数', '对象'],
    ['图片数', '图片'],
  ]
    .map(([label, title]) => {
      const value = getItemValue(items, label);
      return value ? `${title} ${value}` : null;
    })
    .filter(Boolean);

  return parts.length > 0 ? parts.join(' / ') : `${items.length} 项参数`;
}

type EditableStatus = 'idle' | 'saving' | 'saved' | 'error';

function parseResolutionValue(value: string) {
  const match = value.trim().match(/^(\d+)\s*[xX×]\s*(\d+)$/);
  if (!match) {
    return null;
  }
  return {
    resolutionX: Number(match[1]),
    resolutionY: Number(match[2]),
  };
}

function parseFrameRangeValue(value: string) {
  const match = value.trim().match(/^(-?\d+)\s*[-~至]\s*(-?\d+)$/);
  if (!match) {
    return null;
  }
  return {
    frameStart: Number(match[1]),
    frameEnd: Number(match[2]),
  };
}

function buildBlenderEditPayload(editKey: string, value: string): BlenderSceneRenderEdit | null {
  if (editKey === 'scene.resolution') {
    return parseResolutionValue(value);
  }

  if (editKey === 'scene.frameRange') {
    return parseFrameRangeValue(value);
  }

  if (editKey === 'scene.fps') {
    const fps = Number(value.trim());
    return Number.isFinite(fps) && fps > 0 ? { fps } : null;
  }

  if (editKey === 'scene.outputPath') {
    return { outputPath: value };
  }

  return null;
}

async function saveBlenderDetailEdit(
  filePath: string,
  editKey: string,
  value: string,
) {
  const edit = buildBlenderEditPayload(editKey, value);
  if (!edit) {
    throw new Error('输入格式不正确');
  }

  await invoke<BlenderWriteReport>('update_blender_scene_render', {
    path: filePath,
    sceneSelector: { kind: 'first' },
    edit,
    options: { backup: true },
  });
}

function BlenderEditableValue({
  item,
  filePath,
  onSaved,
}: {
  item: FileDetailsResponse['sections'][number]['items'][number];
  filePath: string;
  onSaved: () => Promise<void>;
}) {
  const [value, setValue] = useState(item.value);
  const [status, setStatus] = useState<EditableStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const editKey = item.editKey;

  useEffect(() => {
    setValue(item.value);
    setStatus('idle');
    setError(null);
  }, [item.value, editKey, filePath]);

  useEffect(() => {
    if (!editKey || value === item.value) {
      return;
    }

    const timer = window.setTimeout(() => {
      void commit();
    }, 900);

    return () => window.clearTimeout(timer);
  }, [editKey, filePath, item.value, value]);

  const commit = async () => {
    if (!editKey || value === item.value || status === 'saving') {
      return;
    }

    setStatus('saving');
    setError(null);
    try {
      await saveBlenderDetailEdit(filePath, editKey, value);
      await onSaved();
      setStatus('saved');
      window.setTimeout(() => {
        setStatus((current) => (current === 'saved' ? 'idle' : current));
      }, 1200);
    } catch (saveError) {
      setStatus('error');
      setError(String(saveError));
    }
  };

  return (
    <div className="flex min-w-0 flex-1 flex-col items-end gap-1">
      <input
        type={editKey === 'scene.outputPath' ? 'text' : 'text'}
        value={value}
        disabled={status === 'saving'}
        onChange={(event) => {
          setValue(event.target.value);
          if (status !== 'saving') {
            setStatus('idle');
            setError(null);
          }
        }}
        onBlur={() => {
          void commit();
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.currentTarget.blur();
          }
        }}
        className="w-full min-w-0 rounded-md border border-transparent bg-transparent px-2 py-1 text-right text-sm text-gray-900 outline-none transition-colors hover:border-gray-200 hover:bg-gray-50 focus:border-blue-300 focus:bg-white dark:text-gray-100 dark:hover:border-gray-700 dark:hover:bg-gray-800 dark:focus:border-blue-700 dark:focus:bg-gray-900"
      />
      {status !== 'idle' || error ? (
        <span
          className={`max-w-full truncate text-[11px] ${
            status === 'error'
              ? 'text-red-600 dark:text-red-300'
              : status === 'saved'
                ? 'text-emerald-600 dark:text-emerald-300'
                : 'text-blue-600 dark:text-blue-300'
          }`}
          title={error || undefined}
        >
          {status === 'saving' ? '保存中...' : status === 'saved' ? '已保存' : error || '保存失败'}
        </span>
      ) : null}
    </div>
  );
}

function DetailInlineList({
  item,
}: {
  item: FileDetailsResponse['sections'][number]['items'][number];
}) {
  const details = item.details;

  if (!details) {
    return null;
  }

  if (details.kind === 'textList') {
    return (
      <div className="mt-2 max-h-56 overflow-auto rounded-lg border border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-800/70">
        {details.values.map((value, index) => (
          <div
            key={`${value}-${index}`}
            className="border-b border-gray-200 px-3 py-2 text-xs text-gray-700 last:border-b-0 dark:border-gray-700 dark:text-gray-200 break-all"
          >
            {value}
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="mt-2 max-h-72 overflow-auto rounded-lg border border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-800/70">
      {details.records.map((record, index) => (
        <div
          key={index}
          className="border-b border-gray-200 px-3 py-2 last:border-b-0 dark:border-gray-700"
        >
          {details.columns.map((column) => {
            const value = formatDetailValue(record[column.key]);
            if (value === '-') {
              return null;
            }

            return (
              <div key={column.key} className="grid grid-cols-[76px_minmax(0,1fr)] gap-2 py-0.5 text-xs">
                <span className="text-gray-500 dark:text-gray-400">{column.label}</span>
                <span className="break-all text-gray-800 dark:text-gray-100">{value}</span>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

function DetailsModal({
  item,
  isOpen,
  onClose,
}: {
  item: FileDetailsResponse['sections'][number]['items'][number] | null;
  isOpen: boolean;
  onClose: () => void;
}) {
  const details = item?.details;

  return (
    <Dialog
      isOpen={isOpen && !!details}
      onClose={onClose}
      title={item?.label || '完整列表'}
      size="xl"
    >
      {details?.kind === 'textList' ? (
        <div className="max-h-[65vh] overflow-auto rounded-lg border border-gray-200 dark:border-gray-700">
          {details.values.map((value, index) => (
            <div
              key={`${value}-${index}`}
              className="border-b border-gray-200 px-3 py-2 text-sm text-gray-800 last:border-b-0 dark:border-gray-700 dark:text-gray-100 break-all"
            >
              {value}
            </div>
          ))}
        </div>
      ) : null}

      {details?.kind === 'records' ? (
        <div className="max-h-[65vh] overflow-auto rounded-lg border border-gray-200 dark:border-gray-700">
          {details.records.map((record, index) => (
            <div
              key={index}
              className="border-b border-gray-200 px-4 py-3 last:border-b-0 dark:border-gray-700"
            >
              {details.columns.map((column) => {
                const value = formatDetailValue(record[column.key]);
                if (value === '-') {
                  return null;
                }

                return (
                  <div key={column.key} className="grid grid-cols-[96px_minmax(0,1fr)] gap-3 py-1 text-sm">
                    <span className="text-gray-500 dark:text-gray-400">{column.label}</span>
                    <span className="break-all text-gray-900 dark:text-gray-100">{value}</span>
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      ) : null}
    </Dialog>
  );
}

function SectionBlock({
  id,
  title,
  items,
  filePath,
  onBlenderEditSaved,
  onOpenDetails,
}: {
  id: string;
  title: string;
  items: FileDetailsResponse['sections'][number]['items'];
  filePath: string;
  onBlenderEditSaved: () => Promise<void>;
  onOpenDetails: (item: FileDetailsResponse['sections'][number]['items'][number]) => void;
}) {
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());
  const [isParameterGroupExpanded, setIsParameterGroupExpanded] = useState(false);

  if (items.length === 0) {
    return null;
  }

  const parameterItems = id === 'media' ? items.filter(isBlenderParameterItem) : [];
  const regularItems = parameterItems.length > 0 ? items.filter((item) => !isBlenderParameterItem(item)) : items;

  const renderEntry = (
    entry: FileDetailsResponse['sections'][number]['items'][number],
    index: number,
    keyPrefix: string,
  ) => {
    const key = `${keyPrefix}-${entry.label}-${index}`;
    const isExpanded = expandedKeys.has(key);
    const hasDetails = !!entry.details;

    return (
      <div
        key={key}
        className={`text-sm ${
          entry.editKey
            ? 'rounded-lg border border-blue-100 bg-blue-50/45 px-2 py-1.5 dark:border-blue-900/40 dark:bg-blue-950/20'
            : ''
        }`}
      >
        <div className="flex items-start gap-3">
          <span className="min-w-[72px] text-gray-500">{entry.label}</span>
          {entry.editKey && filePath ? (
            <BlenderEditableValue
              item={entry}
              filePath={filePath}
              onSaved={onBlenderEditSaved}
            />
          ) : (
            <span className="flex-1 text-right text-gray-900 dark:text-gray-100 break-all">{entry.value}</span>
          )}
        </div>

        {hasDetails && (
          <div className="mt-1 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => {
                setExpandedKeys((current) => {
                  const next = new Set(current);
                  if (next.has(key)) {
                    next.delete(key);
                  } else {
                    next.add(key);
                  }
                  return next;
                });
              }}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-blue-600 transition-colors hover:bg-blue-50 dark:text-blue-300 dark:hover:bg-blue-900/20"
            >
              {isExpanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
              {isExpanded ? '收起' : '展开'}
            </button>
            <button
              type="button"
              onClick={() => onOpenDetails(entry)}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              <Maximize2 className="h-3.5 w-3.5" />
              小面板
            </button>
          </div>
        )}

        {isExpanded && <DetailInlineList item={entry} />}
      </div>
    );
  };

  return (
    <div className="pt-4 border-t border-gray-200 dark:border-gray-700 first:pt-0 first:border-t-0">
      <div className="flex items-center gap-2 mb-3">
        <Hash className="w-4 h-4 text-gray-400" />
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">{title}</span>
      </div>
      <div className="space-y-3">
        {parameterItems.length > 0 && (
          <div className="rounded-lg border border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-800/60">
            <button
              type="button"
              onClick={() => setIsParameterGroupExpanded((value) => !value)}
              className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left"
            >
              <span className="flex min-w-0 items-center gap-2">
                {isParameterGroupExpanded ? <ChevronDown className="h-4 w-4 shrink-0 text-gray-500" /> : <ChevronRight className="h-4 w-4 shrink-0 text-gray-500" />}
                <span className="text-sm font-medium text-gray-700 dark:text-gray-200">文件解析参数</span>
              </span>
              <span className="min-w-0 flex-1 truncate text-right text-xs text-gray-500 dark:text-gray-400">
                {buildBlenderParameterSummary(parameterItems)}
              </span>
            </button>
            {isParameterGroupExpanded && (
              <div className="space-y-3 border-t border-gray-200 px-3 py-3 dark:border-gray-700">
                {parameterItems.map((entry, index) => renderEntry(entry, index, 'parameter'))}
              </div>
            )}
          </div>
        )}

        {regularItems.map((entry, index) => renderEntry(entry, index, 'item'))}
      </div>
    </div>
  );
}

function RelatedMdtSection({
  relatedMdtEntries,
  projectPath,
  onOpenMdt,
}: {
  relatedMdtEntries: MdtReferenceEntry[];
  projectPath: string | null;
  onOpenMdt: (filePath: string) => void;
}) {
  if (relatedMdtEntries.length === 0) {
    return null;
  }

  return (
    <div className="pt-4 border-t border-gray-200 dark:border-gray-700">
      <div className="flex items-center gap-2 mb-3">
        <Link2 className="w-4 h-4 text-gray-400" />
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">关联 MDT</span>
      </div>
      <div className="space-y-2">
        {relatedMdtEntries.map((entry) => (
          <button
            key={`${entry.mdtPath}-${entry.createdAt || ''}`}
            type="button"
            onClick={() => onOpenMdt(entry.mdtPath)}
            className="w-full rounded-lg border border-sky-200 bg-sky-50/70 px-3 py-2 text-left transition-colors hover:bg-sky-100 dark:border-sky-900/40 dark:bg-sky-900/20 dark:hover:bg-sky-900/30"
          >
            <div className="flex items-center justify-between gap-3">
              <span className="truncate text-sm font-medium text-sky-900 dark:text-sky-100">
                {entry.mdtTitle}
              </span>
              <span className="shrink-0 rounded-full bg-white/70 px-2 py-0.5 text-[11px] text-sky-700 dark:bg-black/20 dark:text-sky-200">
                {entry.openTaskCount} 待办
              </span>
            </div>
            <div className="mt-1 truncate text-xs text-sky-700/90 dark:text-sky-200/90">
              {projectPath ? getMdtRelativePath(projectPath, entry.mdtPath) : entry.mdtRelativePath}
            </div>
            <div className="mt-1 text-xs text-sky-800 dark:text-sky-100">
              {entry.summary}
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}

function formatVirtualEntryTimestamp(timestamp: number | undefined) {
  if (!timestamp) {
    return '-';
  }

  return new Date(timestamp * 1000).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function VirtualEntryDetails({ file, view }: { file: FileInfo; view: 'panel' | 'dialog' }) {
  const isCollection = file.entry_kind === 'manual_collection';
  const sequence = file.sequence;
  const directoryPath = file.directory_path || sequence?.directory_path || null;
  const entryLabel = isCollection ? '集合' : '图片序列';
  const EntryIcon = isCollection ? Layers : Film;

  const entries = isCollection
    ? [
        ['成员数量', `${file.item_count ?? file.collection_member_paths?.length ?? 0} 个`],
        ['创建时间', formatVirtualEntryTimestamp(file.created_at)],
        ['修改时间', formatVirtualEntryTimestamp(file.updated_at)],
      ]
    : [
        ['帧范围', sequence ? `${sequence.start_frame}-${sequence.end_frame}` : '-'],
        ['有效帧', `${sequence?.frame_count ?? file.item_count ?? 0} 帧`],
        ['缺失帧', `${sequence?.missing_count ?? 0} 帧`],
        ['格式', sequence?.extension?.toUpperCase() || '-'],
      ];

  return (
    <div className={`h-full flex flex-col bg-white dark:bg-gray-900 ${view === 'panel' ? 'overflow-auto' : ''}`}>
      <div className="flex justify-center border-b border-gray-200 bg-gray-50 py-8 dark:border-gray-700 dark:bg-gray-800">
        <EntryIcon className={`h-16 w-16 ${isCollection ? 'text-violet-500' : 'text-teal-500'}`} />
      </div>

      <div className="space-y-4 p-4">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="min-w-0 break-all text-sm font-medium text-gray-900 dark:text-gray-100">{file.name}</h3>
            <span className={`shrink-0 rounded px-1.5 py-0.5 text-[11px] font-medium ${isCollection ? 'bg-violet-100 text-violet-700 dark:bg-violet-950/50 dark:text-violet-200' : 'bg-teal-100 text-teal-700 dark:bg-teal-950/50 dark:text-teal-200'}`}>
              {entryLabel}
            </span>
          </div>
          <p className="mt-1 break-all text-xs text-gray-500 dark:text-gray-400">{file.path}</p>
        </div>

        <div className="border-t border-gray-200 pt-4 dark:border-gray-700">
          <div className="mb-3 flex items-center gap-2">
            <Hash className="h-4 w-4 text-gray-400" />
            <span className="text-sm font-medium text-gray-700 dark:text-gray-300">{entryLabel}信息</span>
          </div>
          <div className="space-y-3">
            {entries.map(([label, value]) => (
              <div key={label} className="flex items-start gap-3 text-sm">
                <span className="min-w-[72px] text-gray-500 dark:text-gray-400">{label}</span>
                <span className="min-w-0 flex-1 break-all text-right text-gray-900 dark:text-gray-100">{value}</span>
              </div>
            ))}
            <div className="flex items-start gap-3 text-sm">
              <span className="min-w-[72px] text-gray-500 dark:text-gray-400">所属目录</span>
              <span className="min-w-0 flex-1 break-all text-right text-gray-900 dark:text-gray-100">{directoryPath || '-'}</span>
            </div>
          </div>
        </div>

        {directoryPath && (
          <div className="border-t border-gray-200 pt-4 dark:border-gray-700">
            <button
              type="button"
              className="flex w-full items-center justify-center gap-2 rounded bg-gray-100 px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700"
              onClick={() => {
                void invoke('open_path', { path: directoryPath });
              }}
            >
              <ExternalLink className="h-4 w-4" />
              打开所属目录
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function FileDetailsContent({
  file,
  fileTagList,
  relatedMdtEntries,
  view,
  selectedCount = 0,
}: FileDetailsContentProps) {
  const isVirtualEntry = Boolean(file && isVirtualFile(file));
  const { details, isLoading, isRefreshing, errorMessage, refresh, replaceDetails } = useFileDetails(
    isVirtualEntry ? null : file,
    view,
  );
  const projectPath = useOptionalProjectStore((state) => state.projectPath);
  const openFileInTab = useWorkspaceTabStore((state) => state.openFileInTab);
  const [detailsModalItem, setDetailsModalItem] = useState<FileDetailsResponse['sections'][number]['items'][number] | null>(null);

  const displayPath = details?.basic.path || file?.path || '';
  const displayName = details?.basic.name || file?.name || '';
  const sections = details?.sections || [];
  const hasDetails = details !== null;

  const refreshDetailsAfterBlenderEdit = useCallback(async () => {
    if (!file) {
      return;
    }

    const result = await invoke<FileDetailsResponse>('get_file_details', {
      path: file.path,
      view,
      toolPaths: null,
      forceRefresh: true,
    });
    replaceDetails(result);
  }, [file, replaceDetails, view]);

  const actionButton = useMemo(() => {
    if (!file) {
      return null;
    }

    return (
      <button
        className="w-full px-3 py-2 text-sm text-gray-700 bg-gray-100 hover:bg-gray-200
                   dark:text-gray-300 dark:bg-gray-800 dark:hover:bg-gray-700
                   rounded transition-colors flex items-center justify-center gap-2"
        onClick={() => {
          invoke('show_in_folder', { path: file.path }).catch(() => {});
        }}
      >
        <ExternalLink className="w-4 h-4" />
        在文件夹中显示
      </button>
    );
  }, [file]);

  if (!file) {
    return (
      <div className="h-full flex flex-col bg-white dark:bg-gray-900">
        <div className="flex-1 flex items-center justify-center text-gray-400 text-sm p-4 text-center">
          <div>
            <FileIcon className="w-12 h-12 mx-auto mb-3 opacity-50" />
            <p>选择一个文件查看详情</p>
            <p className="text-xs mt-1 text-gray-300">
              {selectedCount > 1 ? `已选择 ${selectedCount} 个文件` : '没有选择文件'}
            </p>
          </div>
        </div>
      </div>
    );
  }

  if (isVirtualEntry) {
    return <VirtualEntryDetails file={file} view={view} />;
  }

  return (
    <div className={`h-full flex flex-col bg-white dark:bg-gray-900 ${view === 'panel' ? 'overflow-auto' : ''}`}>
      <FilePreviewHeader file={file} projectPath={projectPath} />

      <div className="p-4 space-y-4">
        <div>
          <h3 className="font-medium text-sm text-gray-900 dark:text-gray-100 break-all">{displayName}</h3>
          <p className="text-xs text-gray-500 mt-1 break-all">{displayPath}</p>
        </div>

        {isLoading && (
          <div className="rounded-lg border border-blue-200 bg-blue-50/80 px-3 py-2 text-sm text-blue-700">
            正在分析文件信息...
          </div>
        )}

        {isRefreshing && hasDetails && (
          <div className="rounded-lg border border-blue-200 bg-blue-50/80 px-3 py-2 text-sm text-blue-700">
            正在刷新文件信息...
          </div>
        )}

        {errorMessage && (
          <div
            className={`rounded-lg px-3 py-2 text-sm break-all ${
              hasDetails
                ? 'border border-yellow-200 bg-yellow-50/80 text-yellow-800'
                : 'border border-red-200 bg-red-50/80 text-red-700'
            }`}
          >
            {hasDetails ? errorMessage : `无法读取详细信息：${errorMessage}`}
          </div>
        )}

        {!isLoading && sections.map((section) => (
          <SectionBlock
            key={section.id}
            id={section.id}
            title={section.title}
            items={section.items}
            filePath={file.path}
            onBlenderEditSaved={refreshDetailsAfterBlenderEdit}
            onOpenDetails={setDetailsModalItem}
          />
        ))}

        <div className="pt-4 border-t border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 mb-3">
            <TagIcon className="w-4 h-4 text-gray-400" />
            <span className="text-sm font-medium text-gray-700 dark:text-gray-300">标签</span>
          </div>

          {fileTagList.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {fileTagList.map((tag) => (
                <span
                  key={tag.id}
                  className="px-2 py-1 text-xs rounded"
                  style={{
                    backgroundColor: `${tag.color}20`,
                    color: tag.color,
                    border: `1px solid ${tag.color}40`,
                  }}
                >
                  {tag.name}
                </span>
              ))}
            </div>
          ) : (
            <p className="text-xs text-gray-400">暂无标签</p>
          )}
        </div>

        <RelatedMdtSection
          relatedMdtEntries={relatedMdtEntries}
          projectPath={projectPath}
          onOpenMdt={(filePath) => {
            void openFileInTab(filePath);
          }}
        />

        <div className="pt-4 border-t border-gray-200 dark:border-gray-700 space-y-2">
          <button
            className="w-full px-3 py-2 text-sm text-gray-700 bg-gray-100 hover:bg-gray-200
                       dark:text-gray-300 dark:bg-gray-800 dark:hover:bg-gray-700
                       rounded transition-colors flex items-center justify-center gap-2 disabled:opacity-60 disabled:cursor-not-allowed"
            onClick={() => {
              void refresh();
            }}
            disabled={isLoading || isRefreshing}
          >
            <RefreshCw className={`w-4 h-4 ${isRefreshing ? 'animate-spin' : ''}`} />
            {isRefreshing ? '刷新中...' : '刷新'}
          </button>
          {actionButton}
        </div>

        <UiExtensionSlot
          targetComponentId="nexora.project-manager"
          pointId="nexora.project-manager.file-details"
          projectPath={projectPath}
          relativeSelection={[file.path]}
        />
      </div>

      <DetailsModal
        item={detailsModalItem}
        isOpen={!!detailsModalItem}
        onClose={() => setDetailsModalItem(null)}
      />
    </div>
  );
}

export function FileDetailsPanel({
  file,
  fileTagList,
  relatedMdtEntries,
  selectedCount = 0,
}: {
  file: FileInfo | null;
  fileTagList: Tag[];
  relatedMdtEntries: MdtReferenceEntry[];
  selectedCount?: number;
}) {
  return (
    <FileDetailsContent
      file={file}
      fileTagList={fileTagList}
      relatedMdtEntries={relatedMdtEntries}
      view="panel"
      selectedCount={selectedCount}
    />
  );
}

export function FileDetailsDialog({
  file,
  fileTagList,
  relatedMdtEntries,
  isOpen,
  onClose,
}: FileDetailsDialogProps) {
  return (
    <Dialog
      isOpen={isOpen && !!file}
      onClose={onClose}
      title="详细信息"
      size="lg"
    >
      <FileDetailsContent
        file={file}
        fileTagList={fileTagList}
        relatedMdtEntries={relatedMdtEntries}
        view="dialog"
      />
    </Dialog>
  );
}
