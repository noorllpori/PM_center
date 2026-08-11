import { useEffect, useMemo, useRef, useState } from 'react';
import { FileCode2, MoreHorizontal } from 'lucide-react';
import { BUILTIN_TOOL_BY_ID, isBuiltinToolId, type BuiltinToolDefinition, type OpenBuiltinTool } from '../../features/builtinTools';
import { useBuiltinToolsStore } from '../../stores/builtinToolsStore';
import { useLanCollaborationStore } from '../../stores/lanCollaborationStore';
import { getActiveRenderCount, useRenderStore } from '../../stores/renderStore';
import { useTaskStore } from '../../stores/taskStore';
import { isContributionAvailable } from '../../features/contributionRegistry';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';
import { useAutomationStore } from '../../stores/automationStore';
import type { ScriptSurfaceTool } from '../BuiltinToolsCenter';

interface PinnedToolsToolbarProps {
  compact?: boolean;
  onOpenTool: OpenBuiltinTool;
  onOpenScriptSurface: (surface: ScriptSurfaceTool) => void;
}

type PinnedToolbarItem =
  | { id: BuiltinToolDefinition['id']; kind: 'builtin'; tool: BuiltinToolDefinition }
  | { id: string; kind: 'surface'; surface: ScriptSurfaceTool };

function getVisibleCapacity(compact: boolean) {
  const width = window.innerWidth;
  if (width < 900) return compact ? 1 : 2;
  if (width < 1200) return compact ? 2 : 3;
  if (width < 1500) return compact ? 3 : 4;
  return compact ? 4 : 5;
}

function badgeLabel(value: number) {
  return value > 99 ? '99+' : String(value);
}

