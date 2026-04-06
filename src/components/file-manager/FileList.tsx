import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
import { FileIcon, FolderIcon, Image, Film, FileText, Box } from "lucide-react";
import {
  CurrentDirectoryContextMenu,
  FileContextMenu,
} from "./FileContextMenu";
import { FileDetailsDialog } from "./FileDetailsView";
import { InputDialog } from "../Dialog";
import {
  canMovePathsToDirectory,
  compactDraggedPaths,
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
import { isImageExtension } from "../image-viewer/imageViewerUtils";
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

const MIN_COLUMN_WIDTHS: Record<string, number> = {
  name: 220,
  size: 90,
  modified: 150,
  type: 100,
  tags: 120,
};

const LIST_ROW_HEIGHT = 40;
const LIST_OVERSCAN_COUNT = 12;
const GRID_CARD_HEIGHT = 176;
const GRID_GAP = 16;
const GRID_ROW_HEIGHT = GRID_CARD_HEIGHT + GRID_GAP;
const GRID_OVERSCAN_ROWS = 2;

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

const ListRow = memo(function ListRow({
  file,
  visibleColumns,
  selectedFiles,
  dropTargetPath,
  showExcludedFiles,
  isExcluded,
  resolveFileTags,
  suppressInteraction,
  onSelect,
  onDoubleClick,
  onContextMenu,
  onDragStart,
  onDragEnd,
  getExternalDragPaths,
  onDropToDirectory,
  onHoverDirectory,
  canDropToDirectory,
  getDraggedPathsFromDataTransfer,
}: {
  file: FileInfo;
  visibleColumns: ColumnConfig[];
  selectedFiles: Set<string>;
  dropTargetPath: string | null;
  showExcludedFiles: boolean;
  isExcluded: (file: FileInfo) => boolean;
  resolveFileTags: ResolveFileTags;
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  getExternalDragPaths: (file: FileInfo) => string[];
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (
    dataTransfer: DataTransfer | null,
  ) => string[];
}) {
  const isSelected = selectedFiles.has(file.path);
  const fileTagList = resolveFileTags(file.path);
  const isDropTarget = file.is_dir && dropTargetPath === file.path;
  const excluded = isExcluded(file);
  const externalDragHandleVisibilityClass = isSelected
    ? "opacity-100"
    : "opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto";

  return (
    <div
      draggable
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
        onSelect(file.path, e.ctrlKey || e.metaKey);
      }}
      onDoubleClick={(e) => {
        if (suppressInteraction(e)) return;
        onDoubleClick(file, e.ctrlKey || e.metaKey);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(file, e.clientX, e.clientY);
      }}
      onDragStart={(e) => onDragStart(file, e)}
      onDragEnd={onDragEnd}
      onDragOver={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths))
          return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        onHoverDirectory(file.path);
      }}
      onDragEnter={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths))
          return;
        e.preventDefault();
        onHoverDirectory(file.path);
      }}
      onDrop={async (e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths))
          return;
        e.preventDefault();
        e.stopPropagation();
        await onDropToDirectory(file.path, internalDragPaths);
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
              <ExternalDragHandle
                resolvePaths={() => getExternalDragPaths(file)}
                className={`inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-gray-400 transition hover:bg-gray-200 hover:text-gray-700 dark:hover:bg-gray-700 dark:hover:text-gray-100 ${externalDragHandleVisibilityClass} ${
                  isSelected ? "text-blue-700 dark:text-blue-200" : ""
                }`}
              />
            </div>
          )}
          {col.key === "size" && formatSize(file.size)}
          {col.key === "modified" && formatDate(file.modified)}
          {col.key === "type" &&
            (file.is_dir ? "文件夹" : file.extension?.toUpperCase() || "文件")}
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
  onHoverDirectory,
  canDropToDirectory,
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
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
  onBackgroundContextMenu: (x: number, y: number) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  getExternalDragPaths: (file: FileInfo) => string[];
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
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
                  suppressInteraction={suppressInteraction}
                  onSelect={onSelect}
                  onDoubleClick={onDoubleClick}
                  onContextMenu={onContextMenu}
                  onDragStart={onDragStart}
                  onDragEnd={onDragEnd}
                  getExternalDragPaths={getExternalDragPaths}
                  onDropToDirectory={onDropToDirectory}
                  onHoverDirectory={onHoverDirectory}
                  canDropToDirectory={canDropToDirectory}
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
  suppressInteraction,
  onSelect,
  onDoubleClick,
  onContextMenu,
  onDragStart,
  onDragEnd,
  getExternalDragPaths,
  onDropToDirectory,
  onHoverDirectory,
  canDropToDirectory,
  getDraggedPathsFromDataTransfer,
}: {
  file: FileInfo;
  selectedFiles: Set<string>;
  dropTargetPath: string | null;
  showExcludedFiles: boolean;
  isExcluded: (file: FileInfo) => boolean;
  resolveFileTags: ResolveFileTags;
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  getExternalDragPaths: (file: FileInfo) => string[];
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (
    dataTransfer: DataTransfer | null,
  ) => string[];
}) {
  const isSelected = selectedFiles.has(file.path);
  const fileTagList = resolveFileTags(file.path);
  const isDropTarget = file.is_dir && dropTargetPath === file.path;
  const excluded = isExcluded(file);
  const externalDragHandleVisibilityClass = isSelected
    ? "opacity-100"
    : "opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto";

  return (
    <div
      draggable
      className={`
        group relative h-full p-3 rounded-lg cursor-pointer select-none transition-all
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
        onSelect(file.path, e.ctrlKey || e.metaKey);
      }}
      onDoubleClick={(e) => {
        if (suppressInteraction(e)) return;
        onDoubleClick(file, e.ctrlKey || e.metaKey);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(file, e.clientX, e.clientY);
      }}
      onDragStart={(e) => onDragStart(file, e)}
      onDragEnd={onDragEnd}
      onDragOver={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths))
          return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        onHoverDirectory(file.path);
      }}
      onDragEnter={(e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths))
          return;
        e.preventDefault();
        onHoverDirectory(file.path);
      }}
      onDrop={async (e) => {
        const internalDragPaths = getDraggedPathsFromDataTransfer(
          e.dataTransfer,
        );
        if (!file.is_dir || !canDropToDirectory(file.path, internalDragPaths))
          return;
        e.preventDefault();
        e.stopPropagation();
        await onDropToDirectory(file.path, internalDragPaths);
      }}
    >
      <div className="absolute right-2 top-2 flex items-center gap-1">
        {isSelected && (
          <div className="rounded-full bg-blue-600 px-1.5 py-0.5 text-[10px] font-medium text-white dark:bg-blue-500">
            已选中
          </div>
        )}
        <ExternalDragHandle
          resolvePaths={() => getExternalDragPaths(file)}
          className={`inline-flex h-7 w-7 items-center justify-center rounded-full bg-white/90 text-gray-500 shadow-sm ring-1 ring-black/5 transition hover:bg-white hover:text-gray-800 dark:bg-gray-900/85 dark:text-gray-300 dark:ring-white/10 dark:hover:bg-gray-800 dark:hover:text-gray-100 ${externalDragHandleVisibilityClass}`}
          iconClassName="h-4 w-4"
        />
      </div>

      <div
        className={`mb-2 aspect-square flex items-center justify-center rounded-xl transition-colors ${
          isSelected ? "bg-blue-200/70 dark:bg-blue-900/50" : ""
        }`}
      >
        {getFileIcon(file)}
      </div>

      <div
        className={`text-sm text-center truncate ${
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
        <div className="flex justify-center gap-1 mt-1 flex-wrap">
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
  onHoverDirectory,
  canDropToDirectory,
  getDraggedPathsFromDataTransfer,
  suppressInteraction,
  dropTargetPath,
  currentPath,
  isExcluded,
  showExcludedFiles,
  resolveFileTags,
}: {
  files: FileInfo[];
  selectedFiles: Set<string>;
  onSelect: (path: string, multi: boolean) => void;
  onDoubleClick: (file: FileInfo, openInStandalone: boolean) => void;
  onContextMenu: (file: FileInfo, x: number, y: number) => void;
  onBackgroundContextMenu: (x: number, y: number) => void;
  onDragStart: (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  getExternalDragPaths: (file: FileInfo) => string[];
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (
    dataTransfer: DataTransfer | null,
  ) => string[];
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  dropTargetPath: string | null;
  currentPath: string;
  isExcluded: (file: FileInfo) => boolean;
  showExcludedFiles: boolean;
  resolveFileTags: ResolveFileTags;
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
                suppressInteraction={suppressInteraction}
                onSelect={onSelect}
                onDoubleClick={onDoubleClick}
                onContextMenu={onContextMenu}
                onDragStart={onDragStart}
                onDragEnd={onDragEnd}
                getExternalDragPaths={getExternalDragPaths}
                onDropToDirectory={onDropToDirectory}
                onHoverDirectory={onHoverDirectory}
                canDropToDirectory={canDropToDirectory}
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

export function FileList() {
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
  const [detailsDialogFile, setDetailsDialogFile] = useState<FileInfo | null>(
    null,
  );
  const [createFolderDialog, setCreateFolderDialog] = useState({
    isOpen: false,
    suggestedName: "",
    folderName: "",
  });
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [resizingColumnKey, setResizingColumnKey] = useState<string | null>(
    null,
  );
  const columnResizeStateRef = useRef<{
    key: string;
    startX: number;
    startWidth: number;
  } | null>(null);

  const displayFiles = searchQuery ? searchResults : files;
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

  const detailsDialogTagList = detailsDialogFile
    ? resolveFileTags(detailsDialogFile.path)
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
    [showToast],
  );

  const handleDoubleClick = useCallback(
    async (file: FileInfo, openInStandalone: boolean) => {
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
        if (openInStandalone) {
          const opened = await openFileInStandaloneWindow(file.path, {
            projectPath: projectPath || undefined,
          });
          if (!opened) {
            await handleSystemOpenFile(file);
          }
          return;
        }

        const tabId = await openFileInTab(file.path);
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
      openFileInStandaloneWindow,
      openFileInTab,
      projectPath,
      showToast,
    ],
  );

  const handleContextMenu = useCallback(
    (file: FileInfo, x: number, y: number) => {
      if (!selectedFiles.has(file.path)) {
        projectStore.setState({
          selectedFiles: new Set([file.path]),
        });
      }
      setContextMenu({ kind: "file", file, x, y });
    },
    [projectStore, selectedFiles],
  );

  const handleBackgroundContextMenu = useCallback(
    (x: number, y: number) => {
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

  const handleCloseDetailsDialog = useCallback(() => {
    setDetailsDialogFile(null);
  }, []);

  const handleRefresh = useCallback(() => {
    void refresh();
  }, [refresh]);

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
      if (selectedFiles.has(file.path) && selectedFiles.size > 1) {
        return Array.from(selectedFiles);
      }
      return [file.path];
    },
    [selectedFiles],
  );

  const canDropToDirectory = useCallback(
    (targetDir: string, dragPaths = draggedPaths) => {
      return canMovePathsToDirectory(targetDir, dragPaths);
    },
    [draggedPaths],
  );

  const handleDragStart = useCallback(
    (file: FileInfo, event: React.DragEvent<HTMLDivElement>) => {
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
          onSelect={selectFile}
          onDoubleClick={handleDoubleClick}
          onContextMenu={handleContextMenu}
          onBackgroundContextMenu={handleBackgroundContextMenu}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          getExternalDragPaths={getDraggedItems}
          onDropToDirectory={handleDropToDirectory}
          onHoverDirectory={handleHoverDirectory}
          canDropToDirectory={canDropToDirectory}
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
        />
      ) : (
        <GridView
          files={displayFiles}
          selectedFiles={selectedFiles}
          onSelect={selectFile}
          onDoubleClick={handleDoubleClick}
          onContextMenu={handleContextMenu}
          onBackgroundContextMenu={handleBackgroundContextMenu}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          getExternalDragPaths={getDraggedItems}
          onDropToDirectory={handleDropToDirectory}
          onHoverDirectory={handleHoverDirectory}
          canDropToDirectory={canDropToDirectory}
          getDraggedPathsFromDataTransfer={getDraggedPathsFromDataTransfer}
          suppressInteraction={suppressInteraction}
          dropTargetPath={dropTargetPath}
          currentPath={currentPath || ""}
          isExcluded={isExcluded}
          showExcludedFiles={showExcludedFiles}
          resolveFileTags={resolveFileTags}
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
          onOpenFile={handleSystemOpenFile}
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
          onRunPluginAction={(action) => runPluginAction(action, [])}
        />
      )}

      <FileDetailsDialog
        file={detailsDialogFile}
        fileTagList={detailsDialogTagList}
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

      {conflictDialog}
    </div>
  );
}
