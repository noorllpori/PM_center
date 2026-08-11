import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ChevronDown,
  Code2,
  FolderOpen,
  RefreshCw,
  ScanSearch,
} from 'lucide-react';
import { getComponentRuntimeOverview } from '../../api/componentRuntime';
import {
  getDevelopmentComponentSnapshot,
  reloadDevelopmentComponents,
} from '../../api/scriptAutomation';
import type { DevelopmentComponentSnapshot } from '../../types/automation';

export function DevelopmentReloadControl({
  onOpenDeveloperWorkbench,
}: {
  onOpenDeveloperWorkbench: () => void;
}) {
  const [developmentComponents, setDevelopmentComponents] = useState<DevelopmentComponentSnapshot[]>([]);
  const [menuOpen, setMenuOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);

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
    if (!menuOpen) return;
    const handlePointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    window.addEventListener('pointerdown', handlePointerDown);
    return () => window.removeEventListener('pointerdown', handlePointerDown);
  }, [menuOpen]);

  const reloadDevelopment = async (onlyDirty: boolean) => {
    setBusy(true);
    setMessage(null);
    try {
      const result = await reloadDevelopmentComponents(onlyDirty);
      const parts = [
        result.reloaded.length ? `已重载 ${result.reloaded.length} 个` : '',
        result.errors.length ? `${result.errors.length} 个失败` : '',
      ].filter(Boolean);
      setMessage(parts.join('，') || '没有需要重载的开发组件');
      if (result.errors.length) console.error('Development component reload errors', result.errors);
      await refreshDevelopmentComponents();
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  };

  const openComponentLogs = async () => {
    try {
      const overview = await getComponentRuntimeOverview();
      await invoke('open_path', { path: `${overview.rootPath}\\logs` });
    } catch (error) {
      setMessage(`无法打开日志目录：${String(error)}`);
    }
  };

  if (developmentComponents.length === 0) return null;

  const dirtyCount = developmentComponents.filter((component) => component.dirty).length;
  const invalidCount = developmentComponents.filter((component) => !component.valid).length;

  return (
    <div ref={menuRef} className="relative flex shrink-0 items-center">
      <button
        type="button"
        onClick={() => void reloadDevelopment(true)}
        disabled={busy}
        className="relative inline-flex h-7 items-center gap-1 rounded-l-md px-1.5 text-[10px] font-semibold text-sky-700 transition-colors hover:bg-sky-50 disabled:opacity-50 dark:text-sky-300 dark:hover:bg-sky-950/30"
        title="扫描并重载已变化的开发组件"
      >
        <RefreshCw className={`h-3.5 w-3.5 ${busy ? 'animate-spin' : ''}`} />
        DEV
        {dirtyCount || invalidCount ? (
          <span className={`absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full ${invalidCount ? 'bg-red-500' : 'bg-amber-500'}`} />
        ) : null}
      </button>
      <button
        type="button"
        onClick={() => setMenuOpen((open) => !open)}
        className="flex h-7 w-5 items-center justify-center rounded-r-md text-sky-700 transition-colors hover:bg-sky-50 dark:text-sky-300 dark:hover:bg-sky-950/30"
        title="开发组件重载菜单"
      >
        <ChevronDown className="h-3 w-3" />
      </button>
      {menuOpen ? (
        <div className="absolute right-0 top-8 z-50 w-64 rounded-md border border-gray-200 bg-white p-1.5 shadow-lg dark:border-gray-700 dark:bg-gray-900">
          <div className="px-2 py-1.5 text-[11px] text-gray-500 dark:text-gray-400">
            {developmentComponents.length} 个受信任开发目录
            {dirtyCount ? `，${dirtyCount} 个有变化` : ''}
          </div>
          <button type="button" onClick={() => void refreshDevelopmentComponents()} className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-gray-100 dark:hover:bg-gray-800">
            <ScanSearch className="h-3.5 w-3.5" />重新扫描
          </button>
          <button type="button" onClick={() => void reloadDevelopment(false)} disabled={busy} className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800">
            <RefreshCw className="h-3.5 w-3.5" />重载全部开发组件
          </button>
          <button type="button" onClick={() => void openComponentLogs()} className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-gray-100 dark:hover:bg-gray-800">
            <FolderOpen className="h-3.5 w-3.5" />打开组件日志
          </button>
          <button type="button" onClick={() => { setMenuOpen(false); onOpenDeveloperWorkbench(); }} className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-gray-100 dark:hover:bg-gray-800">
            <Code2 className="h-3.5 w-3.5" />进入开发者工作台
          </button>
          {message ? (
            <p className="mt-1 border-t border-gray-100 px-2 pt-2 text-[11px] leading-4 text-gray-500 dark:border-gray-800 dark:text-gray-400">
              {message}
            </p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
