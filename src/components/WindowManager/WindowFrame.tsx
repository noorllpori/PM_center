import { useState, useRef, useEffect, type ReactNode } from 'react';
import { useWindowStore, type WindowInstance } from '../../stores/windowStore';
import { 
  X, Minus, Square, Maximize2, Layers, GripVertical,
  Monitor, LayoutGrid, Copy
} from 'lucide-react';

interface WindowFrameProps {
  windowInstance: WindowInstance;
  children: ReactNode;
  // 自定义标题栏内容
  headerContent?: ReactNode;
  // 自定义工具栏内容
  toolbarContent?: ReactNode;
  // 是否显示默认控制按钮
  showDefaultControls?: boolean;
  // 窗口类名
  className?: string;
}

export function WindowFrame({ 
  windowInstance, 
  children, 
  headerContent,
  toolbarContent,
  showDefaultControls = true,
  className = '' 
}: WindowFrameProps) {
  const {
    closeWindow,
    minimizeWindow,
    maximizeWindow,
    restoreWindow,
    toggleAlwaysOnTop,
    focusWindow,
    updatePosition,
    updateSize,
    bringToFront,
    windowOrder,
  } = useWindowStore();

  const frameRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isResizing, setIsResizing] = useState(false);
  const [resizeDirection, setResizeDirection] = useState<string>('');
  const [dragStart, setDragStart] = useState({ x: 0, y: 0, w: 0, h: 0 });
  const [showContextMenu, setShowContextMenu] = useState(false);

  const { id, title, isMinimized, isMaximized, isAlwaysOnTop, position, size } = windowInstance;

  // 计算 z-index
  const zIndex = isAlwaysOnTop ? 1000 + windowOrder.indexOf(id) : 100 + windowOrder.indexOf(id);

  // 拖拽和缩放逻辑
  useEffect(() => {
    if (!isDragging && !isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (isDragging) {
        updatePosition(id, {
          x: e.clientX - dragStart.x,
          y: e.clientY - dragStart.y,
        });
      }
      
      if (isResizing) {
        let newWidth = size.width;
        let newHeight = size.height;
        let newX = position.x;
        let newY = position.y;

        if (resizeDirection.includes('e')) {
          newWidth = Math.max(300, dragStart.w + (e.clientX - dragStart.x));
        }
        if (resizeDirection.includes('w')) {
          const delta = dragStart.x - e.clientX;
          newWidth = Math.max(300, dragStart.w + delta);
          newX = position.x - delta;
        }
        if (resizeDirection.includes('s')) {
          newHeight = Math.max(200, dragStart.h + (e.clientY - dragStart.y));
        }
        if (resizeDirection.includes('n')) {
          const delta = dragStart.y - e.clientY;
          newHeight = Math.max(200, dragStart.h + delta);
          newY = position.y - delta;
        }

        updateSize(id, { width: newWidth, height: newHeight });
        if (resizeDirection.includes('w') || resizeDirection.includes('n')) {
          updatePosition(id, { x: newX, y: newY });
        }
      }
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      setIsResizing(false);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging, isResizing, dragStart, id, position, size, updatePosition, updateSize]);

  if (isMinimized) return null;

  const handleHeaderMouseDown = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('.no-drag')) return;
    
    setIsDragging(true);
    setDragStart({
      x: e.clientX - position.x,
      y: e.clientY - position.y,
      w: size.width,
      h: size.height,
    });
    bringToFront(id);
  };

  const handleResizeStart = (e: React.MouseEvent, direction: string) => {
    e.preventDefault();
    e.stopPropagation();
    setIsResizing(true);
    setResizeDirection(direction);
    setDragStart({
      x: e.clientX,
      y: e.clientY,
      w: size.width,
      h: size.height,
    });
  };

  const handleMaximize = () => {
    if (isMaximized) {
      restoreWindow(id);
    } else {
      maximizeWindow(id);
    }
  };

  // 调整大小手柄
  const resizeHandles = [
    { dir: 'n', className: 'top-0 left-4 right-4 h-1 cursor-n-resize' },
    { dir: 's', className: 'bottom-0 left-4 right-4 h-1 cursor-s-resize' },
    { dir: 'w', className: 'top-4 bottom-4 left-0 w-1 cursor-w-resize' },
    { dir: 'e', className: 'top-4 bottom-4 right-0 w-1 cursor-e-resize' },
    { dir: 'nw', className: 'top-0 left-0 w-3 h-3 cursor-nw-resize' },
    { dir: 'ne', className: 'top-0 right-0 w-3 h-3 cursor-ne-resize' },
    { dir: 'sw', className: 'bottom-0 left-0 w-3 h-3 cursor-sw-resize' },
    { dir: 'se', className: 'bottom-0 right-0 w-3 h-3 cursor-se-resize' },
  ];

  return (
    <div
      ref={frameRef}
      className={`fixed flex flex-col bg-white dark:bg-gray-900 rounded-lg shadow-2xl overflow-hidden border border-gray-200 dark:border-gray-700 transition-shadow ${
        windowInstance.isFocused ? 'ring-2 ring-blue-500/30' : ''
      } ${className}`}
      style={{
        left: position.x,
        top: position.y,
        width: size.width,
        height: size.height,
        zIndex,
      }}
      onMouseDown={() => focusWindow(id)}
      onContextMenu={(e) => {
        e.preventDefault();
        setShowContextMenu(true);
      }}
    >
      {/* 标题栏 */}
      <div
        ref={headerRef}
        className={`flex items-center justify-between px-3 py-2 bg-gray-100 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 select-none ${
          isMaximized ? '' : 'cursor-move'
        }`}
        onMouseDown={handleHeaderMouseDown}
      >
        <div className="flex items-center gap-2 flex-1 min-w-0">
          {!isMaximized && <GripVertical className="w-4 h-4 text-gray-400 flex-shrink-0" />}
          
          {/* 自定义头部内容或默认标题 */}
          {headerContent || (
            <>
              <span className="text-sm font-medium truncate">
                {title}
              </span>
            </>
          )}
        </div>

        {showDefaultControls && (
          <div className="flex items-center gap-0.5 no-drag flex-shrink-0">
            {/* 置顶 */}
            <button
              onClick={() => toggleAlwaysOnTop(id)}
              className={`p-1.5 rounded transition-colors ${
                isAlwaysOnTop
                  ? 'text-yellow-500 bg-yellow-50 dark:bg-yellow-900/20'
                  : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-700'
              }`}
              title={isAlwaysOnTop ? '取消置顶' : '置顶窗口'}
            >
              <Layers className="w-3.5 h-3.5" />
            </button>

            {/* 最小化 */}
            <button
              onClick={() => minimizeWindow(id)}
              className="p-1.5 text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-700 rounded transition-colors"
              title="最小化"
            >
              <Minus className="w-3.5 h-3.5" />
            </button>

            {/* 最大化/还原 */}
            <button
              onClick={handleMaximize}
              className="p-1.5 text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-700 rounded transition-colors"
              title={isMaximized ? '还原' : '最大化'}
            >
              {isMaximized ? <Copy className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
            </button>

            {/* 关闭 */}
            <button
              onClick={() => closeWindow(id)}
              className="p-1.5 text-gray-500 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors"
              title="关闭"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        )}
      </div>

      {/* 自定义工具栏 */}
      {toolbarContent && (
        <div className="flex items-center gap-2 px-3 py-1.5 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700 no-drag">
          {toolbarContent}
        </div>
      )}

      {/* 内容区域 */}
      <div className="flex-1 overflow-hidden relative no-drag">
        {children}
      </div>

      {/* 调整大小手柄 */}
      {!isMaximized && resizeHandles.map(({ dir, className }) => (
        <div
          key={dir}
          className={`absolute ${className} hover:bg-blue-500/20`}
          onMouseDown={(e) => handleResizeStart(e, dir)}
        />
      ))}

      {/* 右键菜单 */}
      {showContextMenu && (
        <WindowContextMenu
          windowId={id}
          onClose={() => setShowContextMenu(false)}
        />
      )}

      {/* 点击外部关闭菜单 */}
      {showContextMenu && (
        <div
          className="fixed inset-0 z-[9999]"
          onClick={() => setShowContextMenu(false)}
        />
      )}
    </div>
  );
}

