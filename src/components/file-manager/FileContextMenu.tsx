import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ChevronRight,
  ClipboardCopy,
  Copy,
  ExternalLink,
  FileEdit,
  FileInput,
  FolderOpen,
  FolderPlus,
  Info,
  Scissors,
  Trash2,
} from 'lucide-react';

import { FileInfo } from '../../types';
import type { PluginAction } from '../../types/plugin';
import {
  buildFileContextPluginMenuEntries,
  type PluginFileContextMenuEntry,
  type PluginFileContextSubmenuEntry,
} from '../../utils/pluginActions';
import { useClipboardStore } from '../../stores/clipboardStore';
import { useUiStore } from '../../stores/uiStore';

interface SystemClipboardStatus {
  hasFiles: boolean;
  hasImage: boolean;
}

interface ContextMenuProps {
  file: FileInfo;
  x: number;
  y: number;
  currentPath: string;
  projectPath: string;
  pluginActions?: PluginAction[];
  pluginDebugInfo?: string;
  onClose: () => void;
  onRefresh?: () => void;
  onShowDetails?: (file: FileInfo) => void;
  onDelete?: (file: FileInfo) => Promise<void> | void;
  onCreateFolder?: () => Promise<void> | void;
  onOpenFile?: (file: FileInfo) => Promise<void> | void;
  onRunPluginAction?: (action: PluginAction) => void;
}

interface CurrentDirectoryContextMenuProps {
  x: number;
  y: number;
  currentPath: string;
  projectPath: string;
  pluginActions?: PluginAction[];
  pluginDebugInfo?: string;
  onClose: () => void;
  onRefresh?: () => void;
  onCreateFolder: () => Promise<void> | void;
  onRunPluginAction?: (action: PluginAction) => void;
}

interface OpenPluginSubmenu {
  key: string;
  title: string;
  actions: PluginAction[];
  anchorRect: DOMRect;
}

const CONTEXT_MENU_MIN_WIDTH = 220;
const CONTEXT_MENU_VIEWPORT_PADDING = 8;
const CONTEXT_SUBMENU_GAP = 4;

function useContextMenuDismiss(
  menuRefs: Array<React.RefObject<HTMLElement | null>>,
  onClose: () => void,
) {
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      const isInsideAnyMenu = menuRefs.some((menuRef) => {
        return menuRef.current?.contains(target);
      });

      if (!isInsideAnyMenu) {
        onClose();
      }
    };

    const handleEsc = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEsc);

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEsc);
    };
  }, [menuRefs, onClose]);
}

function getMenuStyle(
  x: number,
  y: number,
  width: number,
  height: number,
): React.CSSProperties {
  const maxHeight = Math.max(160, window.innerHeight - CONTEXT_MENU_VIEWPORT_PADDING * 2);
  const clampedHeight = Math.min(height, maxHeight);

  return {
    position: 'fixed',
    left: Math.max(
      CONTEXT_MENU_VIEWPORT_PADDING,
      Math.min(x, window.innerWidth - width - CONTEXT_MENU_VIEWPORT_PADDING),
    ),
    top: Math.max(
      CONTEXT_MENU_VIEWPORT_PADDING,
      Math.min(y, window.innerHeight - clampedHeight - CONTEXT_MENU_VIEWPORT_PADDING),
    ),
    zIndex: 9999,
    maxHeight,
    overflowY: 'auto',
  };
}

function getFlyoutMenuStyle(
  anchorRect: DOMRect,
  width: number,
  height: number,
): React.CSSProperties {
  const maxHeight = Math.max(160, window.innerHeight - CONTEXT_MENU_VIEWPORT_PADDING * 2);
  const clampedHeight = Math.min(height, maxHeight);
  const preferredLeft = anchorRect.right + CONTEXT_SUBMENU_GAP;
  const fallbackLeft = anchorRect.left - width - CONTEXT_SUBMENU_GAP;
  const left = preferredLeft + width <= window.innerWidth - CONTEXT_MENU_VIEWPORT_PADDING
    ? preferredLeft
    : Math.max(CONTEXT_MENU_VIEWPORT_PADDING, fallbackLeft);

  return {
    position: 'fixed',
    left,
    top: Math.max(
      CONTEXT_MENU_VIEWPORT_PADDING,
      Math.min(anchorRect.top, window.innerHeight - clampedHeight - CONTEXT_MENU_VIEWPORT_PADDING),
    ),
    zIndex: 10000,
    maxHeight,
    overflowY: 'auto',
  };
}