export function PinnedToolsToolbar({
  compact = false,
  onOpenTool,
  onOpenScriptSurface,
}: PinnedToolsToolbarProps) {
  const pinnedToolIds = useBuiltinToolsStore((state) => state.pinnedToolIds);
  const loadPreferences = useBuiltinToolsStore((state) => state.loadPreferences);
  const unreadCount = useLanCollaborationStore((state) => state.unreadCount);
  const runningTasks = useTaskStore((state) => state.stats.running);
  const renderCount = useRenderStore((state) => getActiveRenderCount(state.jobsByProject));
  const contributionSnapshot = useContributionRegistryStore((state) => state.snapshot);
  const automationSnapshot = useAutomationStore((state) => state.snapshot);
  const initializeAutomation = useAutomationStore((state) => state.initialize);
  const [visibleCapacity, setVisibleCapacity] = useState(() => getVisibleCapacity(compact));
  const [isOverflowOpen, setIsOverflowOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    void Promise.all([loadPreferences(), initializeAutomation()]);
  }, [initializeAutomation, loadPreferences]);

  useEffect(() => {
    const updateCapacity = () => setVisibleCapacity(getVisibleCapacity(compact));
    updateCapacity();
    window.addEventListener('resize', updateCapacity);
    return () => window.removeEventListener('resize', updateCapacity);
  }, [compact]);

  useEffect(() => {
    if (!isOverflowOpen) {
      return;
    }

    const closeOnOutside = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setIsOverflowOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setIsOverflowOpen(false);
    };
    document.addEventListener('mousedown', closeOnOutside);
    document.addEventListener('keydown', closeOnEscape);
    return () => {
      document.removeEventListener('mousedown', closeOnOutside);
      document.removeEventListener('keydown', closeOnEscape);
    };
  }, [isOverflowOpen]);

  const scriptSurfaceById = useMemo(() => new Map(
    (automationSnapshot?.running ? automationSnapshot.availableComponents : [])
      .flatMap((component) => component.surfaces
        .filter((surface) => surface.placements.includes('shell'))
        .map((surface): ScriptSurfaceTool => ({
          componentId: component.componentId,
          surfaceId: surface.id,
          title: surface.name,
          description: `${component.componentName} · 隔离组件页面`,
          pinnable: true,
          requiresProject: (surface.allowedCommands ?? []).some((commandName) => component.commands.some((command) => (
            (command.command === commandName || command.id === commandName)
            && command.contextRequirement === 'project-required'
          ))),
        })))
      .map((surface) => [surface.surfaceId, surface] as const),
  ), [automationSnapshot]);
  const tools = useMemo(() => pinnedToolIds.reduce<PinnedToolbarItem[]>((items, toolId) => {
    if (isBuiltinToolId(toolId)) {
      const tool = BUILTIN_TOOL_BY_ID.get(toolId);
      if (tool && isContributionAvailable(contributionSnapshot, tool.contribution)) {
        items.push({ id: tool.id, kind: 'builtin', tool });
      }
      return items;
    }
    const surface = scriptSurfaceById.get(toolId);
    if (surface) items.push({ id: toolId, kind: 'surface', surface });
    return items;
  }, []), [contributionSnapshot, pinnedToolIds, scriptSurfaceById]);
  const visibleTools = tools.slice(0, visibleCapacity);
  const overflowTools = tools.slice(visibleCapacity);

  const getBadge = (tool: BuiltinToolDefinition) => {
    if (tool.id === 'p2p-chat') return unreadCount;
    if (tool.id === 'task-center') return runningTasks + renderCount;
    return 0;
  };

  const openTool = (tool: (typeof tools)[number]) => {
    setIsOverflowOpen(false);
    if (tool.kind === 'builtin') onOpenTool(tool.tool.id);
    else onOpenScriptSurface(tool.surface);
  };

  if (tools.length === 0) {
    return null;
  }

  return (
    <div className="flex min-w-0 shrink-0 items-center gap-0.5 border-r border-gray-200 pr-1 dark:border-gray-700">
      {visibleTools.map((tool) => {
        const Icon = tool.kind === 'builtin' ? tool.tool.icon : FileCode2;
        const title = tool.kind === 'builtin' ? tool.tool.title : tool.surface.title;
        const badge = tool.kind === 'builtin' ? getBadge(tool.tool) : 0;
        return (
          <button
            key={tool.id}
            type="button"
            onClick={() => openTool(tool)}
            className="relative flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title={title}
          >
            <Icon className="h-4 w-4" />
            {badge > 0 ? (
              <span className="absolute -right-1 -top-1 min-w-4 rounded-full bg-red-500 px-1 text-center text-[9px] font-semibold leading-4 text-white">
                {badgeLabel(badge)}
              </span>
            ) : null}
          </button>
        );
      })}

      {overflowTools.length > 0 ? (
        <div ref={menuRef} className="relative">
          <button
            type="button"
            onClick={() => setIsOverflowOpen((value) => !value)}
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title={`更多已固定功能（${overflowTools.length}）`}
          >
            <MoreHorizontal className="h-4 w-4" />
          </button>

          {isOverflowOpen ? (
            <div className="absolute right-0 top-full z-40 mt-2 w-64 overflow-hidden rounded-md border border-gray-200 bg-white py-1 shadow-xl dark:border-gray-700 dark:bg-gray-900">
              {overflowTools.map((tool) => {
                const Icon = tool.kind === 'builtin' ? tool.tool.icon : FileCode2;
                const title = tool.kind === 'builtin' ? tool.tool.title : tool.surface.title;
                const badge = tool.kind === 'builtin' ? getBadge(tool.tool) : 0;
                return (
                  <button
                    key={tool.id}
                    type="button"
                    onClick={() => openTool(tool)}
                    className="flex w-full items-center gap-3 px-3 py-2 text-left text-sm text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-800"
                  >
                    <Icon className="h-4 w-4 shrink-0 text-gray-500" />
                    <span className="min-w-0 flex-1 truncate">{title}</span>
                    {badge > 0 ? (
                      <span className="rounded-full bg-red-500 px-1.5 py-0.5 text-[10px] font-semibold text-white">
                        {badgeLabel(badge)}
                      </span>
                    ) : null}
                  </button>
                );
              })}
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
