import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  AlertTriangle,
  CheckCircle2,
  Database,
  FileQuestion,
  FolderOpen,
  Image,
  LoaderCircle,
  Puzzle,
  RefreshCw,
  RotateCcw,
  SearchCheck,
  ShieldCheck,
  Trash2,
  Wrench,
} from 'lucide-react';
import { useFileOperationStore } from '../../stores/fileOperationStore';
import { useProjectStoreApi, useProjectStoreShallow } from '../../stores/projectStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { mergeExcludePatterns, readProjectExcludePatterns } from '../../utils/excludePatterns';

interface CacheCategoryReport {
  id: 'thumbnails' | 'treeIndex' | 'fileDetails' | 'projectData' | 'extensions' | 'other' | string;
  label: string;
  physicalBytes: number;
  logicalBytes: number | null;
  entryCount: number;
  anomalyCount: number;
  status: string;
  protected: boolean;
  description: string;
}

interface TreeCacheSummary {
  entryCount: number;
  directoryCount: number;
  dirtyDirectoryCount: number;
  fileDetailsCount: number;
  expiredFileDetailsCount: number;
  missingFileDetailsSources: number;
  fileDetailsLogicalBytes: number;
  lastFullScanTs: number | null;
  isDirty: boolean;
}

interface CacheReport {
  projectPath: string;
  pmCenterPath: string;
  generatedAt: number;
  totalBytes: number;
  reclaimableBytes: number;
  protectedBytes: number;
  healthStatus: string;
  healthMessage: string;
  categories: CacheCategoryReport[];
  tree: TreeCacheSummary;
}

interface CacheCheckIssue {
  code: string;
  category: string;
  severity: string;
  message: string;
  suggestedAction: CacheAction | null;
  affectedCount: number | null;
}

interface CacheCheckReport {
  mode: 'quick' | 'deep';
  status: string;
  checkedAt: number;
  issues: CacheCheckIssue[];
  scannedItems: number;
}

type CacheAction =
  | 'clearThumbnails'
  | 'clearFileDetails'
  | 'rebuildTree'
  | 'clearReclaimable'
  | 'resetAndRepair';

interface CacheActionResult {
  action: CacheAction;
  bytesReclaimed: number;
  affectedItems: number;
  report: CacheReport;
}

interface MaintenanceProgress {
  operationId: string;
  projectPath: string;
  action: string;
  phase: string;
  processedItems: number;
  totalItems: number | null;
  currentPath: string | null;
  cancellable: boolean;
}

const ACTION_LABELS: Record<CacheAction, string> = {
  clearThumbnails: '清理缩略图',
  clearFileDetails: '清理解析缓存',
  rebuildTree: '重建目录树索引',
  clearReclaimable: '清理可回收缓存',
  resetAndRepair: '重置缓存并修复',
};

const PHASE_LABELS: Record<string, string> = {
  preparing: '准备维护',
  clearingThumbnails: '清理缩略图',
  clearingFileDetails: '清理解析缓存',
  rebuildingTree: '扫描并重建目录索引',
  scanningProject: '扫描项目文件',
  checkingThumbnails: '验证缩略图',
  completed: '完成',
};

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function formatDate(timestamp: number | null): string {
  if (!timestamp) return '尚未完整扫描';
  return new Date(timestamp * 1000).toLocaleString();
}

function getCategoryIcon(id: string) {
  switch (id) {
    case 'thumbnails': return Image;
    case 'treeIndex': return Database;
    case 'fileDetails': return SearchCheck;
    case 'projectData': return ShieldCheck;
    case 'extensions': return Puzzle;
    default: return FileQuestion;
  }
}

function getActionEstimate(report: CacheReport | null, action: CacheAction): number {
  if (!report || action === 'rebuildTree') return 0;
  if (action === 'clearThumbnails') {
    return report.categories.find((category) => category.id === 'thumbnails')?.physicalBytes || 0;
  }
  if (action === 'clearFileDetails') {
    return report.categories.find((category) => category.id === 'fileDetails')?.logicalBytes || 0;
  }
  return report.reclaimableBytes;
}

