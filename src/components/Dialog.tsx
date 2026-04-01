import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';

interface DialogProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: React.ReactNode;
  footer?: React.ReactNode;
  size?: 'sm' | 'md' | 'lg' | 'xl';
}

export function Dialog({ 
  isOpen, 
  onClose, 
  title, 
  children, 
  footer,
  size = 'md' 
}: DialogProps) {
  const overlayRef = useRef<HTMLDivElement>(null);
  const shouldCloseOnMouseUpRef = useRef(false);

  // ESC 键关闭
  useEffect(() => {
    if (!isOpen) return;
    
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  // 只有在按下和抬起都发生在遮罩层本身时才关闭，避免误触关闭
  const handleOverlayMouseDown = (e: React.MouseEvent) => {
    shouldCloseOnMouseUpRef.current = e.target === overlayRef.current;
  };

  const handleOverlayMouseUp = (e: React.MouseEvent) => {
    const shouldClose =
      shouldCloseOnMouseUpRef.current && e.target === overlayRef.current;

    shouldCloseOnMouseUpRef.current = false;

    if (shouldClose) {
      onClose();
    }
  };

  if (!isOpen) return null;

  const sizeClasses = {
    sm: 'w-[360px]',
    md: 'w-[420px]',
    lg: 'w-[560px]',
    xl: 'w-[760px]',
  };

  return (
    <div
      ref={overlayRef}
      onMouseDown={handleOverlayMouseDown}
      onMouseUp={handleOverlayMouseUp}
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 animate-in fade-in duration-150"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className={`bg-white dark:bg-gray-900 rounded-xl shadow-2xl ${sizeClasses[size]} max-w-[95vw] max-h-[90vh] flex flex-col animate-in zoom-in-95 duration-150`}
      >
        {/* 头部 */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200 dark:border-gray-700">
          <h3 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
            {title}
          </h3>
          <button
            onClick={onClose}
            className="p-1.5 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* 内容 */}
        <div className="p-5 overflow-auto">
          {children}
        </div>

        {/* 底部按钮 */}
        {footer && (
          <div className="flex justify-end gap-2 px-5 py-4 border-t border-gray-200 dark:border-gray-700">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}

// 确认对话框专用组件
interface ConfirmDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title?: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  type?: 'info' | 'warning' | 'danger';
}

export function ConfirmDialog({
  isOpen,
  onClose,
  onConfirm,
  title = '确认',
  message,
  confirmText = '确认',
  cancelText = '取消',
  type = 'info',
}: ConfirmDialogProps) {
  const handleConfirm = () => {
    onConfirm();
    onClose();
  };

  const buttonClasses = {
    info: 'bg-blue-600 hover:bg-blue-700',
    warning: 'bg-yellow-600 hover:bg-yellow-700',
    danger: 'bg-red-600 hover:bg-red-700',
  };

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onClose}
      title={title}
      size="sm"
      footer={
        <>
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
          >
            {cancelText}
          </button>
          <button
            onClick={handleConfirm}
            className={`px-4 py-2 text-sm text-white rounded-lg transition-colors ${buttonClasses[type]}`}
          >
            {confirmText}
          </button>
        </>
      }
    >
      <p className="text-gray-700 dark:text-gray-300 whitespace-pre-line">{message}</p>
    </Dialog>
  );
}

// 提示对话框（只有确定按钮）
interface AlertDialogProps {
  isOpen: boolean;
  onClose: () => void;
  title?: string;
  message: string;
  confirmText?: string;
}

export function AlertDialog({
  isOpen,
  onClose,
  title = '提示',
  message,
  confirmText = '确定',
}: AlertDialogProps) {
  return (
    <Dialog
      isOpen={isOpen}
      onClose={onClose}
      title={title}
      size="sm"
      footer={
        <button
          onClick={onClose}
          className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors"
        >
          {confirmText}
        </button>
      }
    >
      <p className="text-gray-700 dark:text-gray-300 whitespace-pre-line">{message}</p>
    </Dialog>
  );
}
