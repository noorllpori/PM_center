import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import {
  ChevronDown,
  ChevronRight,
  Clock3,
  FileText,
  Image as ImageIcon,
  Link2,
  Plus,
  RefreshCw,
} from 'lucide-react';
import { Dialog } from '../Dialog';
import { useProjectStoreShallow } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useWorkspaceTabStore } from '../../stores/workspaceTabStore';
import { ensureMdtContent, getMdtRelativePath } from '../../utils/mdt';

interface MdtOverviewPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

function formatDateTime(value: string | null) {
  if (!value) {
    return '未知';
  }

  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed)) {
    return value;
  }

  return new Date(parsed).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function getPathSeparator(path: string) {
  return path.includes('\\') ? '\\' : '/';
}

function joinPath(basePath: string, fileName: string) {
  const separator = getPathSeparator(basePath);
  return `${basePath.replace(/[\\/]+$/, '')}${separator}${fileName}`;
}

function formatLocalIsoTimestamp(date: Date) {
  const pad = (value: number) => String(value).padStart(2, '0');
  const timezoneOffsetMinutes = -date.getTimezoneOffset();
  const offsetSign = timezoneOffsetMinutes >= 0 ? '+' : '-';
  const offsetHours = pad(Math.floor(Math.abs(timezoneOffsetMinutes) / 60));
  const offsetMinutes = pad(Math.abs(timezoneOffsetMinutes) % 60);

  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}${offsetSign}${offsetHours}:${offsetMinutes}`;
}

function buildBaseTaskFileName(date: Date) {
  const pad = (value: number) => String(value).padStart(2, '0');
  return `任务代办-${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`;
}

export function MdtOverviewPanel({ isOpen, onClose }: MdtOverviewPanelProps) {
  const {
    projectName,
    projectPath,
    mdtDocuments,
    isLoadingMdtIndex,
    mdtIndexError,
    refreshMdtIndex,
  } = useProjectStoreShallow((state) => ({
    projectName: state.projectName,
    projectPath: state.projectPath,
    mdtDocuments: state.mdtDocuments,
    isLoadingMdtIndex: state.isLoadingMdtIndex,
    mdtIndexError: state.mdtIndexError,
    refreshMdtIndex: state.refreshMdtIndex,
  }));
  const openFileInTab = useWorkspaceTabStore((state) => state.openFileInTab);
  const showToast = useUiStore((state) => state.showToast);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [isCreatingTaskList, setIsCreatingTaskList] = useState(false);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    void refreshMdtIndex();
  }, [isOpen, refreshMdtIndex]);

  useEffect(() => {
    if (!isOpen) {
      setExpandedPaths(new Set());
    }
  }, [isOpen]);

  const summaryStats = useMemo(() => {
    return mdtDocuments.reduce(
      (accumulator, document) => {
        accumulator.documentCount += 1;
        accumulator.openTaskCount += document.openTaskCount;
        accumulator.completedTaskCount += document.completedTaskCount;
        accumulator.relatedFileCount += document.relatedFiles.length;
        return accumulator;
      },
      {
        documentCount: 0,
        openTaskCount: 0,
        completedTaskCount: 0,
        relatedFileCount: 0,
      },
    );
  }, [mdtDocuments]);

  const toggleExpanded = (filePath: string) => {
    setExpandedPaths((current) => {
      const next = new Set(current);
      if (next.has(filePath)) {
        next.delete(filePath);
      } else {
        next.add(filePath);
      }
      return next;
    });
  };

  const handleOpenMdt = (filePath: string) => {
    void openFileInTab(filePath);
    onClose();
  };

  const handleCreateTaskList = async () => {
    if (!projectPath || isCreatingTaskList) {
      return;
    }

    setIsCreatingTaskList(true);

    try {
      const now = new Date();
      const createdAt = formatLocalIsoTimestamp(now);
      const baseName = buildBaseTaskFileName(now);
      let candidateName = `${baseName}.mdt`;
      let candidatePath = joinPath(projectPath, candidateName);
      let duplicateIndex = 2;

      while (await invoke<boolean>('path_exists', { path: candidatePath })) {
        candidateName = `${baseName}-${duplicateIndex}.mdt`;
        candidatePath = joinPath(projectPath, candidateName);
        duplicateIndex += 1;
      }

      const nextContent = ensureMdtContent('', {
        filePath: candidatePath,
        defaultCreatedAt: createdAt,
      }).content;

      await writeTextFile(candidatePath, nextContent);
      await refreshMdtIndex();
      await openFileInTab(candidatePath);
      showToast({
        title: '任务代办已创建',
        message: getMdtRelativePath(projectPath, candidatePath),
        tone: 'success',
      });
      onClose();
    } catch (error) {
      showToast({
        title: '创建失败',
        message: String(error),
        tone: 'error',
      });
    } finally {
      setIsCreatingTaskList(false);
    }
  };

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onClose}
      title={`任务代办总览${projectName ? ` · ${projectName}` : ''}`}
      size="xl"
      footer={(
        <>
          <button
            type="button"
            onClick={() => {
              void handleCreateTaskList();
            }}
            disabled={!projectPath || isCreatingTaskList}
            className="inline-flex items-center gap-1.5 rounded-lg bg-blue-600 px-4 py-2 text-sm text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300"
          >
            <Plus className="h-4 w-4" />
            {isCreatingTaskList ? '创建中...' : '创建任务列表'}
          </button>
          <button
            type="button"
            onClick={() => {
              void refreshMdtIndex();
            }}
            className="inline-flex items-center gap-1.5 rounded-lg px-4 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
          >
            <RefreshCw className={`h-4 w-4 ${isLoadingMdtIndex ? 'animate-spin' : ''}`} />
            刷新
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-gray-200 px-4 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            关闭
          </button>
        </>
      )}
    >
      <div className="space-y-4">
        <div className="grid gap-3 sm:grid-cols-4">
          <div className="rounded-xl border border-gray-200 bg-gray-50 px-4 py-3 dark:border-gray-700 dark:bg-gray-800/60">
            <div className="text-xs text-gray-500 dark:text-gray-400">代办文件</div>
            <div className="mt-1 text-xl font-semibold text-gray-900 dark:text-gray-100">
              {summaryStats.documentCount}
            </div>
          </div>
          <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 dark:border-amber-900/40 dark:bg-amber-900/20">
            <div className="text-xs text-amber-700 dark:text-amber-300">未完成</div>
            <div className="mt-1 text-xl font-semibold text-amber-900 dark:text-amber-100">
              {summaryStats.openTaskCount}
            </div>
          </div>
          <div className="rounded-xl border border-emerald-200 bg-emerald-50 px-4 py-3 dark:border-emerald-900/40 dark:bg-emerald-900/20">
            <div className="text-xs text-emerald-700 dark:text-emerald-300">已完成</div>
            <div className="mt-1 text-xl font-semibold text-emerald-900 dark:text-emerald-100">
              {summaryStats.completedTaskCount}
            </div>
          </div>
          <div className="rounded-xl border border-sky-200 bg-sky-50 px-4 py-3 dark:border-sky-900/40 dark:bg-sky-900/20">
            <div className="text-xs text-sky-700 dark:text-sky-300">关联文件</div>
            <div className="mt-1 text-xl font-semibold text-sky-900 dark:text-sky-100">
              {summaryStats.relatedFileCount}
            </div>
          </div>
        </div>

        <div className="rounded-xl border border-dashed border-gray-200 bg-gray-50 px-4 py-3 text-xs text-gray-600 dark:border-gray-700 dark:bg-gray-800/50 dark:text-gray-300">
          快捷键 `Ctrl+Shift+M` 可随时打开这个面板。这里会汇总项目里的 `.mdt` 任务代办文件，并展示任务、日志、关联文件和媒体引用。
        </div>

        {mdtIndexError ? (
          <div className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900/40 dark:bg-red-900/20 dark:text-red-200">
            读取任务代办索引失败：{mdtIndexError}
          </div>
        ) : null}

        {isLoadingMdtIndex && mdtDocuments.length === 0 ? (
          <div className="rounded-xl border border-gray-200 bg-white px-4 py-10 text-center text-sm text-gray-500 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-400">
            正在扫描项目中的任务代办文件...
          </div>
        ) : null}

        {!isLoadingMdtIndex && mdtDocuments.length === 0 ? (
          <div className="rounded-xl border border-gray-200 bg-white px-4 py-10 text-center dark:border-gray-700 dark:bg-gray-900">
            <FileText className="mx-auto h-10 w-10 text-gray-300 dark:text-gray-600" />
            <p className="mt-3 text-sm font-medium text-gray-900 dark:text-gray-100">
              当前项目还没有任务代办文件
            </p>
            <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
              点击下方按钮会在项目根目录创建一个 `.mdt` 任务代办文件，并自动补齐模板与创建时间。
            </p>
            <div className="mt-4">
              <button
                type="button"
                onClick={() => {
                  void handleCreateTaskList();
                }}
                disabled={!projectPath || isCreatingTaskList}
                className="inline-flex items-center gap-1.5 rounded-lg bg-blue-600 px-4 py-2 text-sm text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300"
              >
                <Plus className="h-4 w-4" />
                {isCreatingTaskList ? '创建中...' : '创建任务列表'}
              </button>
            </div>
          </div>
        ) : null}

        {mdtDocuments.map((document) => {
          const isExpanded = expandedPaths.has(document.filePath);
          const visibleTasks = isExpanded ? document.tasks : document.tasks.slice(0, 5);
          const visibleLogs = isExpanded ? document.logEntries : document.logEntries.slice(0, 3);

          return (
            <div
              key={document.filePath}
              className="overflow-hidden rounded-2xl border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-900"
            >
              <div className="flex items-start gap-3 px-4 py-4">
                <button
                  type="button"
                  onClick={() => toggleExpanded(document.filePath)}
                  className="flex min-w-0 flex-1 items-start gap-3 text-left transition-colors hover:text-blue-700 dark:hover:text-blue-300"
                >
                  <div className="mt-0.5 text-gray-400 dark:text-gray-500">
                    {isExpanded ? (
                      <ChevronDown className="h-4 w-4" />
                    ) : (
                      <ChevronRight className="h-4 w-4" />
                    )}
                  </div>

                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <h3 className="min-w-0 flex-1 truncate text-sm font-semibold text-gray-900 dark:text-gray-100">
                        {document.title}
                      </h3>
                      <span className="rounded-full bg-amber-50 px-2 py-0.5 text-xs text-amber-700 dark:bg-amber-900/30 dark:text-amber-200">
                        {document.openTaskCount} 未完成
                      </span>
                      <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-xs text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-200">
                        {document.completedTaskCount} 已完成
                      </span>
                    </div>

                    <div className="mt-2 flex flex-wrap items-center gap-3 text-xs text-gray-500 dark:text-gray-400">
                      <span className="inline-flex items-center gap-1">
                        <Clock3 className="h-3.5 w-3.5" />
                        {formatDateTime(document.createdAt)}
                      </span>
                      <span className="inline-flex items-center gap-1">
                        <Link2 className="h-3.5 w-3.5" />
                        {document.relatedFiles.length} 关联文件
                      </span>
                      <span className="inline-flex items-center gap-1">
                        <ImageIcon className="h-3.5 w-3.5" />
                        {document.mediaFiles.length} 媒体
                      </span>
                    </div>

                    <p className="mt-2 text-sm text-gray-600 dark:text-gray-300">
                      {document.summary}
                    </p>
                    <p className="mt-2 truncate text-xs text-gray-400 dark:text-gray-500">
                      {document.relativePath}
                    </p>
                  </div>
                </button>

                <button
                  type="button"
                  onClick={() => handleOpenMdt(document.filePath)}
                  className="shrink-0 rounded-lg border border-gray-200 px-3 py-1.5 text-xs text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
                >
                  打开
                </button>
              </div>

              {isExpanded ? (
                <div className="border-t border-gray-200 px-4 py-4 dark:border-gray-700">
                  {document.parseError ? (
                    <div className="mb-4 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900/40 dark:bg-red-900/20 dark:text-red-200">
                      这个任务代办读取时出错，但仍然保留在总览里供你打开修复：{document.parseError}
                    </div>
                  ) : null}

                  <div className="grid gap-4 lg:grid-cols-2">
                    <div className="space-y-3">
                      <div>
                        <div className="text-xs font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                          任务清单
                        </div>
                        {visibleTasks.length > 0 ? (
                          <div className="mt-2 space-y-2">
                            {visibleTasks.map((task, index) => (
                              <div
                                key={`${document.filePath}-task-${index}`}
                                className={`rounded-lg border px-3 py-2 text-sm ${
                                  task.checked
                                    ? 'border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900/40 dark:bg-emerald-900/20 dark:text-emerald-200'
                                    : 'border-amber-200 bg-amber-50 text-amber-900 dark:border-amber-900/40 dark:bg-amber-900/20 dark:text-amber-100'
                                }`}
                              >
                                {task.checked ? '[x]' : '[ ]'} {task.text}
                              </div>
                            ))}
                          </div>
                        ) : (
                          <div className="mt-2 rounded-lg border border-dashed border-gray-200 px-3 py-3 text-sm text-gray-500 dark:border-gray-700 dark:text-gray-400">
                            还没有任务项。
                          </div>
                        )}
                      </div>

                      <div>
                        <div className="text-xs font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                          关联文件
                        </div>
                        {document.relatedFiles.length > 0 ? (
                          <div className="mt-2 flex flex-wrap gap-2">
                            {document.relatedFiles.map((relatedFile) => (
                              <span
                                key={`${document.filePath}-${relatedFile}`}
                                className="rounded-full border border-sky-200 bg-sky-50 px-2.5 py-1 text-xs text-sky-800 dark:border-sky-900/40 dark:bg-sky-900/20 dark:text-sky-200"
                                title={relatedFile}
                              >
                                {projectPath ? getMdtRelativePath(projectPath, relatedFile) : relatedFile}
                              </span>
                            ))}
                          </div>
                        ) : (
                          <div className="mt-2 rounded-lg border border-dashed border-gray-200 px-3 py-3 text-sm text-gray-500 dark:border-gray-700 dark:text-gray-400">
                            暂无关联文件。
                          </div>
                        )}
                      </div>
                    </div>

                    <div className="space-y-3">
                      <div>
                        <div className="text-xs font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                          日志摘要
                        </div>
                        {visibleLogs.length > 0 ? (
                          <div className="mt-2 space-y-2">
                            {visibleLogs.map((logEntry, index) => (
                              <div
                                key={`${document.filePath}-log-${index}`}
                                className="rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-700 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300"
                              >
                                {logEntry}
                              </div>
                            ))}
                          </div>
                        ) : (
                          <div className="mt-2 rounded-lg border border-dashed border-gray-200 px-3 py-3 text-sm text-gray-500 dark:border-gray-700 dark:text-gray-400">
                            暂无日志条目。
                          </div>
                        )}
                      </div>

                      <div>
                        <div className="text-xs font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                          媒体引用
                        </div>
                        {document.mediaFiles.length > 0 ? (
                          <div className="mt-2 flex flex-wrap gap-2">
                            {document.mediaFiles.map((mediaFile) => (
                              <span
                                key={`${document.filePath}-${mediaFile}`}
                                className="rounded-full border border-violet-200 bg-violet-50 px-2.5 py-1 text-xs text-violet-800 dark:border-violet-900/40 dark:bg-violet-900/20 dark:text-violet-200"
                                title={mediaFile}
                              >
                                {projectPath ? getMdtRelativePath(projectPath, mediaFile) : mediaFile}
                              </span>
                            ))}
                          </div>
                        ) : (
                          <div className="mt-2 rounded-lg border border-dashed border-gray-200 px-3 py-3 text-sm text-gray-500 dark:border-gray-700 dark:text-gray-400">
                            暂无媒体引用。
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </Dialog>
  );
}
