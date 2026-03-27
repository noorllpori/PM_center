import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { FileInfo, TreeNode, Tag, ColumnConfig, DisplayRule, ViewMode } from '../types';
import { clearFileDetailsCache } from '../components/file-manager/useFileDetails';

// 获取项目的排除规则
function getExcludePatterns(projectPath: string): string[] {
  const saved = localStorage.getItem(`project_exclude_${projectPath}`);
  if (saved) {
    return JSON.parse(saved);
  }
  // 默认规则
  return ['.pm_center', '.git', '*.tmp', '*.temp', 'Thumbs.db', '.DS_Store'];
}

// 检查文件名是否匹配排除规则
function shouldExclude(fileName: string, patterns: string[]): boolean {
  return patterns.some(pattern => {
    // 简单通配符匹配
    if (pattern.includes('*')) {
      const regex = new RegExp('^' + pattern.replace(/\./g, '\\.').replace(/\*/g, '.*') + '$');
      return regex.test(fileName);
    }
    // 精确匹配或目录匹配
    return fileName === pattern || fileName.startsWith(pattern + '/');
  });
}

function replacePathPrefix(path: string, oldPath: string, newPath: string): string | null {
  const normalizedPath = path.replace(/\\/g, '/');
  const normalizedOldPath = oldPath.replace(/\\/g, '/');

  if (normalizedPath === normalizedOldPath) {
    return newPath;
  }

  if (normalizedPath.startsWith(`${normalizedOldPath}/`)) {
    return newPath + path.slice(oldPath.length);
  }

  return null;
}

function getFileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

// 默认列配置
const defaultColumns: ColumnConfig[] = [
  { key: 'name', title: '名称', width: 300, visible: true, sortable: true },
  { key: 'size', title: '大小', width: 100, visible: true, sortable: true, align: 'right' },
  { key: 'modified', title: '修改时间', width: 180, visible: true, sortable: true },
  { key: 'type', title: '类型', width: 100, visible: true, sortable: true },
  { key: 'tags', title: '标签', width: 150, visible: true, sortable: false },
];

interface ProjectState {
  // 项目
  projectPath: string | null;
  projectName: string | null;
  isInitialized: boolean;
  
  // 文件浏览
  currentPath: string | null;
  files: FileInfo[];
  treeData: TreeNode | null;
  selectedFiles: Set<string>;
  expandedKeys: Set<string>;
  
  // 视图
  viewMode: ViewMode;
  columns: ColumnConfig[];
  displayRules: DisplayRule[];
  
  // 标签
  tags: Tag[];
  fileTags: Map<string, string[]>;
  
  // 搜索
  searchQuery: string;
  searchResults: FileInfo[];
  isSearching: boolean;
  
  // 操作
  setProject: (path: string) => Promise<void>;
  loadDirectory: (path: string) => Promise<void>;
  loadTree: () => Promise<void>;
  refresh: () => Promise<void>;
  closeProject: () => void;
  applyMovedPath: (oldPath: string, newPath: string) => void;
  
  // 选择
  selectFile: (path: string, multi?: boolean) => void;
  clearSelection: () => void;
  toggleExpanded: (path: string) => void;
  
  // 视图设置
  setViewMode: (mode: ViewMode) => void;
  updateColumn: (key: string, updates: Partial<ColumnConfig>) => void;
  reorderColumns: (keys: string[]) => void;
  
  // 标签
  loadTags: () => Promise<void>;
  addTag: (tag: Omit<Tag, 'id'>) => Promise<void>;
  deleteTag: (id: string) => Promise<void>;
  addTagToFile: (filePath: string, tagId: string) => Promise<void>;
  removeTagFromFile: (filePath: string, tagId: string) => Promise<void>;
  
  // 搜索
  search: (query: string) => Promise<void>;
  clearSearch: () => void;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  // 初始状态
  projectPath: null,
  projectName: null,
  isInitialized: false,
  currentPath: null,
  files: [],
  treeData: null,
  selectedFiles: new Set(),
  expandedKeys: new Set(),
  viewMode: 'list',
  columns: defaultColumns,
  displayRules: [],
  tags: [],
  fileTags: new Map(),
  searchQuery: '',
  searchResults: [],
  isSearching: false,