function useContextMenuStyle(
  menuRef: React.RefObject<HTMLDivElement | null>,
  x: number,
  y: number,
  estimatedHeight: number,
  deps: readonly unknown[] = [],
) {
  const [menuStyle, setMenuStyle] = useState<React.CSSProperties>(() =>
    getMenuStyle(x, y, CONTEXT_MENU_MIN_WIDTH, estimatedHeight),
  );

  useLayoutEffect(() => {
    const updateStyle = () => {
      const rect = menuRef.current?.getBoundingClientRect();
      const width = Math.max(CONTEXT_MENU_MIN_WIDTH, Math.ceil(rect?.width ?? CONTEXT_MENU_MIN_WIDTH));
      const height = Math.ceil(rect?.height ?? estimatedHeight);
      setMenuStyle(getMenuStyle(x, y, width, height));
    };

    updateStyle();
    window.addEventListener('resize', updateStyle);
    return () => window.removeEventListener('resize', updateStyle);
  }, [menuRef, x, y, estimatedHeight, ...deps]);

  return menuStyle;
}

function useFlyoutMenuStyle(
  menuRef: React.RefObject<HTMLDivElement | null>,
  anchorRect: DOMRect | null,
  estimatedHeight: number,
  deps: readonly unknown[] = [],
) {
  const [menuStyle, setMenuStyle] = useState<React.CSSProperties | null>(() => {
    if (!anchorRect) {
      return null;
    }

    return getFlyoutMenuStyle(anchorRect, CONTEXT_MENU_MIN_WIDTH, estimatedHeight);
  });

  useLayoutEffect(() => {
    if (!anchorRect) {
      setMenuStyle(null);
      return;
    }

    const updateStyle = () => {
      const rect = menuRef.current?.getBoundingClientRect();
      const width = Math.max(CONTEXT_MENU_MIN_WIDTH, Math.ceil(rect?.width ?? CONTEXT_MENU_MIN_WIDTH));
      const height = Math.ceil(rect?.height ?? estimatedHeight);
      setMenuStyle(getFlyoutMenuStyle(anchorRect, width, height));
    };

    updateStyle();
    window.addEventListener('resize', updateStyle);
    return () => window.removeEventListener('resize', updateStyle);
  }, [
    menuRef,
    anchorRect?.left,
    anchorRect?.top,
    anchorRect?.right,
    anchorRect?.height,
    estimatedHeight,
    ...deps,
  ]);

  return menuStyle;
}

function buildPluginSubmenuEstimate(actionCount: number) {
  return Math.max(120, actionCount * 40 + 16);
}

function PluginMenuEntries({
  entries,
  openSubmenuKey,
  onToggleSubmenu,
  onRunPluginAction,
}: {
  entries: PluginFileContextMenuEntry[];
  openSubmenuKey?: string | null;
  onToggleSubmenu: (
    entry: PluginFileContextSubmenuEntry,
    button: HTMLButtonElement,
  ) => void;
  onRunPluginAction: (action: PluginAction) => void;
}) {
  return (
    <>
      {entries.map((entry) => {
        if (entry.kind === 'action') {
          return (
            <MenuItem
              key={entry.key}
              onClick={() => onRunPluginAction(entry.action)}
              icon={<FolderOpen className="w-4 h-4" />}
            >
              {entry.action.title}
            </MenuItem>
          );
        }

        return (
          <MenuSubmenuTrigger
            key={entry.key}
            onToggle={(event) => onToggleSubmenu(entry, event.currentTarget)}
            icon={<FolderOpen className="w-4 h-4" />}
            active={openSubmenuKey === entry.key}
          >
            {entry.title}
          </MenuSubmenuTrigger>
        );
      })}
    </>
  );
}

