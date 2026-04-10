import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileInfo, TreeNode } from "../../types";
import {
  useProjectStoreApi,
  useProjectStoreShallow,
} from "../../stores/projectStore";
import { usePluginStore } from "../../stores/pluginStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useTaskStore } from "../../stores/taskStore";
import { useUiStore } from "../../stores/uiStore";
import { APP_VERSION } from "../../config/appMeta";
import type { PluginAction } from "../../types/plugin";
import {
  ChevronRight,
  ChevronDown,
  Folder,
  FolderOpen,
  ArrowUp,
  RefreshCw,
} from "lucide-react";
import {
  canMovePathsToDirectory,
  getParentPath,
  getPathLabel,
  joinPath,
} from "./dragDrop";
import { ExternalDragHandle } from "./ExternalDragHandle";
import { InputDialog } from "../Dialog";
import { FileDetailsDialog } from "./FileDetailsView";
import { FileContextMenu } from "./FileContextMenu";
import { useFileDropMove } from "./useFileDropMove";
import { useInternalFileDrag } from "./useInternalFileDrag";
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

const SYSTEM_CONTEXT_DOUBLE_TRIGGER_MS = 350;

interface TreeItemProps {
  node: TreeNode;
  level: number;
  dropTargetPath: string | null;
  onDragStart: (path: string, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (
    dataTransfer: DataTransfer | null,
  ) => string[];
  onContextMenu: (node: TreeNode, x: number, y: number) => void;
  suppressInteraction: (event: React.SyntheticEvent<HTMLElement>) => boolean;
  isExcluded: (node: TreeNode) => boolean;
  showExcludedFiles: boolean;
}

function TreeItem({
  node,
  level,
  dropTargetPath,
  onDragStart,
  onDragEnd,
  onDropToDirectory,
  onHoverDirectory,
  canDropToDirectory,
  getDraggedPathsFromDataTransfer,
  onContextMenu,
  suppressInteraction,
  isExcluded,
  showExcludedFiles,
}: TreeItemProps) {
  const { currentPath, expandedKeys, toggleExpanded, loadDirectory } =
    useProjectStoreShallow((state) => ({
      currentPath: state.currentPath,
      expandedKeys: state.expandedKeys,
      toggleExpanded: state.toggleExpanded,
      loadDirectory: state.loadDirectory,
    }));
  const isExpanded = expandedKeys.has(node.path);
  const isSelected = currentPath === node.path;
  const isDropTarget = dropTargetPath === node.path;
  const excluded = isExcluded(node);
  const itemRef = useRef<HTMLDivElement>(null);
  const externalDragHandleVisibilityClass = isSelected
    ? "opacity-100"
    : "opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto";

  useEffect(() => {
    if (!isSelected) {
      return;
    }

    itemRef.current?.scrollIntoView({
      block: "nearest",
    });
  }, [isSelected]);

  const handleClick = (event: React.MouseEvent<HTMLDivElement>) => {
    if (suppressInteraction(event)) {
      return;
    }

    loadDirectory(node.path);
  };

  const handleToggle = (e: React.MouseEvent) => {
    e.stopPropagation();
    toggleExpanded(node.path);
  };

  return (
    <div>
      <div
        ref={itemRef}
        className={`
          group flex items-center py-1 px-2 cursor-pointer select-none
          hover:bg-gray-100 dark:hover:bg-gray-800
          ${isSelected ? "bg-blue-50 dark:bg-blue-900/20 border-r-2 border-blue-500" : ""}
          ${isDropTarget ? "bg-blue-50 ring-2 ring-inset ring-blue-500" : ""}
          ${showExcludedFiles && excluded ? "opacity-70" : ""}
        `}
        style={{ paddingLeft: `${level * 16 + 8}px` }}
        onClick={handleClick}
        draggable={level > 0}
        onDragStart={(e) => onDragStart(node.path, e)}
        onDragEnd={onDragEnd}
        onDragOver={(e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(
            e.dataTransfer,
          );
          if (!canDropToDirectory(node.path, internalDragPaths)) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = "move";
          onHoverDirectory(node.path);
        }}
        onDragEnter={(e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(
            e.dataTransfer,
          );
          if (!canDropToDirectory(node.path, internalDragPaths)) return;
          e.preventDefault();
          e.stopPropagation();
          onHoverDirectory(node.path);
        }}
        onDrop={async (e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(
            e.dataTransfer,
          );
          if (!canDropToDirectory(node.path, internalDragPaths)) return;
          e.preventDefault();
          e.stopPropagation();
          await onDropToDirectory(node.path, internalDragPaths);
        }}
        onContextMenu={(event) => {
          if (suppressInteraction(event)) {
            return;
          }

          event.preventDefault();
          event.stopPropagation();
          onContextMenu(node, event.clientX, event.clientY);
        }}
      >
        {node.children.length > 0 ? (
          <button
            onClick={handleToggle}
            className="w-4 h-4 flex items-center justify-center mr-1 hover:bg-gray-200 dark:hover:bg-gray-700 rounded"
          >
            {isExpanded ? (
              <ChevronDown className="w-3 h-3 text-gray-500" />
            ) : (
              <ChevronRight className="w-3 h-3 text-gray-500" />
            )}
          </button>
        ) : (
          <span className="w-4 mr-1" />
        )}

        {isExpanded ? (
          <FolderOpen className="w-4 h-4 text-yellow-500 mr-2" />
        ) : (
          <Folder className="w-4 h-4 text-yellow-500 mr-2" />
        )}

        <span className="text-sm truncate">{node.name}</span>
        {showExcludedFiles && excluded && (
          <span className="ml-2 shrink-0 rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">
            已排除
          </span>
        )}
        {level > 0 && (
          <ExternalDragHandle
            resolvePaths={() => [node.path]}
            className={`ml-auto inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-gray-400 transition hover:bg-gray-200 hover:text-gray-700 dark:hover:bg-gray-700 dark:hover:text-gray-100 ${externalDragHandleVisibilityClass} ${
              isSelected ? "text-blue-700 dark:text-blue-200" : ""
            }`}
          />
        )}
      </div>

      {isExpanded && node.children.length > 0 && (
        <div>
          {node.children.map((child) => (
            <TreeItem
              key={child.path}
              node={child}
              level={level + 1}
              dropTargetPath={dropTargetPath}
              onDragStart={onDragStart}
              onDragEnd={onDragEnd}
              onDropToDirectory={onDropToDirectory}
              onHoverDirectory={onHoverDirectory}
              canDropToDirectory={canDropToDirectory}
              getDraggedPathsFromDataTransfer={getDraggedPathsFromDataTransfer}
              onContextMenu={onContextMenu}
              suppressInteraction={suppressInteraction}
              isExcluded={isExcluded}
              showExcludedFiles={showExcludedFiles}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function filterTreeNode(
  node: TreeNode,
  shouldHideExcluded: boolean,
  isExcluded: (node: TreeNode) => boolean,
  isRoot = false,
): TreeNode | null {
  if (!isRoot && shouldHideExcluded && isExcluded(node)) {
    return null;
  }

  const children = node.children
    .map((child) => filterTreeNode(child, shouldHideExcluded, isExcluded))
    .filter((child): child is TreeNode => child !== null);

  return {
    ...node,
    children,
  };
}

function toTreeNodeFileInfo(node: TreeNode): FileInfo {
  return {
    name: node.name,
    path: node.path,
    is_dir: true,
    size: 0,
    modified: null,
    created: null,
    extension: null,
    thumbnail: null,
  };
}

function getTreeContextDirectory(
  file: FileInfo | null,
  projectPath: string | null,
  fallbackPath: string | null,
) {
  if (!file) {
    return fallbackPath;
  }

  if (!projectPath || file.path === projectPath) {
    return file.path;
  }

  return getParentPath(file.path);
}

export function FileTree() {
  const projectStore = useProjectStoreApi();
  const {
    treeData,
    projectName,
    projectPath,
    currentPath,
    showExcludedFiles,
    refresh,
    loadDirectory,
  } = useProjectStoreShallow((state) => ({
    treeData: state.treeData,
    projectName: state.projectName,
    projectPath: state.projectPath,
    currentPath: state.currentPath,
    showExcludedFiles: state.showExcludedFiles,
    refresh: state.refresh,
    loadDirectory: state.loadDirectory,
  }));
  const globalExcludePatterns = useSettingsStore(
    (state) => state.globalExcludePatterns,
  );
  const addTask = useTaskStore((state) => state.addTask);
  const showToast = useUiStore((state) => state.showToast);
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
  const [searchTerm, setSearchTerm] = useState("");
  const [dropTargetPath, setDropTargetPath] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    file: FileInfo;
    x: number;
    y: number;
  } | null>(null);
  const lastTreeContextMenuTriggerRef = useRef<{
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
    targetDirectory: "",
  });
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const { movePathsToDirectory, conflictDialog } = useFileDropMove(async () => {
    await projectStore.getState().refresh();
  });

  const handleDragStart = useCallback(
    (path: string, event: React.DragEvent<HTMLDivElement>) => {
      startInternalDrag(event, [path]);
    },
    [startInternalDrag],
  );

  const handleDragEnd = useCallback(() => {
    finishInternalDrag();
    setDropTargetPath(null);
  }, [finishInternalDrag]);

  const canDropToDirectory = useCallback(
    (targetDir: string, dragPaths = draggedPaths) => {
      return canMovePathsToDirectory(targetDir, dragPaths);
    },
    [draggedPaths],
  );

  const handleDropToDirectory = useCallback(
    async (targetDir: string, dragPaths?: string[]) => {
      const currentDraggedPaths =
        dragPaths && dragPaths.length > 0 ? dragPaths : draggedPaths;
      if (currentDraggedPaths.length === 0) {
        return;
      }

      setDropTargetPath(null);
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
    [draggedPaths, movePathsToDirectory, projectStore],
  );

  const excludePatterns = projectPath
    ? mergeExcludePatterns(
        globalExcludePatterns,
        readProjectExcludePatterns(projectPath),
      )
    : [];

  const isExcluded = useCallback(
    (node: TreeNode) => {
      return (
        excludePatterns.length > 0 &&
        shouldExcludeFile(node.name, excludePatterns)
      );
    },
    [excludePatterns],
  );

  const visibleTreeData = useMemo(() => {
    if (!treeData) {
      return null;
    }
    return (
      filterTreeNode(treeData, !showExcludedFiles, isExcluded, true) ?? treeData
    );
  }, [isExcluded, showExcludedFiles, treeData]);

  useEffect(() => {
    if (!projectPath) {
      return;
    }

    void loadPlugins(projectPath);
  }, [loadPlugins, projectPath]);

  const openSystemContextMenu = useCallback(
    async (file: FileInfo) => {
      try {
        const result = await invoke<{ status: string }>(
          "show_system_context_menu",
          {
            paths: [file.path],
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
    [refresh, showToast],
  );

  const handleContextMenu = useCallback(
    (node: TreeNode, x: number, y: number) => {
      const file = toTreeNodeFileInfo(node);
      const now = Date.now();
      const lastTrigger = lastTreeContextMenuTriggerRef.current;
      const shouldOpenSystemMenu =
        lastTrigger?.path === file.path &&
        now - lastTrigger.timestamp <= SYSTEM_CONTEXT_DOUBLE_TRIGGER_MS;

      if (shouldOpenSystemMenu) {
        lastTreeContextMenuTriggerRef.current = null;
        setContextMenu(null);
        void openSystemContextMenu(file);
        return;
      }

      lastTreeContextMenuTriggerRef.current = {
        path: file.path,
        timestamp: now,
      };
      setContextMenu({ file, x, y });
    },
    [openSystemContextMenu],
  );

  const handleCloseContextMenu = useCallback(() => {
    setContextMenu(null);
  }, []);

  const buildFileContext = useCallback(
    (selectedItems: FileInfo[], contextPath: string | null) => ({
      projectPath: projectPath || "",
      currentPath: contextPath,
      selectedItems: buildPluginContextItems(selectedItems),
      trigger: "file-context",
      pluginScope: "",
      appVersion: APP_VERSION,
    }),
    [projectPath],
  );

  const contextDirectory = useMemo(
    () =>
      getTreeContextDirectory(
        contextMenu?.file ?? null,
        projectPath,
        currentPath,
      ),
    [contextMenu, currentPath, projectPath],
  );

  const fileContextPluginContext = useMemo(() => {
    if (!contextMenu) {
      return null;
    }

    return buildFileContext([contextMenu.file], contextDirectory);
  }, [buildFileContext, contextDirectory, contextMenu]);

  const fileContextPluginActions = useMemo(() => {
    if (!projectPath || !contextMenu || !fileContextPluginContext) {
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
    if (!projectPath || !contextMenu || !fileContextPluginContext) {
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

  useEffect(() => {
    if (!contextMenu || !fileContextPluginDebugInfo) {
      return;
    }

    console.info(
      "[plugin-debug:file-tree-context]",
      JSON.parse(fileContextPluginDebugInfo),
    );
  }, [contextMenu, fileContextPluginDebugInfo]);

  const runPluginAction = useCallback(
    (action: PluginAction, selectedItems: FileInfo[]) => {
      if (!projectPath) {
        return;
      }

      const context = buildFileContext(selectedItems, contextDirectory);
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
    [addTask, buildFileContext, contextDirectory, projectPath, showToast],
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
    const targetDirectory = contextDirectory;
    if (!targetDirectory) {
      return;
    }

    try {
      const suggestedName = await getSuggestedFolderName(targetDirectory);
      setCreateFolderDialog({
        isOpen: true,
        suggestedName,
        folderName: suggestedName,
        targetDirectory,
      });
    } catch (error) {
      console.error("Failed to open create folder dialog:", error);
      showToast({
        title: "创建失败",
        message: String(error),
        tone: "error",
      });
    }
  }, [contextDirectory, getSuggestedFolderName, showToast]);

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
      const targetDirectory = createFolderDialog.targetDirectory;
      if (!targetDirectory) {
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
        const targetPath = joinPath(targetDirectory, folderName);
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
    [createFolderDialog.targetDirectory, refresh, showToast],
  );

  const handleDelete = useCallback(
    async (file: FileInfo) => {
      try {
        const deletedCount = await invoke<number>("delete_paths", {
          paths: [file.path],
        });
        await refresh();

        if (deletedCount === 0) {
          showToast({
            title: "未删除任何项目",
            message: "选中的文件夹可能已经不存在，列表已刷新。",
            tone: "warning",
          });
          return;
        }

        showToast({
          title: "文件夹已移动到回收站",
          message: file.name,
          tone: "success",
        });
      } catch (error) {
        console.error("Failed to delete tree item:", error);
        showToast({
          title: "删除失败",
          message: String(error),
          tone: "error",
        });
      }
    },
    [refresh, showToast],
  );

  if (!visibleTreeData) {
    return (
      <div className="h-full flex items-center justify-center text-gray-400 text-sm">
        请先打开一个项目
      </div>
    );
  }

  const atProjectRoot =
    !currentPath || !projectPath || currentPath === projectPath;

  const handleGoUp = () => {
    if (atProjectRoot || !currentPath) {
      return;
    }

    void loadDirectory(getParentPath(currentPath));
  };

  return (
    <div className="h-full flex flex-col bg-white dark:bg-gray-900">
      <div className="px-3 py-2 border-b border-gray-200 dark:border-gray-700 flex items-center gap-2">
        <h2
          className="flex-1 font-semibold text-sm truncate"
          title={projectPath || ""}
        >
          {projectName}
        </h2>
        <button
          onClick={handleGoUp}
          disabled={atProjectRoot}
          className="p-1.5 rounded-md text-gray-500 hover:text-gray-900 hover:bg-gray-100 dark:text-gray-400 dark:hover:text-gray-100 dark:hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          title={atProjectRoot ? "已经在项目根目录" : "返回上级目录"}
        >
          <ArrowUp className="w-4 h-4" />
        </button>
        <button
          onClick={() => void refresh()}
          className="p-1.5 rounded-md text-gray-500 hover:text-gray-900 hover:bg-gray-100 dark:text-gray-400 dark:hover:text-gray-100 dark:hover:bg-gray-800 transition-colors"
          title="刷新"
        >
          <RefreshCw className="w-4 h-4" />
        </button>
      </div>

      <div className="px-2 py-2">
        <input
          type="text"
          placeholder="筛选文件夹..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="w-full px-2 py-1 text-sm border border-gray-300 dark:border-gray-600 rounded
                     bg-white dark:bg-gray-800 focus:outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>

      <div className="flex-1 overflow-auto">
        <TreeItem
          node={visibleTreeData}
          level={0}
          dropTargetPath={dropTargetPath}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          onDropToDirectory={handleDropToDirectory}
          onHoverDirectory={setDropTargetPath}
          canDropToDirectory={canDropToDirectory}
          getDraggedPathsFromDataTransfer={getDraggedPathsFromDataTransfer}
          onContextMenu={handleContextMenu}
          suppressInteraction={suppressInteraction}
          isExcluded={isExcluded}
          showExcludedFiles={showExcludedFiles}
        />
      </div>

      {contextMenu && (
        <FileContextMenu
          file={contextMenu.file}
          x={contextMenu.x}
          y={contextMenu.y}
          currentPath={contextDirectory || ""}
          projectPath={projectPath || ""}
          pluginActions={fileContextPluginActions}
          pluginDebugInfo={fileContextPluginDebugInfo}
          onClose={handleCloseContextMenu}
          onRefresh={handleRefresh}
          onShowDetails={handleShowDetails}
          onDelete={handleDelete}
          onCreateFolder={handleCreateFolder}
          onRunPluginAction={(action) =>
            runPluginAction(action, [contextMenu.file])
          }
        />
      )}

      <FileDetailsDialog
        file={detailsDialogFile}
        fileTagList={[]}
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