  // 设置项目
  setProject: async (path: string) => {
    try {
      clearFileDetailsCache();
      await invoke('init_project', { projectPath: path });
      const name = path.split(/[\\/]/).pop() || 'Project';
      
      set({
        projectPath: path,
        projectName: name,
        isInitialized: true,
        currentPath: path,
        expandedKeys: new Set([path]),
      });
      
      // 加载初始数据
      await get().loadDirectory(path);
      await get().loadTree();
      await get().loadTags();
    } catch (error) {
      console.error('Failed to initialize project:', error);
      throw error;
    }
  },

  // 加载目录
  loadDirectory: async (path: string) => {
    try {
      let files = await invoke<FileInfo[]>('read_directory', { path });
      
      // 应用排除规则过滤
      const projectPath = get().projectPath;
      if (projectPath) {
        const excludePatterns = getExcludePatterns(projectPath);
        files = files.filter(file => !shouldExclude(file.name, excludePatterns));
      }
      
      // 加载每个文件的标签
      const fileTags = new Map<string, string[]>();
      for (const file of files) {
        const tags = await invoke<string[]>('get_file_tags', { filePath: file.path });
        if (tags.length > 0) {
          fileTags.set(file.path, tags);
        }
      }
      
      set(state => ({
        currentPath: path,
        files,
        fileTags: new Map([...state.fileTags, ...fileTags]),
        selectedFiles: new Set(),
      }));
    } catch (error) {
      console.error('Failed to load directory:', error);
    }
  },

  // 加载树
  loadTree: async () => {
    const { projectPath } = get();
    if (!projectPath) return;
    
    try {
      const treeData = await invoke<TreeNode>('get_directory_tree', { path: projectPath });
      set({ treeData });
    } catch (error) {
      console.error('Failed to load tree:', error);
    }
  },

  // 刷新
  refresh: async () => {
    const { currentPath, searchQuery } = get();
    if (searchQuery) {
      await get().search(searchQuery);
    } else if (currentPath) {
      await get().loadDirectory(currentPath);
    }
    await get().loadTree();
  },

  // 关闭项目（返回项目列表）
  closeProject: () => {
    clearFileDetailsCache();
    set({
      projectPath: null,
      projectName: null,
      isInitialized: false,
      currentPath: null,
      files: [],
      treeData: null,
      selectedFiles: new Set(),
      searchQuery: '',
      searchResults: [],
    });
  },

  applyMovedPath: (oldPath, newPath) => {
    set((state) => {
      const nextFiles = state.files.map((file) => {
        const replacedPath = replacePathPrefix(file.path, oldPath, newPath);
        if (!replacedPath) {
          return file;
        }

        return {
          ...file,
          path: replacedPath,
          name: getFileName(replacedPath),
        };
      });

      const nextSelectedFiles = new Set<string>();
      state.selectedFiles.forEach((path) => {
        nextSelectedFiles.add(replacePathPrefix(path, oldPath, newPath) || path);
      });

      const nextExpandedKeys = new Set<string>();
      state.expandedKeys.forEach((path) => {
        nextExpandedKeys.add(replacePathPrefix(path, oldPath, newPath) || path);
      });

      const nextFileTags = new Map<string, string[]>();
      state.fileTags.forEach((tagIds, path) => {
        nextFileTags.set(replacePathPrefix(path, oldPath, newPath) || path, tagIds);
      });

      return {
        files: nextFiles,
        currentPath: state.currentPath ? (replacePathPrefix(state.currentPath, oldPath, newPath) || state.currentPath) : null,
        selectedFiles: nextSelectedFiles,
        expandedKeys: nextExpandedKeys,
        fileTags: nextFileTags,
      };
    });
  },