function PluginSubmenuPanel({
  submenu,
  submenuRef,
  submenuStyle,
  onRunPluginAction,
}: {
  submenu: OpenPluginSubmenu | null;
  submenuRef: React.RefObject<HTMLDivElement | null>;
  submenuStyle: React.CSSProperties | null;
  onRunPluginAction: (action: PluginAction) => void;
}) {
  if (!submenu || !submenuStyle) {
    return null;
  }

  return (
    <div
      ref={submenuRef}
      className="bg-white dark:bg-gray-800 rounded-lg shadow-xl border border-gray-200 dark:border-gray-700 py-1 min-w-[180px]"
      style={submenuStyle}
    >
      {submenu.actions.map((action) => (
        <MenuItem
          key={action.id}
          onClick={() => onRunPluginAction(action)}
          icon={<FolderOpen className="w-4 h-4" />}
        >
          {action.title}
        </MenuItem>
      ))}
    </div>
  );
}

export function FileContextMenu({
  file,
  x,
  y,
  currentPath,
  projectPath,
  pluginActions = [],
  pluginDebugInfo,
  onClose,
  onRefresh,
  onShowDetails,
  onDelete,
  onCreateFolder,
  onOpenFile,
  onRunPluginAction,
}: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const submenuRef = useRef<HTMLDivElement>(null);
  const { items: clipboardItems, cut, copy, paste, hasItem } = useClipboardStore();
  const showToast = useUiStore((state) => state.showToast);
  const [systemClipboardStatus, setSystemClipboardStatus] = useState<SystemClipboardStatus>({
    hasFiles: false,
    hasImage: false,
  });
  const [openPluginSubmenu, setOpenPluginSubmenu] = useState<OpenPluginSubmenu | null>(null);
  const pluginMenuEntries = buildFileContextPluginMenuEntries(pluginActions);

  useContextMenuDismiss([menuRef, submenuRef], onClose);

  useEffect(() => {
    let isCancelled = false;

    const loadSystemClipboardStatus = async () => {
      try {
        const status = await invoke<SystemClipboardStatus>('get_system_clipboard_status');
        if (!isCancelled) {
          setSystemClipboardStatus(status);
        }
      } catch (error) {
        console.error('Failed to get system clipboard status:', error);
      }
    };

    void loadSystemClipboardStatus();

    return () => {
      isCancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!openPluginSubmenu) {
      return;
    }

    const submenuStillExists = [...pluginMenuEntries.inlineEntries, ...pluginMenuEntries.sectionEntries]
      .some((entry) => entry.kind === 'submenu' && entry.key === openPluginSubmenu.key);

    if (!submenuStillExists) {
      setOpenPluginSubmenu(null);
    }
  }, [openPluginSubmenu, pluginMenuEntries.inlineEntries, pluginMenuEntries.sectionEntries]);

  const handleOpen = async () => {
    try {
      if (file.is_dir) {
        await invoke('open_path', { path: file.path });
      } else if (onOpenFile) {
        await onOpenFile(file);
      } else {
        await invoke('open_file', { path: file.path });
      }
    } catch (error) {
      console.error('Failed to open:', error);
    }
    onClose();
  };

  const handleReveal = async () => {
    try {
      await invoke('reveal_in_explorer', { path: file.path });
    } catch (error) {
      console.error('Failed to reveal:', error);
    }
    onClose();
  };

  const handleCopyPath = async () => {
    try {
      await navigator.clipboard.writeText(file.path);
    } catch (error) {
      console.error('Failed to copy path:', error);
    }
    onClose();
  };

  const handleCopyName = async () => {
    try {
      await navigator.clipboard.writeText(file.name);
    } catch (error) {
      console.error('Failed to copy name:', error);
    }
    onClose();
  };

  const handleCut = () => {
    cut(file.path, file.name, projectPath);
    onClose();
  };

  const handleCopyToClipboard = () => {
    copy(file.path, file.name, projectPath);
    onClose();
  };

  const handlePaste = async () => {
    const targetDir = file.is_dir ? file.path : currentPath;

    try {
      let success = false;

      if (clipboardItems.length > 0) {
        success = await paste(targetDir, projectPath);
      } else {
        const pastedPaths = await invoke<string[]>('paste_system_clipboard', { targetDir });
        success = pastedPaths.length > 0;
      }

      if (success) {
        onRefresh?.();
      }
    } catch (error) {
      console.error('Failed to paste:', error);
      showToast({
        title: '粘贴失败',
        message: String(error),
        tone: 'error',
      });
    }

    onClose();
  };

  const handleDelete = async () => {
    try {
      await onDelete?.(file);
    } catch (error) {
      console.error('Failed to delete:', error);
    }
    onClose();
  };

  const handleRename = async () => {
    const newName = prompt('新名称', file.name);
    if (newName && newName !== file.name) {
      try {
        await invoke('rename_project_entry', {
          projectPath,
          path: file.path,
          newName,
        });
        onRefresh?.();
      } catch (error) {
        console.error('Failed to rename:', error);
        const message = String(error).startsWith('PM_CONFLICT:')
          ? '目标位置已存在同名文件或文件夹'
          : `重命名失败: ${error}`;
        alert(message);
      }
    }
    onClose();
  };

  const handleCreateFolder = async () => {
    try {
      await onCreateFolder?.();
    } catch (error) {
      console.error('Failed to create folder:', error);
    }
    onClose();
  };

  const handleShowDetails = () => {
    onShowDetails?.(file);
    onClose();
  };

  const handleCopyPluginDebugInfo = async () => {
    if (!pluginDebugInfo) {
      return;
    }

    try {
      await navigator.clipboard.writeText(pluginDebugInfo);
      showToast({
        title: '插件调试信息已复制',
        message: '把剪贴板内容直接发给我就行。',
        tone: 'success',
      });
    } catch (error) {
      console.error('Failed to copy plugin debug info:', error);
      showToast({
        title: '复制插件调试信息失败',
        message: String(error),
        tone: 'error',
      });
    }
    onClose();
  };

  const handleRunPluginAction = (action: PluginAction) => {
    setOpenPluginSubmenu(null);
    onRunPluginAction?.(action);
    onClose();
  };

  const handleTogglePluginSubmenu = (
    entry: PluginFileContextSubmenuEntry,
    button: HTMLButtonElement,
  ) => {
    const anchorRect = button.getBoundingClientRect();
    setOpenPluginSubmenu((current) => {
      if (current?.key === entry.key) {
        return null;
      }

      return {
        key: entry.key,
        title: entry.title,
        actions: entry.actions,
        anchorRect,
      };
    });
  };

  const closePluginSubmenu = () => {
    setOpenPluginSubmenu(null);
  };

  const hasSystemPasteSource = systemClipboardStatus.hasFiles || systemClipboardStatus.hasImage;
  const canPaste = hasItem() || hasSystemPasteSource;
  const isCutItem = clipboardItems.some((item) => item.action === 'cut' && item.path === file.path);
  const menuStyle = useContextMenuStyle(menuRef, x, y, 560, [
    file.is_dir,
    pluginActions.length,
    pluginMenuEntries.inlineEntries.length,
    pluginMenuEntries.sectionEntries.length,
  ]);
  const submenuStyle = useFlyoutMenuStyle(
    submenuRef,
    openPluginSubmenu?.anchorRect ?? null,
    buildPluginSubmenuEstimate(openPluginSubmenu?.actions.length ?? 0),
    [openPluginSubmenu?.key, openPluginSubmenu?.actions.length ?? 0],
  );

  return (
    <>
      <div
        ref={menuRef}
        className="bg-white dark:bg-gray-800 rounded-lg shadow-xl border border-gray-200 dark:border-gray-700 py-1 min-w-[180px]"
        style={menuStyle}
        onScroll={closePluginSubmenu}
      >
        <MenuItem onClick={handleOpen} icon={<FolderOpen className="w-4 h-4" />}>
          {file.is_dir ? '打开文件夹' : '打开'}
        </MenuItem>

        <MenuItem onClick={handleReveal} icon={<ExternalLink className="w-4 h-4" />}>
          在资源管理器中显示
        </MenuItem>

        <MenuDivider />

        <MenuItem
          onClick={handleCut}
          icon={<Scissors className="w-4 h-4" />}
          active={isCutItem}
        >
          剪切
        </MenuItem>

        <MenuItem onClick={handleCopyToClipboard} icon={<Copy className="w-4 h-4" />}>
          复制
        </MenuItem>

        <MenuItem
          onClick={handlePaste}
          icon={<FileInput className="w-4 h-4" />}
          disabled={!canPaste}
        >
          粘贴
        </MenuItem>

        <MenuDivider />

        <MenuItem onClick={handleCreateFolder} icon={<FolderPlus className="w-4 h-4" />}>
          在当前目录新建文件夹
        </MenuItem>

        <MenuDivider />

        <MenuItem onClick={handleCopyName} icon={<ClipboardCopy className="w-4 h-4" />}>
          复制文件名
        </MenuItem>

        <MenuItem onClick={handleCopyPath} icon={<ClipboardCopy className="w-4 h-4" />}>
          复制完整路径
        </MenuItem>

        <MenuDivider />

        <MenuItem onClick={handleRename} icon={<FileEdit className="w-4 h-4" />}>
          重命名
        </MenuItem>

        <MenuItem
          onClick={handleDelete}
          icon={<Trash2 className="w-4 h-4" />}
          danger
        >
          删除
        </MenuItem>

        {pluginMenuEntries.inlineEntries.length > 0 ? (
          <>
            <MenuDivider />
            <PluginMenuEntries
              entries={pluginMenuEntries.inlineEntries}
              openSubmenuKey={openPluginSubmenu?.key}
              onToggleSubmenu={handleTogglePluginSubmenu}
              onRunPluginAction={handleRunPluginAction}
            />
            <MenuDivider />
          </>
        ) : (
          <MenuDivider />
        )}

        <MenuItem onClick={handleShowDetails} icon={<Info className="w-4 h-4" />}>
          详细信息
        </MenuItem>

        {pluginDebugInfo && (
          <>
            <MenuDivider />
            <MenuItem onClick={handleCopyPluginDebugInfo} icon={<ClipboardCopy className="w-4 h-4" />}>
              复制插件调试信息
            </MenuItem>
          </>
        )}

        {pluginMenuEntries.sectionEntries.length > 0 && (
          <>
            <MenuDivider />
            <MenuSectionLabel>插件</MenuSectionLabel>
            <PluginMenuEntries
              entries={pluginMenuEntries.sectionEntries}
              openSubmenuKey={openPluginSubmenu?.key}
              onToggleSubmenu={handleTogglePluginSubmenu}
              onRunPluginAction={handleRunPluginAction}
            />
          </>
        )}
      </div>

      <PluginSubmenuPanel
        submenu={openPluginSubmenu}
        submenuRef={submenuRef}
        submenuStyle={submenuStyle}
        onRunPluginAction={handleRunPluginAction}
      />
    </>
  );
}