// 窗口右键菜单
interface WindowContextMenuProps {
  windowId: string;
  onClose: () => void;
}

function WindowContextMenu({ windowId, onClose }: WindowContextMenuProps) {
  const { 
    closeWindow, 
    minimizeWindow, 
    maximizeWindow, 
    restoreWindow,
    bringToFront, 
    sendToBack,
    tileWindows,
    cascadeWindows,
    getWindowById,
    closeOtherWindows,
  } = useWindowStore();

  const window = getWindowById(windowId);
  if (!window) return null;

  const menuItems = [
    { label: window.isMinimized ? '还原' : '最小化', onClick: () => {
      window.isMinimized ? restoreWindow(windowId) : minimizeWindow(windowId);
      onClose();
    }},
    { label: window.isMaximized ? '还原' : '最大化', onClick: () => {
      window.isMaximized ? restoreWindow(windowId) : maximizeWindow(windowId);
      onClose();
    }},
    { divider: true },
    { label: '置于顶层', onClick: () => { bringToFront(windowId); onClose(); } },
    { label: '置于底层', onClick: () => { sendToBack(windowId); onClose(); } },
    { divider: true },
    { label: '平铺排列', onClick: () => { tileWindows(); onClose(); } },
    { label: '层叠排列', onClick: () => { cascadeWindows(); onClose(); } },
    { divider: true },
    { label: '关闭', onClick: () => { closeWindow(windowId); onClose(); }, danger: true },
    { label: '关闭其他', onClick: () => { closeOtherWindows(windowId); onClose(); } },
  ];

  return (
    <div className="absolute top-8 right-2 w-40 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-xl z-[10000] py-1">
      {menuItems.map((item, index) => (
        item.divider ? (
          <div key={index} className="my-1 border-t border-gray-200 dark:border-gray-700" />
        ) : (
          <button
            key={index}
            onClick={item.onClick}
            className={`w-full text-left px-4 py-1.5 text-sm hover:bg-gray-100 dark:hover:bg-gray-700 ${
              item.danger ? 'text-red-500 hover:text-red-600' : ''
            }`}
          >
            {item.label}
          </button>
        )
      ))}
    </div>
  );
}