  // 选择文件
  selectFile: (path: string, multi = false) => {
    set(state => {
      const newSelection = new Set(state.selectedFiles);
      
      if (multi) {
        if (newSelection.has(path)) {
          newSelection.delete(path);
        } else {
          newSelection.add(path);
        }
      } else {
        if (newSelection.has(path) && newSelection.size === 1) {
          newSelection.clear();
        } else {
          newSelection.clear();
          newSelection.add(path);
        }
      }
      
      return { selectedFiles: newSelection };
    });
  },

  // 清除选择
  clearSelection: () => {
    set({ selectedFiles: new Set() });
  },

  // 展开/折叠
  toggleExpanded: (path: string) => {
    set(state => {
      const newExpanded = new Set(state.expandedKeys);
      if (newExpanded.has(path)) {
        newExpanded.delete(path);
      } else {
        newExpanded.add(path);
      }
      return { expandedKeys: newExpanded };
    });
  },

  // 设置视图模式
  setViewMode: (mode: ViewMode) => {
    set({ viewMode: mode });
  },

  // 更新列
  updateColumn: (key: string, updates: Partial<ColumnConfig>) => {
    set(state => ({
      columns: state.columns.map(col =>
        col.key === key ? { ...col, ...updates } : col
      ),
    }));
  },

  // 重新排序列
  reorderColumns: (keys: string[]) => {
    set(state => {
      const columnMap = new Map(state.columns.map(c => [c.key, c]));
      return {
        columns: keys.map(key => columnMap.get(key)!).filter(Boolean),
      };
    });
  },

  // 加载标签
  loadTags: async () => {
    try {
      const tags = await invoke<Tag[]>('get_tags');
      set({ tags });
    } catch (error) {
      console.error('Failed to load tags:', error);
    }
  },

  // 添加标签
  addTag: async (tag: Omit<Tag, 'id'>) => {
    try {
      const id = `tag_${Date.now()}`;
      await invoke('add_tag', { id, name: tag.name, color: tag.color });
      await get().loadTags();
    } catch (error) {
      console.error('Failed to add tag:', error);
    }
  },

  // 删除标签
  deleteTag: async (id: string) => {
    try {
      await invoke('delete_tag', { id });
      await get().loadTags();
    } catch (error) {
      console.error('Failed to delete tag:', error);
    }
  },

  // 添加文件标签
  addTagToFile: async (filePath: string, tagId: string) => {
    try {
      await invoke('add_tag_to_file', { filePath, tagId });
      set(state => {
        const newFileTags = new Map(state.fileTags);
        const tags = newFileTags.get(filePath) || [];
        if (!tags.includes(tagId)) {
          newFileTags.set(filePath, [...tags, tagId]);
        }
        return { fileTags: newFileTags };
      });
    } catch (error) {
      console.error('Failed to add tag to file:', error);
    }
  },

  // 移除文件标签
  removeTagFromFile: async (filePath: string, tagId: string) => {
    try {
      await invoke('remove_tag_from_file', { filePath, tagId });
      set(state => {
        const newFileTags = new Map(state.fileTags);
        const tags = newFileTags.get(filePath) || [];
        newFileTags.set(filePath, tags.filter(id => id !== tagId));
        return { fileTags: newFileTags };
      });
    } catch (error) {
      console.error('Failed to remove tag from file:', error);
    }
  },

  // 搜索
  search: async (query: string) => {
    const { projectPath } = get();
    if (!projectPath || !query.trim()) {
      set({ searchQuery: '', searchResults: [], isSearching: false });
      return;
    }
    
    set({ isSearching: true, searchQuery: query });
    
    try {
      const results = await invoke<FileInfo[]>('search_files', {
        rootPath: projectPath,
        query: query.trim(),
      });
      set({ searchResults: results, isSearching: false });
    } catch (error) {
      console.error('Failed to search:', error);
      set({ isSearching: false });
    }
  },

  // 清除搜索
  clearSearch: () => {
    set({ searchQuery: '', searchResults: [], isSearching: false });
  },
}));
