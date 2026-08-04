import { useEffect, useMemo, useRef, useState } from 'react';
import { MoreHorizontal } from 'lucide-react';
import { BUILTIN_TOOL_BY_ID, type BuiltinToolDefinition, type OpenBuiltinTool } from '../../features/builtinTools';
import { useBuiltinToolsStore } from '../../stores/builtinToolsStore';
import { useLanCollaborationStore } from '../../stores/lanCollaborationStore';
import { getActiveRenderCount, useRenderStore } from '../../stores/renderStore';
import { useTaskStore } from '../../stores/taskStore';
import { isContributionAvailable } from '../../features/contributionRegistry';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';

interface PinnedToolsToolbarProps {
  compact?: boolean;
  onOpenTool: OpenBuiltinTool;
}

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

export function PinnedToolsToolbar({ compact = false, onOpenTool }: PinnedToolsToolbarProps) {
  const pinnedToolIds = useBuiltinToolsStore((state) => state.pinnedToolIds);
  const loadPreferences = useBuiltinToolsStore((state) => state.loadPreferences);
  const unreadCount = useLanCollaborationStore((state) => state.unreadCount);
  const runningTasks = useTaskStore((state) => state.stats.running);
  const renderCount = useRenderStore((state) => getActiveRenderCount(state.jobsByProject));
  const contributionSnapshot = useContributionRegistryStore((state) => state.snapshot);
  const [visibleCapacity, setVisibleCapacity] = useState(() => getVisibleCapacity(compact));
  const [isOverflowOpen, setIsOverflowOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    void loadPreferences();
  }, [loadPreferences]);

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

  const tools = useMemo(() => pinnedToolIds.flatMap((toolId) => {
    const tool = BUILTIN_TOOL_BY_ID.get(toolId);
    return tool && isContributionAvailable(contributionSnapshot, tool.contribution) ? [tool] : [];
  }), [contributionSnapshot, pinnedToolIds]);
  const visibleTools = tools.slice(0, visibleCapacity);
  const overflowTools = tools.slice(visibleCapacity);

  const getBadge = (tool: BuiltinToolDefinition) => {
    if (tool.id === 'p2p-chat') return unreadCount;
    if (tool.id === 'task-center') return runningTasks + renderCount;
    return 0;
  };

  const openTool = (tool: BuiltinToolDefinition) => {
    setIsOverflowOpen(false);
    onOpenTool(tool.id);
  };

  if (tools.length === 0) {
    return null;
  }

  return (
    <div className="flex min-w-0 items-center gap-0.5 border-l border-gray-200 pl-1 dark:border-gray-700">
      {visibleTools.map((tool) => {
        const Icon = tool.icon;
        const badge = getBadge(tool);
        return (
          <button
            key={tool.id}
            type="button"
            onClick={() => openTool(tool)}
            className="relative flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            title={tool.title}
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
                const Icon = tool.icon;
                const badge = getBadge(tool);
                return (
                  <button
                    key={tool.id}
                    type="button"
                    onClick={() => openTool(tool)}
                    className="flex w-full items-center gap-3 px-3 py-2 text-left text-sm text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-800"
                  >
                    <Icon className="h-4 w-4 shrink-0 text-gray-500" />
                    <span className="min-w-0 flex-1 truncate">{tool.title}</span>
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
