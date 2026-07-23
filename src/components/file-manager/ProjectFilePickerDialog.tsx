import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import {
  ArrowUp,
  Check,
  ChevronDown,
  ChevronRight,
  File as FileIcon,
  Folder,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  RefreshCw,
} from 'lucide-react';
import { Dialog } from '../Dialog';
import type { FileInfo, TreeNode } from '../../types';

export type ProjectFilePickerTarget = 'file' | 'directory';
export type ProjectFilePickerSelectionMode = 'single' | 'multiple';

interface ProjectFilePickerDialogProps {
  isOpen: boolean;
  projectPath: string;
  title?: string;
  target?: ProjectFilePickerTarget;
  selectionMode?: ProjectFilePickerSelectionMode;
  extensions?: string[];
  initialDirectory?: string;
  onClose: () => void;
  onSelect: (paths: string[]) => void | Promise<void>;
}

function normalizePath(path: string) {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').toLocaleLowerCase();
}

function samePath(left: string, right: string) {
  return normalizePath(left) === normalizePath(right);
}

function isPathInsideProject(projectPath: string, targetPath: string) {
  const projectKey = normalizePath(projectPath);
  const targetKey = normalizePath(targetPath);
  return targetKey === projectKey || targetKey.startsWith(`${projectKey}/`);
}

function parentDirectory(path: string) {
  const normalized = path.replace(/[\\/]+$/, '');
  const separator = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'));
  if (separator === 2 && /^[A-Za-z]:/.test(normalized)) return normalized.slice(0, 3);
  return separator > 0 ? normalized.slice(0, separator) : normalized;
}

function relativePath(projectPath: string, targetPath: string) {
  const projectKey = normalizePath(projectPath);
  const targetKey = normalizePath(targetPath);
  if (projectKey === targetKey) return '项目根目录';
  if (!targetKey.startsWith(`${projectKey}/`)) return targetPath;
  return targetPath.replace(/\\/g, '/').slice(projectPath.replace(/\\/g, '/').replace(/\/+$/, '').length + 1);
}

function normalizeExtensions(extensions: string[]) {
  return new Set(extensions.map((value) => value.replace(/^\./, '').trim().toLocaleLowerCase()).filter(Boolean));
}

function findDirectoryPath(node: TreeNode, targetPath: string): string[] | null {
  if (samePath(node.path, targetPath)) return [node.path];
  for (const child of node.children) {
    const childPath = findDirectoryPath(child, targetPath);
    if (childPath) return [node.path, ...childPath];
  }
  return null;
}

interface DirectoryTreeItemProps {
  node: TreeNode;
  depth: number;
  currentDirectory: string;
  expandedPaths: Set<string>;
  onToggle: (path: string) => void;
  onNavigate: (path: string) => void;
}

