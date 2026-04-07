import { useState } from 'react';
import { Folder, Home, X } from 'lucide-react';
import { ConfirmDialog } from '../Dialog';
import type { ShellTab } from '../../stores/shellTabStore';

interface ShellTabBarProps {
  tabs: ShellTab[];
  activeTabId: string;
  onActivateTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onReorderTabs: (fromId: string, toId: string) => void;
}

function getTabIcon(tab: ShellTab) {
  switch (tab.type) {
    case 'home':
      return <Home className="h-4 w-4 text-sky-500" />;
    case 'project':
      return <Folder className="h-4 w-4 text-blue-500" />;
    default:
      return null;
  }
}

export function ShellTabBar({
  tabs,
  activeTabId,
  onActivateTab,
  onCloseTab,
  onReorderTabs,
}: ShellTabBarProps) {
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dropTargetId, setDropTargetId] = useState<string | null>(null);
  const [pendingCloseTab, setPendingCloseTab] = useState<ShellTab | null>(null);

  const requestCloseTab = (tab: ShellTab) => {
    if (tab.type !== 'project') {
      onCloseTab(tab.id);
      return;
    }

    setPendingCloseTab(tab);
  };

  const handleConfirmClose = () => {
    if (!pendingCloseTab) {
      return;
    }

    onCloseTab(pendingCloseTab.id);
    setPendingCloseTab(null);
  };

  return (
    <>
      <div className="border-b border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
        <div className="flex items-stretch gap-1 overflow-x-auto px-2 py-1">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            const isDropTarget = tab.id === dropTargetId && tab.id !== 'home';

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
                title={tab.projectPath || tab.title}
                onClick={() => onActivateTab(tab.id)}
                onDragStart={() => {
                  if (!tab.closable) {
                    return;
                  }
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
                <span className="truncate">{tab.title}</span>
                {tab.closable && (
                  <button
                    className={`rounded-sm p-0.5 transition-colors ${
                      isActive
                        ? 'text-blue-500 hover:bg-blue-100 hover:text-blue-700 dark:hover:bg-blue-800/60 dark:hover:text-blue-100'
                        : 'text-gray-400 hover:bg-gray-200 hover:text-gray-700 dark:hover:bg-gray-700 dark:hover:text-gray-100'
                    }`}
                    onClick={(event) => {
                      event.stopPropagation();
                      requestCloseTab(tab);
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
      </div>
      <ConfirmDialog
        isOpen={!!pendingCloseTab}
        onClose={() => setPendingCloseTab(null)}
        onConfirm={handleConfirmClose}
        title="关闭项目标签"
        message={
          pendingCloseTab
            ? `确定关闭项目标签“${pendingCloseTab.title}”吗？`
            : ''
        }
        confirmText="关闭"
        cancelText="取消"
        type="warning"
      />
    </>
  );
}
