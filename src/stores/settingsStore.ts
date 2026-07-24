import { create } from 'zustand';
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from '@tauri-apps/plugin-autostart';
import { load } from '@tauri-apps/plugin-store';
import { DEFAULT_EXCLUDE_PATTERNS } from '../utils/excludePatterns';

export interface RecentProject {
  path: string;
  name: string;
  openedAt: number; // 时间戳
}

export interface ToolPaths {
  ffprobe: string | null;
  ffmpeg: string | null;
  blender: string | null;
}

export interface BlenderInstallationInfo {
  path: string;
  version: string | null;
  versionLine: string | null;
  status: string;
  source: string;
  lastCheckedAt: number;
  message: string | null;
  isFavorite: boolean;
}

type BlenderInstallationInput = Omit<BlenderInstallationInfo, 'isFavorite'> & {
  isFavorite?: boolean;
};

interface SettingsState {
  recentProjects: RecentProject[];
  autoOpenLastProject: boolean;
  launchOnStartup: boolean;
  launchOnStartupAvailable: boolean;
  confirmProjectTabClose: boolean;
  confirmFileTabClose: boolean;
  projectsRootDir: string | null; // 项目根目录（扫描用）
  ignoredProjects: string[]; // 被忽略的项目路径列表
  toolPaths: ToolPaths;
  blenderInstallations: BlenderInstallationInfo[];
  globalExcludePatterns: string[];
  
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
  // 设置开机自启动
  setLaunchOnStartup: (enabled: boolean) => Promise<void>;
  // 设置关闭项目标签页时的确认提示
  setConfirmProjectTabClose: (enabled: boolean) => Promise<void>;
  // 设置关闭项目内工作区标签页时的确认提示
  setConfirmFileTabClose: (enabled: boolean) => Promise<void>;
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
  // 设置 Blender 安装列表
  setBlenderInstallations: (installations: BlenderInstallationInput[]) => Promise<void>;
  addOrUpdateBlenderInstallation: (installation: BlenderInstallationInput) => Promise<void>;
  updateBlenderInstallationFavorite: (path: string, isFavorite: boolean) => Promise<void>;
  removeBlenderInstallation: (path: string) => Promise<void>;
  // 设置全局排除规则
  setGlobalExcludePatterns: (patterns: string[]) => Promise<void>;
}

// Store 文件名
const STORE_FILE = 'settings.json';
const MAX_RECENT_PROJECTS = 10;
const LAUNCH_ON_STARTUP_KEY = 'launchOnStartup';

