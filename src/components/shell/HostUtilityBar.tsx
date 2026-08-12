import { useEffect, useState } from 'react';
import { ShieldCheck } from 'lucide-react';
import type { HostToolbarMode } from '../../types/platform';
import { LauncherButton } from '../Launcher';
import type { OpenBuiltinTool } from '../../features/builtinTools';
import type { ScriptSurfaceTool } from '../BuiltinToolsCenter';
import { PinnedToolsToolbar } from '../file-manager/PinnedToolsToolbar';
import { DevelopmentReloadControl } from './DevelopmentReloadControl';

interface HostUtilityBarProps {
  mode?: HostToolbarMode;
  hasActiveProject: boolean;
  activeProjectName?: string;
  onOpenTool: OpenBuiltinTool;
  onOpenScriptSurface: (surface: ScriptSurfaceTool) => void;
  onOpenRecovery: () => void;
  onOpenDeveloperWorkbench: () => void;
}

export function HostUtilityBar({
  mode = 'fixed',
  hasActiveProject,
  activeProjectName,
  onOpenTool,
  onOpenScriptSurface,
  onOpenRecovery,
  onOpenDeveloperWorkbench,
}: HostUtilityBarProps) {
  const [expanded, setExpanded] = useState(mode === 'fixed');

  const collapseOnPointerLeave = () => {
    if (mode === 'auto-hide') setExpanded(false);
  };

  useEffect(() => {
    setExpanded(mode === 'fixed');
  }, [mode]);

  useEffect(() => {
    const revealOnShortcut = (event: KeyboardEvent) => {
      if (event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'q') {
        setExpanded(true);
      }
    };
    window.addEventListener('keydown', revealOnShortcut);
    return () => window.removeEventListener('keydown', revealOnShortcut);
  }, []);

  return (
    <div
      className={mode === 'auto-hide'
        ? 'pointer-events-none relative z-[60] h-6 shrink-0 border-b border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900'
        : 'relative z-50 flex h-12 shrink-0 items-center justify-end border-b border-gray-200 bg-white px-2 dark:border-gray-700 dark:bg-gray-900'}
      onMouseEnter={() => {
        if (mode === 'auto-hide') {
          setExpanded(true);
        }
      }}
    >
      {mode === 'auto-hide' && !expanded ? (
        <button
          type="button"
          onFocus={() => setExpanded(true)}
          className="pointer-events-auto absolute inset-0 flex w-full items-start justify-center pt-1 text-transparent"
          title="展开宿主工具带"
          aria-label="展开宿主工具带"
        >
          <span className="block h-1 w-16 rounded-b bg-gray-300/70 opacity-70 transition-opacity duration-200 hover:opacity-100 dark:bg-gray-600/80" />
        </button>
      ) : null}
      <div
        className={mode === 'auto-hide'
          ? `pointer-events-auto absolute inset-x-0 top-0 flex h-12 items-center justify-end border-b border-gray-200 bg-white px-2 shadow-md transition-transform duration-200 ease-out dark:border-gray-700 dark:bg-gray-900 ${expanded ? 'translate-y-0' : '-translate-y-11'}`
          : 'flex h-12 shrink-0 items-center justify-end'}
      onMouseEnter={() => setExpanded(true)}
      onMouseLeave={collapseOnPointerLeave}
      onFocusCapture={() => setExpanded(true)}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) collapseOnPointerLeave();
      }}
      >
        <div className="flex h-12 shrink-0 items-center gap-1.5">
          <PinnedToolsToolbar onOpenTool={onOpenTool} onOpenScriptSurface={onOpenScriptSurface} />
          <DevelopmentReloadControl onOpenDeveloperWorkbench={onOpenDeveloperWorkbench} />
          <button
            type="button"
            onClick={onOpenRecovery}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title="维护中心"
          >
            <ShieldCheck className="h-4 w-4" />
          </button>
          <LauncherButton
            hasActiveProject={hasActiveProject}
            activeProjectName={activeProjectName}
            onOpenTool={onOpenTool}
            onOpenScriptSurface={onOpenScriptSurface}
          />
        </div>
      </div>
    </div>
  );
}
