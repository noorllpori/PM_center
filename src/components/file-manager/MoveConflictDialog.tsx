import { useEffect, useId, useRef } from 'react';
import { AlertTriangle, FilePenLine, FolderOutput } from 'lucide-react';
import { Dialog } from '../Dialog';

interface MoveConflictDialogProps {
  isOpen: boolean;
  sourceName: string;
  targetLabel: string;
  renameValue: string;
  onRenameValueChange: (value: string) => void;
  onOverwrite: () => void;
  onRename: () => void;
  onCancel: () => void;
  actionLabel?: string;
  renameButtonText?: string;
  overwriteButtonText?: string;
}

export function MoveConflictDialog({
  isOpen,
  sourceName,
  targetLabel,
  renameValue,
  onRenameValueChange,
  onOverwrite,
  onRename,
  onCancel,
  actionLabel = '导入',
  renameButtonText = '重命名',
  overwriteButtonText = '覆盖',
}: MoveConflictDialogProps) {
  const formId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const trimmedRenameValue = renameValue.trim();
  const hasInvalidSeparator = /[\\/]/.test(trimmedRenameValue);
  const isUnchangedName = trimmedRenameValue === sourceName;
  const canRename =
    trimmedRenameValue.length > 0 && !hasInvalidSeparator && !isUnchangedName;

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const animationFrameId = window.requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });

    return () => window.cancelAnimationFrame(animationFrameId);
  }, [isOpen]);

  const handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canRename) {
      return;
    }
    onRename();
  };

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onCancel}
      title="检测到同名文件"
      size="md"
      footer={
        <>
          <button
            onClick={onCancel}
            className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
          >
            取消
          </button>
          <button
            type="submit"
            form={formId}
            disabled={!canRename}
            className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 disabled:bg-blue-300 disabled:cursor-not-allowed text-white rounded-lg transition-colors"
          >
            {renameButtonText}
          </button>
          <button
            onClick={onOverwrite}
            className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors"
          >
            {overwriteButtonText}
          </button>
        </>
      }
    >
      <form id={formId} onSubmit={handleSubmit} className="space-y-4">
        <div className="rounded-xl border border-amber-200 bg-amber-50/90 p-4 dark:border-amber-900/40 dark:bg-amber-950/30">
          <div className="flex items-start gap-3">
            <div className="mt-0.5 rounded-lg bg-amber-100 p-2 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300">
              <AlertTriangle className="h-4 w-4" />
            </div>
            <div className="min-w-0 space-y-1">
              <p className="text-sm font-medium text-gray-900 dark:text-gray-100">
                目标目录里已经存在同名项
              </p>
              <p className="text-sm text-gray-600 dark:text-gray-300">
                {`你可以直接覆盖${actionLabel}，或者修改下面的文件名后再${actionLabel}。`}
              </p>
            </div>
          </div>
        </div>

        <div className="grid gap-3 rounded-xl border border-gray-200 bg-gray-50/80 p-4 dark:border-gray-800 dark:bg-gray-900/60">
          <div className="flex items-start gap-3">
            <FolderOutput className="mt-0.5 h-4 w-4 text-gray-400 dark:text-gray-500" />
            <div className="min-w-0">
              <p className="text-xs font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                目标目录
              </p>
              <p className="break-all text-sm text-gray-900 dark:text-gray-100">
                {targetLabel}
              </p>
            </div>
          </div>

          <div className="flex items-start gap-3">
            <FilePenLine className="mt-0.5 h-4 w-4 text-gray-400 dark:text-gray-500" />
            <div className="min-w-0">
              <p className="text-xs font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                冲突文件
              </p>
              <p className="break-all text-sm font-medium text-gray-900 dark:text-gray-100">
                {sourceName}
              </p>
            </div>
          </div>
        </div>

        <div className="space-y-2">
          <label
            htmlFor={`${formId}-rename`}
            className="text-sm font-medium text-gray-900 dark:text-gray-100"
          >
            {`重命名后${actionLabel}`}
          </label>
          <input
            id={`${formId}-rename`}
            ref={inputRef}
            type="text"
            value={renameValue}
            onChange={(event) => onRenameValueChange(event.target.value)}
            className="w-full rounded-xl border border-gray-300 bg-white px-3 py-2.5 text-sm text-gray-900 outline-none transition focus:border-blue-500 focus:ring-2 focus:ring-blue-500/20 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100 dark:focus:border-blue-400 dark:focus:ring-blue-400/20"
            placeholder="请输入新的文件名"
          />
          <p
            className={`text-xs ${
              hasInvalidSeparator || isUnchangedName
                ? 'text-red-500'
                : 'text-gray-500 dark:text-gray-400'
            }`}
          >
            {hasInvalidSeparator
              ? '文件名不能包含路径分隔符。'
              : isUnchangedName
                ? '新文件名需要和原文件名不同。'
                : '默认已经填好了可用的新文件名，你也可以手动修改。'}
          </p>
        </div>
      </form>
    </Dialog>
  );
}
