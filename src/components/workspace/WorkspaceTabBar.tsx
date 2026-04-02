import { useState } from 'react';
import { FileText, Folder, History, Image as ImageIcon, X } from 'lucide-react';
import type { WorkspaceTab } from '../../stores/workspaceTabStore';

interface WorkspaceTabBarProps {
  tabs: WorkspaceTab[];
  activeTabId: string;
  onActivateTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onReorderTabs: (fromId: string, toId: string) => void;
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
    default:
      return null;
  }
}

export function WorkspaceTabBar({
  tabs,
  activeTabId,
  onActivateTab,
  onCloseTab,
  onReorderTabs,
}: WorkspaceTabBarProps) {
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dropTargetId, setDropTargetId] = useState<string | null>(null);

  return (
    <div className="border-b border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
      <div className="flex items-stretch gap-1 overflow-x-auto px-2 py-1.5">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          const isDropTarget = tab.id === dropTargetId && tab.id !== 'files';

          return (
            <div
              key={tab.id}
              draggable={tab.closable}
              className={`group flex min-w-0 max-w-[260px] items-center gap-2 rounded-lg border px-3 py-2 text-sm transition-colors ${
                isActive
                  ? 'border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-700 dark:bg-blue-900/30 dark:text-blue-200'
                  : 'border-transparent bg-gray-50 text-gray-600 hover:bg-gray-100 dark:bg-gray-800/60 dark:text-gray-300 dark:hover:bg-gray-800'
              } ${
                isDropTarget ? 'ring-2 ring-blue-400' : ''
              } ${tab.closable ? 'cursor-grab active:cursor-grabbing' : 'cursor-default'}`}
              title={tab.filePath || tab.title}
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
              <span className="shrink-0">{getTabIcon(tab)}</span>
              <span className="truncate">
                {tab.title}
                {tab.isDirty ? ' *' : ''}
              </span>
              {tab.closable && (
                <button
                  className={`rounded p-0.5 transition-colors ${
                    isActive
                      ? 'text-blue-500 hover:bg-blue-100 hover:text-blue-700 dark:hover:bg-blue-800/60 dark:hover:text-blue-100'
                      : 'text-gray-400 hover:bg-gray-200 hover:text-gray-700 dark:hover:bg-gray-700 dark:hover:text-gray-100'
                  }`}
                  onClick={(event) => {
                    event.stopPropagation();
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
    </div>
  );
}
