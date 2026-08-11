import { useEffect, useRef, useState } from 'react';
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
  const collapseTimer = useRef<number | null>(null);

  const cancelCollapse = () => {
    if (collapseTimer.current !== null) {
      window.clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
  };

  const scheduleCollapse = () => {
    if (mode !== 'auto-hide') return;
    cancelCollapse();
    collapseTimer.current = window.setTimeout(() => setExpanded(false), 900);
  };

  useEffect(() => {
    setExpanded(mode === 'fixed');
    return cancelCollapse;
  }, [mode]);

  useEffect(() => {
    const revealOnShortcut = (event: KeyboardEvent) => {
      if (event.altKey && event.key.toLowerCase() === 'q') setExpanded(true);
    };
    window.addEventListener('keydown', revealOnShortcut);
    return () => window.removeEventListener('keydown', revealOnShortcut);
  }, []);

  if (mode === 'auto-hide' && !expanded) {
    return (
      <div
        className="group relative z-50 h-1 shrink-0 bg-transparent"
        onMouseEnter={() => setExpanded(true)}
      >
        <button
          type="button"
          onFocus={() => setExpanded(true)}
          className="absolute left-1/2 top-0 h-1 w-16 -translate-x-1/2 rounded-b bg-gray-300/70 opacity-0 transition-opacity group-hover:opacity-100 focus:opacity-100 dark:bg-gray-600/80"
          title="展开宿主工具带"
          aria-label="展开宿主工具带"
        />
      </div>
    );
  }

  return (
    <div
      className="relative z-50 flex h-12 shrink-0 items-center justify-end border-b border-gray-200 bg-white px-2 dark:border-gray-700 dark:bg-gray-900"
      onMouseEnter={cancelCollapse}
      onMouseLeave={scheduleCollapse}
      onFocusCapture={cancelCollapse}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) scheduleCollapse();
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
  );
}
