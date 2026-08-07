import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ChevronDown,
  Clapperboard,
  Code2,
  Folder,
  FolderOpen,
  Home,
  LibraryBig,
  MessagesSquare,
  RefreshCw,
  ScanSearch,
  ShieldAlert,
  X,
} from 'lucide-react';
import { ConfirmDialog } from '../Dialog';
import type { ShellTab } from '../../stores/shellTabStore';
import { useSettingsStore } from '../../stores/settingsStore';
import {
  getDevelopmentComponentSnapshot,
  reloadDevelopmentComponents,
} from '../../api/scriptAutomation';
import { getComponentRuntimeOverview } from '../../api/componentRuntime';
import type { DevelopmentComponentSnapshot } from '../../types/automation';

interface ShellTabBarProps {
  tabs: ShellTab[];
  activeTabId: string;
  onActivateTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onReorderTabs: (fromId: string, toId: string) => void;
  onOpenRecovery: () => void;
  onOpenDeveloperWorkbench: () => void;
}

function getTabIcon(tab: ShellTab) {
  switch (tab.type) {
    case 'home':
      return <Home className="h-4 w-4 text-sky-500" />;
    case 'project':
      return <Folder className="h-4 w-4 text-blue-500" />;
    case 'lan':
      return <MessagesSquare className="h-4 w-4 text-emerald-600" />;
    case 'external-render-station':
      return <Clapperboard className="h-4 w-4 text-orange-500" />;
    case 'media-library':
      return <LibraryBig className="h-4 w-4 text-teal-600" />;
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
  onOpenRecovery,
  onOpenDeveloperWorkbench,
}: ShellTabBarProps) {
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dropTargetId, setDropTargetId] = useState<string | null>(null);
  const [pendingCloseTab, setPendingCloseTab] = useState<ShellTab | null>(null);
  const [developmentComponents, setDevelopmentComponents] = useState<DevelopmentComponentSnapshot[]>([]);
  const [developmentMenuOpen, setDevelopmentMenuOpen] = useState(false);
  const [developmentBusy, setDevelopmentBusy] = useState(false);
  const [developmentMessage, setDevelopmentMessage] = useState<string | null>(null);
  const developmentMenuRef = useRef<HTMLDivElement | null>(null);
  const confirmProjectTabClose = useSettingsStore((state) => state.confirmProjectTabClose);

  const refreshDevelopmentComponents = useCallback(async () => {
    try {
      setDevelopmentComponents(await getDevelopmentComponentSnapshot());
    } catch (error) {
      console.error('Failed to scan development components', error);
    }
  }, []);

  useEffect(() => {
    void refreshDevelopmentComponents();
    const handleFocus = () => void refreshDevelopmentComponents();
    const intervalId = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refreshDevelopmentComponents();
    }, 15000);
    window.addEventListener('focus', handleFocus);
    return () => {
      window.clearInterval(intervalId);
      window.removeEventListener('focus', handleFocus);
    };
  }, [refreshDevelopmentComponents]);

  useEffect(() => {
    if (!developmentMenuOpen) return;
    const handlePointerDown = (event: PointerEvent) => {
      if (!developmentMenuRef.current?.contains(event.target as Node)) {
        setDevelopmentMenuOpen(false);
      }
    };
    window.addEventListener('pointerdown', handlePointerDown);
    return () => window.removeEventListener('pointerdown', handlePointerDown);
  }, [developmentMenuOpen]);

  const reloadDevelopment = async (onlyDirty: boolean) => {
    setDevelopmentBusy(true);
    setDevelopmentMessage(null);
    try {
      const result = await reloadDevelopmentComponents(onlyDirty);
      const parts = [
        result.reloaded.length ? `已重载 ${result.reloaded.length} 个` : '',
        result.errors.length ? `${result.errors.length} 个失败` : '',
      ].filter(Boolean);
      setDevelopmentMessage(parts.join('，') || '没有需要重载的开发组件');
      if (result.errors.length) {
        console.error('Development component reload errors', result.errors);
      }
      await refreshDevelopmentComponents();
    } catch (error) {
      setDevelopmentMessage(String(error));
    } finally {
      setDevelopmentBusy(false);
    }
  };

  const openComponentLogs = async () => {
    try {
      const overview = await getComponentRuntimeOverview();
      await invoke('open_path', { path: `${overview.rootPath}\\logs` });
    } catch (error) {
      setDevelopmentMessage(`无法打开日志目录：${String(error)}`);
    }
  };

  const dirtyDevelopmentCount = developmentComponents.filter((component) => component.dirty).length;
  const invalidDevelopmentCount = developmentComponents.filter((component) => !component.valid).length;

  const requestCloseTab = (tab: ShellTab) => {
    if (tab.type !== 'project' || !confirmProjectTabClose) {
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
        <div className="flex min-w-0 items-stretch">
        <div className="flex min-w-0 flex-1 items-stretch gap-1 overflow-x-auto px-2 py-1">
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
                onMouseDown={(event) => {
                  if (event.button !== 1 || !tab.closable) {
                    return;
                  }

                  event.preventDefault();
                  event.stopPropagation();
                  requestCloseTab(tab);
                }}
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
        <div className="flex shrink-0 items-center gap-1 border-l border-gray-200 px-1.5 dark:border-gray-700">
          {developmentComponents.length ? (
            <div ref={developmentMenuRef} className="relative flex items-center">
              <button
                type="button"
                onClick={() => void reloadDevelopment(true)}
                disabled={developmentBusy}
                className="relative inline-flex h-7 items-center gap-1 rounded-l-md px-1.5 text-[10px] font-semibold text-sky-700 transition-colors hover:bg-sky-50 disabled:opacity-50 dark:text-sky-300 dark:hover:bg-sky-950/30"
                title="扫描并重载已变化的开发组件"
              >
                <RefreshCw className={`h-3.5 w-3.5 ${developmentBusy ? 'animate-spin' : ''}`} />
                DEV
                {dirtyDevelopmentCount || invalidDevelopmentCount ? (
                  <span className={`absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full ${invalidDevelopmentCount ? 'bg-red-500' : 'bg-amber-500'}`} />
                ) : null}
              </button>
              <button
                type="button"
                onClick={() => setDevelopmentMenuOpen((open) => !open)}
                className="flex h-7 w-5 items-center justify-center rounded-r-md text-sky-700 transition-colors hover:bg-sky-50 dark:text-sky-300 dark:hover:bg-sky-950/30"
                title="开发组件重载菜单"
              >
                <ChevronDown className="h-3 w-3" />
              </button>
              {developmentMenuOpen ? (
                <div className="absolute right-0 top-8 z-50 w-64 rounded-md border border-gray-200 bg-white p-1.5 shadow-lg dark:border-gray-700 dark:bg-gray-900">
                  <div className="px-2 py-1.5 text-[11px] text-gray-500 dark:text-gray-400">
                    {developmentComponents.length} 个受信任开发目录
                    {dirtyDevelopmentCount ? `，${dirtyDevelopmentCount} 个有变化` : ''}
                  </div>
                  <button type="button" onClick={() => void refreshDevelopmentComponents()} className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-gray-100 dark:hover:bg-gray-800">
                    <ScanSearch className="h-3.5 w-3.5" />重新扫描
                  </button>
                  <button type="button" onClick={() => void reloadDevelopment(false)} disabled={developmentBusy} className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800">
                    <RefreshCw className="h-3.5 w-3.5" />重载全部开发组件
                  </button>
                  <button type="button" onClick={() => void openComponentLogs()} className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-gray-100 dark:hover:bg-gray-800">
                    <FolderOpen className="h-3.5 w-3.5" />打开组件日志
                  </button>
                  <button type="button" onClick={() => { setDevelopmentMenuOpen(false); onOpenDeveloperWorkbench(); }} className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-gray-100 dark:hover:bg-gray-800">
                    <Code2 className="h-3.5 w-3.5" />进入开发者工作台
                  </button>
                  {developmentMessage ? (
                    <p className="mt-1 border-t border-gray-100 px-2 pt-2 text-[11px] leading-4 text-gray-500 dark:border-gray-800 dark:text-gray-400">
                      {developmentMessage}
                    </p>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : null}
          <button
            type="button"
            onClick={onOpenRecovery}
            className="flex h-7 w-7 items-center justify-center rounded-md text-gray-500 transition-colors hover:bg-amber-50 hover:text-amber-700 dark:text-gray-400 dark:hover:bg-amber-950/30 dark:hover:text-amber-300"
            title="恢复设置"
          >
            <ShieldAlert className="h-4 w-4" />
          </button>
        </div>
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