// 最小化窗口任务栏
export function WindowTaskbar() {
  const { 
    windows, 
    restoreWindow, 
    focusWindow,
    showTaskbar,
    setShowTaskbar,
    focusedWindowId,
  } = useWindowStore();

  const minimized = windows.filter(w => w.isMinimized);

  if (!showTaskbar || minimized.length === 0) return null;

  return (
    <div className="fixed bottom-0 left-0 right-0 h-12 bg-gray-100 dark:bg-gray-900 border-t border-gray-200 dark:border-gray-700 flex items-center px-2 gap-1 z-[90]">
      {/* 显示/隐藏任务栏按钮 */}
      <button
        onClick={() => setShowTaskbar(!showTaskbar)}
        className="p-2 text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800 rounded"
        title="隐藏任务栏"
      >
        <Monitor className="w-4 h-4" />
      </button>

      <div className="w-px h-6 bg-gray-300 dark:bg-gray-700" />

      {/* 最小化窗口列表 */}
      <div className="flex items-center gap-1 flex-1 overflow-x-auto">
        {minimized.map((window) => (
          <button
            key={window.id}
            onClick={() => {
              restoreWindow(window.id);
              focusWindow(window.id);
            }}
            className={`flex items-center gap-2 px-3 py-1.5 rounded transition-colors min-w-0 ${
              focusedWindowId === window.id
                ? 'bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300'
                : 'bg-white dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700'
            }`}
          >
            <WindowIcon type={window.contentType} />
            <span className="text-sm truncate max-w-[120px]">{window.title}</span>
          </button>
        ))}
      </div>

      {/* 窗口计数 */}
      <div className="text-xs text-gray-500 px-2">
        {minimized.length} 个最小化窗口
      </div>
    </div>
  );
}

// 根据窗口类型显示图标
function WindowIcon({ type }: { type: string }) {
  switch (type) {
    case 'code-editor':
      return <span className="text-blue-500">{'</>'}</span>;
    case 'image-viewer':
      return <span className="text-green-500">🖼</span>;
    case 'markdown-preview':
      return <span className="text-purple-500">Md</span>;
    case 'terminal':
      return <span className="text-gray-600">$</span>;
    default:
      return <span className="text-gray-400">□</span>;
  }
}
