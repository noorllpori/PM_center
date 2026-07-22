import {
  Check,
  ChevronDown,
  ChevronUp,
  ClipboardPaste,
  Copy,
  Database,
  LoaderCircle,
  Scissors,
  Trash2,
  Upload,
  X,
} from 'lucide-react';
import {
  useFileOperationStore,
  type FileOperation,
  type FileOperationKind,
} from '../../stores/fileOperationStore';

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return '0 B';
  }

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }

  return `${value.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}

function getOperationIcon(kind: FileOperationKind) {
  switch (kind) {
    case 'import':
      return Upload;
    case 'move':
      return Scissors;
    case 'paste':
      return ClipboardPaste;
    case 'maintenance':
      return Database;
    default:
      return Copy;
  }
}

function getProgress(operation: FileOperation): number | null {
  if (operation.status === 'completed') {
    return 100;
  }
  if (operation.totalBytes > 0) {
    return Math.min(100, Math.max(0, (operation.bytesCompleted / operation.totalBytes) * 100));
  }
  if (operation.itemCount > 0) {
    return Math.min(100, Math.max(0, (operation.completedItems / operation.itemCount) * 100));
  }
  return null;
}

function getStatusLabel(operation: FileOperation, progress: number | null): string {
  switch (operation.status) {
    case 'completed':
      return '完成';
    case 'failed':
      return '失败';
    case 'cancelled':
      return '已取消';
    case 'cancelling':
      return '取消中';
    default:
      return progress === null ? '处理中' : `${Math.round(progress)}%`;
  }
}

function FileOperationRow({ operation }: { operation: FileOperation }) {
  const cancelOperation = useFileOperationStore((state) => state.cancelOperation);
  const removeOperation = useFileOperationStore((state) => state.removeOperation);
  const Icon = getOperationIcon(operation.kind);
  const progress = getProgress(operation);
  const isActive = operation.status === 'running' || operation.status === 'cancelling';
  const statusColor = operation.status === 'failed'
    ? 'text-red-600 dark:text-red-400'
    : operation.status === 'completed'
      ? 'text-green-600 dark:text-green-400'
      : operation.status === 'cancelled'
        ? 'text-gray-500 dark:text-gray-400'
        : 'text-blue-600 dark:text-blue-400';

  return (
    <div className="border-t border-gray-200 px-3 py-3 dark:border-gray-700">
      <div className="flex items-start gap-2.5">
        <div className={`mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-gray-100 ${statusColor} dark:bg-gray-800`}>
          {operation.status === 'completed' ? (
            <Check className="h-4 w-4" />
          ) : isActive ? (
            <LoaderCircle className="h-4 w-4 animate-spin" />
          ) : (
            <Icon className="h-4 w-4" />
          )}
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <p className="truncate text-sm font-medium text-gray-900 dark:text-gray-100">
              {operation.title}
            </p>
            <span className={`shrink-0 text-xs font-medium ${statusColor}`}>
              {getStatusLabel(operation, progress)}
            </span>
          </div>

          {operation.currentName && (
            <p
              className="mt-0.5 truncate text-xs text-gray-500 dark:text-gray-400"
              title={operation.currentName}
            >
              {operation.currentName}
            </p>
          )}

          {isActive && (
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700">
              {progress === null ? (
                <div className="h-full w-1/3 animate-pulse rounded-full bg-blue-500" />
              ) : (
                <div
                  className="h-full rounded-full bg-blue-500 transition-[width] duration-150"
                  style={{ width: `${progress}%` }}
                />
              )}
            </div>
          )}

          <div className="mt-1.5 flex items-center justify-between gap-3 text-xs text-gray-500 dark:text-gray-400">
            <span className="truncate">
              {operation.error || operation.detail}
            </span>
            <span className="shrink-0">
              {operation.totalBytes > 0
                ? `${formatBytes(operation.bytesCompleted)} / ${formatBytes(operation.totalBytes)}`
                : operation.itemCount > 0
                  ? `${Math.min(operation.itemIndex || operation.completedItems, operation.itemCount)}/${operation.itemCount}`
                  : ''}
            </span>
          </div>
        </div>

        {(!isActive || operation.onCancel) && (
          <button
            type="button"
            onClick={() => isActive ? cancelOperation(operation.id) : removeOperation(operation.id)}
            disabled={operation.status === 'cancelling'}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-700 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-gray-800 dark:hover:text-gray-200"
            title={isActive ? '取消任务' : '移除记录'}
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>
    </div>
  );
}

export function FileOperationPanel() {
  const operations = useFileOperationStore((state) => state.operations);
  const isCollapsed = useFileOperationStore((state) => state.isCollapsed);
  const toggleCollapsed = useFileOperationStore((state) => state.toggleCollapsed);
  const clearFinished = useFileOperationStore((state) => state.clearFinished);

  if (operations.length === 0) {
    return null;
  }

  const activeCount = operations.filter((operation) =>
    operation.status === 'running' || operation.status === 'cancelling',
  ).length;
  const finishedCount = operations.length - activeCount;

  return (
    <aside className="fixed bottom-4 right-4 z-[130] w-[360px] max-w-[calc(100vw-2rem)] overflow-hidden rounded-lg border border-gray-200 bg-white shadow-xl dark:border-gray-700 dark:bg-gray-900">
      <div className="flex h-11 items-center gap-2 px-3">
        <ClipboardPaste className="h-4 w-4 text-gray-500 dark:text-gray-400" />
        <p className="min-w-0 flex-1 text-sm font-semibold text-gray-900 dark:text-gray-100">
          后台任务
          {activeCount > 0 && (
            <span className="ml-2 text-xs font-normal text-gray-500 dark:text-gray-400">
              {activeCount} 个进行中
            </span>
          )}
        </p>

        {finishedCount > 0 && (
          <button
            type="button"
            onClick={clearFinished}
            className="flex h-7 w-7 items-center justify-center rounded-md text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-800 dark:hover:text-gray-200"
            title="清除已完成任务"
          >
            <Trash2 className="h-4 w-4" />
          </button>
        )}

        <button
          type="button"
          onClick={toggleCollapsed}
          className="flex h-7 w-7 items-center justify-center rounded-md text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-800 dark:hover:text-gray-200"
          title={isCollapsed ? '展开任务面板' : '收起任务面板'}
        >
          {isCollapsed ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
        </button>
      </div>

      {!isCollapsed && (
        <div className="max-h-[360px] overflow-y-auto">
          {operations.map((operation) => (
            <FileOperationRow key={operation.id} operation={operation} />
          ))}
        </div>
      )}
    </aside>
  );
}
