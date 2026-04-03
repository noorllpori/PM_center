import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { TreeNode } from '../../types';
import { useProjectStoreApi, useProjectStoreShallow } from '../../stores/projectStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { ChevronRight, ChevronDown, Folder, FolderOpen, ArrowUp, RefreshCw } from 'lucide-react';
import { canMovePathsToDirectory, getParentPath, getPathLabel } from './dragDrop';
import { useFileDropMove } from './useFileDropMove';
import { useInternalFileDrag } from './useInternalFileDrag';
import {
  mergeExcludePatterns,
  readProjectExcludePatterns,
  shouldExcludeFile,
} from '../../utils/excludePatterns';

interface TreeItemProps {
  node: TreeNode;
  level: number;
  dropTargetPath: string | null;
  onDragStart: (path: string, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
  canDropToDirectory: (targetDir: string, dragPaths?: string[]) => boolean;
  getDraggedPathsFromDataTransfer: (dataTransfer: DataTransfer | null) => string[];
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
  suppressInteraction,
  isExcluded,
  showExcludedFiles,
}: TreeItemProps) {
  const { currentPath, expandedKeys, toggleExpanded, loadDirectory } = useProjectStoreShallow((state) => ({
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

  useEffect(() => {
    if (!isSelected) {
      return;
    }

    itemRef.current?.scrollIntoView({
      block: 'nearest',
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
          flex items-center py-1 px-2 cursor-pointer select-none
          hover:bg-gray-100 dark:hover:bg-gray-800
          ${isSelected ? 'bg-blue-50 dark:bg-blue-900/20 border-r-2 border-blue-500' : ''}
          ${isDropTarget ? 'bg-blue-50 ring-2 ring-inset ring-blue-500' : ''}
          ${showExcludedFiles && excluded ? 'opacity-70' : ''}
        `}
        style={{ paddingLeft: `${level * 16 + 8}px` }}
        onClick={handleClick}
        draggable={level > 0}
        onDragStart={(e) => onDragStart(node.path, e)}
        onDragEnd={onDragEnd}
        onDragOver={(e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
          if (!canDropToDirectory(node.path, internalDragPaths)) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = 'move';
          onHoverDirectory(node.path);
        }}
        onDragEnter={(e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
          if (!canDropToDirectory(node.path, internalDragPaths)) return;
          e.preventDefault();
          e.stopPropagation();
          onHoverDirectory(node.path);
        }}
        onDrop={async (e) => {
          const internalDragPaths = getDraggedPathsFromDataTransfer(e.dataTransfer);
          if (!canDropToDirectory(node.path, internalDragPaths)) return;
          e.preventDefault();
          e.stopPropagation();
          await onDropToDirectory(node.path, internalDragPaths);
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
      </div>
      
      {isExpanded && node.children.length > 0 && (
        <div>
          {node.children.map(child => (
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
  const globalExcludePatterns = useSettingsStore((state) => state.globalExcludePatterns);
  const {
    draggedPaths,
    startInternalDrag,
    finishInternalDrag,
    suppressInteraction,
    getDraggedPathsFromDataTransfer,
  } = useInternalFileDrag();
  const [searchTerm, setSearchTerm] = useState('');
  const [dropTargetPath, setDropTargetPath] = useState<string | null>(null);
  const { movePathsToDirectory, conflictDialog } = useFileDropMove(async () => {
    await projectStore.getState().refresh();
  });

  const handleDragStart = useCallback((path: string, event: React.DragEvent<HTMLDivElement>) => {
    startInternalDrag(event, [path]);
  }, [startInternalDrag]);

  const handleDragEnd = useCallback(() => {
    finishInternalDrag();
    setDropTargetPath(null);
  }, [finishInternalDrag]);

  const canDropToDirectory = useCallback((targetDir: string, dragPaths = draggedPaths) => {
    return canMovePathsToDirectory(targetDir, dragPaths);
  }, [draggedPaths]);

  const handleDropToDirectory = useCallback(async (targetDir: string, dragPaths?: string[]) => {
    const currentDraggedPaths = dragPaths && dragPaths.length > 0 ? dragPaths : draggedPaths;
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
  }, [draggedPaths, movePathsToDirectory, projectStore]);

  const excludePatterns = projectPath
    ? mergeExcludePatterns(globalExcludePatterns, readProjectExcludePatterns(projectPath))
    : [];

  const isExcluded = useCallback((node: TreeNode) => {
    return excludePatterns.length > 0 && shouldExcludeFile(node.name, excludePatterns);
  }, [excludePatterns]);

  const visibleTreeData = useMemo(() => {
    if (!treeData) {
      return null;
    }
    return filterTreeNode(treeData, !showExcludedFiles, isExcluded, true) ?? treeData;
  }, [isExcluded, showExcludedFiles, treeData]);

  if (!visibleTreeData) {
    return (
      <div className="h-full flex items-center justify-center text-gray-400 text-sm">
        请先打开一个项目
      </div>
    );
  }

  const atProjectRoot = !currentPath || !projectPath || currentPath === projectPath;

  const handleGoUp = () => {
    if (atProjectRoot || !currentPath) {
      return;
    }

    void loadDirectory(getParentPath(currentPath));
  };

  return (
    <div className="h-full flex flex-col bg-white dark:bg-gray-900">
      <div className="px-3 py-2 border-b border-gray-200 dark:border-gray-700 flex items-center gap-2">
        <h2 className="flex-1 font-semibold text-sm truncate" title={projectPath || ''}>
          {projectName}
        </h2>
        <button
          onClick={handleGoUp}
          disabled={atProjectRoot}
          className="p-1.5 rounded-md text-gray-500 hover:text-gray-900 hover:bg-gray-100 dark:text-gray-400 dark:hover:text-gray-100 dark:hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          title={atProjectRoot ? '已经在项目根目录' : '返回上级目录'}
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
          suppressInteraction={suppressInteraction}
          isExcluded={isExcluded}
          showExcludedFiles={showExcludedFiles}
        />
      </div>

      {conflictDialog}
    </div>
  );
}
