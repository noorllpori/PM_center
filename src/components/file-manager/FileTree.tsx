import { useCallback, useState } from 'react';
import { TreeNode } from '../../types';
import { useProjectStore } from '../../stores/projectStore';
import { useFileDragStore } from '../../stores/fileDragStore';
import { ChevronRight, ChevronDown, Folder, FolderOpen } from 'lucide-react';
import { getInternalDragPaths, getPathLabel, getParentPath, isSameOrDescendantPath, setFileDragData } from './dragDrop';
import { useFileDropMove } from './useFileDropMove';

interface TreeItemProps {
  node: TreeNode;
  level: number;
  draggedPaths: string[];
  dropTargetPath: string | null;
  onDragStart: (path: string, event: React.DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  onDropToDirectory: (targetDir: string, dragPaths?: string[]) => Promise<void>;
  onHoverDirectory: (targetDir: string) => void;
}

function TreeItem({
  node,
  level,
  draggedPaths,
  dropTargetPath,
  onDragStart,
  onDragEnd,
  onDropToDirectory,
  onHoverDirectory,
}: TreeItemProps) {
  const { currentPath, expandedKeys, toggleExpanded, loadDirectory } = useProjectStore();
  const isExpanded = expandedKeys.has(node.path);
  const isSelected = currentPath === node.path;
  const isDropTarget = dropTargetPath === node.path;

  const canDropToDirectory = draggedPaths.some((path) => {
    if (getParentPath(path) === node.path) {
      return false;
    }

    return !isSameOrDescendantPath(node.path, path);
  });

  const handleClick = () => {
    loadDirectory(node.path);
  };

  const handleToggle = (e: React.MouseEvent) => {
    e.stopPropagation();
    toggleExpanded(node.path);
  };

  return (
    <div>
      <div
        className={`
          flex items-center py-1 px-2 cursor-pointer select-none
          hover:bg-gray-100 dark:hover:bg-gray-800
          ${isSelected ? 'bg-blue-50 dark:bg-blue-900/20 border-r-2 border-blue-500' : ''}
          ${isDropTarget ? 'bg-blue-50 ring-2 ring-inset ring-blue-500' : ''}
        `}
        style={{ paddingLeft: `${level * 16 + 8}px` }}
        onClick={handleClick}
        draggable={level > 0}
        onDragStart={(e) => onDragStart(node.path, e)}
        onDragEnd={onDragEnd}
        onDragOver={(e) => {
          const internalDragPaths = getInternalDragPaths(e.dataTransfer);
          if (internalDragPaths.length > 0 && !internalDragPaths.some((path) => {
            if (getParentPath(path) === node.path) {
              return false;
            }
            return !isSameOrDescendantPath(node.path, path);
          })) {
            return;
          }
          if (!canDropToDirectory && internalDragPaths.length === 0) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = 'move';
          onHoverDirectory(node.path);
        }}
        onDragEnter={(e) => {
          const internalDragPaths = getInternalDragPaths(e.dataTransfer);
          if (internalDragPaths.length > 0 && !internalDragPaths.some((path) => {
            if (getParentPath(path) === node.path) {
              return false;
            }
            return !isSameOrDescendantPath(node.path, path);
          })) {
            return;
          }
          if (!canDropToDirectory && internalDragPaths.length === 0) return;
          e.preventDefault();
          e.stopPropagation();
          onHoverDirectory(node.path);
        }}
        onDrop={async (e) => {
          const internalDragPaths = getInternalDragPaths(e.dataTransfer);
          if (internalDragPaths.length > 0 && !internalDragPaths.some((path) => {
            if (getParentPath(path) === node.path) {
              return false;
            }
            return !isSameOrDescendantPath(node.path, path);
          })) {
            return;
          }
          if (!canDropToDirectory && internalDragPaths.length === 0) return;
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
      </div>
      
      {isExpanded && node.children.length > 0 && (
        <div>
          {node.children.map(child => (
            <TreeItem
              key={child.path}
              node={child}
              level={level + 1}
              draggedPaths={draggedPaths}
              dropTargetPath={dropTargetPath}
              onDragStart={onDragStart}
              onDragEnd={onDragEnd}
              onDropToDirectory={onDropToDirectory}
              onHoverDirectory={onHoverDirectory}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function FileTree() {
  const { treeData, projectName, projectPath } = useProjectStore();
  const draggedPaths = useFileDragStore((state) => state.draggedPaths);
  const startDrag = useFileDragStore((state) => state.startDrag);
  const clearDrag = useFileDragStore((state) => state.clearDrag);
  const [searchTerm, setSearchTerm] = useState('');
  const [dropTargetPath, setDropTargetPath] = useState<string | null>(null);
  const { movePathsToDirectory, conflictDialog } = useFileDropMove(async () => {
    await useProjectStore.getState().refresh();
  });

  const handleDragStart = useCallback((path: string, event: React.DragEvent<HTMLDivElement>) => {
    const paths = [path];
    startDrag(paths);
    setFileDragData(event.dataTransfer, paths);
  }, [startDrag]);

  const handleDragEnd = useCallback(() => {
    clearDrag();
    setDropTargetPath(null);
  }, [clearDrag]);

  const handleDropToDirectory = useCallback(async (targetDir: string, dragPaths?: string[]) => {
    const currentDraggedPaths = dragPaths && dragPaths.length > 0 ? dragPaths : draggedPaths;
    if (currentDraggedPaths.length === 0) {
      return;
    }

    setDropTargetPath(null);
    await movePathsToDirectory(
      currentDraggedPaths,
      targetDir,
      getPathLabel(targetDir, useProjectStore.getState().projectPath, useProjectStore.getState().projectName),
    );
    clearDrag();
  }, [clearDrag, draggedPaths, movePathsToDirectory]);

  if (!treeData) {
    return (
      <div className="h-full flex items-center justify-center text-gray-400 text-sm">
        请先打开一个项目
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-white dark:bg-gray-900">
      {/* 项目标题 */}
      <div className="px-3 py-2 border-b border-gray-200 dark:border-gray-700">
        <h2 className="font-semibold text-sm truncate" title={projectPath || ''}>
          {projectName}
        </h2>
      </div>
      
      {/* 搜索 */}
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
      
      {/* 树 */}
      <div className="flex-1 overflow-auto">
        <TreeItem
          node={treeData}
          level={0}
          draggedPaths={draggedPaths}
          dropTargetPath={dropTargetPath}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          onDropToDirectory={handleDropToDirectory}
          onHoverDirectory={setDropTargetPath}
        />
      </div>

      {conflictDialog}
    </div>
  );
}
