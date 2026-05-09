import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Box,
  ChevronDown,
  ChevronRight,
  Database,
  ExternalLink,
  FileIcon,
  Hash,
  Image,
  Link2,
  Maximize2,
  RefreshCw,
} from 'lucide-react';
import { Dialog } from '../Dialog';
import { useResolvedImageSource } from '../image-viewer/useResolvedImageSource';
import { useSettingsStore } from '../../stores/settingsStore';
import type {
  BlenderSceneRenderEdit,
  BlenderWriteReport,
  FileDetailsResponse,
} from '../../types';

type DetailSection = FileDetailsResponse['sections'][number];
type DetailItem = DetailSection['items'][number];
type SaveStatus = 'idle' | 'saving' | 'saved' | 'error';

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

const EXTERNAL_SECTION_IDS = new Set([
  'external-textures',
  'external-libraries',
  'external-texts',
  'linked-data-blocks',
]);

function getFileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
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

function getItem(items: DetailItem[], label: string) {
  return items.find((item) => item.label === label) ?? null;
}

function getItemValue(items: DetailItem[], label: string) {
  return getItem(items, label)?.value || '-';
}

function isBlenderParameterItem(item: DetailItem) {
  return BLENDER_PARAMETER_LABELS.has(item.label);
}

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

async function saveBlenderDetailEdits(
  filePath: string,
  items: DetailItem[],
  draftValues: Record<string, string>,
) {
  const edit: BlenderSceneRenderEdit = {};

  for (const item of items) {
    const editKey = item.editKey;
    if (!editKey || draftValues[editKey] === item.value) {
      continue;
    }

    const partialEdit = buildBlenderEditPayload(editKey, draftValues[editKey] ?? item.value);
    if (!partialEdit) {
      throw new Error(`${item.label} 输入格式不正确`);
    }

    Object.assign(edit, partialEdit);
  }

  await invoke<BlenderWriteReport>('update_blender_scene_render', {
    path: filePath,
    sceneSelector: { kind: 'first' },
    edit,
    options: { backup: true },
  });
}

function buildParameterSummary(items: DetailItem[]) {
  const parts = [
    ['场景数', '场景'],
    ['对象数', '对象'],
    ['图片数', '图片'],
    ['材质数', '材质'],
  ]
    .map(([label, title]) => {
      const value = getItem(items, label)?.value;
      return value ? `${title} ${value}` : null;
    })
    .filter(Boolean);

  return parts.length > 0 ? parts.join(' / ') : `${items.length} 项`;
}

function getDetailsCount(item: DetailItem) {
  const details = item.details;
  if (!details) {
    return null;
  }

  return details.kind === 'textList' ? details.values.length : details.records.length;
}

function SummaryTile({
  label,
  value,
}: {
  label: string;
  value: string;
}) {
  return (
    <div className="rounded-lg border border-gray-200 bg-white px-3 py-2 dark:border-gray-700 dark:bg-gray-900">
      <div className="text-[11px] text-gray-500 dark:text-gray-400">{label}</div>
      <div className="mt-1 truncate text-sm font-medium text-gray-900 dark:text-gray-100" title={value}>
        {value}
      </div>
    </div>
  );
}

