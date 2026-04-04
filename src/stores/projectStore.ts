import { createContext, createElement, useContext, type ReactNode } from 'react';
import { useStore } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import { createStore } from 'zustand/vanilla';
import { invoke } from '@tauri-apps/api/core';
import { FileInfo, TreeNode, Tag, FileMetadata, ColumnConfig, DisplayRule, ViewMode } from '../types';
import { clearFileDetailsCache } from '../components/file-manager/useFileDetails';
import { useSettingsStore } from './settingsStore';
import { getParentPath, normalizePath } from '../components/file-manager/dragDrop';
import {
  mergeExcludePatterns,
  readProjectExcludePatterns,
  shouldExcludeFile,
} from '../utils/excludePatterns';

// 获取项目的排除规则
function getExcludePatterns(projectPath: string): string[] {
  const globalPatterns = useSettingsStore.getState().globalExcludePatterns;
  const projectPatterns = readProjectExcludePatterns(projectPath);
  return mergeExcludePatterns(globalPatterns, projectPatterns);
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

function buildExpandedPathChain(projectPath: string, targetPath: string): Set<string> {
  const expandedPaths = new Set<string>();
  const normalizedProjectPath = normalizePath(projectPath);
  const normalizedTargetPath = normalizePath(targetPath);

  if (
    normalizedTargetPath !== normalizedProjectPath &&
    !normalizedTargetPath.startsWith(`${normalizedProjectPath}/`)
  ) {
    expandedPaths.add(projectPath);
    return expandedPaths;
  }

  let currentPath = targetPath;

  while (true) {
    expandedPaths.add(currentPath);

    if (normalizePath(currentPath) === normalizedProjectPath) {
      break;
    }

    const parentPath = getParentPath(currentPath);
    if (!parentPath || normalizePath(parentPath) === normalizePath(currentPath)) {
      break;
    }

    currentPath = parentPath;
  }

  expandedPaths.add(projectPath);
  return expandedPaths;
}

// 默认列配置
const defaultColumns: ColumnConfig[] = [
  { key: 'name', title: '名称', width: 300, visible: true, sortable: true },
  { key: 'size', title: '大小', width: 100, visible: true, sortable: true, align: 'right' },
  { key: 'modified', title: '修改时间', width: 180, visible: true, sortable: true },
  { key: 'type', title: '类型', width: 100, visible: true, sortable: true },
  { key: 'tags', title: '标签', width: 150, visible: true, sortable: false },
];

export interface ProjectState {
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
  showExcludedFiles: boolean;

  // 操作
  setProject: (path: string) => Promise<void>;
  activateProject: () => Promise<void>;
  loadDirectory: (path: string, forceRefresh?: boolean, preserveSelection?: boolean) => Promise<void>;
  loadTree: (forceRefresh?: boolean) => Promise<void>;
  refresh: (forceRefresh?: boolean, preserveSelection?: boolean) => Promise<void>;
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
  toggleShowExcludedFiles: () => void;
}

function createInitialProjectState(): Omit<ProjectState, keyof {
  setProject: unknown;
  activateProject: unknown;
  loadDirectory: unknown;
  loadTree: unknown;
  refresh: unknown;
  closeProject: unknown;
  applyMovedPath: unknown;
  selectFile: unknown;
  clearSelection: unknown;
  toggleExpanded: unknown;
  setViewMode: unknown;
  updateColumn: unknown;
  reorderColumns: unknown;
  loadTags: unknown;
  addTag: unknown;
  deleteTag: unknown;
  addTagToFile: unknown;
  removeTagFromFile: unknown;
  search: unknown;
  clearSearch: unknown;
  toggleShowExcludedFiles: unknown;
}> {
  return {
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
    showExcludedFiles: false,
  };
}

export function createProjectStore() {
  return createStore<ProjectState>((set, get) => ({
    ...createInitialProjectState(),

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

        await get().loadDirectory(path);
        await get().loadTree();
        await get().loadTags();
      } catch (error) {
        console.error('Failed to initialize project:', error);
        throw error;
      }
    },

    activateProject: async () => {
      const { projectPath, isInitialized } = get();
      if (!projectPath || !isInitialized) {
        return;
      }

      try {
        await invoke('activate_project', { projectPath });
      } catch (error) {
        console.error('Failed to activate project:', error);
        throw error;
      }
    },

    loadDirectory: async (path: string, forceRefresh = false, preserveSelection = false) => {
      try {
        const activeProjectPath = get().projectPath;
        let files = await invoke<FileInfo[]>('read_directory', {
          path,
          projectPath: activeProjectPath,
          forceRefresh,
        });

        const { projectPath, showExcludedFiles } = get();
        if (projectPath && !showExcludedFiles) {
          const excludePatterns = getExcludePatterns(projectPath);
          files = files.filter((file) => !shouldExcludeFile(file.name, excludePatterns));
        }

        const fileTags = new Map<string, string[]>();
        if (activeProjectPath) {
          const filePaths = files.map((file) => file.path);
          if (filePaths.length > 0) {
            const tagsByPath = await invoke<Record<string, string[]>>('get_file_tags_batch', {
              projectPath: activeProjectPath,
              filePaths,
            });

            for (const [filePath, tags] of Object.entries(tagsByPath)) {
              if (Array.isArray(tags) && tags.length > 0) {
                fileTags.set(filePath, tags);
              }
            }
          }
        }

        const filePathSet = new Set(files.map((file) => file.path));

        set((state) => ({
          currentPath: path,
          files,
          fileTags: new Map([...state.fileTags, ...fileTags]),
          selectedFiles: preserveSelection
            ? new Set(Array.from(state.selectedFiles).filter((selectedPath) => filePathSet.has(selectedPath)))
            : new Set(),
          expandedKeys: projectPath
            ? new Set([...state.expandedKeys, ...buildExpandedPathChain(projectPath, path)])
            : state.expandedKeys,
        }));

      } catch (error) {
        console.error('Failed to load directory:', error);
      }
    },

    loadTree: async (forceRefresh = false) => {
      const { projectPath } = get();
      if (!projectPath) return;

      try {
        const treeData = await invoke<TreeNode>('get_directory_tree', {
          path: projectPath,
          projectPath,
          forceRefresh,
        });
        set({ treeData });
      } catch (error) {
        console.error('Failed to load tree:', error);
      }
    },

    refresh: async (forceRefresh = true, preserveSelection = false) => {
      const { currentPath, searchQuery } = get();
      if (searchQuery) {
        await get().search(searchQuery);
      } else if (currentPath) {
        await get().loadDirectory(currentPath, forceRefresh, preserveSelection);
      }
      await get().loadTree(forceRefresh);
    },

    closeProject: () => {
      clearFileDetailsCache();
      set({
        ...createInitialProjectState(),
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
          currentPath: state.currentPath
            ? (replacePathPrefix(state.currentPath, oldPath, newPath) || state.currentPath)
            : null,
          selectedFiles: nextSelectedFiles,
          expandedKeys: nextExpandedKeys,
          fileTags: nextFileTags,
        };
      });
    },

    selectFile: (path: string, multi = false) => {
      set((state) => {
        const newSelection = new Set(state.selectedFiles);

        if (multi) {
          if (newSelection.has(path)) {
            newSelection.delete(path);
          } else {
            newSelection.add(path);
          }
        } else {
          newSelection.clear();
          newSelection.add(path);
        }

        return { selectedFiles: newSelection };
      });
    },

    clearSelection: () => {
      set({ selectedFiles: new Set() });
    },

    toggleExpanded: (path: string) => {
      set((state) => {
        const newExpanded = new Set(state.expandedKeys);
        if (newExpanded.has(path)) {
          newExpanded.delete(path);
        } else {
          newExpanded.add(path);
        }
        return { expandedKeys: newExpanded };
      });
    },

    setViewMode: (mode: ViewMode) => {
      set({ viewMode: mode });
    },

    updateColumn: (key: string, updates: Partial<ColumnConfig>) => {
      set((state) => ({
        columns: state.columns.map((col) => (
          col.key === key ? { ...col, ...updates } : col
        )),
      }));
    },

    reorderColumns: (keys: string[]) => {
      set((state) => {
        const columnMap = new Map(state.columns.map((column) => [column.key, column]));
        return {
          columns: keys.map((key) => columnMap.get(key)!).filter(Boolean),
        };
      });
    },

    loadTags: async () => {
      const { projectPath } = get();
      if (!projectPath) {
        set({ tags: [] });
        return;
      }

      try {
        const tags = await invoke<Tag[]>('get_tags', { projectPath });
        set({ tags });
      } catch (error) {
        console.error('Failed to load tags:', error);
      }
    },

    addTag: async (tag: Omit<Tag, 'id'>) => {
      const { projectPath } = get();
      if (!projectPath) {
        return;
      }

      try {
        const id = `tag_${Date.now()}`;
        await invoke('add_tag', { projectPath, id, name: tag.name, color: tag.color });
        await get().loadTags();
      } catch (error) {
        console.error('Failed to add tag:', error);
      }
    },

    deleteTag: async (id: string) => {
      const { projectPath } = get();
      if (!projectPath) {
        return;
      }

      try {
        await invoke('delete_tag', { projectPath, id });
        await get().loadTags();
      } catch (error) {
        console.error('Failed to delete tag:', error);
      }
    },

    addTagToFile: async (filePath: string, tagId: string) => {
      const { projectPath } = get();
      if (!projectPath) {
        return;
      }

      try {
        await invoke('add_tag_to_file', { projectPath, filePath, tagId });
        set((state) => {
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

    removeTagFromFile: async (filePath: string, tagId: string) => {
      const { projectPath } = get();
      if (!projectPath) {
        return;
      }

      try {
        await invoke('remove_tag_from_file', { projectPath, filePath, tagId });
        set((state) => {
          const newFileTags = new Map(state.fileTags);
          const tags = newFileTags.get(filePath) || [];
          newFileTags.set(filePath, tags.filter((id) => id !== tagId));
          return { fileTags: newFileTags };
        });
      } catch (error) {
        console.error('Failed to remove tag from file:', error);
      }
    },

    search: async (query: string) => {
      const { projectPath, showExcludedFiles } = get();
      if (!projectPath || !query.trim()) {
        set({ searchQuery: '', searchResults: [], isSearching: false });
        return;
      }

      set({ isSearching: true, searchQuery: query });

      try {
        let results = await invoke<FileInfo[]>('search_files', {
          rootPath: projectPath,
          query: query.trim(),
        });
        if (!showExcludedFiles) {
          const excludePatterns = getExcludePatterns(projectPath);
          results = results.filter((file) => !shouldExcludeFile(file.name, excludePatterns));
        }
        set({ searchResults: results, isSearching: false });
      } catch (error) {
        console.error('Failed to search:', error);
        set({ isSearching: false });
      }
    },

    clearSearch: () => {
      set({ searchQuery: '', searchResults: [], isSearching: false });
    },

    toggleShowExcludedFiles: () => {
      set((state) => ({ showExcludedFiles: !state.showExcludedFiles }));
    },
  }));
}

export type ProjectStoreApi = ReturnType<typeof createProjectStore>;

const ProjectStoreContext = createContext<ProjectStoreApi | null>(null);
const fallbackProjectStore = createProjectStore();

export function ProjectStoreProvider({
  store,
  children,
}: {
  store: ProjectStoreApi;
  children: ReactNode;
}) {
  return createElement(ProjectStoreContext.Provider, { value: store }, children);
}

export function useProjectStoreApi() {
  const store = useContext(ProjectStoreContext);
  if (!store) {
    throw new Error('useProjectStoreApi must be used within a ProjectStoreProvider');
  }
  return store;
}

export function useOptionalProjectStoreApi() {
  return useContext(ProjectStoreContext) ?? fallbackProjectStore;
}

export function useProjectStore<T>(selector: (state: ProjectState) => T) {
  return useStore(useProjectStoreApi(), selector);
}

export function useOptionalProjectStore<T>(selector: (state: ProjectState) => T) {
  return useStore(useOptionalProjectStoreApi(), selector);
}

export function useProjectStoreShallow<T>(selector: (state: ProjectState) => T) {
  return useStore(useProjectStoreApi(), useShallow(selector));
}

export function useOptionalProjectStoreShallow<T>(selector: (state: ProjectState) => T) {
  return useStore(useOptionalProjectStoreApi(), useShallow(selector));
}
