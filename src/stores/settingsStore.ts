import { create } from 'zustand';
import { load } from '@tauri-apps/plugin-store';

export interface RecentProject {
  path: string;
  name: string;
  openedAt: number; // 时间戳
}

export interface ToolPaths {
  ffprobe: string | null;
  blender: string | null;
}

interface SettingsState {
  recentProjects: RecentProject[];
  autoOpenLastProject: boolean;
  projectsRootDir: string | null; // 项目根目录（扫描用）
  ignoredProjects: string[]; // 被忽略的项目路径列表
  toolPaths: ToolPaths;
  
  // 加载设置
  loadSettings: () => Promise<void>;
  // 添加最近项目
  addRecentProject: (path: string, name: string) => Promise<void>;
  // 移除最近项目
  removeRecentProject: (path: string) => Promise<void>;
  // 清除所有历史
  clearAllRecentProjects: () => Promise<void>;
  // 设置自动打开
  setAutoOpen: (enabled: boolean) => Promise<void>;
  // 设置项目根目录
  setProjectsRootDir: (path: string | null) => Promise<void>;
  // 忽略项目
  ignoreProject: (path: string) => Promise<void>;
  // 取消忽略项目
  unignoreProject: (path: string) => Promise<void>;
  // 清除所有忽略
  clearIgnoredProjects: () => Promise<void>;
  // 设置工具路径
  setToolPath: (tool: keyof ToolPaths, path: string | null) => Promise<void>;
}

// Store 文件名
const STORE_FILE = 'settings.json';
const MAX_RECENT_PROJECTS = 10;

export const useSettingsStore = create<SettingsState>((set, get) => ({
  recentProjects: [],
  autoOpenLastProject: true,
  projectsRootDir: null,
  ignoredProjects: [],
  toolPaths: {
    ffprobe: null,
    blender: null,
  },

  loadSettings: async () => {
    try {
      const store = await load(STORE_FILE);
      const recent = await store.get<RecentProject[]>('recentProjects');
      const autoOpen = await store.get<boolean>('autoOpenLastProject');
      const rootDir = await store.get<string | null>('projectsRootDir');
      const ignored = await store.get<string[]>('ignoredProjects');
      const toolPaths = await store.get<ToolPaths>('toolPaths');
      
      if (recent) {
        // 过滤掉不存在的路径（可选，这里先保留）
        set({ recentProjects: recent });
      }
      
      if (autoOpen !== undefined) {
        set({ autoOpenLastProject: autoOpen });
      }
      
      if (rootDir !== undefined) {
        set({ projectsRootDir: rootDir });
      }
      
      if (ignored) {
        set({ ignoredProjects: ignored });
      }

      if (toolPaths) {
        set({
          toolPaths: {
            ffprobe: toolPaths.ffprobe ?? null,
            blender: toolPaths.blender ?? null,
          },
        });
      }
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  },

  addRecentProject: async (path: string, name: string) => {
    try {
      const store = await load(STORE_FILE);
      
      // 获取现有列表
      let recent = await store.get<RecentProject[]>('recentProjects') || [];
      
      // 移除重复项（如果存在）
      recent = recent.filter(p => p.path !== path);
      
      // 添加到开头
      recent.unshift({
        path,
        name,
        openedAt: Date.now(),
      });
      
      // 限制数量
      if (recent.length > MAX_RECENT_PROJECTS) {
        recent = recent.slice(0, MAX_RECENT_PROJECTS);
      }
      
      await store.set('recentProjects', recent);
      await store.save();
      
      set({ recentProjects: recent });
    } catch (error) {
      console.error('Failed to add recent project:', error);
    }
  },

  removeRecentProject: async (path: string) => {
    try {
      const store = await load(STORE_FILE);
      
      let recent = await store.get<RecentProject[]>('recentProjects') || [];
      recent = recent.filter(p => p.path !== path);
      
      await store.set('recentProjects', recent);
      await store.save();
      
      set({ recentProjects: recent });
    } catch (error) {
      console.error('Failed to remove recent project:', error);
    }
  },

  clearAllRecentProjects: async () => {
    try {
      const store = await load(STORE_FILE);
      await store.delete('recentProjects');
      await store.save();
      
      set({ recentProjects: [] });
    } catch (error) {
      console.error('Failed to clear recent projects:', error);
    }
  },

  setAutoOpen: async (enabled: boolean) => {
    try {
      const store = await load(STORE_FILE);
      await store.set('autoOpenLastProject', enabled);
      await store.save();
      
      set({ autoOpenLastProject: enabled });
    } catch (error) {
      console.error('Failed to set auto open:', error);
    }
  },

  setProjectsRootDir: async (path: string | null) => {
    try {
      const store = await load(STORE_FILE);
      if (path) {
        await store.set('projectsRootDir', path);
      } else {
        await store.delete('projectsRootDir');
      }
      await store.save();
      
      set({ projectsRootDir: path });
    } catch (error) {
      console.error('Failed to set projects root dir:', error);
    }
  },

  ignoreProject: async (path: string) => {
    try {
      const store = await load(STORE_FILE);
      let ignored = await store.get<string[]>('ignoredProjects') || [];
      
      // 避免重复
      if (!ignored.includes(path)) {
        ignored.push(path);
        await store.set('ignoredProjects', ignored);
        await store.save();
      }
      
      set(state => ({ ignoredProjects: [...state.ignoredProjects, path] }));
    } catch (error) {
      console.error('Failed to ignore project:', error);
    }
  },

  unignoreProject: async (path: string) => {
    try {
      const store = await load(STORE_FILE);
      let ignored = await store.get<string[]>('ignoredProjects') || [];
      ignored = ignored.filter(p => p !== path);
      
      await store.set('ignoredProjects', ignored);
      await store.save();
      
      set(state => ({ 
        ignoredProjects: state.ignoredProjects.filter(p => p !== path) 
      }));
    } catch (error) {
      console.error('Failed to unignore project:', error);
    }
  },

  clearIgnoredProjects: async () => {
    try {
      const store = await load(STORE_FILE);
      await store.delete('ignoredProjects');
      await store.save();
      
      set({ ignoredProjects: [] });
    } catch (error) {
      console.error('Failed to clear ignored projects:', error);
    }
  },

  setToolPath: async (tool, path) => {
    try {
      const nextToolPaths = {
        ...get().toolPaths,
        [tool]: path,
      };

      const store = await load(STORE_FILE);
      await store.set('toolPaths', nextToolPaths);
      await store.save();

      set({ toolPaths: nextToolPaths });
    } catch (error) {
      console.error(`Failed to set tool path for ${tool}:`, error);
    }
  },
}));
