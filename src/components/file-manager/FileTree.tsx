import { useState } from 'react';
import { TreeNode } from '../../types';
import { useProjectStore } from '../../stores/projectStore';
import { ChevronRight, ChevronDown, Folder, FolderOpen } from 'lucide-react';

interface TreeItemProps {
  node: TreeNode;
  level: number;
}

function TreeItem({ node, level }: TreeItemProps) {
  const { currentPath, expandedKeys, toggleExpanded, loadDirectory } = useProjectStore();
  const isExpanded = expandedKeys.has(node.path);
  const isSelected = currentPath === node.path;

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
        `}
        style={{ paddingLeft: `${level * 16 + 8}px` }}
        onClick={handleClick}
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
            <TreeItem key={child.path} node={child} level={level + 1} />
          ))}
        </div>
      )}
    </div>
  );
}

export function FileTree() {
  const { treeData, projectName, projectPath } = useProjectStore();
  const [searchTerm, setSearchTerm] = useState('');

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
        <TreeItem node={treeData} level={0} />
      </div>
    </div>
  );
}
