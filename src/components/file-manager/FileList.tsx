import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { ColumnConfig, FileInfo, Tag } from "../../types";
import {
  useProjectStoreApi,
  useProjectStoreShallow,
} from "../../stores/projectStore";
import { usePluginStore } from "../../stores/pluginStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useTaskStore } from "../../stores/taskStore";
import { useUiStore } from "../../stores/uiStore";
import { useWorkspaceTabStore } from "../../stores/workspaceTabStore";
import { APP_VERSION } from "../../config/appMeta";
import type { PluginAction } from "../../types/plugin";
import { FileIcon, FolderIcon, Image, Film, FileText, Box, Layers } from "lucide-react";
import {
  CurrentDirectoryContextMenu,
  FileContextMenu,
} from "./FileContextMenu";
import { FileDetailsDialog } from "./FileDetailsView";
import { ConfirmDialog, Dialog, InputDialog } from "../Dialog";
import {
  canMovePathsToDirectory,
  compactDraggedPaths,
  getParentPath,
  getPathLabel,
  joinPath,
} from "./dragDrop";
import { ExternalDragHandle } from "./ExternalDragHandle";
import { useFileDropMove } from "./useFileDropMove";
import { useInternalFileDrag } from "./useInternalFileDrag";
import {
  getWorkspaceOpenTarget,
  isTextExtension,
  isVideoExtension,
} from "../workspace/fileOpeners";
import {
  isDirectPreviewImageExtension,
  isImageExtension,
} from "../image-viewer/imageViewerUtils";
import {
  buildPluginContextItems,
  buildPluginVisibilityDiagnostics,
  getVisiblePluginActions,
} from "../../utils/pluginActions";
import {
  mergeExcludePatterns,
  readProjectExcludePatterns,
  shouldExcludeFile,
} from "../../utils/excludePatterns";
import { useResolvedImageSource } from "../image-viewer/useResolvedImageSource";
import { normalizeMdtReferenceKey } from "../../utils/mdt";
import { cacheResolvedPreviewThumbnail } from "./thumbnailCache";
import { isVirtualFile } from "../../utils/collections";

function formatSize(bytes: number): string {
  if (bytes === 0) return "-";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }
  return `${size.toFixed(1)} ${units[unitIndex]}`;
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) return "-";
  const date = new Date(dateStr);
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function getFileIcon(file: FileInfo) {
  if (file.entry_kind === "manual_collection") {
    return <Layers className="w-5 h-5 text-violet-500" />;
  }

  if (file.entry_kind === "image_sequence") {
    return <Film className="w-5 h-5 text-teal-500" />;
  }

  if (file.is_dir) {
    return <FolderIcon className="w-5 h-5 text-yellow-500" />;
  }

  const ext = file.extension?.toLowerCase();

  if (isImageExtension(ext)) {
    return <Image className="w-5 h-5 text-purple-500" />;
  }

  if (isVideoExtension(ext)) {
    return <Film className="w-5 h-5 text-red-500" />;
  }

  if (ext === "blend") {
    return <Box className="w-5 h-5 text-orange-500" />;
  }

  if (isTextExtension(ext)) {
    return <FileText className="w-5 h-5 text-blue-500" />;
  }

  return <FileIcon className="w-5 h-5 text-gray-400" />;
}

function resolvePreviewSource(path: string | null) {
  if (!path) {
    return null;
  }

  if (/^(asset|https?|data|blob):/i.test(path)) {
    return path;
  }

  return convertFileSrc(path);
}

function getGridPreview(file: FileInfo): { kind: "image" | "video"; src: string } | null {
  if (file.is_dir) {
    return null;
  }

  if (file.thumbnail) {
    const src = resolvePreviewSource(file.thumbnail);
    return src ? { kind: "image", src } : null;
  }

  const extension = file.extension?.toLowerCase() || "";

  if (isImageExtension(extension) && isDirectPreviewImageExtension(extension)) {
    const src = resolvePreviewSource(file.path);
    return src ? { kind: "image", src } : null;
  }

  if (extension === "psd") {
    return { kind: "image", src: file.path };
  }

  if (extension === "blend") {
    return { kind: "image", src: file.path };
  }

  if (isVideoExtension(extension)) {
    const src = resolvePreviewSource(file.path);
    return src ? { kind: "video", src } : null;
  }

  return null;
}

const MIN_COLUMN_WIDTHS: Record<string, number> = {
  name: 220,
  size: 90,
  modified: 150,
  type: 100,
  tags: 120,
};

const LIST_ROW_HEIGHT = 40;
const LIST_OVERSCAN_COUNT = 12;
const GRID_CARD_HEIGHT = 212;
const GRID_PREVIEW_HEIGHT = "85%";
const GRID_GAP = 16;
const GRID_ROW_HEIGHT = GRID_CARD_HEIGHT + GRID_GAP;
const GRID_OVERSCAN_ROWS = 2;
const SYSTEM_CONTEXT_DOUBLE_TRIGGER_MS = 350;

function clampColumnWidth(key: string, width: number) {
  return Math.max(MIN_COLUMN_WIDTHS[key] ?? 80, Math.round(width));
}

function getGridColumnCount(width: number): number {
  if (width >= 1280) return 6;
  if (width >= 1024) return 5;
  if (width >= 768) return 4;
  if (width >= 640) return 3;
  return 2;
}

type ResolveFileTags = (filePath: string) => Tag[];
type ResolveRelatedMdtCount = (filePath: string) => number;
interface CollectionMemberUpdate {
  collection_id: string;
  added_count: number;
  already_present_count: number;
  item_count: number;
}
interface ProjectCollectionOption {
  id: string;
  name: string;
  item_count: number;
}
type SelectFileHandler = (
  path: string,
  multi: boolean,
  range: boolean,
) => void;