function EditableFieldGrid({
  items,
  filePath,
  onSaved,
}: {
  items: DetailItem[];
  filePath: string;
  onSaved: () => Promise<void>;
}) {
  const [draftValues, setDraftValues] = useState<Record<string, string>>({});
  const [status, setStatus] = useState<SaveStatus>('idle');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraftValues(
      Object.fromEntries(
        items
          .filter((item) => item.editKey)
          .map((item) => [item.editKey!, item.value]),
      ),
    );
    setStatus('idle');
    setError(null);
  }, [filePath, items]);

  if (items.length === 0) {
    return null;
  }

  const dirtyItems = items.filter((item) => {
    const editKey = item.editKey;
    return Boolean(editKey) && draftValues[editKey!] !== undefined && draftValues[editKey!] !== item.value;
  });
  const hasChanges = dirtyItems.length > 0;
  const isSaving = status === 'saving';

  const handleSave = async () => {
    if (!hasChanges || isSaving) {
      return;
    }

    setStatus('saving');
    setError(null);

    try {
      await saveBlenderDetailEdits(filePath, items, draftValues);
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
    <section className="rounded-lg border border-blue-100 bg-blue-50/30 p-4 dark:border-blue-900/40 dark:bg-blue-950/10">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">首场景渲染参数</h3>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {status !== 'idle' || error ? (
            <span
              className={`max-w-[220px] truncate text-xs ${
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
          ) : hasChanges ? (
            <span className="text-xs text-blue-600 dark:text-blue-300">{dirtyItems.length} 项未保存</span>
          ) : null}
          <button
            type="button"
            onClick={handleSave}
            disabled={!hasChanges || isSaving}
            className="inline-flex h-8 items-center rounded-md border border-blue-200 bg-blue-600 px-3 text-xs font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:border-gray-200 disabled:bg-gray-100 disabled:text-gray-400 dark:border-blue-700 dark:bg-blue-600 dark:hover:bg-blue-500 dark:disabled:border-gray-700 dark:disabled:bg-gray-800 dark:disabled:text-gray-500"
          >
            保存
          </button>
        </div>
      </div>

      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        {items.map((item) => (
          <div key={item.editKey || item.label} className="min-w-0">
            <div className="mb-1 text-xs font-medium text-gray-500 dark:text-gray-400">{item.label}</div>
            <input
              type="text"
              value={item.editKey ? draftValues[item.editKey] ?? item.value : item.value}
              disabled={isSaving}
              onChange={(event) => {
                if (!item.editKey) {
                  return;
                }

                setDraftValues((current) => ({
                  ...current,
                  [item.editKey!]: event.target.value,
                }));
                if (status !== 'saving') {
                  setStatus('idle');
                  setError(null);
                }
              }}
              onKeyDown={(event) => {
                if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
                  event.preventDefault();
                  void handleSave();
                }
              }}
              className="h-8 w-full min-w-0 rounded-md border border-blue-100 bg-blue-50/55 px-2 text-sm font-medium text-gray-900 outline-none transition-colors hover:border-blue-200 focus:border-blue-400 focus:bg-white disabled:opacity-70 dark:border-blue-900/40 dark:bg-blue-950/20 dark:text-gray-100 dark:hover:border-blue-800 dark:focus:border-blue-600 dark:focus:bg-gray-900"
            />
          </div>
        ))}
      </div>
    </section>
  );
}

function AccordionBlock({
  title,
  summary,
  icon,
  defaultOpen = false,
  children,
}: {
  title: string;
  summary?: string;
  icon: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [isOpen, setIsOpen] = useState(defaultOpen);

  return (
    <section className="rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
      <button
        type="button"
        onClick={() => setIsOpen((value) => !value)}
        className="flex w-full items-center justify-between gap-4 px-4 py-3 text-left transition-colors hover:bg-gray-50 dark:hover:bg-gray-800/70"
      >
        <span className="flex min-w-0 items-center gap-2">
          {isOpen ? (
            <ChevronDown className="h-4 w-4 shrink-0 text-gray-500" />
          ) : (
            <ChevronRight className="h-4 w-4 shrink-0 text-gray-500" />
          )}
          <span className="shrink-0 text-gray-400">{icon}</span>
          <span className="truncate text-sm font-semibold text-gray-900 dark:text-gray-100">{title}</span>
        </span>
        {summary ? (
          <span className="min-w-0 truncate text-right text-xs text-gray-500 dark:text-gray-400">
            {summary}
          </span>
        ) : null}
      </button>

      {isOpen ? (
        <div className="border-t border-gray-200 p-4 dark:border-gray-700">
          {children}
        </div>
      ) : null}
    </section>
  );
}

function ItemGrid({ items }: { items: DetailItem[] }) {
  if (items.length === 0) {
    return null;
  }

  return (
    <div className="grid gap-x-6 gap-y-3 md:grid-cols-2 xl:grid-cols-3">
      {items.map((item) => (
        <div key={item.label} className="min-w-0 border-b border-gray-100 pb-2 last:border-b-0 dark:border-gray-800">
          <div className="text-xs text-gray-500 dark:text-gray-400">{item.label}</div>
          <div className="mt-1 break-all text-sm text-gray-900 dark:text-gray-100">{item.value}</div>
        </div>
      ))}
    </div>
  );
}

function TextListPreview({ item }: { item: DetailItem }) {
  const details = item.details;
  if (!details || details.kind !== 'textList') {
    return null;
  }

  return (
    <div className="max-h-72 overflow-auto">
      <div className="flex flex-wrap gap-2">
        {details.values.map((value, index) => (
          <span
            key={`${value}-${index}`}
            className="max-w-full truncate rounded-md border border-gray-200 bg-gray-50 px-2 py-1 text-xs text-gray-700 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-200"
            title={value}
          >
            {value}
          </span>
        ))}
      </div>
    </div>
  );
}

function RecordsPreview({ item }: { item: DetailItem }) {
  const details = item.details;
  if (!details || details.kind !== 'records') {
    return null;
  }

  return (
    <div className="max-h-80 overflow-auto rounded-md border border-gray-200 dark:border-gray-700">
      {details.records.map((record, index) => (
        <div
          key={index}
          className="border-b border-gray-200 px-3 py-2 last:border-b-0 dark:border-gray-700"
        >
          <div className="grid gap-x-4 gap-y-1 text-xs md:grid-cols-2 xl:grid-cols-3">
            {details.columns.map((column) => {
              const value = formatDetailValue(record[column.key]);
              if (value === '-') {
                return null;
              }

              return (
                <div key={column.key} className="min-w-0">
                  <span className="mr-1 text-gray-500 dark:text-gray-400">{column.label}</span>
                  <span className="break-all text-gray-900 dark:text-gray-100">{value}</span>
                </div>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}

function DetailsContent({ item }: { item: DetailItem }) {
  if (!item.details) {
    return (
      <div className="break-all text-sm text-gray-900 dark:text-gray-100">
        {item.value}
      </div>
    );
  }

  if (item.details.kind === 'textList') {
    return <TextListPreview item={item} />;
  }

  return <RecordsPreview item={item} />;
}

function DataItemBlock({
  item,
  defaultOpen = false,
  onOpenPanel,
}: {
  item: DetailItem;
  defaultOpen?: boolean;
  onOpenPanel: (item: DetailItem) => void;
}) {
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const count = getDetailsCount(item);
  const hasDetails = Boolean(item.details);

  return (
    <div className="border-b border-gray-100 pb-3 last:border-b-0 dark:border-gray-800">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-gray-900 dark:text-gray-100">{item.label}</div>
          <div className="mt-1 break-all text-xs text-gray-500 dark:text-gray-400">{item.value}</div>
        </div>

        {hasDetails ? (
          <div className="flex shrink-0 items-center gap-1">
            <button
              type="button"
              onClick={() => setIsOpen((value) => !value)}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-blue-600 transition-colors hover:bg-blue-50 dark:text-blue-300 dark:hover:bg-blue-900/20"
            >
              {isOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
              {isOpen ? '收起' : '展开'}
            </button>
            <button
              type="button"
              onClick={() => onOpenPanel(item)}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              <Maximize2 className="h-3.5 w-3.5" />
              小面板
            </button>
          </div>
        ) : null}
      </div>

      {count !== null ? (
        <div className="mt-2 text-[11px] text-gray-400 dark:text-gray-500">{count} 项</div>
      ) : null}

      {isOpen ? (
        <div className="mt-3">
          <DetailsContent item={item} />
        </div>
      ) : null}
    </div>
  );
}

function DataSection({
  section,
  defaultOpen,
  onOpenPanel,
}: {
  section: DetailSection;
  defaultOpen?: boolean;
  onOpenPanel: (item: DetailItem) => void;
}) {
  if (section.items.length === 0) {
    return null;
  }

  const summary = section.items
    .map((item) => {
      const count = getDetailsCount(item);
      return count === null ? item.value : `${item.label} ${count} 项`;
    })
    .join(' / ');

  return (
    <AccordionBlock
      title={section.title}
      summary={summary}
      defaultOpen={defaultOpen}
      icon={EXTERNAL_SECTION_IDS.has(section.id) ? <Link2 className="h-4 w-4" /> : <Database className="h-4 w-4" />}
    >
      <div className="space-y-3">
        {section.items.map((item) => (
          <DataItemBlock
            key={item.label}
            item={item}
            defaultOpen={defaultOpen}
            onOpenPanel={onOpenPanel}
          />
        ))}
      </div>
    </AccordionBlock>
  );
}

function DetailsModal({
  item,
  onClose,
}: {
  item: DetailItem | null;
  onClose: () => void;
}) {
  return (
    <Dialog
      isOpen={Boolean(item?.details)}
      onClose={onClose}
      title={item?.label || '完整列表'}
      size="xl"
    >
      {item ? <DetailsContent item={item} /> : null}
    </Dialog>
  );
}

function StatusBanner({
  tone,
  children,
}: {
  tone: 'info' | 'error' | 'warning';
  children: ReactNode;
}) {
  const className =
    tone === 'error'
      ? 'border-red-200 bg-red-50 text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-200'
      : tone === 'warning'
        ? 'border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-200'
        : 'border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-900/50 dark:bg-blue-950/30 dark:text-blue-200';

  return (
    <div className={`rounded-lg border px-3 py-2 text-sm ${className}`}>
      {children}
    </div>
  );
}

function BlenderPreviewPanel({
  filePath,
  title,
}: {
  filePath: string;
  title: string;
}) {
  const { resolvedSource, isLoading, errorMessage } = useResolvedImageSource(filePath);
  const [hasPreviewError, setHasPreviewError] = useState(false);

  useEffect(() => {
    setHasPreviewError(false);
  }, [filePath, resolvedSource]);

  const hasError = Boolean(errorMessage) || hasPreviewError;
  const showImage = Boolean(resolvedSource) && !hasError;

  return (
    <section className="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Image className="h-4 w-4 shrink-0 text-gray-400" />
          <h3 className="truncate text-sm font-semibold text-gray-900 dark:text-gray-100">预览图</h3>
        </div>
        {isLoading || hasError ? (
          <span className={`shrink-0 text-xs ${hasError ? 'text-amber-600 dark:text-amber-300' : 'text-gray-500 dark:text-gray-400'}`}>
            {hasError ? '无预览' : '读取中...'}
          </span>
        ) : null}
      </div>

      <div className="relative flex aspect-[4/3] items-center justify-center overflow-hidden rounded-md bg-gray-950">
        {showImage ? (
          <img
            src={resolvedSource!}
            alt={title}
            className="h-full w-full object-contain"
            draggable={false}
            onError={() => setHasPreviewError(true)}
          />
        ) : (
          <div className="flex h-full w-full items-center justify-center p-6 text-center text-gray-300">
            {isLoading ? (
              <div>
                <RefreshCw className="mx-auto mb-3 h-8 w-8 animate-spin opacity-80" />
                <p className="text-sm">正在读取预览...</p>
              </div>
            ) : (
              <div className="max-w-sm" title={errorMessage || undefined}>
                <Image className="mx-auto mb-2 h-10 w-10 opacity-40" />
                <p className="text-sm font-medium">{hasPreviewError ? '预览图渲染失败' : '暂无可用预览图'}</p>
                {errorMessage ? (
                  <p className="mt-2 line-clamp-2 break-all text-xs text-gray-400">{errorMessage}</p>
                ) : null}
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
}

export function BlenderFileTab({
  filePath,
  title,
}: {
  filePath: string;
  title?: string;
}) {
  const toolPaths = useSettingsStore((state) => state.toolPaths);
  const [details, setDetails] = useState<FileDetailsResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [detailsModalItem, setDetailsModalItem] = useState<DetailItem | null>(null);
  const detailsRef = useRef<FileDetailsResponse | null>(null);
  const requestIdRef = useRef(0);

  const loadDetails = useCallback(
    async (forceRefresh = false) => {
      const requestId = ++requestIdRef.current;
      const hasCurrentDetails = detailsRef.current !== null;

      setErrorMessage(null);
      setIsLoading(!hasCurrentDetails);
      setIsRefreshing(hasCurrentDetails);

      try {
        const result = await invoke<FileDetailsResponse>('get_file_details', {
          path: filePath,
          view: 'dialog',
          toolPaths,
          forceRefresh,
        });

        if (requestId !== requestIdRef.current) {
          return;
        }

        detailsRef.current = result;
        setDetails(result);
      } catch (error) {
        if (requestId !== requestIdRef.current) {
          return;
        }

        setErrorMessage(String(error));
        if (!hasCurrentDetails) {
          detailsRef.current = null;
          setDetails(null);
        }
      } finally {
        if (requestId !== requestIdRef.current) {
          return;
        }

        setIsLoading(false);
        setIsRefreshing(false);
      }
    },
    [filePath, toolPaths],
  );

  useEffect(() => {
    detailsRef.current = null;
    setDetails(null);
    setErrorMessage(null);
    void loadDetails(false);
  }, [loadDetails]);

  const displayName = details?.basic.name || title || getFileNameFromPath(filePath);
  const displayPath = details?.basic.path || filePath;
  const sections = details?.sections || [];
  const mediaSection = sections.find((section) => section.id === 'media') ?? null;
  const basicSection = sections.find((section) => section.id === 'basic') ?? null;
  const parserSection = sections.find((section) => section.id === 'parser-status') ?? null;
  const mediaItems = mediaSection?.items || [];
  const editableItems = mediaItems.filter((item) => item.editKey);
  const parameterItems = mediaItems.filter(isBlenderParameterItem);
  const sceneItems = mediaItems.filter((item) => !item.editKey && !isBlenderParameterItem(item));
  const metadataSections = sections.filter((section) => section.id === 'metadata');
  const externalSections = sections.filter((section) => EXTERNAL_SECTION_IDS.has(section.id));
  const sideSections = [basicSection, parserSection].filter((section): section is DetailSection => Boolean(section));

  const summaryTiles = useMemo(() => {
    if (!mediaSection) {
      return [];
    }

    return [
      ['Blender 版本', getItemValue(mediaItems, 'Blender 版本')],
      ['首场景', getItemValue(mediaItems, '首场景') !== '-' ? getItemValue(mediaItems, '首场景') : getItemValue(mediaItems, '场景')],
      ['对象', getItemValue(mediaItems, '对象数')],
      ['贴图/图片', getItemValue(mediaItems, '图片数')],
      ['材质', getItemValue(mediaItems, '材质数')],
      ['动作', getItemValue(mediaItems, '动作数')],
    ];
  }, [mediaItems, mediaSection]);

  const handleRefresh = useCallback(async () => {
    await loadDetails(true);
  }, [loadDetails]);

  const handleEditSaved = useCallback(async () => {
    await loadDetails(true);
  }, [loadDetails]);

  return (
    <div className="h-full min-h-0 overflow-auto bg-gray-50 text-gray-900 dark:bg-gray-950 dark:text-gray-100">
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 px-5 py-5">
        <header className="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="flex min-w-0 items-start gap-3">
              <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg border border-orange-100 bg-orange-50 text-orange-500 dark:border-orange-900/40 dark:bg-orange-950/20 dark:text-orange-300">
                <Box className="h-6 w-6" />
              </div>
              <div className="min-w-0">
                <h2 className="truncate text-base font-semibold text-gray-950 dark:text-gray-50">
                  {displayName}
                </h2>
                <p className="mt-1 break-all text-xs text-gray-500 dark:text-gray-400">{displayPath}</p>
              </div>
            </div>

            <div className="flex shrink-0 items-center gap-2">
              <button
                type="button"
                onClick={handleRefresh}
                disabled={isLoading || isRefreshing}
                className="inline-flex h-8 items-center gap-2 rounded-md border border-gray-200 bg-white px-3 text-xs text-gray-700 transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-60 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
              >
                <RefreshCw className={`h-3.5 w-3.5 ${isRefreshing ? 'animate-spin' : ''}`} />
                {isRefreshing ? '刷新中...' : '刷新'}
              </button>
              <button
                type="button"
                onClick={() => {
                  void invoke('show_in_folder', { path: filePath });
                }}
                className="inline-flex h-8 items-center gap-2 rounded-md border border-gray-200 bg-white px-3 text-xs text-gray-700 transition-colors hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
              >
                <ExternalLink className="h-3.5 w-3.5" />
                文件夹
              </button>
            </div>
          </div>

          {summaryTiles.length > 0 ? (
            <div className="mt-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6">
              {summaryTiles.map(([label, value]) => (
                <SummaryTile key={label} label={label} value={value} />
              ))}
            </div>
          ) : null}
        </header>

        {isLoading ? (
          <StatusBanner tone="info">正在分析 Blender 文件信息...</StatusBanner>
        ) : null}

        {errorMessage ? (
          <StatusBanner tone={details ? 'warning' : 'error'}>
            {details ? `刷新失败，当前显示缓存信息：${errorMessage}` : `无法读取 Blender 信息：${errorMessage}`}
          </StatusBanner>
        ) : null}

        {details ? (
          <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_320px]">
            <main className="min-w-0 space-y-4">
              <EditableFieldGrid
                items={editableItems}
                filePath={filePath}
                onSaved={handleEditSaved}
              />

              {sceneItems.length > 0 ? (
                <AccordionBlock
                  title="场景概览"
                  summary={sceneItems.map((item) => `${item.label} ${item.value}`).slice(0, 3).join(' / ')}
                  defaultOpen
                  icon={<Hash className="h-4 w-4" />}
                >
                  <ItemGrid items={sceneItems} />
                </AccordionBlock>
              ) : null}

              {parameterItems.length > 0 ? (
                <AccordionBlock
                  title="文件解析参数"
                  summary={buildParameterSummary(parameterItems)}
                  icon={<Database className="h-4 w-4" />}
                >
                  <ItemGrid items={parameterItems} />
                </AccordionBlock>
              ) : null}

              {metadataSections.map((section) => (
                <DataSection
                  key={section.id}
                  section={section}
                  defaultOpen
                  onOpenPanel={setDetailsModalItem}
                />
              ))}

              {externalSections.map((section) => (
                <DataSection
                  key={section.id}
                  section={section}
                  defaultOpen={section.id === 'external-textures'}
                  onOpenPanel={setDetailsModalItem}
                />
              ))}
            </main>

            <aside className="min-w-0 space-y-4">
              <BlenderPreviewPanel
                filePath={filePath}
                title={displayName}
              />

              {sideSections.map((section) => (
                <section
                  key={section.id}
                  className="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900"
                >
                  <div className="mb-3 flex items-center gap-2">
                    {section.id === 'basic' ? (
                      <FileIcon className="h-4 w-4 text-gray-400" />
                    ) : (
                      <Image className="h-4 w-4 text-gray-400" />
                    )}
                    <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">{section.title}</h3>
                  </div>
                  <div className="space-y-2">
                    {section.items.map((item) => (
                      <div key={item.label} className="grid grid-cols-[76px_minmax(0,1fr)] gap-3 text-sm">
                        <span className="text-gray-500 dark:text-gray-400">{item.label}</span>
                        <span className="break-all text-right text-gray-900 dark:text-gray-100">{item.value}</span>
                      </div>
                    ))}
                  </div>
                </section>
              ))}
            </aside>
          </div>
        ) : null}
      </div>

      <DetailsModal
        item={detailsModalItem}
        onClose={() => setDetailsModalItem(null)}
      />
    </div>
  );
}
