import { Dialog } from '../Dialog';

interface MoveConflictDialogProps {
  isOpen: boolean;
  sourceName: string;
  targetLabel: string;
  onOverwrite: () => void;
  onRename: () => void;
  onCancel: () => void;
}

export function MoveConflictDialog({
  isOpen,
  sourceName,
  targetLabel,
  onOverwrite,
  onRename,
  onCancel,
}: MoveConflictDialogProps) {
  return (
    <Dialog
      isOpen={isOpen}
      onClose={onCancel}
      title="检测到同名文件"
      size="sm"
      footer={
        <>
          <button
            onClick={onCancel}
            className="px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 rounded-lg transition-colors"
          >
            取消
          </button>
          <button
            onClick={onRename}
            className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors"
          >
            自动重命名
          </button>
          <button
            onClick={onOverwrite}
            className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors"
          >
            覆盖
          </button>
        </>
      }
    >
      <p className="text-sm text-gray-700 whitespace-pre-line">
        {`目标目录 ${targetLabel} 中已经存在同名项：\n${sourceName}\n\n请选择要覆盖，还是保留原文件并自动重命名新文件。`}
      </p>
    </Dialog>
  );
}
