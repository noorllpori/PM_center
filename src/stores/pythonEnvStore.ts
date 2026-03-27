import { create } from 'zustand';
import { load } from '@tauri-apps/plugin-store';
import { invoke } from '@tauri-apps/api/core';

export interface PythonEnv {
  id: string;           // 唯一标识
  name: string;         // 显示名称
  path: string;         // python 可执行文件路径
  version?: string;     // Python 版本
  isSystem: boolean;    // 是否是系统 Python
  isVenv: boolean;      // 是否是虚拟环境
  venvPath?: string;    // 虚拟环境目录（如果是 venv）
}

interface PythonEnvState {
  // 所有检测到的环境
  envs: PythonEnv[];
  // 当前选中的环境 ID
  selectedEnvId: string | null;
  // 是否正在检测
  isDetecting: boolean;
  // 是否正在创建 venv
  isCreatingVenv: boolean;
  
  // 方法
  loadSettings: () => Promise<void>;
  detectEnvs: () => Promise<void>;
  selectEnv: (id: string) => Promise<void>;
  createVenv: (name: string, basePythonId?: string) => Promise<void>;
  deleteVenv: (id: string) => Promise<void>;
  installPackage: (envId: string, packageName: string) => Promise<string>;
  uninstallPackage: (envId: string, packageName: string) => Promise<string>;
  getInstalledPackages: (envId: string) => Promise<string[]>;
}

const STORE_FILE = 'python-envs.json';

export const usePythonEnvStore = create<PythonEnvState>((set, get) => ({
  envs: [],
  selectedEnvId: null,
  isDetecting: false,
  isCreatingVenv: false,

  loadSettings: async () => {
    try {
      const store = await load(STORE_FILE);
      const envs = await store.get<PythonEnv[]>('pythonEnvs');
      const selectedId = await store.get<string>('selectedPythonEnvId');
      
      if (envs) {
        set({ envs });
      }
      if (selectedId) {
        set({ selectedEnvId: selectedId });
      }
    } catch (error) {
      console.error('Failed to load Python env settings:', error);
    }
  },

  detectEnvs: async () => {
    set({ isDetecting: true, envs: [] }); // 清空列表显示加载状态
    try {
      // 同时调用：检测系统 Python + 扫描应用目录下的 venvs
      const systemEnvs = await invoke<PythonEnv[]>('detect_system_python');
      const appVenvs = await invoke<PythonEnv[]>('scan_app_venvs');
      
      // 合并：系统环境 + 应用目录下的 venvs，根据 path 去重
      const envMap = new Map<string, PythonEnv>();
      
      // 先添加系统环境
      systemEnvs.forEach(env => {
        const normalizedPath = env.path.toLowerCase().replace(/\\/g, '/');
        envMap.set(normalizedPath, env);
      });
      
      // 再添加 venvs（如果 path 已存在则覆盖，保持 venv 优先）
      appVenvs.forEach(env => {
        const normalizedPath = env.path.toLowerCase().replace(/\\/g, '/');
        envMap.set(normalizedPath, env);
      });
      
      let mergedEnvs = Array.from(envMap.values());
      
      // 排序：venv 优先显示在前面，然后按版本号排序（新版本在前）
      mergedEnvs.sort((a, b) => {
        // venv 优先
        if (a.isVenv && !b.isVenv) return -1;
        if (!a.isVenv && b.isVenv) return 1;
        
        // 同类型按版本排序（新版本在前）
        const versionA = a.version || '0';
        const versionB = b.version || '0';
        return versionB.localeCompare(versionA, undefined, { numeric: true });
      });
      
      set({ envs: mergedEnvs });
      
      // 保存
      const store = await load(STORE_FILE);
      await store.set('pythonEnvs', mergedEnvs);
      await store.save();
      
      // 如果没有选中环境，默认选第一个系统 Python 或第一个环境
      if (!get().selectedEnvId && mergedEnvs.length > 0) {
        const systemPython = mergedEnvs.find(e => e.isSystem);
        await get().selectEnv(systemPython?.id || mergedEnvs[0].id);
      }
    } catch (error) {
      console.error('Failed to detect Python envs:', error);
    } finally {
      set({ isDetecting: false });
    }
  },

  selectEnv: async (id: string) => {
    set({ selectedEnvId: id });
    try {
      const store = await load(STORE_FILE);
      await store.set('selectedPythonEnvId', id);
      await store.save();
    } catch (error) {
      console.error('Failed to save selected env:', error);
    }
  },

  createVenv: async (name: string, basePythonId?: string) => {
    set({ isCreatingVenv: true });
    try {
      let basePythonPath: string | undefined;
      if (basePythonId) {
        const baseEnv = get().envs.find(e => e.id === basePythonId);
        if (baseEnv) {
          basePythonPath = baseEnv.path;
        }
      }
      
      await invoke('create_venv', {
        name,
        basePythonPath,
      });
      
      // 创建成功后刷新列表
      await get().detectEnvs();
    } catch (error) {
      console.error('Failed to create venv:', error);
      throw error;
    } finally {
      set({ isCreatingVenv: false });
    }
  },

  deleteVenv: async (id: string) => {
    try {
      const env = get().envs.find(e => e.id === id);
      if (!env || env.isSystem) return;
      
      await invoke('delete_venv', { venvPath: env.venvPath });
      
      // 删除成功后刷新列表
      await get().detectEnvs();
    } catch (error) {
      console.error('Failed to delete venv:', error);
      throw error;
    }
  },

  installPackage: async (envId: string, packageName: string) => {
    const env = get().envs.find(e => e.id === envId);
    if (!env) throw new Error('Environment not found');
    
    return invoke('pip_install_package', {
      pythonPath: env.path,
      packageName,
    });
  },

  uninstallPackage: async (envId: string, packageName: string) => {
    const env = get().envs.find(e => e.id === envId);
    if (!env) throw new Error('Environment not found');
    
    return invoke('pip_uninstall_package', {
      pythonPath: env.path,
      packageName,
    });
  },

  getInstalledPackages: async (envId: string) => {
    const env = get().envs.find(e => e.id === envId);
    if (!env) return [];
    
    return invoke('pip_list_packages', {
      pythonPath: env.path,
    });
  },
}));
