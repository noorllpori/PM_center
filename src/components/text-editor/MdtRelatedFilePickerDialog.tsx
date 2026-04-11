import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ArrowUp,
  File as FileIcon,
  Folder,
  Image as ImageIcon,
  Link2,
  Loader2,
  Plus,
  RefreshCw,
} from 'lucide-react';
import { Dialog } from '../Dialog';
import type { FileInfo } from '../../types';
import { isImageExtension } from '../image-viewer/imageViewerUtils';

export interface MdtRelatedFilePickResult {
  targetPath: string;
  mode: 'link' | 'image';
}

interface MdtRelatedFilePickerDialogProps {
  isOpen: boolean;
  projectPath: string;
  currentFilePath?: string;
  onClose: () => void;
  onPick: (result: MdtRelatedFilePickResult) => void;
}

function normalizePathKey(path: string) {
  return path.replace(/\\/g, '/').replace(/\/+$/, '');
}

function isSamePath(left: string, right: string) {
  return normalizePathKey(left) === normalizePathKey(right);
}

function getParentDirectory(path: string) {
  const normalized = path.replace(/[\\/]+$/, '');
  const lastSeparatorIndex = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'));

  if (lastSeparatorIndex <= 0) {
    return normalized;
  }

  if (lastSeparatorIndex === 2 && /^[A-Za-z]:/.test(normalized)) {
    return normalized.slice(0, 3);
  }

  return normalized.slice(0, lastSeparatorIndex);
}

function getRelativeDirectoryLabel(projectPath: string, targetPath: string) {
  const normalizedProjectPath = normalizePathKey(projectPath);
  const normalizedTargetPath = normalizePathKey(targetPath);

  if (normalizedProjectPath === normalizedTargetPath) {
    return '项目根目录';
  }

  if (!normalizedTargetPath.startsWith(`${normalizedProjectPath}/`)) {
    return targetPath;
  }

  return normalizedTargetPath.slice(normalizedProjectPath.length + 1);
}

