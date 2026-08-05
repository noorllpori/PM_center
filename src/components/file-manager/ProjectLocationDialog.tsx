import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import {
  AlertTriangle,
  CheckCircle2,
  FolderOpen,
  FolderSearch,
  LoaderCircle,
  LocateFixed,
  Wrench,
} from 'lucide-react';
import { Dialog } from '../Dialog';
import type {
  ProjectLocationCandidate,
  ProjectLocationReport,
} from '../../api/projects';

interface ProjectLocationDialogProps {
  report: ProjectLocationReport | null;
  candidates: ProjectLocationCandidate[];
  isSearching: boolean;
  hasSearched: boolean;
  searchError: string | null;
  isOpening: boolean;
  onClose: () => void;
  onInitialize: () => void;
  onRepair: () => void;
  onSearch: () => void;
  onSelectLocation: (path: string) => void;
}

function getDialogTitle(report: ProjectLocationReport) {
  switch (report.status) {
    case 'missingDirectory':
      return '项目位置不可用';
    case 'missingPmCenter':
      return '初始化项目';
    case 'incompletePmCenter':
      return '项目数据需要修复';
    case 'invalidDataDb':
      return '项目数据无法安全打开';
    default:
      return '项目位置检查';
  }
}

export function ProjectLocationDialog({
  report,
  candidates,
  isSearching,
  hasSearched,
  searchError,
  isOpening,
  onClose,
  onInitialize,
  onRepair,
  onSearch,
  onSelectLocation,
}: ProjectLocationDialogProps) {
  const [isPickingDirectory, setIsPickingDirectory] = useState(false);

  if (!report) {
    return null;
  }

  const handlePickDirectory = async () => {
    setIsPickingDirectory(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: '选择项目目录',
      });
      if (typeof selected === 'string') {
        onSelectLocation(selected);
      }
    } catch (error) {
      console.error('Failed to select project directory:', error);
    } finally {
      setIsPickingDirectory(false);
    }
  };

  const isMissingDirectory = report.status === 'missingDirectory';
  const isDataDbInvalid = report.status === 'invalidDataDb';
  const busy = isOpening || isPickingDirectory;

  return (
    <Dialog
      isOpen
      onClose={onClose}
      title={getDialogTitle(report)}
      size="lg"
      footer={
        <>
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="rounded-md px-3 py-2 text-sm text-gray-600 transition-colors hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-50 dark:text-gray-300 dark:hover:bg-gray-800"
          >
            取消
          </button>
          <button
            type="button"
            onClick={() => void handlePickDirectory()}
            disabled={busy}
            className="inline-flex items-center gap-1.5 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-200 dark:hover:bg-gray-700"
          >
            {isPickingDirectory ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <FolderOpen className="h-4 w-4" />}
            手动选择目录
          </button>
          {isMissingDirectory ? (
            <button
              type="button"
              onClick={onSearch}
              disabled={busy || isSearching}
              className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-2 text-sm text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isSearching ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <FolderSearch className="h-4 w-4" />}
              自动查找
            </button>
          ) : null}
          {report.canInitialize ? (
            <button
              type="button"
              onClick={onInitialize}
              disabled={busy}
              className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-2 text-sm text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isOpening ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <CheckCircle2 className="h-4 w-4" />}
              初始化并打开
            </button>
          ) : null}
          {report.canRepair ? (
            <button
              type="button"
              onClick={onRepair}
              disabled={busy}
              className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-2 text-sm text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isOpening ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Wrench className="h-4 w-4" />}
              修复并打开
            </button>
          ) : null}
        </>
      }
    >
      <div className="space-y-4">
        <div className="rounded-md border border-gray-200 bg-gray-50 px-3 py-2.5 dark:border-gray-700 dark:bg-gray-800/70">
          <div className="flex items-start gap-2">
            <LocateFixed className="mt-0.5 h-4 w-4 shrink-0 text-gray-400" />
            <div className="min-w-0">
              <p className="text-xs font-medium text-gray-500 dark:text-gray-400">原项目位置</p>
              <p className="mt-1 break-all font-mono text-xs text-gray-700 dark:text-gray-200">{report.projectPath}</p>
            </div>
          </div>
        </div>

        <div className="space-y-2">
          {report.issues.map((issue) => (
            <div
              key={issue.code}
              className={`flex items-start gap-2 rounded-md border px-3 py-2.5 text-sm ${
                issue.severity === 'error'
                  ? 'border-red-200 bg-red-50 text-red-800 dark:border-red-900/70 dark:bg-red-950/30 dark:text-red-200'
                  : 'border-amber-200 bg-amber-50 text-amber-900 dark:border-amber-900/70 dark:bg-amber-950/30 dark:text-amber-100'
              }`}
            >
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
              <p>{issue.message}</p>
            </div>
          ))}
        </div>

        {isDataDbInvalid ? (
          <p className="text-xs leading-5 text-gray-500 dark:text-gray-400">
            请先用资源管理器备份 <code className="font-mono">.pm_center</code>，修复或恢复备份后再选择该项目目录。Nexora 不会自动重建 data.db，以免覆盖标签、集合和项目记录。
          </p>
        ) : null}

        {isMissingDirectory ? (
          <div className="space-y-3 border-t border-gray-200 pt-4 dark:border-gray-700">
            <p className="text-xs leading-5 text-gray-500 dark:text-gray-400">
              自动查找会检查原位置附近和全局设置中的项目根目录，最多列出 20 个候选项目。也可以直接手动选择项目的新位置。
            </p>
            {searchError ? (
              <p className="rounded-md bg-red-50 px-3 py-2 text-xs text-red-700 dark:bg-red-950/30 dark:text-red-200">{searchError}</p>
            ) : null}
            {candidates.length > 0 ? (
              <div className="max-h-52 space-y-1 overflow-auto rounded-md border border-gray-200 p-1 dark:border-gray-700">
                {candidates.map((candidate) => (
                  <button
                    key={candidate.path}
                    type="button"
                    onClick={() => onSelectLocation(candidate.path)}
                    disabled={busy}
                    className="flex w-full items-start gap-2 rounded px-2.5 py-2 text-left transition-colors hover:bg-blue-50 disabled:cursor-not-allowed disabled:opacity-50 dark:hover:bg-blue-950/30"
                  >
                    <FolderOpen className="mt-0.5 h-4 w-4 shrink-0 text-blue-500" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium text-gray-800 dark:text-gray-100">{candidate.name}</span>
                      <span className="mt-0.5 block truncate text-xs text-gray-500 dark:text-gray-400">{candidate.path}</span>
                      <span className="mt-1 block text-[11px] text-blue-600 dark:text-blue-300">{candidate.matchReason}</span>
                    </span>
                  </button>
                ))}
              </div>
            ) : hasSearched && !isSearching && !searchError ? (
              <p className="rounded-md border border-dashed border-gray-300 px-3 py-2.5 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400">
                没有找到可识别的 Nexora 项目。请确认项目磁盘已连接，或手动选择项目的新位置。
              </p>
            ) : null}
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}