const ListRow = memo(function ListRow({
  file,
  visibleColumns,
  selectedFiles,
  dropTargetPath,
  showExcludedFiles,
  isExcluded,
  resolveFileTags,
  resolveRelatedMdtCount,
  suppressInteraction,
  onSelect,
  onDoubleClick,
  onContextMenu,
  onDragStart,
  onDragEnd,
  getExternalDragPaths,
  onDropToDirectory,
  onDropToCollection,
  onHoverDirectory,
  canDropToDirectory,
  canDropToCollection,
  getDraggedPathsFromDataTransfer,
}: {
  file: FileInfo;
  visibleColumns: ColumnConfig[];
  selectedFiles: Set<string>;
  dropTargetPath: string | null;
  showExcludedFiles: boolean;
  isExcluded: (file: FileInfo) => boolean;
  resolveFileTags: ResolveFileTags;
  resolveRelatedMdtCount: ResolveRelatedMdtCount;
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  onSelect: SelectFileHandler;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (
    file: FileInfo,
    x: number,
    y: number,
  ) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  getExternalDragPaths: (file: FileInfo) => string[];
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onDropToCollection: (collection: FileInfo, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  canDropToCollection: (collection: FileInfo, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (
    dataTransfer: DataTransfer | null,
  ) => string[];
}) {
  const isSelected = selectedFiles.has(file.path);
  const fileTagList = resolveFileTags(file.path);
  const relatedMdtCount = resolveRelatedMdtCount(file.path);
  const isManualCollection = file.entry_kind === "manual_collection";
  const isDropTarget = (file.is_dir || isManualCollection) && dropTargetPath === file.path;
  const excluded = isExcluded(file);
  const externalDragZoneToneClass = isSelected
    ? "border-blue-500/25 bg-blue-950/18 text-blue-800 shadow-[inset_0_1px_0_rgba(255,255,255,0.24)] dark:border-blue-300/20 dark:bg-blue-950/35 dark:text-blue-100"
    : "border-slate-950/10 bg-slate-950/10 text-slate-600 shadow-[inset_0_1px_0_rgba(255,255,255,0.18)] dark:border-white/10 dark:bg-black/25 dark:text-slate-300";

  return (
    <div
      draggable={!isVirtualFile(file)}
      style={{ height: LIST_ROW_HEIGHT }}
      className={`
        group relative flex min-w-max items-center border-b border-gray-100 dark:border-gray-800
        cursor-pointer select-none transition-colors
        ${
          isSelected
            ? "bg-blue-100/90 dark:bg-blue-950/45 text-blue-950 dark:text-blue-50 ring-1 ring-inset ring-blue-500/70 shadow-[inset_4px_0_0_0_#2563eb] dark:shadow-[inset_4px_0_0_0_#60a5fa]"
            : "hover:bg-gray-50 dark:hover:bg-gray-800/50"
        }
        ${isDropTarget ? "ring-2 ring-inset ring-blue-500 bg-blue-50" : ""}
        ${showExcludedFiles && excluded ? "opacity-70" : ""}
      `}
      onClick={(e) => {
        if (suppressInteraction(e)) return;
        onSelect(file.path, e.ctrlKey || e.metaKey, e.shiftKey);
      }}
      onDoubleClick={(e) => {
        if (suppressInteraction(e)) return;
        onDoubleClick(file, e.ctrlKey || e.metaKey);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(file, e.clientX, e.clientY);
      }}
      onDragStart={(e) => {
        if (isVirtualFile(file)) {
          e.preventDefault();
          return;
        }
        onDragStart(file, e);
      }}
      onDragEnd={onDragEnd}
      onDragOver={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        const canDropOnCollection = isManualCollection && canDropToCollection(file, internalDragPaths);
        const canDropOnDirectory = !isVirtualFile(file) && file.is_dir && canDropToDirectory(file.path, internalDragPaths);
        if (!canDropOnCollection && !canDropOnDirectory)
          return;
        e.preventDefault();
        e.dataTransfer.dropEffect = canDropOnCollection ? "copy" : "move";
        onHoverDirectory(file.path);
      }}
      onDragEnter={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        const canDropOnCollection = isManualCollection && canDropToCollection(file, internalDragPaths);
        const canDropOnDirectory = !isVirtualFile(file) && file.is_dir && canDropToDirectory(file.path, internalDragPaths);
        if (!canDropOnCollection && !canDropOnDirectory)
          return;
        e.preventDefault();
        onHoverDirectory(file.path);
      }}
      onDrop={async (e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        const canDropOnCollection = isManualCollection && canDropToCollection(file, internalDragPaths);
        const canDropOnDirectory = !isVirtualFile(file) && file.is_dir && canDropToDirectory(file.path, internalDragPaths);
        if (!canDropOnCollection && !canDropOnDirectory)
          return;
        e.preventDefault();
        e.stopPropagation();
        if (canDropOnCollection) {
          await onDropToCollection(file, internalDragPaths);
        } else {
          await onDropToDirectory(file.path, internalDragPaths);
        }
      }}
    >
      {isSelected && (
        <div className="absolute left-0 top-1/2 h-6 w-1 -translate-y-1/2 rounded-r-full bg-blue-600 dark:bg-blue-400" />
      )}

      {visibleColumns.map((col) => (
        <div
          key={col.key}
          className={`shrink-0 px-3 py-2 text-sm truncate ${
            isSelected
              ? "text-blue-950 dark:text-blue-50"
              : "text-gray-700 dark:text-gray-200"
          }`}
          style={{ width: col.width, textAlign: col.align || "left" }}
        >
          {col.key === "name" && (
            <div className="flex min-w-0 items-center gap-2">
              {getFileIcon(file)}
              <div className="relative min-w-0 flex-1">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className={`min-w-0 flex-1 truncate ${isSelected ? "font-semibold" : ""}`}
                  >
                    {file.name}
                  </span>
                  {showExcludedFiles && excluded && (
                    <span className="shrink-0 rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">
                      已排除
                    </span>
                  )}
                  {relatedMdtCount > 0 && (
                    <span className="shrink-0 rounded-full bg-sky-100 px-1.5 py-0.5 text-[10px] text-sky-700 dark:bg-sky-900/30 dark:text-sky-200">
                      MDT {relatedMdtCount}
                    </span>
                  )}
                  {isManualCollection && isDropTarget && (
                    <span className="shrink-0 rounded-full bg-violet-600 px-1.5 py-0.5 text-[10px] font-medium text-white dark:bg-violet-500">
                      松开加入
                    </span>
                  )}
                </div>
                <ExternalDragHandle
                  resolvePaths={() => getExternalDragPaths(file)}
                  className={`absolute inset-y-0 right-0 flex w-[35%] translate-x-2 items-center justify-center overflow-hidden rounded-md border px-3 opacity-0 pointer-events-none transition-all duration-200 group-hover:pointer-events-auto group-hover:translate-x-0 group-hover:opacity-100 ${externalDragZoneToneClass}`}
                >
                  <span
                    className="pointer-events-none absolute inset-0 bg-gradient-to-r from-transparent via-slate-950/8 to-slate-950/22 dark:via-black/10 dark:to-black/35"
                    draggable={false}
                  />
                  <span
                    className="pointer-events-none absolute inset-y-1 left-0 w-px bg-white/20 dark:bg-white/10"
                    draggable={false}
                  />
                  <span
                    className="pointer-events-none relative flex items-center gap-1.5 whitespace-nowrap text-[11px] font-semibold"
                    draggable={false}
                  >
                    <span
                      className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-white/16 text-current dark:bg-white/10"
                      draggable={false}
                    >
                      <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        className="h-3.5 w-3.5"
                      >
                        <path d="M15 3h6v6" />
                        <path d="M10 14 21 3" />
                        <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                      </svg>
                    </span>
                    <span>拖出</span>
                  </span>
                </ExternalDragHandle>
              </div>
            </div>
          )}
          {col.key === "size" && formatSize(file.size)}
          {col.key === "modified" && formatDate(file.modified)}
          {col.key === "type" &&
            (file.entry_kind === "manual_collection"
              ? "集合"
              : file.entry_kind === "image_sequence"
                ? `图片序列 ${file.sequence?.frame_count ?? file.item_count ?? 0} 帧`
                : file.is_dir ? "文件夹" : file.extension?.toUpperCase() || "文件")}
          {col.key === "tags" && (
            <div className="flex gap-1 flex-wrap">
              {fileTagList.map((tag) => (
                <span
                  key={tag.id}
                  className="px-1.5 py-0.5 text-xs rounded"
                  style={{
                    backgroundColor: `${tag.color}20`,
                    color: tag.color,
                  }}
                >
                  {tag.name}
                </span>
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  );
});

function ListView({
  files,
  selectedFiles,
  onSelect,
  onDoubleClick,
  onContextMenu,
  onBackgroundContextMenu,
  onDragStart,
  onDragEnd,
  getExternalDragPaths,
  onDropToDirectory,
  onDropToCollection,
  onHoverDirectory,
  canDropToDirectory,
  canDropToCollection,
  getDraggedPathsFromDataTransfer,
  suppressInteraction,
  dropTargetPath,
  currentPath,
  columns,
  resizingColumnKey,
  onStartColumnResize,
  isExcluded,
  showExcludedFiles,
  resolveFileTags,
  resolveRelatedMdtCount,
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: SelectFileHandler;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (
    file: FileInfo,
    x: number,
    y: number,
  ) => void;
  onBackgroundContextMenu: (x: number, y: number) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  getExternalDragPaths: (file: FileInfo) => string[];
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onDropToCollection: (collection: FileInfo, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  canDropToCollection: (collection: FileInfo, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (
    dataTransfer: DataTransfer | null,
  ) => string[];
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  dropTargetPath: string | null;
  currentPath: string;
  columns: ColumnConfig[];
  resizingColumnKey: string | null;
  onStartColumnResize: (
    key: string,
    width: number,
    event: React.MouseEvent<HTMLDivElement>,
  ) => void;
  isExcluded: (file: FileInfo) => boolean;
  showExcludedFiles: boolean;
  resolveFileTags: ResolveFileTags;
  resolveRelatedMdtCount: ResolveRelatedMdtCount;
}) {
  const visibleColumns = columns.filter((col) => col.visible);
  const tableMinWidth = visibleColumns.reduce((sum, col) => sum + col.width, 0);
  const headerScrollRef = useRef<HTMLDivElement>(null);
  const bodyScrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);

  useEffect(() => {
    const bodyElement = bodyScrollRef.current;
    if (!bodyElement) {
      return;
    }

    const updateViewport = () => {
      setViewportHeight(bodyElement.clientHeight);
    };

    updateViewport();
    const observer = new ResizeObserver(updateViewport);
    observer.observe(bodyElement);

    return () => {
      observer.disconnect();
    };
  }, []);

  const handleBodyScroll = useCallback(
    (event: React.UIEvent<HTMLDivElement>) => {
      const target = event.currentTarget;
      setScrollTop(target.scrollTop);
      if (headerScrollRef.current) {
        headerScrollRef.current.scrollLeft = target.scrollLeft;
      }
    },
    [],
  );

  const visibleCount = Math.max(1, Math.ceil(viewportHeight / LIST_ROW_HEIGHT));
  const startIndex = Math.max(
    0,
    Math.floor(scrollTop / LIST_ROW_HEIGHT) - LIST_OVERSCAN_COUNT,
  );
  const endIndex = Math.min(
    files.length,
    startIndex + visibleCount + LIST_OVERSCAN_COUNT * 2,
  );
  const offsetY = startIndex * LIST_ROW_HEIGHT;
  const visibleFiles = files.slice(startIndex, endIndex);

  return (
    <div className="flex flex-col h-full">
      <div
        ref={headerScrollRef}
        className="overflow-hidden border-b border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-800"
      >
        <div className="flex min-w-max" style={{ minWidth: tableMinWidth }}>
          {visibleColumns.map((col, index) => {
            const isLastVisibleColumn = index === visibleColumns.length - 1;
            const isResizing = resizingColumnKey === col.key;
            return (
              <div
                key={col.key}
                className="relative shrink-0 px-3 py-2 text-xs font-medium uppercase tracking-wider text-gray-500"
                style={{ width: col.width, textAlign: col.align || "left" }}
              >
                {col.title}
                {!isLastVisibleColumn && (
                  <div
                    onMouseDown={(event) =>
                      onStartColumnResize(col.key, col.width, event)
                    }
                    className="absolute inset-y-0 -right-1 z-10 flex w-2 cursor-col-resize items-center justify-center"
                    title="拖动调整列宽"
                  >
                    <div
                      className={`h-5 w-px transition-colors ${
                        isResizing
                          ? "bg-blue-500"
                          : "bg-gray-200 dark:bg-gray-700 hover:bg-blue-400 dark:hover:bg-blue-500"
                      }`}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      <div
        ref={bodyScrollRef}
        className={`flex-1 overflow-auto ${dropTargetPath === currentPath ? "bg-blue-50/60" : ""}`}
        onScroll={handleBodyScroll}
        onContextMenu={(e) => {
          if (e.target !== e.currentTarget) return;
          e.preventDefault();
          onBackgroundContextMenu(e.clientX, e.clientY);
        }}
        onDragOver={(e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(
            e.dataTransfer,
          );
          if (!canDropToDirectory(currentPath, internalDragPaths)) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          onHoverDirectory(currentPath);
        }}
        onDrop={async (e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(
            e.dataTransfer,
          );
          if (!canDropToDirectory(currentPath, internalDragPaths)) return;
          e.preventDefault();
          await onDropToDirectory(currentPath, internalDragPaths);
        }}
      >
        {files.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-400">
            当前目录没有可显示的文件
          </div>
        ) : (
          <div
            style={{
              height: files.length * LIST_ROW_HEIGHT,
              minWidth: tableMinWidth,
              position: "relative",
            }}
          >
            <div
              style={{ position: "absolute", left: 0, right: 0, top: offsetY }}
            >
              {visibleFiles.map((file) => (
                <ListRow
                  key={file.path}
                  file={file}
                  visibleColumns={visibleColumns}
                  selectedFiles={selectedFiles}
                  dropTargetPath={dropTargetPath}
                  showExcludedFiles={showExcludedFiles}
                  isExcluded={isExcluded}
                  resolveFileTags={resolveFileTags}
                  resolveRelatedMdtCount={resolveRelatedMdtCount}
                  suppressInteraction={suppressInteraction}
                  onSelect={onSelect}
                  onDoubleClick={onDoubleClick}
                  onContextMenu={onContextMenu}
                  onDragStart={onDragStart}
                  onDragEnd={onDragEnd}
                  getExternalDragPaths={getExternalDragPaths}
                  onDropToDirectory={onDropToDirectory}
                  onDropToCollection={onDropToCollection}
                  onHoverDirectory={onHoverDirectory}
                  canDropToDirectory={canDropToDirectory}
                  canDropToCollection={canDropToCollection}
                  getDraggedPathsFromDataTransfer={
                    getDraggedPathsFromDataTransfer
                  }
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

const GridCard = memo(function GridCard({
  file,
  selectedFiles,
  dropTargetPath,
  showExcludedFiles,
  isExcluded,
  resolveFileTags,
  resolveRelatedMdtCount,
  suppressInteraction,
  onSelect,
  onDoubleClick,
  onContextMenu,
  onDragStart,
  onDragEnd,
  getExternalDragPaths,
  onDropToDirectory,
  onDropToCollection,
  onHoverDirectory,
  canDropToDirectory,
  canDropToCollection,
  getDraggedPathsFromDataTransfer,
}: {
  file: FileInfo;
  selectedFiles: Set<string>;
  dropTargetPath: string | null;
  showExcludedFiles: boolean;
  isExcluded: (file: FileInfo) => boolean;
  resolveFileTags: ResolveFileTags;
  resolveRelatedMdtCount: ResolveRelatedMdtCount;
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  onSelect: SelectFileHandler;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (
    file: FileInfo,
    x: number,
    y: number,
  ) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  getExternalDragPaths: (file: FileInfo) => string[];
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onDropToCollection: (collection: FileInfo, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  canDropToCollection: (collection: FileInfo, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (
    dataTransfer: DataTransfer | null,
  ) => string[];
}) {
  const isSelected = selectedFiles.has(file.path);
  const fileTagList = resolveFileTags(file.path);
  const relatedMdtCount = resolveRelatedMdtCount(file.path);
  const isManualCollection = file.entry_kind === "manual_collection";
  const isDropTarget = (file.is_dir || isManualCollection) && dropTargetPath === file.path;
  const excluded = isExcluded(file);
  const externalDragHandleVisibilityClass = isSelected
    ? "opacity-100"
    : "opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto";

  return (
    <div
      draggable={!isVirtualFile(file)}
      className={`
        group relative flex h-full flex-col p-3 rounded-lg cursor-pointer select-none transition-all
        ${
          isSelected
            ? "bg-blue-100 dark:bg-blue-950/40 ring-2 ring-blue-500 shadow-lg shadow-blue-500/10 dark:shadow-blue-950/30"
            : "hover:bg-gray-50 dark:hover:bg-gray-800"
        }
        ${isDropTarget ? "ring-2 ring-blue-500 bg-blue-50" : ""}
        ${showExcludedFiles && excluded ? "opacity-70" : ""}
      `}
      onClick={(e) => {
        if (suppressInteraction(e)) return;
        onSelect(file.path, e.ctrlKey || e.metaKey, e.shiftKey);
      }}
      onDoubleClick={(e) => {
        if (suppressInteraction(e)) return;
        onDoubleClick(file, e.ctrlKey || e.metaKey);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(file, e.clientX, e.clientY);
      }}
      onDragStart={(e) => {
        if (isVirtualFile(file)) {
          e.preventDefault();
          return;
        }
        onDragStart(file, e);
      }}
      onDragEnd={onDragEnd}
      onDragOver={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        const canDropOnCollection = isManualCollection && canDropToCollection(file, internalDragPaths);
        const canDropOnDirectory = !isVirtualFile(file) && file.is_dir && canDropToDirectory(file.path, internalDragPaths);
        if (!canDropOnCollection && !canDropOnDirectory)
          return;
        e.preventDefault();
        e.dataTransfer.dropEffect = canDropOnCollection ? "copy" : "move";
        onHoverDirectory(file.path);
      }}
      onDragEnter={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        const canDropOnCollection = isManualCollection && canDropToCollection(file, internalDragPaths);
        const canDropOnDirectory = !isVirtualFile(file) && file.is_dir && canDropToDirectory(file.path, internalDragPaths);
        if (!canDropOnCollection && !canDropOnDirectory)
          return;
        e.preventDefault();
        onHoverDirectory(file.path);
      }}
      onDrop={async (e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        const canDropOnCollection = isManualCollection && canDropToCollection(file, internalDragPaths);
        const canDropOnDirectory = !isVirtualFile(file) && file.is_dir && canDropToDirectory(file.path, internalDragPaths);
        if (!canDropOnCollection && !canDropOnDirectory)
          return;
        e.preventDefault();
        e.stopPropagation();
        if (canDropOnCollection) {
          await onDropToCollection(file, internalDragPaths);
        } else {
          await onDropToDirectory(file.path, internalDragPaths);
        }
      }}
    >
      <div className="absolute right-2 top-2 flex items-center gap-1">
        {isSelected && (
          <div className="rounded-full bg-blue-600 px-1.5 py-0.5 text-[10px] font-medium text-white dark:bg-blue-500">
            已选中
          </div>
        )}
        {isManualCollection && isDropTarget && (
          <div className="rounded-full bg-violet-600 px-1.5 py-0.5 text-[10px] font-medium text-white dark:bg-violet-500">
            松开加入
          </div>
        )}
        {relatedMdtCount > 0 && (
          <div className="rounded-full bg-sky-600 px-1.5 py-0.5 text-[10px] font-medium text-white dark:bg-sky-500">
            MDT {relatedMdtCount}
          </div>
        )}
        {!isVirtualFile(file) && (
          <ExternalDragHandle
            resolvePaths={() => getExternalDragPaths(file)}
            className={`inline-flex h-7 w-7 items-center justify-center rounded-full bg-white/90 text-gray-500 shadow-sm ring-1 ring-black/5 transition hover:bg-white hover:text-gray-800 dark:bg-gray-900/85 dark:text-gray-300 dark:ring-white/10 dark:hover:bg-gray-800 dark:hover:text-gray-100 ${externalDragHandleVisibilityClass}`}
            iconClassName="h-4 w-4"
          />
        )}
      </div>

      <div
        className={`mb-2 flex shrink-0 items-center justify-center overflow-hidden rounded-xl transition-colors ${
          isSelected ? "bg-blue-200/70 dark:bg-blue-900/50" : ""
        }`}
        style={{ height: GRID_PREVIEW_HEIGHT }}
      >
        <GridCardPreview file={file} />
      </div>

      <div
        className={`shrink-0 text-sm text-center truncate ${
          isSelected ? "font-semibold text-blue-950 dark:text-blue-50" : ""
        }`}
        title={file.name}
      >
        {file.name}
      </div>
      {showExcludedFiles && excluded && (
        <div className="mt-1 text-center">
          <span className="rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">
            已排除
          </span>
        </div>
      )}

      {fileTagList.length > 0 && (
        <div className="mt-1 flex shrink-0 justify-center gap-1 flex-wrap">
          {fileTagList.slice(0, 2).map((tag) => (
            <span
              key={tag.id}
              className="w-2 h-2 rounded-full"
              style={{ backgroundColor: tag.color }}
              title={tag.name}
            />
          ))}
          {fileTagList.length > 2 && (
            <span className="text-xs text-gray-400">
              +{fileTagList.length - 2}
            </span>
          )}
        </div>
      )}
    </div>
  );
});

const GridCardPreview = memo(function GridCardPreview({
  file,
}: {
  file: FileInfo;
}) {
  const projectPath = useProjectStoreShallow((state) => state.projectPath);
  const preview = useMemo(() => getGridPreview(file), [file]);
  const {
    resolvedSource,
    isLoading: isImageLoading,
    errorMessage: imageErrorMessage,
  } = useResolvedImageSource(preview?.kind === "image" ? preview.src : "");
  const [hasPreviewError, setHasPreviewError] = useState(false);

  useEffect(() => {
    setHasPreviewError(false);
  }, [preview?.kind, preview?.src, file.path]);

  useEffect(() => {
    if (preview?.kind === "image" && imageErrorMessage) {
      setHasPreviewError(true);
    }
  }, [imageErrorMessage, preview?.kind]);

  useEffect(() => {
    if (preview?.kind !== "image" || !resolvedSource) {
      return;
    }

    void cacheResolvedPreviewThumbnail(projectPath, file, resolvedSource);
  }, [file, preview?.kind, projectPath, resolvedSource]);

  if (!preview || hasPreviewError) {
    return (
      <div className="flex h-full w-full flex-col items-center justify-center gap-2 text-gray-400">
        {getFileIcon(file)}
        {file.entry_kind === "image_sequence" && (
          <span className="max-w-full truncate text-xs text-gray-500">
            {file.sequence?.frame_count ?? file.item_count ?? 0} 帧
          </span>
        )}
      </div>
    );
  }

  if (preview.kind === "image") {
    if (!resolvedSource) {
      return (
        <div className="flex h-full w-full items-center justify-center">
          {isImageLoading ? (
            <div className="h-12 w-12 animate-pulse rounded-2xl bg-white/50 dark:bg-white/10" />
          ) : (
            getFileIcon(file)
          )}
        </div>
      );
    }

    return (
      <img
        src={resolvedSource}
        alt={file.name}
        className="h-full w-full object-cover"
        loading="lazy"
        onError={() => setHasPreviewError(true)}
      />
    );
  }

  return (
    <video
      src={preview.src}
      className="h-full w-full object-cover"
      preload="metadata"
      muted
      playsInline
      onError={() => setHasPreviewError(true)}
    />
  );
});

function GridView({
  files,
  selectedFiles,
  onSelect,
  onDoubleClick,
  onContextMenu,
  onBackgroundContextMenu,
  onDragStart,
  onDragEnd,
  getExternalDragPaths,
  onDropToDirectory,
  onDropToCollection,
  onHoverDirectory,
  canDropToDirectory,
  canDropToCollection,
  getDraggedPathsFromDataTransfer,
  suppressInteraction,
  dropTargetPath,
  currentPath,
  isExcluded,
  showExcludedFiles,
  resolveFileTags,
  resolveRelatedMdtCount,
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: SelectFileHandler;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (
    file: FileInfo,
    x: number,
    y: number,
  ) => void;
  onBackgroundContextMenu: (x: number, y: number) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  getExternalDragPaths: (file: FileInfo) => string[];
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onDropToCollection: (collection: FileInfo, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  canDropToCollection: (collection: FileInfo, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (
    dataTransfer: DataTransfer | null,
  ) => string[];
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  dropTargetPath: string | null;
  currentPath: string;
  isExcluded: (file: FileInfo) => boolean;
  showExcludedFiles: boolean;
  resolveFileTags: ResolveFileTags;
  resolveRelatedMdtCount: ResolveRelatedMdtCount;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [viewportHeight, setViewportHeight] = useState(0);
  const [containerWidth, setContainerWidth] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const updateViewport = () => {
      setViewportHeight(container.clientHeight);
      setContainerWidth(container.clientWidth);
    };

    updateViewport();
    const observer = new ResizeObserver(updateViewport);
    observer.observe(container);

    return () => {
      observer.disconnect();
    };
  }, []);

  const columnCount = getGridColumnCount(containerWidth);
  const rowCount = Math.max(1, Math.ceil(files.length / columnCount));
  const visibleRowCount = Math.max(
    1,
    Math.ceil(viewportHeight / GRID_ROW_HEIGHT),
  );
  const startRow = Math.max(
    0,
    Math.floor(scrollTop / GRID_ROW_HEIGHT) - GRID_OVERSCAN_ROWS,
  );
  const endRow = Math.min(
    rowCount,
    startRow + visibleRowCount + GRID_OVERSCAN_ROWS * 2,
  );
  const startIndex = startRow * columnCount;
  const endIndex = Math.min(files.length, endRow * columnCount);
  const visibleFiles = files.slice(startIndex, endIndex);
  const renderedRows = Math.ceil(visibleFiles.length / columnCount);
  const topSpacer = startRow * GRID_ROW_HEIGHT;
  const bottomSpacer = Math.max(
    0,
    (rowCount - startRow - renderedRows) * GRID_ROW_HEIGHT,
  );

  return (
    <div
      ref={containerRef}
      className={`h-full overflow-auto px-4 py-4 ${dropTargetPath === currentPath ? "bg-blue-50/60" : ""}`}
      onScroll={(e) => {
        setScrollTop(e.currentTarget.scrollTop);
      }}
      onContextMenu={(e) => {
        if (e.target !== e.currentTarget) return;
        e.preventDefault();
        onBackgroundContextMenu(e.clientX, e.clientY);
      }}
        onDragOver={(e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(
            e.dataTransfer,
          );
          if (!canDropToDirectory(currentPath, internalDragPaths)) return;
          e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        onHoverDirectory(currentPath);
      }}
        onDrop={async (e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(
            e.dataTransfer,
          );
          if (!canDropToDirectory(currentPath, internalDragPaths)) return;
          e.preventDefault();
          await onDropToDirectory(currentPath, internalDragPaths);
      }}
    >
      {files.length === 0 ? (
        <div className="flex h-full items-center justify-center text-sm text-gray-400">
          当前目录没有可显示的文件
        </div>
      ) : (
        <>
          <div style={{ height: topSpacer }} />
          <div
            className="grid"
            style={{
              gridTemplateColumns: `repeat(${columnCount}, minmax(0, 1fr))`,
              gap: GRID_GAP,
              gridAutoRows: GRID_CARD_HEIGHT,
            }}
          >
            {visibleFiles.map((file) => (
              <GridCard
                key={file.path}
                file={file}
                selectedFiles={selectedFiles}
                dropTargetPath={dropTargetPath}
                showExcludedFiles={showExcludedFiles}
                isExcluded={isExcluded}
                resolveFileTags={resolveFileTags}
                resolveRelatedMdtCount={resolveRelatedMdtCount}
                suppressInteraction={suppressInteraction}
                onSelect={onSelect}
                onDoubleClick={onDoubleClick}
                onContextMenu={onContextMenu}
                onDragStart={onDragStart}
                onDragEnd={onDragEnd}
                getExternalDragPaths={getExternalDragPaths}
                  onDropToDirectory={onDropToDirectory}
                  onDropToCollection={onDropToCollection}
                  onHoverDirectory={onHoverDirectory}
                  canDropToDirectory={canDropToDirectory}
                  canDropToCollection={canDropToCollection}
                getDraggedPathsFromDataTransfer={
                  getDraggedPathsFromDataTransfer
                }
              />
            ))}
          </div>
          <div style={{ height: bottomSpacer }} />
        </>
      )}
    </div>
  );
}

interface FileListProps {
  onOpenDirectoryTab?: (path: string) => Promise<void> | void;
  onRemoveFromCollection?: (memberPaths: string[]) => Promise<void> | void;
}

export function FileList({
  onOpenDirectoryTab,
  onRemoveFromCollection,
}: FileListProps = {}) {
  const projectStore = useProjectStoreApi();
  const {
    files,
    selectedFiles,
    viewMode,
    columns,
    updateColumn,
    tags,
    fileTags,
    selectFile,
    clearSelection,
    loadDirectory,
    refresh,
    currentPath,
    searchResults,
    isSearching,
    searchQuery,
    projectPath,
    showExcludedFiles,
    mdtReferencesByFile,
  } = useProjectStoreShallow((state) => ({
    files: state.files,
    selectedFiles: state.selectedFiles,
    viewMode: state.viewMode,
    columns: state.columns,
    updateColumn: state.updateColumn,
    tags: state.tags,
    fileTags: state.fileTags,
    selectFile: state.selectFile,
    clearSelection: state.clearSelection,
    loadDirectory: state.loadDirectory,
    refresh: state.refresh,
    currentPath: state.currentPath,
    searchResults: state.searchResults,
    isSearching: state.isSearching,
    searchQuery: state.searchQuery,
    projectPath: state.projectPath,
    showExcludedFiles: state.showExcludedFiles,
    mdtReferencesByFile: state.mdtReferencesByFile,
  }));
  const showToast = useUiStore((state) => state.showToast);
  const addTask = useTaskStore((state) => state.addTask);
  const globalExcludePatterns = useSettingsStore(
    (state) => state.globalExcludePatterns,
  );
  const openFileInTab = useWorkspaceTabStore((state) => state.openFileInTab);
  const openFileInStandaloneWindow = useWorkspaceTabStore(
    (state) => state.openFileInStandaloneWindow,
  );
  const openCollectionInTab = useWorkspaceTabStore(
    (state) => state.openCollectionInTab,
  );
  const pluginProjectKey = projectPath || "__global__";
  const pluginState = usePluginStore(
    (state) => state.byProject[pluginProjectKey],
  );
  const loadPlugins = usePluginStore((state) => state.loadPlugins);
  const {
    draggedPaths,
    startInternalDrag,
    finishInternalDrag,
    suppressInteraction,
    getDraggedPathsFromDataTransfer,
  } = useInternalFileDrag();
  const [dropTargetPath, setDropTargetPath] = useState<string | null>(null);
  const hoverFrameRef = useRef<number | null>(null);
  const pendingHoverTargetRef = useRef<string | null>(null);
  const { movePathsToDirectory, conflictDialog } = useFileDropMove(async () => {
    await refresh();
  });

  const [contextMenu, setContextMenu] = useState<
    | { kind: "file"; file: FileInfo; x: number; y: number }
    | { kind: "directory"; x: number; y: number }
    | null
  >(null);
  const lastFileContextMenuTriggerRef = useRef<{
    path: string;
    timestamp: number;
  } | null>(null);
  const [detailsDialogFile, setDetailsDialogFile] = useState<FileInfo | null>(
    null,
  );
  const [createFolderDialog, setCreateFolderDialog] = useState({
    isOpen: false,
    suggestedName: "",
    folderName: "",
  });
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [collectionDialog, setCollectionDialog] = useState<{
    mode: "create" | "rename" | null;
    isOpen: boolean;
    name: string;
    memberPaths: string[];
    collectionId: string | null;
  }>({
    mode: null,
    isOpen: false,
    name: "",
    memberPaths: [],
    collectionId: null,
  });
  const [isSavingCollection, setIsSavingCollection] = useState(false);
  const [deleteCollectionDialog, setDeleteCollectionDialog] = useState<{
    isOpen: boolean;
    collectionId: string | null;
    name: string;
  }>({
    isOpen: false,
    collectionId: null,
    name: "",
  });
  const [addToCollectionDialog, setAddToCollectionDialog] = useState<{
    isOpen: boolean;
    collectionId: string | null;
    memberPaths: string[];
  }>({
    isOpen: false,
    collectionId: null,
    memberPaths: [],
  });
  const [isAddingToCollection, setIsAddingToCollection] = useState(false);
  const [collectionPickerOptions, setCollectionPickerOptions] = useState<ProjectCollectionOption[]>([]);
  const [resizingColumnKey, setResizingColumnKey] = useState<string | null>(
    null,
  );
  const columnResizeStateRef = useRef<{
    key: string;
    startX: number;
    startWidth: number;
  } | null>(null);

  const displayFiles = searchQuery ? searchResults : files;
  const displayFilePaths = useMemo(
    () => displayFiles.map((file) => file.path),
    [displayFiles],
  );
  const selectionAnchorPathRef = useRef<string | null>(null);
  const excludePatterns = projectPath
    ? mergeExcludePatterns(
        globalExcludePatterns,
        readProjectExcludePatterns(projectPath),
      )
    : [];
  const isExcluded = useCallback(
    (file: FileInfo) => {
      return (
        excludePatterns.length > 0 &&
        shouldExcludeFile(file.name, excludePatterns)
      );
    },
    [excludePatterns],
  );

  const tagById = useMemo(() => {
    return new Map(tags.map((tag) => [tag.id, tag] as const));
  }, [tags]);

  const resolveFileTags = useCallback(
    (filePath: string): Tag[] => {
      const tagIds = fileTags.get(filePath);
      if (!tagIds || tagIds.length === 0) {
        return [];
      }

      const resolved: Tag[] = [];
      for (const tagId of tagIds) {
        const tag = tagById.get(tagId);
        if (tag) {
          resolved.push(tag);
        }
      }
      return resolved;
    },
    [fileTags, tagById],
  );
  const resolveRelatedMdtCount = useCallback(
    (filePath: string) => {
      return mdtReferencesByFile.get(normalizeMdtReferenceKey(filePath))?.length || 0;
    },
    [mdtReferencesByFile],
  );

  const detailsDialogTagList = detailsDialogFile
    ? resolveFileTags(detailsDialogFile.path)
    : [];
  const detailsDialogRelatedMdtEntries = detailsDialogFile
    ? (mdtReferencesByFile.get(normalizeMdtReferenceKey(detailsDialogFile.path)) || [])
    : [];
  const allKnownFiles = useMemo(() => {
    const fileMap = new Map<string, FileInfo>();
    for (const file of [...files, ...searchResults]) {
      fileMap.set(file.path, file);
    }
    return fileMap;
  }, [files, searchResults]);
  const selectedFileInfos = useMemo(() => {
    return Array.from(selectedFiles)
      .map((path) => allKnownFiles.get(path))
      .filter((file): file is FileInfo => Boolean(file));
  }, [allKnownFiles, selectedFiles]);
  const realSelectedFileInfos = useMemo(
    () => selectedFileInfos.filter((file) => !isVirtualFile(file)),
    [selectedFileInfos],
  );
  const canCreateCollectionFromSelection = useMemo(() => {
    if (!currentPath || searchQuery || realSelectedFileInfos.length < 2) {
      return false;
    }

    return realSelectedFileInfos.every(
      (file) => getParentPath(file.path) === currentPath,
    );
  }, [currentPath, realSelectedFileInfos, searchQuery]);
  const canAddSelectionToCollection = Boolean(
    projectPath && realSelectedFileInfos.length > 0,
  );

  useEffect(() => {
    const anchorPath = selectionAnchorPathRef.current;
    if (anchorPath && !displayFilePaths.includes(anchorPath)) {
      selectionAnchorPathRef.current = null;
    }
  }, [displayFilePaths]);

  const handleSelectFile = useCallback<SelectFileHandler>(
    (path, multi, range) => {
      if (range) {
        const anchorPath = selectionAnchorPathRef.current;
        const anchorIndex = anchorPath
          ? displayFilePaths.indexOf(anchorPath)
          : -1;
        const targetIndex = displayFilePaths.indexOf(path);

        if (anchorIndex !== -1 && targetIndex !== -1) {
          const startIndex = Math.min(anchorIndex, targetIndex);
          const endIndex = Math.max(anchorIndex, targetIndex);
          const rangePaths = displayFilePaths.slice(startIndex, endIndex + 1);

          projectStore.setState((state) => {
            const nextSelection = multi
              ? new Set(state.selectedFiles)
              : new Set<string>();

            for (const rangePath of rangePaths) {
              nextSelection.add(rangePath);
            }

            return { selectedFiles: nextSelection };
          });
          return;
        }
      }

      selectFile(path, multi);
      selectionAnchorPathRef.current = path;
    },
    [displayFilePaths, projectStore, selectFile],
  );

  useEffect(() => {
    if (!projectPath) {
      return;
    }

    void loadPlugins(projectPath);
  }, [loadPlugins, projectPath]);

  const clearDropHoverState = useCallback(() => {
    pendingHoverTargetRef.current = null;
    if (hoverFrameRef.current !== null) {
      window.cancelAnimationFrame(hoverFrameRef.current);
      hoverFrameRef.current = null;
    }
    setDropTargetPath(null);
  }, []);

  useEffect(() => {
    return () => {
      clearDropHoverState();
    };
  }, [clearDropHoverState]);

  const handleSystemOpenFile = useCallback(
    async (file: FileInfo) => {
      if (file.entry_kind === "manual_collection" && file.collection_id) {
        if (!projectPath || !currentPath) {
          return;
        }

        openCollectionInTab({
          kind: "manual_collection",
          id: file.collection_id,
          title: file.name,
          projectPath,
          directoryPath: file.directory_path || currentPath,
        });
        return;
      }

      if (file.entry_kind === "image_sequence" && file.sequence) {
        openCollectionInTab({
          kind: "image_sequence",
          title: file.name,
          sequence: file.sequence,
        });
        return;
      }

      try {
        await invoke("open_file", { path: file.path });
        showToast({
          title: "已打开",
          message: file.name,
          tone: "success",
        });
      } catch (error) {
        console.error("Failed to open file:", error);
        showToast({
          title: "打开失败",
          message: String(error),
          tone: "error",
        });
      }
    },
    [currentPath, openCollectionInTab, projectPath, showToast],
  );

  const handleOpenDirectoryTab = useCallback(
    async (file: FileInfo) => {
      const targetPath = file.is_dir ? file.path : getParentPath(file.path);
      if (!targetPath) {
        return;
      }

      await onOpenDirectoryTab?.(targetPath);
    },
    [onOpenDirectoryTab],
  );

  const handleDoubleClick = useCallback(
    async (file: FileInfo, openInStandalone: boolean) => {
      if (file.entry_kind === "manual_collection" && file.collection_id) {
        if (!projectPath || !currentPath) {
          return;
        }

        openCollectionInTab({
          kind: "manual_collection",
          id: file.collection_id,
          title: file.name,
          projectPath,
          directoryPath: file.directory_path || currentPath,
        });
        return;
      }

      if (file.entry_kind === "image_sequence" && file.sequence) {
        openCollectionInTab({
          kind: "image_sequence",
          title: file.name,
          sequence: file.sequence,
        });
        return;
      }

      if (file.is_dir) {
        await loadDirectory(file.path);
        return;
      }

      const openTarget = getWorkspaceOpenTarget(file.path);
      if (!openTarget) {
        await handleSystemOpenFile(file);
        return;
      }

      try {
        if (openInStandalone && openTarget !== 'blend') {
          const opened = await openFileInStandaloneWindow(file.path, {
            projectPath: projectPath || undefined,
          });
          if (!opened) {
            await handleSystemOpenFile(file);
          }
          return;
        }

        const tabId = await openFileInTab(file.path);
        if (!tabId && openTarget === 'blend') {
          showToast({
            title: "打开失败",
            message: "当前窗口暂不支持打开 Blender 标签页。",
            tone: "warning",
          });
          return;
        }

        if (!tabId) {
          await handleSystemOpenFile(file);
        }
      } catch (error) {
        console.error("Failed to open in workspace:", error);
        showToast({
          title: "打开失败",
          message: String(error),
          tone: "error",
        });
      }
    },
    [
      handleSystemOpenFile,
      loadDirectory,
      currentPath,
      openCollectionInTab,
      openFileInStandaloneWindow,
      openFileInTab,
      projectPath,
      showToast,
    ],
  );

  const resolveSystemContextMenuPaths = useCallback(
    (file: FileInfo, selectionIncludesTarget: boolean) => {
      if (isVirtualFile(file)) {
        return [];
      }

      if (!selectionIncludesTarget) {
        return [file.path];
      }

      const candidateFiles =
        selectedFileInfos.length > 0 ? selectedFileInfos : [file];
      const realCandidateFiles = candidateFiles.filter((candidate) => !isVirtualFile(candidate));
      if (realCandidateFiles.length === 0) {
        return [];
      }

      const targetParentPath = getParentPath(file.path);
      const canUseSelection = realCandidateFiles.every(
        (candidate) => getParentPath(candidate.path) === targetParentPath,
      );

      return canUseSelection
        ? realCandidateFiles.map((candidate) => candidate.path)
        : [file.path];
    },
    [selectedFileInfos],
  );

  const openSystemContextMenu = useCallback(
    async (file: FileInfo, selectionIncludesTarget: boolean) => {
      try {
        const paths = resolveSystemContextMenuPaths(file, selectionIncludesTarget);
        if (paths.length === 0) {
          return;
        }

        const result = await invoke<{ status: string }>(
          "show_system_context_menu",
          {
            paths,
          },
        );

        if (result.status === "invoked") {
          await refresh();
        }
      } catch (error) {
        console.error("Failed to show system context menu:", error);
        showToast({
          title: "系统右键菜单打开失败",
          message: String(error),
          tone: "error",
        });
      }
    },
    [refresh, resolveSystemContextMenuPaths, showToast],
  );

  const handleContextMenu = useCallback(
    (file: FileInfo, x: number, y: number) => {
      const selectionIncludesTarget = selectedFiles.has(file.path);
      const keepSelectionForCollectionTarget =
        file.entry_kind === "manual_collection" && realSelectedFileInfos.length > 0;
      const now = Date.now();
      const lastTrigger = lastFileContextMenuTriggerRef.current;
      const shouldOpenSystemMenu =
        !isVirtualFile(file) &&
        lastTrigger?.path === file.path &&
        now - lastTrigger.timestamp <= SYSTEM_CONTEXT_DOUBLE_TRIGGER_MS;

      if (!selectionIncludesTarget && !keepSelectionForCollectionTarget) {
        projectStore.setState({
          selectedFiles: new Set([file.path]),
        });
        selectionAnchorPathRef.current = file.path;
      }

      if (shouldOpenSystemMenu) {
        lastFileContextMenuTriggerRef.current = null;
        setContextMenu(null);
        void openSystemContextMenu(file, selectionIncludesTarget);
        return;
      }

      // Two quick right-clicks on the same item switch from app menu to system menu.
      lastFileContextMenuTriggerRef.current = {
        path: file.path,
        timestamp: now,
      };
      setContextMenu({ kind: "file", file, x, y });
    },
    [openSystemContextMenu, projectStore, realSelectedFileInfos.length, selectedFiles],
  );

  const handleBackgroundContextMenu = useCallback(
    (x: number, y: number) => {
      lastFileContextMenuTriggerRef.current = null;
      clearSelection();
      setContextMenu({ kind: "directory", x, y });
    },
    [clearSelection],
  );

  const handleCloseContextMenu = () => {
    setContextMenu(null);
  };

  const buildFileContext = useCallback(
    (selectedItems: FileInfo[]) => ({
      projectPath: projectPath || "",
      currentPath: currentPath || null,
      selectedItems: buildPluginContextItems(selectedItems),
      trigger: "file-context",
      pluginScope: "",
      appVersion: APP_VERSION,
    }),
    [currentPath, projectPath],
  );

  const runPluginAction = useCallback(
    (action: PluginAction, selectedItems: FileInfo[]) => {
      if (!projectPath) {
        return;
      }

      const context = buildFileContext(selectedItems);
      addTask({
        projectPath,
        name: action.title,
        subName: `${action.pluginName} · 右键插件`,
        script: {
          kind: "plugin-action",
          pluginKey: action.pluginKey,
          pluginId: action.pluginId,
          pluginName: action.pluginName,
          commandId: action.commandId,
          commandTitle: action.title,
          location: action.location,
          interactionResponses: [],
          context: {
            ...context,
            pluginScope: action.scope,
          },
        },
        priority: "medium",
        maxRetries: 0,
        timeout: 0,
        dependencies: [],
      });

      showToast({
        title: "插件任务已加入",
        message: `${action.pluginName} · ${action.title}`,
        tone: "success",
      });
    },
    [addTask, buildFileContext, projectPath, showToast],
  );

  const handleShowDetails = useCallback((file: FileInfo) => {
    setDetailsDialogFile(file);
  }, []);

  const handleRemoveFromCollection = useCallback(
    async (file: FileInfo) => {
      if (!onRemoveFromCollection) {
        return;
      }

      const memberPaths = selectedFiles.has(file.path)
        ? realSelectedFileInfos.map((item) => item.path)
        : [file.path];
      await onRemoveFromCollection(memberPaths);
    },
    [onRemoveFromCollection, realSelectedFileInfos, selectedFiles],
  );

  const handleCloseDetailsDialog = useCallback(() => {
    setDetailsDialogFile(null);
  }, []);

  const handleRefresh = useCallback(() => {
    void refresh();
  }, [refresh]);

  const handleOpenCreateCollectionDialog = useCallback(() => {
    if (!canCreateCollectionFromSelection) {
      showToast({
        title: "无法创建集合",
        message: "请选择当前目录中的至少两个真实项目。",
        tone: "warning",
      });
      return;
    }

    const defaultName =
      realSelectedFileInfos[0]?.name
        ? `${realSelectedFileInfos[0].name} 集合`
        : "新集合";

    setCollectionDialog({
      mode: "create",
      isOpen: true,
      name: defaultName,
      memberPaths: realSelectedFileInfos.map((file) => file.path),
      collectionId: null,
    });
  }, [canCreateCollectionFromSelection, realSelectedFileInfos, showToast]);

  const handleOpenRenameCollectionDialog = useCallback((file: FileInfo) => {
    if (!file.collection_id) {
      return;
    }

    setCollectionDialog({
      mode: "rename",
      isOpen: true,
      name: file.name,
      memberPaths: [],
      collectionId: file.collection_id,
    });
  }, []);

  const handleOpenDeleteCollectionDialog = useCallback((file: FileInfo) => {
    if (!file.collection_id) {
      return;
    }

    setDeleteCollectionDialog({
      isOpen: true,
      collectionId: file.collection_id,
      name: file.name,
    });
  }, []);

  const handleCloseCollectionDialog = useCallback(() => {
    if (isSavingCollection) {
      return;
    }

    setCollectionDialog((state) => ({
      ...state,
      isOpen: false,
    }));
  }, [isSavingCollection]);

  const handleCollectionNameChange = useCallback((name: string) => {
    setCollectionDialog((state) => ({
      ...state,
      name,
    }));
  }, []);

  const handleConfirmCollectionDialog = useCallback(
    async (rawName: string) => {
      if (!projectPath || !currentPath || !collectionDialog.mode) {
        return;
      }

      const name = rawName.trim();
      if (!name) {
        showToast({
          title: "集合名称不能为空",
          message: "请输入一个集合名称。",
          tone: "error",
        });
        return;
      }

      setIsSavingCollection(true);
      try {
        if (collectionDialog.mode === "create") {
          await invoke("create_collection", {
            projectPath,
            name,
            memberPaths: collectionDialog.memberPaths,
          });
          showToast({
            title: "集合已创建",
            message: `${name} · 已显示在项目根目录`,
            tone: "success",
          });
        } else if (collectionDialog.collectionId) {
          await invoke("rename_collection", {
            projectPath,
            collectionId: collectionDialog.collectionId,
            name,
          });
          showToast({
            title: "集合已重命名",
            message: name,
            tone: "success",
          });
        }

        setCollectionDialog((state) => ({
          ...state,
          isOpen: false,
        }));
        await refresh();
      } catch (error) {
        console.error("Failed to save collection:", error);
        showToast({
          title: "集合操作失败",
          message: String(error),
          tone: "error",
        });
      } finally {
        setIsSavingCollection(false);
      }
    },
    [collectionDialog, currentPath, projectPath, refresh, showToast],
  );

  const handleConfirmDeleteCollection = useCallback(async () => {
    if (!projectPath || !deleteCollectionDialog.collectionId) {
      return;
    }

    try {
      await invoke("delete_collection", {
        projectPath,
        collectionId: deleteCollectionDialog.collectionId,
      });
      setDeleteCollectionDialog({
        isOpen: false,
        collectionId: null,
        name: "",
      });
      await refresh();
      showToast({
        title: "集合已删除",
        message: deleteCollectionDialog.name,
        tone: "success",
      });
    } catch (error) {
      console.error("Failed to delete collection:", error);
      showToast({
        title: "删除集合失败",
        message: String(error),
        tone: "error",
      });
    }
  }, [deleteCollectionDialog, projectPath, refresh, showToast]);

  const addItemsToCollection = useCallback(
    async (collection: Pick<FileInfo, "collection_id" | "name">, memberPaths: string[]) => {
      if (!projectPath || !collection.collection_id) {
        return false;
      }

      const uniqueMemberPaths = Array.from(
        new Set(memberPaths.filter((path) => Boolean(path))),
      );
      if (uniqueMemberPaths.length === 0) {
        showToast({
          title: "没有可加入的项目",
          message: "集合只能收纳真实文件或文件夹。",
          tone: "warning",
        });
        return false;
      }

      setIsAddingToCollection(true);
      try {
        const result = await invoke<CollectionMemberUpdate>("add_collection_items", {
          projectPath,
          collectionId: collection.collection_id,
          memberPaths: uniqueMemberPaths,
        });
        await refresh();

        if (result.added_count > 0) {
          const duplicateMessage = result.already_present_count > 0
            ? `，${result.already_present_count} 项已存在`
            : "";
          showToast({
            title: "已加入集合",
            message: `${collection.name} · 新增 ${result.added_count} 项${duplicateMessage}`,
            tone: "success",
          });
        } else {
          showToast({
            title: "集合没有变化",
            message: "选中的项目已全部在该集合中。",
            tone: "warning",
          });
        }
        return true;
      } catch (error) {
        console.error("Failed to add items to collection:", error);
        showToast({
          title: "加入集合失败",
          message: String(error),
          tone: "error",
        });
        return false;
      } finally {
        setIsAddingToCollection(false);
      }
    },
    [projectPath, refresh, showToast],
  );

  const handleAddSelectionToCollection = useCallback(
    async (targetCollection?: FileInfo) => {
      const memberPaths = realSelectedFileInfos.map((file) => file.path);
      if (!projectPath || memberPaths.length === 0) {
        showToast({
          title: "请选择项目",
          message: "先选择要加入集合的真实文件或文件夹。",
          tone: "warning",
        });
        return;
      }

      if (
        targetCollection?.entry_kind === "manual_collection" &&
        targetCollection.collection_id
      ) {
        await addItemsToCollection(targetCollection, memberPaths);
        return;
      }

      try {
        const collections = await invoke<ProjectCollectionOption[]>("list_project_collections", {
          projectPath,
        });
        if (collections.length === 0) {
          showToast({
            title: "还没有集合",
            message: "请先创建集合。",
            tone: "warning",
          });
          return;
        }

        setCollectionPickerOptions(collections);
        setAddToCollectionDialog({
          isOpen: true,
          collectionId: collections[0].id,
          memberPaths,
        });
      } catch (error) {
        console.error("Failed to load project collections:", error);
        showToast({
          title: "读取集合失败",
          message: String(error),
          tone: "error",
        });
      }
    },
    [addItemsToCollection, projectPath, realSelectedFileInfos, showToast],
  );

  const handleCloseAddToCollectionDialog = useCallback(() => {
    if (isAddingToCollection) {
      return;
    }
    setAddToCollectionDialog({
      isOpen: false,
      collectionId: null,
      memberPaths: [],
    });
    setCollectionPickerOptions([]);
  }, [isAddingToCollection]);

  const handleConfirmAddToCollection = useCallback(async () => {
    const collection = collectionPickerOptions.find(
      (item) => item.id === addToCollectionDialog.collectionId,
    );
    if (!collection) {
      showToast({
        title: "请选择集合",
        message: "目标集合已不存在，请刷新后重试。",
        tone: "warning",
      });
      return;
    }

    const succeeded = await addItemsToCollection(
      { collection_id: collection.id, name: collection.name },
      addToCollectionDialog.memberPaths,
    );
    if (succeeded) {
      setAddToCollectionDialog({
        isOpen: false,
        collectionId: null,
        memberPaths: [],
      });
      setCollectionPickerOptions([]);
    }
  }, [addItemsToCollection, addToCollectionDialog, collectionPickerOptions, showToast]);

  const handleDropToCollection = useCallback(
    async (collection: FileInfo, dragPaths?: string[]) => {
      const memberPaths = dragPaths && dragPaths.length > 0 ? dragPaths : draggedPaths;
      clearDropHoverState();
      await addItemsToCollection(collection, memberPaths);
    },
    [addItemsToCollection, clearDropHoverState, draggedPaths],
  );

  const stopColumnResize = useCallback(() => {
    columnResizeStateRef.current = null;
    setResizingColumnKey(null);
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  }, []);

  useEffect(() => {
    if (!resizingColumnKey) {
      return;
    }

    const handleMouseMove = (event: MouseEvent) => {
      const resizeState = columnResizeStateRef.current;
      if (!resizeState) {
        return;
      }

      const nextWidth =
        resizeState.startWidth + (event.clientX - resizeState.startX);
      updateColumn(resizeState.key, {
        width: clampColumnWidth(resizeState.key, nextWidth),
      });
    };

    const handleMouseUp = () => {
      stopColumnResize();
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [resizingColumnKey, stopColumnResize, updateColumn]);

  useEffect(() => {
    return () => {
      stopColumnResize();
    };
  }, [stopColumnResize]);

  const handleStartColumnResize = useCallback(
    (key: string, width: number, event: React.MouseEvent<HTMLDivElement>) => {
      event.preventDefault();
      event.stopPropagation();
      columnResizeStateRef.current = {
        key,
        startX: event.clientX,
        startWidth: width,
      };
      setResizingColumnKey(key);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    },
    [],
  );

  const getSuggestedFolderName = useCallback(async (targetDir: string) => {
    const baseName = "新建文件夹";
    let candidate = baseName;
    let index = 2;

    while (
      await invoke<boolean>("path_exists", {
        path: joinPath(targetDir, candidate),
      })
    ) {
      candidate = `${baseName} ${index}`;
      index += 1;
    }

    return candidate;
  }, []);

  const handleCreateFolder = useCallback(async () => {
    if (!currentPath) {
      return;
    }

    try {
      const suggestedName = await getSuggestedFolderName(currentPath);
      setCreateFolderDialog({
        isOpen: true,
        suggestedName,
        folderName: suggestedName,
      });
    } catch (error) {
      console.error("Failed to open create folder dialog:", error);
      showToast({
        title: "创建失败",
        message: String(error),
        tone: "error",
      });
    }
  }, [currentPath, getSuggestedFolderName, showToast]);

  const handleCloseCreateFolderDialog = useCallback(() => {
    if (isCreatingFolder) {
      return;
    }

    setCreateFolderDialog((state) => ({
      ...state,
      isOpen: false,
    }));
  }, [isCreatingFolder]);

  const handleCreateFolderNameChange = useCallback((folderName: string) => {
    setCreateFolderDialog((state) => ({
      ...state,
      folderName,
    }));
  }, []);

  const handleConfirmCreateFolder = useCallback(
    async (rawFolderName: string) => {
      if (!currentPath) {
        return;
      }

      const folderName = rawFolderName.trim();

      if (!folderName) {
        showToast({
          title: "创建失败",
          message: "请输入文件夹名称。",
          tone: "error",
        });
        return;
      }

      if (/[\\/]/.test(folderName)) {
        showToast({
          title: "创建失败",
          message: "文件夹名称不能包含路径分隔符。",
          tone: "error",
        });
        return;
      }

      if (folderName === "." || folderName === "..") {
        showToast({
          title: "创建失败",
          message: "请输入有效的文件夹名称。",
          tone: "error",
        });
        return;
      }

      setIsCreatingFolder(true);
      try {
        const targetPath = joinPath(currentPath, folderName);
        const exists = await invoke<boolean>("path_exists", {
          path: targetPath,
        });

        if (exists) {
          showToast({
            title: "创建失败",
            message: "当前目录已存在同名文件夹。",
            tone: "error",
          });
          return;
        }

        await invoke("create_directory", { path: targetPath });
        await refresh();
        setCreateFolderDialog((state) => ({
          ...state,
          isOpen: false,
        }));
        showToast({
          title: "文件夹已创建",
          message: folderName,
          tone: "success",
        });
      } catch (error) {
        console.error("Failed to create folder:", error);
        showToast({
          title: "创建失败",
          message: String(error),
          tone: "error",
        });
      } finally {
        setIsCreatingFolder(false);
      }
    },
    [currentPath, refresh, showToast],
  );

  const handleDelete = useCallback(
    async (targetPaths: string[]) => {
      const paths = compactDraggedPaths(targetPaths);
      if (paths.length === 0) {
        return;
      }

      try {
        const deletedCount = await invoke<number>("delete_paths", { paths });
        await refresh();

        if (deletedCount === 0) {
          showToast({
            title: "未删除任何项目",
            message: "选中的文件可能已经不存在，列表已刷新。",
            tone: "warning",
          });
          return;
        }

        showToast({
          title: deletedCount > 1 ? "已移动到回收站" : "文件已移动到回收站",
          message:
            deletedCount > 1
              ? `已将 ${deletedCount} 个项目移动到回收站。`
              : `已将 ${paths[0].split(/[\\/]/).pop() || "该项目"} 移到回收站。`,
          tone: "success",
        });
      } catch (error) {
        console.error("Failed to delete:", error);
        showToast({
          title: "删除失败",
          message: String(error),
          tone: "error",
        });
      }
    },
    [refresh, showToast],
  );

  const handleDeleteFromContextMenu = useCallback(
    async (file: FileInfo) => {
      const targetPaths = selectedFiles.has(file.path)
        ? Array.from(selectedFiles)
        : [file.path];
      await handleDelete(targetPaths);
    },
    [handleDelete, selectedFiles],
  );

  const getDraggedItems = useCallback(
    (file: FileInfo) => {
      if (isVirtualFile(file)) {
        return [];
      }

      if (selectedFiles.has(file.path) && selectedFiles.size > 1) {
        return Array.from(selectedFiles).filter((path) => {
          const selectedFile = allKnownFiles.get(path);
          return selectedFile ? !isVirtualFile(selectedFile) : true;
        });
      }
      return [file.path];
    },
    [allKnownFiles, selectedFiles],
  );

  const canDropToDirectory = useCallback(
    (targetDir: string, dragPaths = draggedPaths) => {
      return canMovePathsToDirectory(targetDir, dragPaths);
    },
    [draggedPaths],
  );

  const canDropToCollection = useCallback(
    (collection: FileInfo, dragPaths = draggedPaths) => {
      return Boolean(
        projectPath &&
          collection.entry_kind === "manual_collection" &&
          collection.collection_id &&
          compactDraggedPaths(dragPaths).length > 0,
      );
    },
    [draggedPaths, projectPath],
  );

  const handleDragStart = useCallback(
    (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => {
      if (isVirtualFile(file)) {
        event.preventDefault();
        return;
      }

      startInternalDrag(event, getDraggedItems(file));
    },
    [getDraggedItems, startInternalDrag],
  );

  const handleDragEnd = useCallback(() => {
    finishInternalDrag();
    clearDropHoverState();
  }, [clearDropHoverState, finishInternalDrag]);

  const handleDropToDirectory = useCallback(
    async (targetDir: string, dragPaths?: string[]) => {
      const currentDraggedPaths =
        dragPaths && dragPaths.length > 0 ? dragPaths : draggedPaths;
      if (currentDraggedPaths.length === 0) {
        return;
      }

      clearDropHoverState();
      await movePathsToDirectory(
        currentDraggedPaths,
        targetDir,
        getPathLabel(
          targetDir,
          projectStore.getState().projectPath,
          projectStore.getState().projectName,
        ),
      );
    },
    [clearDropHoverState, draggedPaths, movePathsToDirectory, projectStore],
  );

  const handleHoverDirectory = useCallback((targetDir: string) => {
    pendingHoverTargetRef.current = targetDir;

    if (hoverFrameRef.current !== null) {
      return;
    }

    hoverFrameRef.current = window.requestAnimationFrame(() => {
      hoverFrameRef.current = null;
      const nextTarget = pendingHoverTargetRef.current;
      pendingHoverTargetRef.current = null;
      if (!nextTarget) {
        return;
      }
      setDropTargetPath((prev) => (prev === nextTarget ? prev : nextTarget));
    });
  }, []);

  const fileContextSelectedItems = useMemo(() => {
    if (contextMenu?.kind !== "file") {
      return selectedFileInfos;
    }

    const selectedIncludesTarget = selectedFileInfos.some(
      (file) => file.path === contextMenu.file.path,
    );
    if (selectedIncludesTarget && selectedFileInfos.length > 0) {
      return selectedFileInfos;
    }

    return [contextMenu.file];
  }, [contextMenu, selectedFileInfos]);

  const fileContextPluginContext = useMemo(() => {
    if (contextMenu?.kind !== "file") {
      return null;
    }

    return buildFileContext(fileContextSelectedItems);
  }, [buildFileContext, contextMenu, fileContextSelectedItems]);

  const fileContextPluginActions = useMemo(() => {
    if (
      !projectPath ||
      contextMenu?.kind !== "file" ||
      !fileContextPluginContext
    ) {
      return [];
    }

    return getVisiblePluginActions(
      pluginState?.descriptors || [],
      "file-context",
      fileContextPluginContext,
    );
  }, [
    contextMenu,
    fileContextPluginContext,
    pluginState?.descriptors,
    projectPath,
  ]);

  const fileContextPluginDebugInfo = useMemo(() => {
    if (
      !projectPath ||
      contextMenu?.kind !== "file" ||
      !fileContextPluginContext
    ) {
      return "";
    }

    return JSON.stringify(
      buildPluginVisibilityDiagnostics(
        pluginState?.descriptors || [],
        "file-context",
        fileContextPluginContext,
      ),
      null,
      2,
    );
  }, [
    contextMenu,
    fileContextPluginContext,
    pluginState?.descriptors,
    projectPath,
  ]);

  const currentDirectoryPluginContext = useMemo(() => {
    if (contextMenu?.kind !== "directory") {
      return null;
    }

    return buildFileContext([]);
  }, [buildFileContext, contextMenu]);

  const currentDirectoryPluginActions = useMemo(() => {
    if (
      !projectPath ||
      contextMenu?.kind !== "directory" ||
      !currentDirectoryPluginContext
    ) {
      return [];
    }

    return getVisiblePluginActions(
      pluginState?.descriptors || [],
      "file-context",
      currentDirectoryPluginContext,
    );
  }, [
    contextMenu,
    currentDirectoryPluginContext,
    pluginState?.descriptors,
    projectPath,
  ]);

  const currentDirectoryPluginDebugInfo = useMemo(() => {
    if (
      !projectPath ||
      contextMenu?.kind !== "directory" ||
      !currentDirectoryPluginContext
    ) {
      return "";
    }

    return JSON.stringify(
      buildPluginVisibilityDiagnostics(
        pluginState?.descriptors || [],
        "file-context",
        currentDirectoryPluginContext,
      ),
      null,
      2,
    );
  }, [
    contextMenu,
    currentDirectoryPluginContext,
    pluginState?.descriptors,
    projectPath,
  ]);

  useEffect(() => {
    if (contextMenu?.kind !== "file" || !fileContextPluginDebugInfo) {
      return;
    }

    console.info(
      "[plugin-debug:file-context]",
      JSON.parse(fileContextPluginDebugInfo),
    );
  }, [contextMenu, fileContextPluginDebugInfo]);

  useEffect(() => {
    if (contextMenu?.kind !== "directory" || !currentDirectoryPluginDebugInfo) {
      return;
    }

    console.info(
      "[plugin-debug:directory-context]",
      JSON.parse(currentDirectoryPluginDebugInfo),
    );
  }, [contextMenu, currentDirectoryPluginDebugInfo]);

  if (isSearching) {
    return (
      <div className="h-full flex items-center justify-center text-gray-400">
        搜索中...
      </div>
    );
  }

  return (
    <div className="h-full">
      {viewMode === "list" ? (
        <ListView
          files={displayFiles}
          selectedFiles={selectedFiles}
          onSelect={handleSelectFile}
          onDoubleClick={handleDoubleClick}
          onContextMenu={handleContextMenu}
          onBackgroundContextMenu={handleBackgroundContextMenu}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          getExternalDragPaths={getDraggedItems}
          onDropToDirectory={handleDropToDirectory}
          onDropToCollection={handleDropToCollection}
          onHoverDirectory={handleHoverDirectory}
          canDropToDirectory={canDropToDirectory}
          canDropToCollection={canDropToCollection}
          getDraggedPathsFromDataTransfer={getDraggedPathsFromDataTransfer}
          suppressInteraction={suppressInteraction}
          dropTargetPath={dropTargetPath}
          currentPath={currentPath || ""}
          columns={columns}
          resizingColumnKey={resizingColumnKey}
          onStartColumnResize={handleStartColumnResize}
                  isExcluded={isExcluded}
                  showExcludedFiles={showExcludedFiles}
                  resolveFileTags={resolveFileTags}
                  resolveRelatedMdtCount={resolveRelatedMdtCount}
                />
      ) : (
        <GridView
          files={displayFiles}
          selectedFiles={selectedFiles}
          onSelect={handleSelectFile}
          onDoubleClick={handleDoubleClick}
          onContextMenu={handleContextMenu}
          onBackgroundContextMenu={handleBackgroundContextMenu}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          getExternalDragPaths={getDraggedItems}
          onDropToDirectory={handleDropToDirectory}
          onDropToCollection={handleDropToCollection}
          onHoverDirectory={handleHoverDirectory}
          canDropToDirectory={canDropToDirectory}
          canDropToCollection={canDropToCollection}
          getDraggedPathsFromDataTransfer={getDraggedPathsFromDataTransfer}
          suppressInteraction={suppressInteraction}
          dropTargetPath={dropTargetPath}
          currentPath={currentPath || ""}
          isExcluded={isExcluded}
          showExcludedFiles={showExcludedFiles}
          resolveFileTags={resolveFileTags}
          resolveRelatedMdtCount={resolveRelatedMdtCount}
        />
      )}

      {contextMenu?.kind === "file" && (
        <FileContextMenu
          file={contextMenu.file}
          x={contextMenu.x}
          y={contextMenu.y}
          currentPath={currentPath || ""}
          projectPath={projectPath || ""}
          pluginActions={fileContextPluginActions}
          pluginDebugInfo={fileContextPluginDebugInfo}
          onClose={handleCloseContextMenu}
          onRefresh={handleRefresh}
          onShowDetails={handleShowDetails}
          onDelete={handleDeleteFromContextMenu}
          onCreateFolder={handleCreateFolder}
          onCreateCollection={handleOpenCreateCollectionDialog}
          canCreateCollection={canCreateCollectionFromSelection}
          onAddSelectionToCollection={handleAddSelectionToCollection}
          canAddSelectionToCollection={canAddSelectionToCollection}
          onRemoveFromCollection={
            onRemoveFromCollection && currentPath?.startsWith("pmc://collection/")
              ? handleRemoveFromCollection
              : undefined
          }
          onRenameCollection={handleOpenRenameCollectionDialog}
          onDeleteCollection={handleOpenDeleteCollectionDialog}
          onOpenFile={handleSystemOpenFile}
          onOpenDirectoryTab={handleOpenDirectoryTab}
          onRunPluginAction={(action) =>
            runPluginAction(action, fileContextSelectedItems)
          }
        />
      )}

      {contextMenu?.kind === "directory" && (
        <CurrentDirectoryContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          currentPath={currentPath || ""}
          projectPath={projectPath || ""}
          pluginActions={currentDirectoryPluginActions}
          pluginDebugInfo={currentDirectoryPluginDebugInfo}
          onClose={handleCloseContextMenu}
          onRefresh={handleRefresh}
          onCreateFolder={handleCreateFolder}
          onOpenDirectoryTab={onOpenDirectoryTab}
          onRunPluginAction={(action) => runPluginAction(action, [])}
        />
      )}

      <FileDetailsDialog
        file={detailsDialogFile}
        fileTagList={detailsDialogTagList}
        relatedMdtEntries={detailsDialogRelatedMdtEntries}
        isOpen={!!detailsDialogFile}
        onClose={handleCloseDetailsDialog}
      />

      <InputDialog
        isOpen={createFolderDialog.isOpen}
        onClose={handleCloseCreateFolderDialog}
        onConfirm={handleConfirmCreateFolder}
        title="新建文件夹"
        label="文件夹名称"
        value={createFolderDialog.folderName}
        onChange={handleCreateFolderNameChange}
        confirmText={isCreatingFolder ? "创建中..." : "创建"}
        disabled={isCreatingFolder}
        description={
          createFolderDialog.suggestedName
            ? `默认名称：${createFolderDialog.suggestedName}`
            : undefined
        }
        selectOnOpen
      />

      <InputDialog
        isOpen={collectionDialog.isOpen}
        onClose={handleCloseCollectionDialog}
        onConfirm={handleConfirmCollectionDialog}
        title={collectionDialog.mode === "rename" ? "重命名集合" : "创建集合"}
        label="集合名称"
        value={collectionDialog.name}
        onChange={handleCollectionNameChange}
        confirmText={isSavingCollection ? "保存中..." : "保存"}
        disabled={isSavingCollection}
        description={
          collectionDialog.mode === "create"
            ? `将显示在项目根目录并收纳 ${collectionDialog.memberPaths.length} 个项目，磁盘文件不会移动。`
            : "只修改集合名称，不影响真实文件。"
        }
        selectOnOpen
      />

      <Dialog
        isOpen={addToCollectionDialog.isOpen}
        onClose={handleCloseAddToCollectionDialog}
        title="加入集合"
        size="sm"
        footer={
          <>
            <button
              type="button"
              onClick={handleCloseAddToCollectionDialog}
              disabled={isAddingToCollection}
              className="rounded-lg px-4 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-50 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              取消
            </button>
            <button
              type="button"
              onClick={() => void handleConfirmAddToCollection()}
              disabled={isAddingToCollection || !addToCollectionDialog.collectionId}
              className="rounded-lg bg-blue-600 px-4 py-2 text-sm text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isAddingToCollection ? "加入中..." : "加入"}
            </button>
          </>
        }
      >
        <div className="space-y-3">
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">目标集合</span>
            <select
              value={addToCollectionDialog.collectionId || ""}
              disabled={isAddingToCollection}
              onChange={(event) =>
                setAddToCollectionDialog((state) => ({
                  ...state,
                  collectionId: event.target.value || null,
                }))
              }
              className="w-full rounded-lg border border-gray-300 bg-white px-3 py-2.5 text-sm text-gray-900 outline-none transition-colors focus:border-blue-500 focus:ring-2 focus:ring-blue-500/20 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100"
            >
              {collectionPickerOptions.map((collection) => (
                <option key={collection.id} value={collection.id}>
                  {collection.name} · {collection.item_count ?? 0} 项
                </option>
              ))}
            </select>
          </label>
          <p className="text-xs text-gray-500 dark:text-gray-400">
            将收纳 {addToCollectionDialog.memberPaths.length} 个项目，磁盘文件不会移动。
          </p>
        </div>
      </Dialog>

      <ConfirmDialog
        isOpen={deleteCollectionDialog.isOpen}
        onClose={() =>
          setDeleteCollectionDialog({
            isOpen: false,
            collectionId: null,
            name: "",
          })
        }
        onConfirm={handleConfirmDeleteCollection}
        title="删除集合"
        message={`删除集合“${deleteCollectionDialog.name}”？\n真实文件不会被删除。`}
        confirmText="删除集合"
        type="danger"
      />

      {conflictDialog}
    </div>
  );
}
