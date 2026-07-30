import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import {
  ExternalLink,
  FileBox,
  FileSearch,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  RefreshCw,
} from 'lucide-react';
import { Dialog } from '../Dialog';
import { ProjectFilePickerDialog } from '../file-manager/ProjectFilePickerDialog';
import { useSettingsStore } from '../../stores/settingsStore';
import type { FileDetailsItem, FileDetailsResponse } from '../../types';

interface BlenderFileParserDialogProps {
  isOpen: boolean;
  onClose: () => void;
  projectPath?: string | null;
  projectName?: string | null;
  initialFilePath?: string | null;
  onOpenInWorkspace?: (filePath: string) => Promise<unknown> | unknown;
}

function normalizePath(path: string) {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').toLocaleLowerCase();
}

function isPathInsideProject(projectPath: string, filePath: string) {
  const projectKey = normalizePath(projectPath);
  const fileKey = normalizePath(filePath);
  return fileKey === projectKey || fileKey.startsWith(`${projectKey}/`);
}

function parentDirectory(path: string) {
  const normalized = path.replace(/[\\/]+$/, '');
  const separator = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'));
  if (separator === 2 && /^[A-Za-z]:/.test(normalized)) return normalized.slice(0, 3);
  return separator > 0 ? normalized.slice(0, separator) : normalized;
}

function isBlenderFile(path: string) {
  return path.toLocaleLowerCase().endsWith('.blend');
}