function DirectoryTreeItem({
  node,
  depth,
  currentDirectory,
  expandedPaths,
  onToggle,
  onNavigate,
}: DirectoryTreeItemProps) {
  const expanded = expandedPaths.has(normalizePath(node.path));
  const active = samePath(node.path, currentDirectory);
  const hasChildren = node.children.length > 0;

  return (
    <div>
      <div
        className={`group flex h-8 cursor-pointer select-none items-center border-r-2 pr-2 text-xs ${active ? 'border-blue-600 bg-blue-100 font-medium text-blue-900 shadow-[inset_3px_0_0_#2563eb] dark:bg-blue-900/50 dark:text-blue-100' : 'border-transparent text-gray-700 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800'}`}
        style={{ paddingLeft: `${8 + depth * 16}px` }}
        onClick={() => onNavigate(node.path)}
        title={node.path}
      >
        {hasChildren ? (
          <button
            type="button"
            title={expanded ? '收起目录' : '展开目录'}
            onClick={(event) => { event.stopPropagation(); onToggle(node.path); }}
            className="mr-1 flex h-5 w-5 shrink-0 items-center justify-center rounded hover:bg-gray-200 dark:hover:bg-gray-700"
          >
            {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          </button>
        ) : <span className="mr-1 h-5 w-5 shrink-0" />}
        {expanded ? <FolderOpen className="mr-2 h-4 w-4 shrink-0 text-amber-500" /> : <Folder className="mr-2 h-4 w-4 shrink-0 text-amber-500" />}
        <span className="truncate">{node.name}</span>
      </div>
      {expanded && hasChildren && node.children.map((child) => (
        <DirectoryTreeItem
          key={child.path}
          node={child}
          depth={depth + 1}
          currentDirectory={currentDirectory}
          expandedPaths={expandedPaths}
          onToggle={onToggle}
          onNavigate={onNavigate}
        />
      ))}
    </div>
  );
}

export function ProjectFilePickerDialog({
  isOpen,
  projectPath,
  title = '选择项目文件',
  target = 'file',
  selectionMode = 'single',
  extensions = [],
  initialDirectory,
  onClose,
  onSelect,
}: ProjectFilePickerDialogProps) {
  const [currentDirectory, setCurrentDirectory] = useState(projectPath);
  const [directoryTree, setDirectoryTree] = useState<TreeNode | null>(null);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [entries, setEntries] = useState<FileInfo[]>([]);
  const [isTreeLoading, setIsTreeLoading] = useState(false);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isOpeningSystem, setIsOpeningSystem] = useState(false);
  const [isConfirming, setIsConfirming] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const extensionSet = useMemo(() => normalizeExtensions(extensions), [extensions]);
  const multiple = selectionMode === 'multiple';

  useEffect(() => {
    if (!isOpen) return;
    const preferredDirectory = initialDirectory && isPathInsideProject(projectPath, initialDirectory)
      ? initialDirectory
      : projectPath;
    setCurrentDirectory(preferredDirectory);
    setSelectedPaths([]);
    setExpandedPaths(new Set([normalizePath(projectPath)]));
    setErrorMessage(null);
    setReloadKey((value) => value + 1);
  }, [initialDirectory, isOpen, projectPath]);

  useEffect(() => {
    if (!isOpen) return;
    let active = true;
    setIsTreeLoading(true);
    invoke<TreeNode>('get_directory_tree', {
      path: projectPath,
      projectPath,
      forceRefresh: false,
      includePmCenter: false,
    }).then((result) => {
      if (active) setDirectoryTree(result);
    }).catch(() => {
      if (active) setDirectoryTree(null);
    }).finally(() => {
      if (active) setIsTreeLoading(false);
    });
    return () => { active = false; };
  }, [isOpen, projectPath, reloadKey]);

  useEffect(() => {
    if (!directoryTree) return;
    const pathToCurrent = findDirectoryPath(directoryTree, currentDirectory);
    if (!pathToCurrent) return;
    setExpandedPaths((current) => {
      const next = new Set(current);
      pathToCurrent.forEach((path) => next.add(normalizePath(path)));
      return next;
    });
  }, [currentDirectory, directoryTree]);

  useEffect(() => {
    if (!isOpen) return;
    let active = true;
    setIsLoading(true);
    setErrorMessage(null);
    invoke<FileInfo[]>('read_directory', {
      path: currentDirectory,
      projectPath,
      forceRefresh: true,
    }).then((result) => {
      if (active) setEntries(result);
    }).catch((error) => {
      if (!active) return;
      setEntries([]);
      setErrorMessage(`读取项目目录失败：${String(error)}`);
    }).finally(() => {
      if (active) setIsLoading(false);
    });
    return () => { active = false; };
  }, [currentDirectory, isOpen, projectPath, reloadKey]);

  const visibleEntries = useMemo(() => entries.filter((entry) => {
    if (entry.is_dir) return true;
    if (entry.entry_kind && entry.entry_kind !== 'file') return false;
    if (extensionSet.size === 0) return true;
    return extensionSet.has((entry.extension || '').replace(/^\./, '').toLocaleLowerCase());
  }), [entries, extensionSet]);

  const atProjectRoot = samePath(currentDirectory, projectPath);
  const defaultDirectorySelection = target === 'directory' && !multiple && selectedPaths.length === 0
    ? currentDirectory
    : null;
  const confirmedPaths = selectedPaths.length > 0
    ? selectedPaths
    : defaultDirectorySelection ? [defaultDirectorySelection] : [];
  const canConfirm = confirmedPaths.length > 0;

  const toggleSelectedPath = (path: string) => {
    if (!multiple) {
      setSelectedPaths([path]);
      return;
    }
    setSelectedPaths((current) => current.some((item) => samePath(item, path))
      ? current.filter((item) => !samePath(item, path))
      : [...current, path]);
  };

  const selectEntry = (entry: FileInfo) => {
    if (entry.is_dir && target === 'file') {
      setCurrentDirectory(entry.path);
      setSelectedPaths([]);
      return;
    }
    if (entry.is_dir && target === 'directory') {
      toggleSelectedPath(entry.path);
      return;
    }
    if (target !== 'file') return;
    toggleSelectedPath(entry.path);
  };

  const navigateToDirectory = (path: string) => {
    setCurrentDirectory(path);
    setSelectedPaths([]);
  };

  const toggleDirectory = (path: string) => {
    const pathKey = normalizePath(path);
    setExpandedPaths((current) => {
      const next = new Set(current);
      if (next.has(pathKey)) next.delete(pathKey);
      else next.add(pathKey);
      return next;
    });
  };

  const finishSelection = async (paths: string[]) => {
    if (paths.length === 0 || isConfirming) return;
    setIsConfirming(true);
    try {
      await onSelect(paths);
      onClose();
    } catch (error) {
      setErrorMessage(`处理所选内容失败：${String(error)}`);
    } finally {
      setIsConfirming(false);
    }
  };

  const openSystemPicker = async () => {
    setIsOpeningSystem(true);
    setErrorMessage(null);
    try {
      const normalizedExtensions = extensions
        .map((value) => value.replace(/^\./, '').trim())
        .filter(Boolean);
      const selected = await open({
        title,
        directory: target === 'directory',
        multiple,
        defaultPath: currentDirectory,
        filters: target === 'file' && normalizedExtensions.length > 0
          ? [{ name: normalizedExtensions.map((value) => `*.${value}`).join(' '), extensions: normalizedExtensions }]
          : undefined,
      });
      const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
      if (paths.length > 0) await finishSelection(paths);
    } catch (error) {
      setErrorMessage(`打开系统选择器失败：${String(error)}`);
    } finally {
      setIsOpeningSystem(false);
    }
  };

  const confirmSelection = () => {
    void finishSelection(confirmedPaths);
  };

  const footer = (
    <>
      <span className="mr-auto self-center text-xs text-gray-500">
        {target === 'file'
          ? `已选择 ${selectedPaths.length} 个文件${multiple ? '（可多选）' : ''}`
          : `已选择 ${confirmedPaths.length} 个文件夹${multiple ? '（可多选）' : ''}`}
      </span>
      <button type="button" disabled={isConfirming} onClick={onClose} className="h-9 rounded px-4 text-sm text-gray-600 hover:bg-gray-100 disabled:opacity-50 dark:text-gray-300 dark:hover:bg-gray-800">取消</button>
      <button type="button" disabled={!canConfirm || isConfirming} onClick={confirmSelection} className="inline-flex h-9 items-center gap-1.5 rounded bg-gray-900 px-4 text-sm font-medium text-white disabled:opacity-40 dark:bg-white dark:text-gray-900">
        {isConfirming ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Check className="h-4 w-4" />}选择
      </button>
    </>
  );

  return (
    <Dialog isOpen={isOpen} onClose={onClose} title={title} size="2xl" footer={footer}>
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <button
            type="button"
            title="上一级"
            disabled={atProjectRoot}
            onClick={() => { setCurrentDirectory(parentDirectory(currentDirectory)); setSelectedPaths([]); }}
            className="flex h-9 w-9 items-center justify-center rounded border border-gray-200 disabled:opacity-40 dark:border-gray-700"
          >
            <ArrowUp className="h-4 w-4" />
          </button>
          <button type="button" title="刷新" onClick={() => setReloadKey((value) => value + 1)} className="flex h-9 w-9 items-center justify-center rounded border border-gray-200 dark:border-gray-700">
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </button>
          <div className="min-w-0 flex-1 truncate rounded border border-gray-200 bg-gray-50 px-3 py-2 text-xs text-gray-600 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-300" title={currentDirectory}>
            {relativePath(projectPath, currentDirectory)}
          </div>
          <button
            type="button"
            disabled={isOpeningSystem || isConfirming}
            onClick={() => void openSystemPicker()}
            className="inline-flex h-9 shrink-0 items-center gap-1.5 rounded border border-gray-300 px-3 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            {isOpeningSystem ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <HardDrive className="h-4 w-4" />}
            系统选择器
          </button>
        </div>

        {target === 'directory' && (
          <button
            type="button"
            onClick={() => toggleSelectedPath(currentDirectory)}
            className={`flex w-full items-center gap-2 rounded border px-3 py-2 text-left text-xs ${confirmedPaths.some((path) => samePath(path, currentDirectory)) ? 'border-blue-600 bg-blue-100 font-medium text-blue-900 ring-1 ring-blue-500/30 dark:bg-blue-900/50 dark:text-blue-100' : 'border-gray-200 text-gray-600 dark:border-gray-700 dark:text-gray-300'}`}
          >
            <FolderOpen className="h-4 w-4" />选择当前文件夹
            {confirmedPaths.some((path) => samePath(path, currentDirectory)) && <Check className="ml-auto h-4 w-4" />}
          </button>
        )}

        <div className="grid h-[520px] min-h-0 grid-cols-1 overflow-hidden rounded border border-gray-200 md:grid-cols-[260px_minmax(0,1fr)] dark:border-gray-700">
          <aside className="flex min-h-0 flex-col border-b border-gray-200 bg-gray-50/60 md:border-b-0 md:border-r dark:border-gray-700 dark:bg-gray-950/40">
            <div className="flex h-10 shrink-0 items-center border-b border-gray-200 px-3 text-xs font-medium text-gray-600 dark:border-gray-700 dark:text-gray-300">项目目录</div>
            <div className="h-[150px] overflow-auto py-1 md:h-auto md:flex-1">
              {isTreeLoading && !directoryTree ? (
                <div className="flex h-full items-center justify-center gap-2 text-xs text-gray-500"><LoaderCircle className="h-4 w-4 animate-spin" />正在读取目录树</div>
              ) : directoryTree ? (
                <DirectoryTreeItem
                  node={directoryTree}
                  depth={0}
                  currentDirectory={currentDirectory}
                  expandedPaths={expandedPaths}
                  onToggle={toggleDirectory}
                  onNavigate={navigateToDirectory}
                />
              ) : (
                <div className="p-4 text-xs text-gray-500">目录树读取失败，可使用上方路径按钮浏览。</div>
              )}
            </div>
          </aside>
          <section className="min-h-0 overflow-auto">
            {isLoading ? (
              <div className="flex h-full items-center justify-center gap-2 text-sm text-gray-500"><LoaderCircle className="h-4 w-4 animate-spin" />正在读取项目目录</div>
            ) : errorMessage ? (
              <div className="p-5 text-sm text-red-600">{errorMessage}</div>
            ) : visibleEntries.length === 0 ? (
              <div className="flex h-full items-center justify-center text-sm text-gray-500">当前目录没有符合条件的内容</div>
            ) : (
              <div className="divide-y divide-gray-100 dark:divide-gray-800">
                {visibleEntries.map((entry) => {
                  const selected = selectedPaths.some((path) => samePath(path, entry.path));
                  const selectable = entry.is_dir ? target === 'directory' : target === 'file';
                  return (
                    <button
                      key={entry.path}
                      type="button"
                      onClick={() => selectEntry(entry)}
                      onDoubleClick={() => {
                        if (entry.is_dir) {
                          navigateToDirectory(entry.path);
                        } else if (target === 'file') {
                          void finishSelection(multiple ? Array.from(new Set([...selectedPaths, entry.path])) : [entry.path]);
                        }
                      }}
                      className={`grid h-11 w-full grid-cols-[24px_minmax(0,1fr)_auto] items-center gap-2 px-3 text-left text-xs ${selected ? 'bg-blue-100 font-medium text-blue-950 shadow-[inset_4px_0_0_#2563eb] dark:bg-blue-900/55 dark:text-blue-50' : selectable || entry.is_dir ? 'text-gray-700 hover:bg-gray-50 dark:text-gray-200 dark:hover:bg-gray-800' : 'text-gray-400 dark:text-gray-600'}`}
                    >
                      {entry.is_dir ? <Folder className="h-4 w-4 text-amber-500" /> : <FileIcon className="h-4 w-4 text-gray-400" />}
                      <span className="truncate" title={entry.path}>{entry.name}</span>
                      <span className={`inline-flex items-center gap-1 text-[10px] ${selected ? 'text-blue-700 dark:text-blue-200' : 'text-gray-400'}`}>
                        {selected && <Check className="h-3.5 w-3.5" />}
                        {entry.is_dir ? '文件夹' : selectable ? (entry.extension || '文件').toUpperCase() : '不可选'}
                      </span>
                    </button>
                  );
                })}
              </div>
            )}
          </section>
        </div>
      </div>
    </Dialog>
  );
}