export function CurrentDirectoryContextMenu({
  x,
  y,
  currentPath,
  projectPath,
  pluginActions = [],
  pluginDebugInfo,
  onClose,
  onRefresh,
  onCreateFolder,
  onRunPluginAction,
}: CurrentDirectoryContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const submenuRef = useRef<HTMLDivElement>(null);
  const { items: clipboardItems, paste, hasItem } = useClipboardStore();
  const showToast = useUiStore((state) => state.showToast);
  const [systemClipboardStatus, setSystemClipboardStatus] = useState<SystemClipboardStatus>({
    hasFiles: false,
    hasImage: false,
  });
  const [openPluginSubmenu, setOpenPluginSubmenu] = useState<OpenPluginSubmenu | null>(null);
  const pluginMenuEntries = buildFileContextPluginMenuEntries(pluginActions);

  useContextMenuDismiss([menuRef, submenuRef], onClose);

  useEffect(() => {
    let isCancelled = false;

    const loadSystemClipboardStatus = async () => {
      try {
        const status = await invoke<SystemClipboardStatus>('get_system_clipboard_status');
        if (!isCancelled) {
          setSystemClipboardStatus(status);
        }
      } catch (error) {
        console.error('Failed to get system clipboard status:', error);
      }
    };

    void loadSystemClipboardStatus();

    return () => {
      isCancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!openPluginSubmenu) {
      return;
    }

    const submenuStillExists = [...pluginMenuEntries.inlineEntries, ...pluginMenuEntries.sectionEntries]
      .some((entry) => entry.kind === 'submenu' && entry.key === openPluginSubmenu.key);

    if (!submenuStillExists) {
      setOpenPluginSubmenu(null);
    }
  }, [openPluginSubmenu, pluginMenuEntries.inlineEntries, pluginMenuEntries.sectionEntries]);

  const handleCreateFolder = async () => {
    try {
      await onCreateFolder();
    } catch (error) {
      console.error('Failed to create folder:', error);
    }
    onClose();
  };

  const handlePaste = async () => {
    try {
      let success = false;

      if (clipboardItems.length > 0) {
        success = await paste(currentPath, projectPath);
      } else {
        const pastedPaths = await invoke<string[]>('paste_system_clipboard', { targetDir: currentPath });
        success = pastedPaths.length > 0;
      }

      if (success) {
        onRefresh?.();
      }
    } catch (error) {
      console.error('Failed to paste:', error);
      showToast({
        title: '粘贴失败',
        message: String(error),
        tone: 'error',
      });
    }
    onClose();
  };

  const handleRunPluginAction = (action: PluginAction) => {
    setOpenPluginSubmenu(null);
    onRunPluginAction?.(action);
    onClose();
  };

  const handleCopyPluginDebugInfo = async () => {
    if (!pluginDebugInfo) {
      return;
    }

    try {
      await navigator.clipboard.writeText(pluginDebugInfo);
      showToast({
        title: '插件调试信息已复制',
        message: '把剪贴板内容直接发给我就行。',
        tone: 'success',
      });
    } catch (error) {
      console.error('Failed to copy plugin debug info:', error);
      showToast({
        title: '复制插件调试信息失败',
        message: String(error),
        tone: 'error',
      });
    }
    onClose();
  };

  const handleTogglePluginSubmenu = (
    entry: PluginFileContextSubmenuEntry,
    button: HTMLButtonElement,
  ) => {
    const anchorRect = button.getBoundingClientRect();
    setOpenPluginSubmenu((current) => {
      if (current?.key === entry.key) {
        return null;
      }

      return {
        key: entry.key,
        title: entry.title,
        actions: entry.actions,
        anchorRect,
      };
    });
  };

  const closePluginSubmenu = () => {
    setOpenPluginSubmenu(null);
  };

  const hasSystemPasteSource = systemClipboardStatus.hasFiles || systemClipboardStatus.hasImage;
  const canPaste = hasItem() || hasSystemPasteSource;
  const menuStyle = useContextMenuStyle(menuRef, x, y, 260, [
    pluginActions.length,
    pluginMenuEntries.inlineEntries.length,
    pluginMenuEntries.sectionEntries.length,
  ]);
  const submenuStyle = useFlyoutMenuStyle(
    submenuRef,
    openPluginSubmenu?.anchorRect ?? null,
    buildPluginSubmenuEstimate(openPluginSubmenu?.actions.length ?? 0),
    [openPluginSubmenu?.key, openPluginSubmenu?.actions.length ?? 0],
  );

  return (
    <>
      <div
        ref={menuRef}
        className="bg-white dark:bg-gray-800 rounded-lg shadow-xl border border-gray-200 dark:border-gray-700 py-1 min-w-[180px]"
        style={menuStyle}
        onScroll={closePluginSubmenu}
      >
        <MenuItem
          onClick={handlePaste}
          icon={<FileInput className="w-4 h-4" />}
          disabled={!canPaste}
        >
          粘贴
        </MenuItem>

        <MenuDivider />

        <MenuItem onClick={handleCreateFolder} icon={<FolderPlus className="w-4 h-4" />}>
          新建文件夹
        </MenuItem>

        {pluginMenuEntries.inlineEntries.length > 0 && (
          <>
            <MenuDivider />
            <PluginMenuEntries
              entries={pluginMenuEntries.inlineEntries}
              openSubmenuKey={openPluginSubmenu?.key}
              onToggleSubmenu={handleTogglePluginSubmenu}
              onRunPluginAction={handleRunPluginAction}
            />
          </>
        )}

        {pluginDebugInfo && (
          <>
            <MenuDivider />
            <MenuItem onClick={handleCopyPluginDebugInfo} icon={<ClipboardCopy className="w-4 h-4" />}>
              复制插件调试信息
            </MenuItem>
          </>
        )}

        {pluginMenuEntries.sectionEntries.length > 0 && (
          <>
            <MenuDivider />
            <MenuSectionLabel>插件</MenuSectionLabel>
            <PluginMenuEntries
              entries={pluginMenuEntries.sectionEntries}
              openSubmenuKey={openPluginSubmenu?.key}
              onToggleSubmenu={handleTogglePluginSubmenu}
              onRunPluginAction={handleRunPluginAction}
            />
          </>
        )}
      </div>

      <PluginSubmenuPanel
        submenu={openPluginSubmenu}
        submenuRef={submenuRef}
        submenuStyle={submenuStyle}
        onRunPluginAction={handleRunPluginAction}
      />
    </>
  );
}