function formatDetailValue(value: unknown): string {
  if (value === null || value === undefined || value === '') return '-';
  if (typeof value === 'boolean') return value ? '是' : '否';
  if (typeof value === 'string' || typeof value === 'number') return String(value);
  if (Array.isArray(value)) return value.map(formatDetailValue).join(', ');
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function findItemValue(details: FileDetailsResponse, label: string) {
  for (const section of details.sections) {
    const item = section.items.find((entry) => entry.label === label);
    if (item) return item.value;
  }
  return '-';
}

function getDetailsCount(item: FileDetailsItem) {
  if (!item.details) return 0;
  return item.details.kind === 'textList' ? item.details.values.length : item.details.records.length;
}

function parserSourceLabel(source: string) {
  switch (source) {
    case 'native':
      return '内置 BlendIO';
    case 'python':
      return 'Blender Python 回退';
    default:
      return source || '未知';
  }
}

function DetailPayload({ item }: { item: FileDetailsItem }) {
  const details = item.details;
  if (!details) return null;

  if (details.kind === 'textList') {
    return (
      <div className="mt-2 max-h-56 overflow-auto rounded border border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-950/60">
        {details.values.map((value, index) => (
          <div key={`${value}-${index}`} className="border-b border-gray-200 px-3 py-2 text-xs text-gray-700 last:border-b-0 dark:border-gray-700 dark:text-gray-200">
            {value}
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="mt-2 max-h-72 overflow-auto rounded border border-gray-200 dark:border-gray-700">
      <table className="w-full min-w-[560px] border-collapse text-xs">
        <thead className="sticky top-0 bg-gray-100 text-left text-gray-600 dark:bg-gray-800 dark:text-gray-300">
          <tr>
            {details.columns.map((column) => (
              <th key={column.key} className="border-b border-gray-200 px-3 py-2 font-medium dark:border-gray-700">{column.label}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {details.records.map((record, index) => (
            <tr key={index} className="border-b border-gray-100 last:border-b-0 dark:border-gray-800">
              {details.columns.map((column) => (
                <td key={column.key} className="max-w-64 break-all px-3 py-2 text-gray-800 dark:text-gray-100">
                  {formatDetailValue(record[column.key])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ParserSection({
  section,
}: {
  section: FileDetailsResponse['sections'][number];
}) {
  return (
    <section className="rounded-md border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
      <div className="border-b border-gray-200 px-4 py-3 dark:border-gray-700">
        <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">{section.title}</h3>
      </div>
      <div className="grid gap-x-6 gap-y-3 p-4 md:grid-cols-2 xl:grid-cols-3">
        {section.items.map((item, index) => {
          const detailCount = getDetailsCount(item);
          return (
            <div key={`${item.label}-${index}`} className={`min-w-0 ${item.details ? 'md:col-span-2 xl:col-span-3' : ''}`}>
              <div className="text-xs text-gray-500 dark:text-gray-400">{item.label}</div>
              <div className="mt-1 break-all text-sm text-gray-900 dark:text-gray-100">{item.value || '-'}</div>
              {item.details ? (
                <details className="mt-1.5">
                  <summary className="cursor-pointer select-none text-xs text-blue-600 dark:text-blue-300">
                    查看 {detailCount} 项
                  </summary>
                  <DetailPayload item={item} />
                </details>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}

export function BlenderFileParserDialog({
  isOpen,
  onClose,
  projectPath,
  projectName,
  initialFilePath,
  onOpenInWorkspace,
}: BlenderFileParserDialogProps) {
  const toolPaths = useSettingsStore((state) => state.toolPaths);
  const [filePath, setFilePath] = useState<string | null>(null);
  const [details, setDetails] = useState<FileDetailsResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isProjectPickerOpen, setIsProjectPickerOpen] = useState(false);
  const [isSystemPickerOpen, setIsSystemPickerOpen] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const requestIdRef = useRef(0);
  const detailsRef = useRef<FileDetailsResponse | null>(null);

  const loadDetails = useCallback(async (targetPath: string, forceRefresh: boolean) => {
    const requestId = ++requestIdRef.current;
    const hasCurrentDetails = detailsRef.current?.basic.path === targetPath;
    setErrorMessage(null);
    setIsLoading(!hasCurrentDetails);
    setIsRefreshing(hasCurrentDetails);

    if (!hasCurrentDetails) {
      detailsRef.current = null;
      setDetails(null);
    }

    try {
      const result = await invoke<FileDetailsResponse>('get_file_details', {
        path: targetPath,
        view: 'dialog',
        toolPaths,
        forceRefresh,
      });
      if (requestId !== requestIdRef.current) return;
      detailsRef.current = result;
      setDetails(result);
    } catch (error) {
      if (requestId !== requestIdRef.current) return;
      setErrorMessage(String(error));
    } finally {
      if (requestId !== requestIdRef.current) return;
      setIsLoading(false);
      setIsRefreshing(false);
    }
  }, [toolPaths]);

  const selectFile = useCallback((targetPath: string) => {
    if (!isBlenderFile(targetPath)) {
      setErrorMessage('请选择扩展名为 .blend 的 Blender 文件。');
      return;
    }
    setFilePath(targetPath);
    void loadDetails(targetPath, false);
  }, [loadDetails]);

  useEffect(() => {
    if (!isOpen) {
      requestIdRef.current += 1;
      return;
    }
    if (initialFilePath && isBlenderFile(initialFilePath)) {
      selectFile(initialFilePath);
    }
  }, [initialFilePath, isOpen, selectFile]);

  const canOpenInWorkspace = Boolean(
    filePath &&
    projectPath &&
    onOpenInWorkspace &&
    isPathInsideProject(projectPath, filePath),
  );
  const summaryItems = useMemo(() => details ? [
    ['文件大小', details.basic.size_formatted],
    ['Blender 版本', findItemValue(details, 'Blender 版本')],
    ['场景', findItemValue(details, '场景数')],
    ['对象', findItemValue(details, '对象数')],
    ['材质', findItemValue(details, '材质数')],
    ['图片', findItemValue(details, '图片数')],
  ] : [], [details]);
  const visibleSections = details?.sections.filter((section) => section.id !== 'parser-status') || [];

  const openSystemPicker = async () => {
    setIsSystemPickerOpen(true);
    setErrorMessage(null);
    try {
      const selected = await open({
        title: '选择 Blender 文件',
        multiple: false,
        defaultPath: filePath ? parentDirectory(filePath) : projectPath || undefined,
        filters: [{ name: 'Blender 文件', extensions: ['blend'] }],
      });
      if (typeof selected === 'string') {
        selectFile(selected);
      }
    } catch (error) {
      setErrorMessage(`打开系统文件选择器失败：${String(error)}`);
    } finally {
      setIsSystemPickerOpen(false);
    }
  };

  const closeDialog = () => {
    if (!isProjectPickerOpen) {
      onClose();
    }
  };

  return (
    <>
      <Dialog isOpen={isOpen} onClose={closeDialog} title="Blender 文件解析器" size="2xl">
        <div className="flex min-h-[560px] flex-col gap-4">
          <div className="flex flex-col gap-3 border-b border-gray-200 pb-4 dark:border-gray-700 lg:flex-row lg:items-center">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <FileSearch className="h-5 w-5 shrink-0 text-orange-500" />
                <span className="truncate text-sm font-semibold text-gray-900 dark:text-gray-100" title={filePath || undefined}>
                  {filePath ? filePath.split(/[\\/]/).pop() : '尚未选择文件'}
                </span>
              </div>
              <p className="mt-1 truncate text-xs text-gray-500 dark:text-gray-400" title={filePath || undefined}>
                {filePath || (projectPath ? `可从 ${projectName || '当前项目'} 或系统选择文件` : '请从系统选择一个 .blend 文件')}
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              {projectPath ? (
                <button
                  type="button"
                  onClick={() => setIsProjectPickerOpen(true)}
                  className="inline-flex h-9 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-xs text-gray-700 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
                >
                  <FileBox className="h-4 w-4" />项目选择
                </button>
              ) : null}
              <button
                type="button"
                onClick={() => void openSystemPicker()}
                disabled={isSystemPickerOpen}
                className="inline-flex h-9 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
              >
                {isSystemPickerOpen ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <HardDrive className="h-4 w-4" />}
                系统选择
              </button>
              <button
                type="button"
                onClick={() => filePath && void loadDetails(filePath, true)}
                disabled={!filePath || isLoading || isRefreshing}
                className="inline-flex h-9 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-40 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
              >
                <RefreshCw className={`h-4 w-4 ${isRefreshing ? 'animate-spin' : ''}`} />重新解析
              </button>
              <button
                type="button"
                onClick={() => filePath && void invoke('show_in_folder', { path: filePath })}
                disabled={!filePath}
                className="flex h-9 w-9 items-center justify-center rounded-md border border-gray-300 text-gray-600 hover:bg-gray-50 disabled:opacity-40 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
                title="打开所在目录"
              >
                <FolderOpen className="h-4 w-4" />
              </button>
              {canOpenInWorkspace ? (
                <button
                  type="button"
                  onClick={() => {
                    if (!filePath || !onOpenInWorkspace) return;
                    void onOpenInWorkspace(filePath);
                    onClose();
                  }}
                  className="inline-flex h-9 items-center gap-1.5 rounded-md bg-gray-900 px-3 text-xs font-medium text-white hover:bg-gray-800 dark:bg-white dark:text-gray-900 dark:hover:bg-gray-200"
                >
                  <ExternalLink className="h-4 w-4" />在工作区打开
                </button>
              ) : null}
            </div>
          </div>

          {isLoading ? (
            <div className="flex flex-1 items-center justify-center gap-2 text-sm text-gray-500">
              <LoaderCircle className="h-5 w-5 animate-spin" />正在分析 Blender 文件…
            </div>
          ) : null}

          {!isLoading && !filePath ? (
            <div className="flex flex-1 flex-col items-center justify-center text-center text-gray-500">
              <FileSearch className="mb-3 h-12 w-12 opacity-30" />
              <p className="text-sm font-medium text-gray-700 dark:text-gray-200">选择一个 .blend 文件开始解析</p>
              <p className="mt-1 max-w-md text-xs">优先使用内置 BlendIO；遇到不兼容文件时会使用设置中登记的 Blender 版本回退。</p>
            </div>
          ) : null}

          {errorMessage ? (
            <div className={`rounded-md border px-3 py-2 text-sm ${details ? 'border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900/50 dark:bg-amber-950/20 dark:text-amber-200' : 'border-red-200 bg-red-50 text-red-700 dark:border-red-900/50 dark:bg-red-950/20 dark:text-red-200'}`}>
              {details ? `当前结果仍保留：${errorMessage}` : errorMessage}
            </div>
          ) : null}

          {details ? (
            <div className="min-h-0 flex-1 space-y-4 overflow-auto pr-1">
              <div className="flex flex-col gap-3 rounded-md border border-gray-200 bg-gray-50 px-4 py-3 dark:border-gray-700 dark:bg-gray-950/50 sm:flex-row sm:items-center sm:justify-between">
                <div className="min-w-0">
                  <div className="flex items-center gap-2 text-sm font-medium text-gray-900 dark:text-gray-100">
                    <span className={`h-2 w-2 rounded-full ${details.parser.status === 'ok' ? 'bg-emerald-500' : 'bg-amber-500'}`} />
                    解析器：{details.parser.id} · {parserSourceLabel(details.parser.source)}
                  </div>
                  {details.parser.warning ? <p className="mt-1 break-all text-xs text-amber-700 dark:text-amber-300">{details.parser.warning}</p> : null}
                </div>
                {isRefreshing ? <span className="flex shrink-0 items-center gap-1.5 text-xs text-blue-600"><LoaderCircle className="h-3.5 w-3.5 animate-spin" />正在校验最新信息</span> : null}
              </div>

              <div className="grid grid-cols-2 gap-2 md:grid-cols-3 xl:grid-cols-6">
                {summaryItems.map(([label, value]) => (
                  <div key={label} className="rounded-md border border-gray-200 bg-white px-3 py-2 dark:border-gray-700 dark:bg-gray-900">
                    <div className="text-[11px] text-gray-500 dark:text-gray-400">{label}</div>
                    <div className="mt-1 truncate text-sm font-medium text-gray-900 dark:text-gray-100" title={value}>{value}</div>
                  </div>
                ))}
              </div>

              {visibleSections.map((section) => <ParserSection key={section.id} section={section} />)}
            </div>
          ) : null}
        </div>
      </Dialog>

      {projectPath ? (
        <ProjectFilePickerDialog
          isOpen={isProjectPickerOpen}
          projectPath={projectPath}
          title="从当前项目选择 Blender 文件"
          target="file"
          selectionMode="single"
          extensions={['blend']}
          initialDirectory={filePath && isPathInsideProject(projectPath, filePath) ? parentDirectory(filePath) : projectPath}
          onClose={() => setIsProjectPickerOpen(false)}
          onSelect={(paths) => {
            if (paths[0]) selectFile(paths[0]);
          }}
        />
      ) : null}
    </>
  );
}
