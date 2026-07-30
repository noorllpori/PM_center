import { useEffect, useMemo, useRef, useState, type DragEvent } from 'react';
import { GripVertical, Pin, PinOff, Search, Wrench } from 'lucide-react';
import { Dialog } from './Dialog';
import {
  BUILTIN_TOOL_BY_ID,
  BUILTIN_TOOL_CATEGORY_LABELS,
  BUILTIN_TOOL_CATEGORY_ORDER,
  BUILTIN_TOOLS,
  type BuiltinToolId,
  type OpenBuiltinTool,
} from '../features/builtinTools';
import { useBuiltinToolsStore } from '../stores/builtinToolsStore';

interface BuiltinToolsCenterProps {
  isOpen: boolean;
  onClose: () => void;
  hasActiveProject: boolean;
  activeProjectName?: string | null;
  onOpenTool: OpenBuiltinTool;
}

export function BuiltinToolsCenter({
  isOpen,
  onClose,
  hasActiveProject,
  activeProjectName,
  onOpenTool,
}: BuiltinToolsCenterProps) {
  const pinnedToolIds = useBuiltinToolsStore((state) => state.pinnedToolIds);
  const loadPreferences = useBuiltinToolsStore((state) => state.loadPreferences);
  const togglePinned = useBuiltinToolsStore((state) => state.togglePinned);
  const reorderPinned = useBuiltinToolsStore((state) => state.reorderPinned);
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [draggedToolId, setDraggedToolId] = useState<BuiltinToolId | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    void loadPreferences();
  }, [loadPreferences]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    setQuery('');
    setSelectedIndex(0);
    window.requestAnimationFrame(() => searchInputRef.current?.focus());
  }, [isOpen]);

  const filteredTools = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    if (!normalizedQuery) {
      return BUILTIN_TOOLS;
    }

    return BUILTIN_TOOLS.filter((tool) => {
      const searchable = [tool.title, tool.description, ...tool.keywords]
        .join(' ')
        .toLocaleLowerCase();
      return searchable.includes(normalizedQuery);
    });
  }, [query]);

  const groupedTools = useMemo(() => BUILTIN_TOOL_CATEGORY_ORDER.flatMap((category) => {
    const tools = filteredTools.filter((tool) => tool.category === category);
    return tools.length > 0 ? [{ category, tools }] : [];
  }), [filteredTools]);
  const orderedFilteredTools = useMemo(
    () => groupedTools.flatMap((group) => group.tools),
    [groupedTools],
  );

  const pinnedTools = pinnedToolIds.flatMap((toolId) => {
    const tool = BUILTIN_TOOL_BY_ID.get(toolId);
    return tool ? [tool] : [];
  });

  const openTool = (toolId: BuiltinToolId) => {
    const tool = BUILTIN_TOOL_BY_ID.get(toolId);
    if (!tool || (tool.requiresProject && !hasActiveProject)) {
      return;
    }

    onOpenTool(toolId);
    onClose();
  };

  const handleSearchKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (orderedFilteredTools.length === 0) {
      return;
    }

    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setSelectedIndex((index) => (index + 1) % orderedFilteredTools.length);
      return;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      setSelectedIndex((index) => (index - 1 + orderedFilteredTools.length) % orderedFilteredTools.length);
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      const selectedTool = orderedFilteredTools[selectedIndex];
      if (selectedTool) openTool(selectedTool.id);
    }
  };

  const handlePinnedDrop = (
    event: DragEvent<HTMLElement>,
    beforeToolId: BuiltinToolId | null,
  ) => {
    event.preventDefault();
    if (!draggedToolId) {
      return;
    }
    void reorderPinned(draggedToolId, beforeToolId);
    setDraggedToolId(null);
  };

  return (
    <Dialog isOpen={isOpen} onClose={onClose} title="功能中心 · Alt+Q" size="2xl">
      <div className="space-y-5">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-400" />
            <input
              ref={searchInputRef}
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setSelectedIndex(0);
              }}
              onKeyDown={handleSearchKeyDown}
              placeholder="搜索功能、用途或工具名称"
              className="h-10 w-full rounded-md border border-gray-300 bg-white pl-9 pr-3 text-sm text-gray-900 outline-none transition-colors focus:border-blue-500 focus:ring-2 focus:ring-blue-500/15 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100"
            />
          </div>
          <div className="shrink-0 text-xs text-gray-500 dark:text-gray-400">
            {hasActiveProject ? `当前项目：${activeProjectName || '已打开项目'}` : '当前未打开项目'}
          </div>
        </div>

        <section>
          <div className="mb-2 flex items-center justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">已固定到快捷栏</h3>
              <p className="mt-0.5 text-xs text-gray-500 dark:text-gray-400">拖动调整顺序，较后的功能会在窄窗口中进入“更多”。</p>
            </div>
            <span className="text-xs text-gray-400">{pinnedTools.length} 项</span>
          </div>

          <div
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => handlePinnedDrop(event, null)}
            className="flex min-h-12 flex-wrap gap-2 rounded-md border border-dashed border-gray-300 bg-gray-50 p-2 dark:border-gray-700 dark:bg-gray-950/50"
          >
            {pinnedTools.length === 0 ? (
              <div className="flex min-h-8 w-full items-center justify-center text-xs text-gray-500">
                尚未固定功能，可从下方列表添加。
              </div>
            ) : pinnedTools.map((tool) => {
              const Icon = tool.icon;
              const unavailable = tool.requiresProject && !hasActiveProject;
              return (
                <div
                  key={tool.id}
                  draggable
                  onDragStart={() => setDraggedToolId(tool.id)}
                  onDragEnd={() => setDraggedToolId(null)}
                  onDragOver={(event) => event.preventDefault()}
                  onDrop={(event) => {
                    event.stopPropagation();
                    handlePinnedDrop(event, tool.id);
                  }}
                  className={`flex h-9 items-center gap-1 rounded border bg-white pl-1 pr-1.5 shadow-sm dark:bg-gray-900 ${draggedToolId === tool.id ? 'border-blue-400 opacity-50' : 'border-gray-200 dark:border-gray-700'}`}
                >
                  <GripVertical className="h-4 w-4 cursor-grab text-gray-300 active:cursor-grabbing dark:text-gray-600" />
                  <button
                    type="button"
                    onClick={() => openTool(tool.id)}
                    disabled={unavailable}
                    className="flex min-w-0 items-center gap-1.5 px-1 text-xs font-medium text-gray-700 disabled:cursor-not-allowed disabled:text-gray-400 dark:text-gray-200 dark:disabled:text-gray-600"
                    title={unavailable ? '需要先打开项目' : `打开${tool.title}`}
                  >
                    <Icon className="h-4 w-4 shrink-0" />
                    <span className="max-w-28 truncate">{tool.title}</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => void togglePinned(tool.id)}
                    className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-800 dark:hover:text-gray-200"
                    title="取消固定"
                  >
                    <PinOff className="h-3.5 w-3.5" />
                  </button>
                </div>
              );
            })}
          </div>
        </section>

        <div className="max-h-[48vh] space-y-5 overflow-auto pr-1">
          {groupedTools.map(({ category, tools }) => (
            <section key={category}>
              <h3 className="mb-2 text-xs font-semibold uppercase text-gray-500 dark:text-gray-400">
                {BUILTIN_TOOL_CATEGORY_LABELS[category]}
              </h3>
              <div className="grid grid-cols-1 gap-2 lg:grid-cols-2">
                {tools.map((tool) => {
                  const Icon = tool.icon;
                  const unavailable = tool.requiresProject && !hasActiveProject;
                  const pinned = pinnedToolIds.includes(tool.id);
                  const active = orderedFilteredTools[selectedIndex]?.id === tool.id;
                  return (
                    <div
                      key={tool.id}
                      onMouseEnter={() => setSelectedIndex(orderedFilteredTools.findIndex((item) => item.id === tool.id))}
                      className={`flex min-w-0 items-center gap-2 rounded-md border p-2 transition-colors ${active ? 'border-blue-300 bg-blue-50/70 dark:border-blue-800 dark:bg-blue-950/20' : 'border-gray-200 bg-white hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-900 dark:hover:bg-gray-800/70'}`}
                    >
                      <button
                        type="button"
                        onClick={() => openTool(tool.id)}
                        disabled={unavailable}
                        className="flex min-w-0 flex-1 items-center gap-3 text-left disabled:cursor-not-allowed"
                      >
                        <span className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-md ${unavailable ? 'bg-gray-100 text-gray-400 dark:bg-gray-800 dark:text-gray-600' : 'bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-200'}`}>
                          <Icon className="h-5 w-5" />
                        </span>
                        <span className="min-w-0">
                          <span className={`block truncate text-sm font-medium ${unavailable ? 'text-gray-400 dark:text-gray-600' : 'text-gray-900 dark:text-gray-100'}`}>
                            {tool.title}
                          </span>
                          <span className="mt-0.5 block truncate text-xs text-gray-500 dark:text-gray-400">
                            {unavailable ? '需要先打开项目' : tool.description}
                          </span>
                        </span>
                      </button>
                      {tool.pinnable ? (
                        <button
                          type="button"
                          onClick={() => void togglePinned(tool.id)}
                          className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-md transition-colors ${pinned ? 'bg-blue-100 text-blue-700 hover:bg-blue-200 dark:bg-blue-900/40 dark:text-blue-300' : 'text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-800 dark:hover:text-gray-200'}`}
                          title={pinned ? '取消固定' : '固定到快捷栏'}
                        >
                          {pinned ? <PinOff className="h-4 w-4" /> : <Pin className="h-4 w-4" />}
                        </button>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            </section>
          ))}

          {filteredTools.length === 0 ? (
            <div className="flex min-h-40 flex-col items-center justify-center text-center text-gray-500">
              <Wrench className="mb-2 h-8 w-8 opacity-40" />
              <p className="text-sm">没有匹配的内置功能</p>
            </div>
          ) : null}
        </div>
      </div>
    </Dialog>
  );
}
