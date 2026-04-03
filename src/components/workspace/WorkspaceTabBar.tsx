import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { ExternalLink, FileText, Film, Folder, History, Image as ImageIcon, X } from 'lucide-react';
import type { WorkspaceTab } from '../../stores/workspaceTabStore';

interface WorkspaceTabBarProps {
  tabs: WorkspaceTab[];
  activeTabId: string;
  onActivateTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onReorderTabs: (fromId: string, toId: string) => void;
  onDetachTab: (tabId: string) => Promise<void> | void;
}

interface TabContextMenuState {
  tabId: string;
  x: number;
  y: number;
}

function getTabIcon(tab: WorkspaceTab) {
  switch (tab.type) {
    case 'files':
      return <Folder className="h-4 w-4 text-blue-500" />;
    case 'logs':
      return <History className="h-4 w-4 text-amber-500" />;
    case 'image':
      return <ImageIcon className="h-4 w-4 text-green-500" />;
    case 'text':
      return <FileText className="h-4 w-4 text-sky-500" />;
    case 'video':
      return <Film className="h-4 w-4 text-rose-500" />;
    default:
      return null;
  }
}

function canDetachTab(tab: WorkspaceTab) {
  return tab.type === 'image' || tab.type === 'text' || tab.type === 'video';
}

function getMenuStyle(x: number, y: number): CSSProperties {
  return {
    position: 'fixed',
    left: Math.min(x, window.innerWidth - 240),
    top: Math.min(y, window.innerHeight - 160),
    zIndex: 9999,
  };
}

function ContextMenuItem({
  icon,
  label,
  danger = false,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors ${
        danger
          ? 'text-red-600 hover:bg-red-50 dark:text-red-300 dark:hover:bg-red-900/20'
          : 'text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-700'
      }`}
    >
      <span className={danger ? 'text-red-500' : 'text-gray-500 dark:text-gray-400'}>{icon}</span>
      <span>{label}</span>
    </button>
  );
}

export function WorkspaceTabBar({
  tabs,
  activeTabId,
  onActivateTab,
  onCloseTab,
  onReorderTabs,
  onDetachTab,
}: WorkspaceTabBarProps) {
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dropTargetId, setDropTargetId] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<TabContextMenuState | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const contextMenuTab = useMemo(
    () => tabs.find((tab) => tab.id === contextMenu?.tabId) ?? null,
    [contextMenu?.tabId, tabs],
  );

  useEffect(() => {
    if (contextMenu && !tabs.some((tab) => tab.id === contextMenu.tabId)) {
      setContextMenu(null);
    }
  }, [contextMenu, tabs]);

  useEffect(() => {
    if (!contextMenu) {
      return;
    }

    const handleClickOutside = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setContextMenu(null);
      }
    };

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setContextMenu(null);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [contextMenu]);

  return (
    <div className="border-b border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
      <div className="flex items-stretch gap-1 overflow-x-auto px-2 py-1">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          const isDropTarget = tab.id === dropTargetId && tab.id !== 'files';

          return (
            <div
              key={tab.id}
              draggable={tab.closable}
              className={`group flex min-w-0 max-w-[240px] items-center gap-1.5 rounded-md border px-2.5 py-1 text-[13px] leading-5 transition-colors ${
                isActive
                  ? 'border-blue-200 bg-blue-50/80 text-blue-700 dark:border-blue-700 dark:bg-blue-900/20 dark:text-blue-200'
                  : 'border-transparent bg-transparent text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800/80'
              } ${
                isDropTarget ? 'ring-2 ring-blue-400' : ''
              } ${tab.closable ? 'cursor-grab active:cursor-grabbing' : 'cursor-default'}`}
              title={tab.filePath || tab.title}
              onClick={() => {
                setContextMenu(null);
                onActivateTab(tab.id);
              }}
              onContextMenu={(event) => {
                if (!tab.closable) {
                  return;
                }

                event.preventDefault();
                onActivateTab(tab.id);
                setContextMenu({
                  tabId: tab.id,
                  x: event.clientX,
                  y: event.clientY,
                });
              }}
              onDragStart={() => {
                if (!tab.closable) {
                  return;
                }
                setContextMenu(null);
                setDraggedTabId(tab.id);
              }}
              onDragEnd={() => {
                setDraggedTabId(null);
                setDropTargetId(null);
              }}
              onDragOver={(event) => {
                if (!draggedTabId || draggedTabId === tab.id || !tab.closable) {
                  return;
                }
                event.preventDefault();
                setDropTargetId(tab.id);
              }}
              onDragLeave={() => {
                if (dropTargetId === tab.id) {
                  setDropTargetId(null);
                }
              }}
              onDrop={(event) => {
                event.preventDefault();
                if (!draggedTabId || draggedTabId === tab.id || !tab.closable) {
                  setDraggedTabId(null);
                  setDropTargetId(null);
                  return;
                }

                onReorderTabs(draggedTabId, tab.id);
                setDraggedTabId(null);
                setDropTargetId(null);
              }}
            >
              <span className="shrink-0 opacity-90">{getTabIcon(tab)}</span>
              <span className="truncate">
                {tab.title}
                {tab.isDirty ? ' *' : ''}
              </span>
              {tab.closable && (
                <button
                  type="button"
                  className={`rounded-sm p-0.5 transition-colors ${
                    isActive
                      ? 'text-blue-500 hover:bg-blue-100 hover:text-blue-700 dark:hover:bg-blue-800/60 dark:hover:text-blue-100'
                      : 'text-gray-400 hover:bg-gray-200 hover:text-gray-700 dark:hover:bg-gray-700 dark:hover:text-gray-100'
                  }`}
                  onClick={(event) => {
                    event.stopPropagation();
                    setContextMenu(null);
                    onCloseTab(tab.id);
                  }}
                  title={`关闭${tab.title}`}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
          );
        })}
      </div>

      {contextMenu && contextMenuTab && (
        <div
          ref={menuRef}
          className="min-w-[220px] overflow-hidden rounded-lg border border-gray-200 bg-white py-1 shadow-xl dark:border-gray-700 dark:bg-gray-800"
          style={getMenuStyle(contextMenu.x, contextMenu.y)}
        >
          {canDetachTab(contextMenuTab) && (
            <ContextMenuItem
              icon={<ExternalLink className="h-4 w-4" />}
              label="在独立窗口中打开"
              onClick={() => {
                setContextMenu(null);
                void onDetachTab(contextMenuTab.id);
              }}
            />
          )}

          <ContextMenuItem
            icon={<X className="h-4 w-4" />}
            label="关闭标签页"
            danger
            onClick={() => {
              setContextMenu(null);
              onCloseTab(contextMenuTab.id);
            }}
          />
        </div>
      )}
    </div>
  );
}