export function MdtRelatedFilePickerDialog({
  isOpen,
  projectPath,
  currentFilePath,
  onClose,
  onPick,
}: MdtRelatedFilePickerDialogProps) {
  const [currentDirectory, setCurrentDirectory] = useState(projectPath);
  const [entries, setEntries] = useState<FileInfo[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [insertMode, setInsertMode] = useState<'link' | 'image'>('link');
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    setCurrentDirectory(projectPath);
    setSelectedPath(null);
    setInsertMode('link');
    setErrorMessage(null);
    setReloadKey(0);
  }, [isOpen, projectPath]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    let isActive = true;

    async function loadDirectory() {
      setIsLoading(true);
      setErrorMessage(null);

      try {
        const nextEntries = await invoke<FileInfo[]>('read_directory', {
          path: currentDirectory,
          projectPath,
          forceRefresh: true,
        });

        if (!isActive) {
          return;
        }

        setEntries(nextEntries);
      } catch (error) {
        if (!isActive) {
          return;
        }

        setEntries([]);
        setErrorMessage(`读取项目目录失败：${String(error)}`);
      } finally {
        if (isActive) {
          setIsLoading(false);
        }
      }
    }

    void loadDirectory();

    return () => {
      isActive = false;
    };
  }, [currentDirectory, isOpen, projectPath, reloadKey]);

  const atProjectRoot = useMemo(
    () => isSamePath(currentDirectory, projectPath),
    [currentDirectory, projectPath],
  );

  const selectedEntry = useMemo(
    () => entries.find((entry) => selectedPath && isSamePath(entry.path, selectedPath)) || null,
    [entries, selectedPath],
  );

  const canInsertAsImage = useMemo(
    () => Boolean(selectedEntry && !selectedEntry.is_dir && isImageExtension(selectedEntry.extension)),
    [selectedEntry],
  );

  useEffect(() => {
    if (!canInsertAsImage && insertMode === 'image') {
      setInsertMode('link');
    }
  }, [canInsertAsImage, insertMode]);

  const handleOpenDirectory = (path: string) => {
    setCurrentDirectory(path);
    setSelectedPath(null);
    setInsertMode('link');
  };

  const handlePick = (targetPath: string, mode: 'link' | 'image' = 'link') => {
    if (currentFilePath && isSamePath(targetPath, currentFilePath)) {
      return;
    }

    onPick({ targetPath, mode });
  };

  const footer = (
    <>
      <button
        type="button"
        onClick={onClose}
        className="rounded-lg px-4 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100"
      >
        取消
      </button>
      {canInsertAsImage ? (
        <button
          type="button"
          onClick={() => selectedPath && handlePick(selectedPath, 'image')}
          disabled={!selectedPath}
          className="inline-flex items-center gap-2 rounded-lg border border-blue-200 bg-blue-50 px-4 py-2 text-sm text-blue-700 transition-colors hover:bg-blue-100 disabled:cursor-not-allowed disabled:border-gray-200 disabled:bg-gray-100 disabled:text-gray-400"
        >
          <ImageIcon className="h-4 w-4" />
          插入为图片
        </button>
      ) : null}
      <button
        type="button"
        onClick={() => selectedPath && handlePick(selectedPath, 'link')}
        disabled={!selectedPath || (currentFilePath ? isSamePath(selectedPath, currentFilePath) : false)}
        className="inline-flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-sm text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300"
      >
        <Plus className="h-4 w-4" />
        插入文件链接
      </button>
    </>
  );

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onClose}
      title="添加关联文件"
      size="xl"
      footer={footer}
    >
      <div className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => handleOpenDirectory(getParentDirectory(currentDirectory))}
            disabled={atProjectRoot}
            className="inline-flex items-center gap-1 rounded-lg border border-gray-200 px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <ArrowUp className="h-4 w-4" />
            上一级
          </button>
          <button
            type="button"
            onClick={() => setReloadKey((value) => value + 1)}
            className="inline-flex items-center gap-1 rounded-lg border border-gray-200 px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100"
          >
            <RefreshCw className="h-4 w-4" />
            刷新
          </button>
          <button
            type="button"
            onClick={() => handlePick(currentDirectory, 'link')}
            className="inline-flex items-center gap-1 rounded-lg border border-blue-200 bg-blue-50 px-3 py-2 text-sm text-blue-700 transition-colors hover:bg-blue-100"
          >
            <Folder className="h-4 w-4" />
            插入当前文件夹
          </button>
          <div className="min-w-0 flex-1 rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-600">
            {getRelativeDirectoryLabel(projectPath, currentDirectory)}
          </div>
        </div>

        <p className="text-xs text-gray-500">
          选中的项目会插入到当前光标位置。普通文件和文件夹会作为文件链接插入，图片文件可以选择插入为图片或文件链接。
        </p>

        <div className="overflow-hidden rounded-xl border border-gray-200">
          <div className="max-h-[420px] overflow-auto">
            {isLoading ? (
              <div className="flex items-center justify-center gap-2 px-4 py-10 text-sm text-gray-500">
                <Loader2 className="h-4 w-4 animate-spin" />
                正在读取项目文件夹...
              </div>
            ) : errorMessage ? (
              <div className="px-4 py-6 text-sm text-red-500">{errorMessage}</div>
            ) : entries.length === 0 ? (
              <div className="px-4 py-10 text-center text-sm text-gray-500">
                当前文件夹没有可选项目。
              </div>
            ) : (
              <div className="divide-y divide-gray-100">
                {entries.map((entry) => {
                  const isCurrentFile = currentFilePath
                    ? isSamePath(entry.path, currentFilePath)
                    : false;
                  const isSelected = selectedPath ? isSamePath(entry.path, selectedPath) : false;
                  const isSelectableImage = !entry.is_dir && isImageExtension(entry.extension);

                  return (
                    <button
                      key={entry.path}
                      type="button"
                      onClick={() => {
                        setSelectedPath(entry.path);
                        setInsertMode(isSelectableImage ? insertMode : 'link');
                      }}
                      onDoubleClick={() => {
                        if (entry.is_dir) {
                          handleOpenDirectory(entry.path);
                          return;
                        }

                        if (!isCurrentFile) {
                          handlePick(entry.path, isSelectableImage ? insertMode : 'link');
                        }
                      }}
                      disabled={isCurrentFile}
                      className={`flex w-full items-center gap-3 px-4 py-3 text-left transition-colors ${
                        isCurrentFile
                          ? 'cursor-not-allowed bg-gray-50 text-gray-400'
                          : isSelected
                            ? 'bg-blue-50 text-blue-900'
                            : 'text-gray-700 hover:bg-gray-50'
                      }`}
                    >
                      {entry.is_dir ? (
                        <Folder className="h-4 w-4 shrink-0 text-amber-500" />
                      ) : isSelectableImage ? (
                        <ImageIcon className="h-4 w-4 shrink-0 text-emerald-500" />
                      ) : (
                        <FileIcon className="h-4 w-4 shrink-0 text-slate-500" />
                      )}
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm font-medium">
                          {entry.name}
                        </div>
                        <div className="truncate text-xs text-gray-500">
                          {getRelativeDirectoryLabel(projectPath, entry.path)}
                        </div>
                      </div>
                      <span
                        className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] ${
                          entry.is_dir
                            ? 'bg-amber-100 text-amber-700'
                            : isSelectableImage
                              ? 'bg-emerald-100 text-emerald-700'
                              : 'bg-slate-100 text-slate-600'
                        }`}
                      >
                        {entry.is_dir ? '文件夹' : isSelectableImage ? '图片' : '文件'}
                      </span>
                      {isCurrentFile ? (
                        <span className="shrink-0 text-[11px] text-gray-400">
                          当前文档
                        </span>
                      ) : null}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>

        {selectedEntry ? (
          <div className="space-y-2">
            <div className="rounded-lg border border-blue-100 bg-blue-50 px-3 py-2 text-xs text-blue-700">
              已选择：{selectedEntry.name}
            </div>

            {canInsertAsImage ? (
              <div className="flex flex-wrap items-center gap-2 rounded-lg border border-gray-200 bg-white px-3 py-2">
                <span className="text-xs text-gray-500">图片插入方式</span>
                <button
                  type="button"
                  onClick={() => setInsertMode('link')}
                  className={`inline-flex items-center gap-1 rounded px-2 py-1 text-xs transition-colors ${
                    insertMode === 'link'
                      ? 'bg-blue-600 text-white'
                      : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
                  }`}
                >
                  <Link2 className="h-3.5 w-3.5" />
                  文件链接
                </button>
                <button
                  type="button"
                  onClick={() => setInsertMode('image')}
                  className={`inline-flex items-center gap-1 rounded px-2 py-1 text-xs transition-colors ${
                    insertMode === 'image'
                      ? 'bg-blue-600 text-white'
                      : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
                  }`}
                >
                  <ImageIcon className="h-3.5 w-3.5" />
                  直接显示图片
                </button>
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}