export const useSettingsStore = create<SettingsState>((set, get) => ({
  recentProjects: [],
  autoOpenLastProject: true,
  launchOnStartup: false,
  launchOnStartupAvailable: true,
  confirmProjectTabClose: true,
  confirmFileTabClose: false,
  projectsRootDir: null,
  ignoredProjects: [],
  toolPaths: {
    ffprobe: null,
    ffmpeg: null,
    blender: null,
  },
  blenderInstallations: [],
  globalExcludePatterns: [...DEFAULT_EXCLUDE_PATTERNS],

  loadSettings: async () => {
    try {
      const store = await load(STORE_FILE);
      const recent = await store.get<RecentProject[]>('recentProjects');
      const autoOpen = await store.get<boolean>('autoOpenLastProject');
      const launchOnStartup = await store.get<boolean>(LAUNCH_ON_STARTUP_KEY);
      const confirmProjectTabClose = await store.get<boolean>('confirmProjectTabClose');
      const confirmFileTabClose = await store.get<boolean>('confirmFileTabClose');
      const rootDir = await store.get<string | null>('projectsRootDir');
      const ignored = await store.get<string[]>('ignoredProjects');
      const toolPaths = await store.get<ToolPaths>('toolPaths');
      const blenderInstallations = await store.get<BlenderInstallationInput[]>('blenderInstallations');
      const globalExcludePatterns = await store.get<string[]>('globalExcludePatterns');
      
      if (recent) {
        // 过滤掉不存在的路径（可选，这里先保留）
        set({ recentProjects: recent });
      }
      
      if (autoOpen !== undefined) {
        set({ autoOpenLastProject: autoOpen });
      }

      if (launchOnStartup !== undefined) {
        set({ launchOnStartup });
      }

      if (confirmProjectTabClose !== undefined) {
        set({ confirmProjectTabClose });
      }

      if (confirmFileTabClose !== undefined) {
        set({ confirmFileTabClose });
      }
      
      if (rootDir !== undefined) {
        set({ projectsRootDir: rootDir });
      }
      
      if (ignored) {
        set({ ignoredProjects: ignored });
      }

      const nextToolPaths = {
        ffprobe: toolPaths?.ffprobe ?? null,
        ffmpeg: toolPaths?.ffmpeg ?? null,
        blender: toolPaths?.blender ?? null,
      };
      const nextBlenderInstallations = sanitizeBlenderInstallations(blenderInstallations);

      if (
        nextToolPaths.blender &&
        !nextBlenderInstallations.some((installation) =>
          normalizePathKey(installation.path) === normalizePathKey(nextToolPaths.blender!),
        )
      ) {
        nextBlenderInstallations.push({
          path: nextToolPaths.blender,
          version: null,
          versionLine: null,
          status: 'unknown',
          source: 'configured',
          lastCheckedAt: 0,
          message: '旧配置迁移，等待重新检测',
          isFavorite: false,
        });
      }

      const syncedToolPaths = syncToolPathsWithBlender(
        nextToolPaths,
        sortBlenderInstallations(nextBlenderInstallations),
      );

      set({
        toolPaths: syncedToolPaths,
        blenderInstallations: sortBlenderInstallations(nextBlenderInstallations),
      });

      if (globalExcludePatterns) {
        set({ globalExcludePatterns });
      }

      try {
        const autostartEnabled = await isAutostartEnabled();
        set({
          launchOnStartup: autostartEnabled,
          launchOnStartupAvailable: true,
        });

        if (launchOnStartup !== autostartEnabled) {
          await store.set(LAUNCH_ON_STARTUP_KEY, autostartEnabled);
          await store.save();
        }
      } catch (error) {
        console.error('Failed to read launch on startup state:', error);
        set({ launchOnStartupAvailable: false });
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

  setLaunchOnStartup: async (enabled: boolean) => {
    try {
      if (enabled) {
        await enableAutostart();
      } else {
        await disableAutostart();
      }

      const store = await load(STORE_FILE);
      await store.set(LAUNCH_ON_STARTUP_KEY, enabled);
      await store.save();

      set({
        launchOnStartup: enabled,
        launchOnStartupAvailable: true,
      });
    } catch (error) {
      console.error('Failed to set launch on startup:', error);
      throw error;
    }
  },

  setConfirmProjectTabClose: async (enabled: boolean) => {
    try {
      const store = await load(STORE_FILE);
      await store.set('confirmProjectTabClose', enabled);
      await store.save();
      set({ confirmProjectTabClose: enabled });
    } catch (error) {
      console.error('Failed to set project tab close confirmation:', error);
    }
  },

  setConfirmFileTabClose: async (enabled: boolean) => {
    try {
      const store = await load(STORE_FILE);
      await store.set('confirmFileTabClose', enabled);
      await store.save();
      set({ confirmFileTabClose: enabled });
    } catch (error) {
      console.error('Failed to set workspace tab close confirmation:', error);
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

  setGlobalExcludePatterns: async (patterns: string[]) => {
    try {
      const store = await load(STORE_FILE);
      await store.set('globalExcludePatterns', patterns);
      await store.save();

      set({ globalExcludePatterns: patterns });
    } catch (error) {
      console.error('Failed to set global exclude patterns:', error);
    }
  },

  setToolPath: async (tool, path) => {
    try {
      const nextToolPaths = syncToolPathsWithBlender({
        ...get().toolPaths,
        [tool]: path,
      }, get().blenderInstallations);

      const store = await load(STORE_FILE);
      await store.set('toolPaths', nextToolPaths);
      await store.save();

      set({ toolPaths: nextToolPaths });
    } catch (error) {
      console.error(`Failed to set tool path for ${tool}:`, error);
    }
  },

  setBlenderInstallations: async (installations) => {
    try {
      const nextInstallations = sanitizeBlenderInstallations(installations, get().blenderInstallations);
      const nextToolPaths = syncToolPathsWithBlender(get().toolPaths, nextInstallations);
      const store = await load(STORE_FILE);
      await store.set('blenderInstallations', nextInstallations);
      await store.set('toolPaths', nextToolPaths);
      await store.save();

      set({
        blenderInstallations: nextInstallations,
        toolPaths: nextToolPaths,
      });
    } catch (error) {
      console.error('Failed to set Blender installations:', error);
    }
  },

  addOrUpdateBlenderInstallation: async (installation) => {
    try {
      const nextInstallations = upsertBlenderInstallation(get().blenderInstallations, installation);
      const nextToolPaths = syncToolPathsWithBlender(get().toolPaths, nextInstallations);
      const store = await load(STORE_FILE);
      await store.set('blenderInstallations', nextInstallations);
      await store.set('toolPaths', nextToolPaths);
      await store.save();

      set({
        blenderInstallations: nextInstallations,
        toolPaths: nextToolPaths,
      });
    } catch (error) {
      console.error('Failed to add Blender installation:', error);
    }
  },

  updateBlenderInstallationFavorite: async (path, isFavorite) => {
    try {
      const key = normalizePathKey(path);
      const nextInstallations = sortBlenderInstallations(
        get().blenderInstallations.map((installation) =>
          normalizePathKey(installation.path) === key
            ? { ...installation, isFavorite }
            : installation,
        ),
      );
      const nextToolPaths = syncToolPathsWithBlender(get().toolPaths, nextInstallations);

      const store = await load(STORE_FILE);
      await store.set('blenderInstallations', nextInstallations);
      await store.set('toolPaths', nextToolPaths);
      await store.save();

      set({
        blenderInstallations: nextInstallations,
        toolPaths: nextToolPaths,
      });
    } catch (error) {
      console.error('Failed to update Blender favorite flag:', error);
    }
  },

  removeBlenderInstallation: async (path) => {
    try {
      const key = normalizePathKey(path);
      const nextInstallations = get().blenderInstallations.filter(
        (installation) => normalizePathKey(installation.path) !== key,
      );
      const nextToolPaths = syncToolPathsWithBlender(get().toolPaths, nextInstallations);

      const store = await load(STORE_FILE);
      await store.set('blenderInstallations', nextInstallations);
      await store.set('toolPaths', nextToolPaths);
      await store.save();

      set({
        blenderInstallations: nextInstallations,
        toolPaths: nextToolPaths,
      });
    } catch (error) {
      console.error('Failed to remove Blender installation:', error);
    }
  },
}));

function normalizePathKey(path: string) {
  return path.replace(/[\\/]+/g, '/').replace(/\/$/, '').toLowerCase();
}

function sanitizeBlenderInstallations(
  installations?: BlenderInstallationInput[] | null,
  existingInstallations: BlenderInstallationInfo[] = [],
): BlenderInstallationInfo[] {
  if (!Array.isArray(installations)) {
    return [];
  }

  return installations.reduce<BlenderInstallationInfo[]>((items, installation) => {
    if (!installation?.path) {
      return items;
    }

    const existingInstallation =
      findBlenderInstallation(items, installation.path) ??
      findBlenderInstallation(existingInstallations, installation.path);

    return upsertBlenderInstallation(
      items,
      normalizeBlenderInstallation(installation, existingInstallation),
    );
  }, []);
}

function upsertBlenderInstallation(
  installations: BlenderInstallationInfo[],
  installation: BlenderInstallationInput,
) {
  const normalizedInstallation = normalizeBlenderInstallation(
    installation,
    findBlenderInstallation(installations, installation.path),
  );
  const key = normalizePathKey(installation.path);
  const next = installations.filter((item) => normalizePathKey(item.path) !== key);
  next.push(normalizedInstallation);
  return sortBlenderInstallations(next);
}

function normalizeBlenderInstallation(
  installation: BlenderInstallationInput,
  existingInstallation?: BlenderInstallationInfo,
): BlenderInstallationInfo {
  return {
    path: installation.path,
    version: installation.version ?? null,
    versionLine: installation.versionLine ?? null,
    status: installation.status || 'unknown',
    source: installation.source || 'manual',
    lastCheckedAt: installation.lastCheckedAt || 0,
    message: installation.message ?? null,
    isFavorite: installation.isFavorite ?? existingInstallation?.isFavorite ?? false,
  };
}

function findBlenderInstallation(
  installations: BlenderInstallationInfo[],
  path: string,
) {
  const key = normalizePathKey(path);
  return installations.find((installation) => normalizePathKey(installation.path) === key);
}

function syncToolPathsWithBlender(
  toolPaths: ToolPaths,
  blenderInstallations: BlenderInstallationInfo[],
): ToolPaths {
  return {
    ...toolPaths,
    blender: resolveAutomaticBlenderPath(blenderInstallations),
  };
}

function resolveAutomaticBlenderPath(installations: BlenderInstallationInfo[]) {
  return installations.find((installation) => installation.status === 'ready')?.path ?? null;
}

function sortBlenderInstallations(installations: BlenderInstallationInfo[]) {
  return [...installations].sort((left, right) => {
    const versionCompare = compareVersion(right.version, left.version);
    if (versionCompare !== 0) {
      return versionCompare;
    }
    return left.path.localeCompare(right.path);
  });
}

function compareVersion(left?: string | null, right?: string | null) {
  const leftParts = (left || '').split('.').map((part) => Number.parseInt(part, 10) || 0);
  const rightParts = (right || '').split('.').map((part) => Number.parseInt(part, 10) || 0);
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const diff = (leftParts[index] || 0) - (rightParts[index] || 0);
    if (diff !== 0) {
      return diff;
    }
  }
  return 0;
}
