import { useEffect, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { ArrowLeft, GripHorizontal, Pin, PinOff } from 'lucide-react';

type WindowModeButtonTone = 'dark' | 'light';

interface StandaloneWindowModeButtonProps {
  tone?: WindowModeButtonTone;
  compact?: boolean;
}

interface StandaloneWindowControlsProps extends StandaloneWindowModeButtonProps {
  onReturn: () => void;
  isReturning?: boolean;
}

function getButtonClassName(tone: WindowModeButtonTone, isActive: boolean): string {
  if (tone === 'dark') {
    return isActive
      ? 'border-blue-300/70 bg-blue-600/85 text-white hover:bg-blue-500'
      : 'border-white/20 bg-black/50 text-white hover:bg-black/65';
  }

  return isActive
    ? 'border-blue-500 bg-blue-600 text-white hover:bg-blue-700 dark:border-blue-400 dark:bg-blue-600 dark:hover:bg-blue-500'
    : 'border-gray-300 bg-white/95 text-gray-700 hover:bg-gray-100 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800';
}

export function StandaloneWindowModeButton({
  tone = 'light',
  compact = false,
}: StandaloneWindowModeButtonProps) {
  const [isPinnedBorderless, setIsPinnedBorderless] = useState(false);
  const [isUpdating, setIsUpdating] = useState(false);

  const refreshWindowMode = async () => {
    const currentWindow = getCurrentWebviewWindow();
    const [isAlwaysOnTop, isDecorated] = await Promise.all([
      currentWindow.isAlwaysOnTop(),
      currentWindow.isDecorated(),
    ]);
    setIsPinnedBorderless(isAlwaysOnTop || !isDecorated);
  };

  useEffect(() => {
    void refreshWindowMode().catch((error) => {
      console.warn('Failed to read standalone window mode:', error);
    });
  }, []);

  const toggleWindowMode = async () => {
    if (isUpdating) {
      return;
    }

    const currentWindow = getCurrentWebviewWindow();
    const nextMode = !isPinnedBorderless;
    setIsUpdating(true);

    try {
      await currentWindow.setAlwaysOnTop(nextMode);
      await currentWindow.setDecorations(!nextMode);
      setIsPinnedBorderless(nextMode);
    } catch (error) {
      console.error('Failed to update standalone window mode:', error);

      // Restore the title bar and stacking order when switching modes only partially succeeds.
      try {
        await currentWindow.setDecorations(true);
        await currentWindow.setAlwaysOnTop(false);
        setIsPinnedBorderless(false);
      } catch (restoreError) {
        console.error('Failed to restore standalone window mode:', restoreError);
        await refreshWindowMode().catch(() => undefined);
      }
    } finally {
      setIsUpdating(false);
    }
  };

  const startDragging = async (event: React.PointerEvent<HTMLButtonElement>) => {
    event.preventDefault();
    try {
      await getCurrentWebviewWindow().startDragging();
    } catch (error) {
      console.warn('Failed to start dragging standalone window:', error);
    }
  };

  const label = isPinnedBorderless ? '恢复普通窗口' : '置顶无边框';
  const title = isPinnedBorderless
    ? '恢复标题栏并取消置顶'
    : '置顶显示并隐藏窗口标题栏';
  const Icon = isPinnedBorderless ? PinOff : Pin;

  return (
    <>
      {isPinnedBorderless && (
        <button
          type="button"
          onPointerDown={startDragging}
          className={`inline-flex items-center justify-center rounded-md border p-1.5 transition-colors ${getButtonClassName(tone, true)}`}
          title="拖动窗口"
          aria-label="拖动窗口"
        >
          <GripHorizontal className="h-3.5 w-3.5" />
        </button>
      )}
      <button
        type="button"
        onClick={toggleWindowMode}
        disabled={isUpdating}
        aria-pressed={isPinnedBorderless}
        className={`inline-flex items-center justify-center rounded-md border text-xs transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${compact ? 'p-1.5' : 'gap-1.5 px-3 py-1.5'} ${getButtonClassName(tone, isPinnedBorderless)}`}
        title={title}
        aria-label={title}
      >
        <Icon className="h-3.5 w-3.5" />
        {!compact && label}
      </button>
    </>
  );
}

export function StandaloneWindowControls({
  onReturn,
  isReturning = false,
  tone = 'light',
  compact = false,
}: StandaloneWindowControlsProps) {
  return (
    <div className="flex shrink-0 items-center gap-1">
      <StandaloneWindowModeButton tone={tone} compact={compact} />
      <button
        type="button"
        onClick={onReturn}
        disabled={isReturning}
        className={`inline-flex items-center justify-center rounded-md border text-xs transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${compact ? 'p-1.5' : 'gap-1.5 px-3 py-1.5'} ${getButtonClassName(tone, false)}`}
        title="回归到项目标签页"
        aria-label="回归到项目标签页"
      >
        <ArrowLeft className="h-3.5 w-3.5" />
        {!compact && '回归项目标签页'}
      </button>
    </div>
  );
}