function MenuItem({
  children,
  onClick,
  icon,
  disabled = false,
  danger = false,
  active = false,
}: {
  children: React.ReactNode;
  onClick: () => void | Promise<void>;
  icon: React.ReactNode;
  disabled?: boolean;
  danger?: boolean;
  active?: boolean;
}) {
  return (
    <button
      onClick={() => {
        void onClick();
      }}
      disabled={disabled}
      className={`
        w-full px-3 py-2 flex items-center gap-2 text-sm text-left
        ${disabled
          ? 'text-gray-400 cursor-not-allowed'
          : danger
            ? 'text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20'
            : active
              ? 'text-blue-600 bg-blue-50 dark:bg-blue-900/20'
              : 'text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700'
        }
        transition-colors
      `}
    >
      <span className={danger ? 'text-red-500' : active ? 'text-blue-500' : 'text-gray-500 dark:text-gray-400'}>
        {icon}
      </span>
      <span className="flex-1 truncate">{children}</span>
    </button>
  );
}

function MenuSubmenuTrigger({
  children,
  onToggle,
  icon,
  active = false,
}: {
  children: React.ReactNode;
  onToggle: (event: React.MouseEvent<HTMLButtonElement>) => void;
  icon: React.ReactNode;
  active?: boolean;
}) {
  return (
    <button
      onClick={onToggle}
      className={`
        w-full px-3 py-2 flex items-center gap-2 text-sm text-left
        ${active
          ? 'text-blue-600 bg-blue-50 dark:bg-blue-900/20'
          : 'text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700'
        }
        transition-colors
      `}
    >
      <span className={active ? 'text-blue-500' : 'text-gray-500 dark:text-gray-400'}>
        {icon}
      </span>
      <span className="flex-1 truncate">{children}</span>
      <ChevronRight className={`w-4 h-4 ${active ? 'text-blue-500' : 'text-gray-400 dark:text-gray-500'}`} />
    </button>
  );
}

function MenuDivider() {
  return (
    <div className="my-1 border-t border-gray-200 dark:border-gray-700" />
  );
}

function MenuSectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-3 py-1 text-[11px] font-medium uppercase tracking-wider text-gray-400">
      {children}
    </div>
  );
}