function healthTone(status: string) {
  if (status === 'error') return 'text-red-600 bg-red-50 dark:text-red-300 dark:bg-red-950/30';
  if (status === 'warning') return 'text-amber-700 bg-amber-50 dark:text-amber-300 dark:bg-amber-950/30';
  return 'text-emerald-700 bg-emerald-50 dark:text-emerald-300 dark:bg-emerald-950/30';
}

export function CacheManagerSurface({ isActive }: { isActive: boolean }) {
  const projectStore = useProjectStoreApi();
  const { projectPath, projectName, isInitialized } = useProjectStoreShallow((state) => ({
    projectPath: state.projectPath,
    projectName: state.projectName,
    isInitialized: state.isInitialized,
  }));
  const globalExcludePatterns = useSettingsStore((state) => state.globalExcludePatterns);
  const showToast = useUiStore((state) => state.showToast);
  const [report, setReport] = useState<CacheReport | null>(null);
  const [checkReport, setCheckReport] = useState<CacheCheckReport | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isChecking, setIsChecking] = useState(false);
  const [activeOperationId, setActiveOperationId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const excludePatterns = useMemo(
    () => mergeExcludePatterns(globalExcludePatterns, readProjectExcludePatterns(projectPath)),
    [globalExcludePatterns, projectPath],
  );

  const refreshReport = useCallback(async () => {
    if (!projectPath) return;
    setIsLoading(true);
    setError(null);
    try {
      setReport(await invoke<CacheReport>('get_project_cache_report', { projectPath }));
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setIsLoading(false);
    }
  }, [projectPath]);

  useEffect(() => {
    if (isActive && projectPath) void refreshReport();
  }, [isActive, projectPath, refreshReport]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<MaintenanceProgress>('pm-center:cache-maintenance-progress', (event) => {
      if (event.payload.projectPath !== projectPath) return;
      const progress = event.payload;
      useFileOperationStore.getState().updateOperation(progress.operationId, {
        currentName: progress.currentPath || PHASE_LABELS[progress.phase] || progress.phase,
        itemIndex: progress.processedItems,
        completedItems: progress.processedItems,
        itemCount: progress.totalItems || 0,
        onCancel: progress.cancellable
          ? () => void invoke('cancel_cache_maintenance', { operationId: progress.operationId })
          : undefined,
      });
    }).then((cleanup) => {
      if (disposed) void cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      if (unlisten) void unlisten();
    };
  }, [projectPath]);

  const refreshWorkspace = useCallback(async () => {
    await projectStore.getState().refresh(true, true);
  }, [projectStore]);

  const runQuickCheck = useCallback(async () => {
    if (!projectPath || isChecking || activeOperationId) return;
    setIsChecking(true);
    setError(null);
    try {
      const result = await invoke<CacheCheckReport>('check_project_cache', {
        projectPath,
        mode: 'quick',
        excludePatterns,
      });
      setCheckReport(result);
      await refreshReport();
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setIsChecking(false);
    }
  }, [activeOperationId, excludePatterns, isChecking, projectPath, refreshReport]);

  const runDeepCheck = useCallback(async () => {
    if (!projectPath || isChecking || activeOperationId) return;
    const operationStore = useFileOperationStore.getState();
    let operationId = '';
    operationId = operationStore.startOperation({
      kind: 'maintenance',
      title: '深度检查缓存',
      detail: projectName || projectPath,
      onCancel: () => void invoke('cancel_cache_maintenance', { operationId }),
    });
    setActiveOperationId(operationId);
    setIsChecking(true);
    setError(null);
    try {
      const result = await invoke<CacheCheckReport>('check_project_cache', {
        projectPath,
        mode: 'deep',
        operationId,
        excludePatterns,
      });
      setCheckReport(result);
      operationStore.completeOperation(operationId, {
        title: '深度检查完成',
        detail: result.issues.length ? `发现 ${result.issues.length} 项问题` : '未发现问题',
        completedItems: result.scannedItems,
      });
      await refreshReport();
    } catch (nextError) {
      const message = String(nextError);
      if (message.includes('已取消')) operationStore.markOperationCancelled(operationId);
      else operationStore.failOperation(operationId, message, { title: '深度检查失败' });
      setError(message);
    } finally {
      setIsChecking(false);
      setActiveOperationId(null);
    }
  }, [activeOperationId, excludePatterns, isChecking, projectName, projectPath, refreshReport]);

  const runAction = useCallback(async (action: CacheAction) => {
    if (!projectPath || activeOperationId) return;
    const estimate = getActionEstimate(report, action);
    const estimateText = estimate > 0 ? `预计可释放约 ${formatBytes(estimate)}。\n` : '';
    const warning = action === 'resetAndRepair'
      ? '将清理缩略图和解析缓存，并立即原子重建目录树索引。'
      : action === 'rebuildTree'
        ? '将扫描整个项目并原子替换目录树索引，原索引会保留到扫描成功。'
        : '缓存会在后续访问文件时按需重新生成。';
    if (!window.confirm(`${ACTION_LABELS[action]}？\n${estimateText}${warning}`)) return;

    const operationStore = useFileOperationStore.getState();
    let operationId = '';
    operationId = operationStore.startOperation({
      kind: 'maintenance',
      title: ACTION_LABELS[action],
      detail: projectName || projectPath,
      onCancel: () => void invoke('cancel_cache_maintenance', { operationId }),
    });
    setActiveOperationId(operationId);
    setError(null);
    try {
      const result = await invoke<CacheActionResult>('run_project_cache_action', {
        projectPath,
        action,
        operationId,
        excludePatterns,
      });
      setReport(result.report);
      operationStore.completeOperation(operationId, {
        title: `${ACTION_LABELS[action]}完成`,
        detail: `释放 ${formatBytes(result.bytesReclaimed)}，处理 ${result.affectedItems} 项`,
        completedItems: result.affectedItems,
      });
      await Promise.all([refreshReport(), runQuickCheck(), refreshWorkspace()]);
      showToast({ title: '缓存维护完成', message: `${ACTION_LABELS[action]}已完成`, tone: 'success' });
    } catch (nextError) {
      const message = String(nextError);
      if (message.includes('已取消')) operationStore.markOperationCancelled(operationId);
      else operationStore.failOperation(operationId, message, { title: `${ACTION_LABELS[action]}失败` });
      setError(message);
      showToast({ title: '缓存维护失败', message, tone: message.includes('已取消') ? 'warning' : 'error' });
    } finally {
      setActiveOperationId(null);
    }
  }, [activeOperationId, excludePatterns, projectName, projectPath, refreshReport, refreshWorkspace, report?.reclaimableBytes, runQuickCheck, showToast]);

  if (!isInitialized || !projectPath) {
    return (
      <div className="flex h-full items-center justify-center bg-gray-50 text-sm text-gray-500 dark:bg-gray-950 dark:text-gray-400">
        项目已关闭，重新打开项目后可继续管理缓存。
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto bg-gray-50 dark:bg-gray-950">
      <header className="border-b border-gray-200 bg-white px-5 py-4 dark:border-gray-800 dark:bg-gray-900">
        <div className="mx-auto flex max-w-6xl flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <Database className="h-5 w-5 text-cyan-600" />
              <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">{projectName} 缓存管理</h1>
            </div>
            <p className="mt-1 truncate text-xs text-gray-500 dark:text-gray-400" title={report?.pmCenterPath}>
              {report?.pmCenterPath || `${projectPath}\\.pm_center`}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button type="button" onClick={() => void refreshReport()} disabled={isLoading} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-300 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800">
              <RefreshCw className={`h-3.5 w-3.5 ${isLoading ? 'animate-spin' : ''}`} />刷新
            </button>
            <button type="button" onClick={() => void invoke('open_path', { path: report?.pmCenterPath || `${projectPath}\\.pm_center` })} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-300 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800">
              <FolderOpen className="h-3.5 w-3.5" />打开目录
            </button>
            <button type="button" onClick={() => void runQuickCheck()} disabled={isChecking || Boolean(activeOperationId)} className="inline-flex h-8 items-center gap-1.5 rounded-md bg-gray-800 px-2.5 text-xs text-white hover:bg-gray-700 disabled:opacity-50 dark:bg-gray-100 dark:text-gray-900">
              {isChecking && !activeOperationId ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <SearchCheck className="h-3.5 w-3.5" />}快速检查
            </button>
            <button type="button" onClick={() => void runDeepCheck()} disabled={isChecking || Boolean(activeOperationId)} className="inline-flex h-8 items-center gap-1.5 rounded-md bg-cyan-600 px-2.5 text-xs text-white hover:bg-cyan-700 disabled:opacity-50">
              <Wrench className="h-3.5 w-3.5" />深度检查
            </button>
          </div>
        </div>
      </header>

      <main className="mx-auto max-w-6xl px-5 py-5">
        {error && (
          <div className="mb-4 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" /><span>{error}</span>
          </div>
        )}

        {report ? (
          <>
            <section className="grid grid-cols-2 gap-px overflow-hidden rounded-md border border-gray-200 bg-gray-200 md:grid-cols-4 dark:border-gray-800 dark:bg-gray-800">
              {[
                ['总占用', formatBytes(report.totalBytes)],
                ['预计可回收', formatBytes(report.reclaimableBytes)],
                ['健康状态', report.healthMessage],
                ['最后目录扫描', formatDate(report.tree.lastFullScanTs)],
              ].map(([label, value], index) => (
                <div key={label} className="min-w-0 bg-white px-4 py-3 dark:bg-gray-900">
                  <p className="text-xs text-gray-500 dark:text-gray-400">{label}</p>
                  <p className={`mt-1 truncate text-sm font-semibold ${index === 2 ? healthTone(report.healthStatus).split(' ')[0] : 'text-gray-900 dark:text-gray-100'}`} title={value}>{value}</p>
                </div>
              ))}
            </section>

            <section className="mt-5 overflow-hidden rounded-md border border-gray-200 bg-white dark:border-gray-800 dark:bg-gray-900">
              <div className="border-b border-gray-200 px-4 py-3 dark:border-gray-800">
                <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">空间分类</h2>
              </div>
              {report.categories.map((category) => {
                const Icon = getCategoryIcon(category.id);
                return (
                  <div key={category.id} className="flex flex-col gap-3 border-b border-gray-100 px-4 py-3 last:border-b-0 sm:flex-row sm:items-center dark:border-gray-800">
                    <div className="flex min-w-0 flex-1 items-start gap-3">
                      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300"><Icon className="h-4 w-4" /></div>
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <h3 className="text-sm font-medium text-gray-900 dark:text-gray-100">{category.label}</h3>
                          <span className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${healthTone(category.status)}`}>{category.protected ? '受保护' : category.status === 'healthy' ? '正常' : '需注意'}</span>
                        </div>
                        <p className="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{category.description}</p>
                      </div>
                    </div>
                    <div className="flex shrink-0 items-center justify-between gap-4 sm:justify-end">
                      <div className="text-right">
                        <p className="text-sm font-semibold text-gray-900 dark:text-gray-100">{category.logicalBytes !== null ? formatBytes(category.logicalBytes) : formatBytes(category.physicalBytes)}</p>
                        <p className="text-xs text-gray-500 dark:text-gray-400">{category.entryCount} 项{category.anomalyCount > 0 ? ` · ${category.anomalyCount} 异常` : ''}</p>
                      </div>
                      {category.id === 'thumbnails' && <button type="button" onClick={() => void runAction('clearThumbnails')} disabled={Boolean(activeOperationId)} className="h-8 rounded-md border border-gray-300 px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800">清理</button>}
                      {category.id === 'fileDetails' && <button type="button" onClick={() => void runAction('clearFileDetails')} disabled={Boolean(activeOperationId)} className="h-8 rounded-md border border-gray-300 px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800">清理</button>}
                      {category.id === 'treeIndex' && <button type="button" onClick={() => void runAction('rebuildTree')} disabled={Boolean(activeOperationId)} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-300 px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"><RotateCcw className="h-3.5 w-3.5" />重建</button>}
                    </div>
                  </div>
                );
              })}
            </section>

            {checkReport && (
              <section className="mt-5 overflow-hidden rounded-md border border-gray-200 bg-white dark:border-gray-800 dark:bg-gray-900">
                <div className="flex items-center justify-between border-b border-gray-200 px-4 py-3 dark:border-gray-800">
                  <div className="flex items-center gap-2">
                    {checkReport.status === 'healthy' ? <CheckCircle2 className="h-4 w-4 text-emerald-600" /> : <AlertTriangle className="h-4 w-4 text-amber-600" />}
                    <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">{checkReport.mode === 'deep' ? '深度检查结果' : '快速检查结果'}</h2>
                  </div>
                  <span className="text-xs text-gray-500 dark:text-gray-400">{checkReport.scannedItems > 0 ? `扫描 ${checkReport.scannedItems} 项` : ''}</span>
                </div>
                {checkReport.issues.length === 0 ? (
                  <p className="px-4 py-4 text-sm text-emerald-700 dark:text-emerald-300">未发现缓存完整性问题。</p>
                ) : checkReport.issues.map((issue) => (
                  <div key={`${issue.code}-${issue.category}`} className="flex items-start gap-3 border-b border-gray-100 px-4 py-3 last:border-b-0 dark:border-gray-800">
                    <AlertTriangle className={`mt-0.5 h-4 w-4 shrink-0 ${issue.severity === 'error' ? 'text-red-500' : 'text-amber-500'}`} />
                    <div className="min-w-0 flex-1"><p className="text-sm text-gray-900 dark:text-gray-100">{issue.message}</p><p className="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{issue.code}{issue.affectedCount ? ` · ${issue.affectedCount} 项` : ''}</p></div>
                    {issue.category === 'projectData' && <button type="button" onClick={() => void invoke('open_path', { path: report.pmCenterPath })} className="shrink-0 text-xs text-cyan-700 hover:underline dark:text-cyan-300">打开目录备份</button>}
                  </div>
                ))}
              </section>
            )}

            <section className="mt-5 rounded-md border border-red-200 bg-white px-4 py-4 dark:border-red-900/60 dark:bg-gray-900">
              <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-center">
                <div><h2 className="text-sm font-semibold text-red-700 dark:text-red-300">危险操作</h2><p className="mt-1 text-xs text-gray-500 dark:text-gray-400">不会删除 data.db、脚本、插件或其他受保护文件。</p></div>
                <div className="flex flex-wrap gap-2">
                  <button type="button" onClick={() => void runAction('clearReclaimable')} disabled={Boolean(activeOperationId)} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-red-300 px-2.5 text-xs text-red-700 hover:bg-red-50 disabled:opacity-50 dark:border-red-800 dark:text-red-300 dark:hover:bg-red-950/30"><Trash2 className="h-3.5 w-3.5" />清理可回收缓存</button>
                  <button type="button" onClick={() => void runAction('resetAndRepair')} disabled={Boolean(activeOperationId)} className="inline-flex h-8 items-center gap-1.5 rounded-md bg-red-600 px-2.5 text-xs text-white hover:bg-red-700 disabled:opacity-50"><Wrench className="h-3.5 w-3.5" />重置缓存并修复</button>
                </div>
              </div>
            </section>
          </>
        ) : (
          <div className="flex min-h-64 items-center justify-center text-sm text-gray-500 dark:text-gray-400">
            {isLoading ? <><LoaderCircle className="mr-2 h-4 w-4 animate-spin" />正在读取缓存状态...</> : '暂无缓存报告'}
          </div>
        )}
      </main>
    </div>
  );
}
