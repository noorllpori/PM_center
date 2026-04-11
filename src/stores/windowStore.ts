import { create } from 'zustand';
import { persist } from 'zustand/middleware';

// 语言类型（与 CodeEditor 共享）
export type EditorLanguage =
  | 'plaintext'
  | 'python'
  | 'javascript'
  | 'typescript'
  | 'html'
  | 'css'
  | 'json'
  | 'rust'
  | 'markdown';

// 检测文件语言类型
export function detectLanguage(filename: string): EditorLanguage {
  const normalizedFilename = filename.split(/[\\/]/).pop()?.toLowerCase() || filename.toLowerCase();
  const ext = normalizedFilename.split('.').pop()?.toLowerCase();
  switch (ext) {
    case 'py':
    case 'pyi':
    case 'pyw':
      return 'python';
    case 'js':
    case 'mjs':
    case 'cjs':
      return 'javascript';
    case 'ts':
    case 'mts':
    case 'cts':
      return 'typescript';
    case 'tsx':
      return 'typescript';
    case 'jsx':
      return 'javascript';
    case 'html':
    case 'htm':
    case 'xml':
    case 'vue':
    case 'svelte':
    case 'astro':
      return 'html';
    case 'css':
      return 'css';
    case 'scss':
      return 'css';
    case 'sass':
      return 'css';
    case 'less':
      return 'css';
    case 'json':
    case 'jsonc':
      return 'json';
    case 'rs':
      return 'rust';
    case 'md':
    case 'markdown':
    case 'mdx':
    case 'mdt':
      return 'markdown';
    default:
      return 'plaintext';
  }
}

// 获取语言显示名称
export function getLanguageName(language: EditorLanguage): string {
  const names: Record<EditorLanguage, string> = {
    plaintext: '纯文本',
    python: 'Python',
    javascript: 'JavaScript',
    typescript: 'TypeScript',
    html: 'HTML',
    css: 'CSS',
    json: 'JSON',
    rust: 'Rust',
    markdown: 'Markdown',
  };
  return names[language];
}

// 窗口内容类型
export type WindowContentType = 
  | 'code-editor'
  | 'image-viewer'
  | 'markdown-preview'
  | 'terminal'
  | 'settings'
  | 'custom';

// 窗口状态
export interface WindowInstance {
  id: string;
  title: string;
  contentType: WindowContentType;
  // 内容特定的数据
  data: Record<string, unknown>;
  
  // 窗口状态
  isMinimized: boolean;
  isMaximized: boolean;
  isAlwaysOnTop: boolean;
  isFocused: boolean;
  
  // 位置和大小
  position: { x: number; y: number };
  size: { width: number; height: number };
  
  // 最小化前的位置和大小（用于恢复）
  prevState?: {
    position: { x: number; y: number };
    size: { width: number; height: number };
  };
  
  createdAt: number;
  updatedAt: number;
}

// 创建窗口的选项
export interface CreateWindowOptions {
  title?: string;
  contentType: WindowContentType;
  data?: Record<string, unknown>;
  position?: { x: number; y: number };
  size?: { width: number; height: number };
  isAlwaysOnTop?: boolean;
}

// 布局配置
export interface LayoutConfig {
  type: 'cascade' | 'tile' | 'stack';
  padding?: number;
}

// 生成唯一ID
function generateId(): string {
  return `win-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

// 默认窗口大小
const DEFAULT_SIZE = { width: 800, height: 600 };
const MIN_SIZE = { width: 300, height: 200 };

interface WindowState {
  // 所有窗口实例
  windows: WindowInstance[];
  // 当前焦点窗口ID
  focusedWindowId: string | null;
  // 窗口层级顺序（用于 z-index）
  windowOrder: string[];
  // 是否显示任务栏
  showTaskbar: boolean;
  // 吸附网格大小（0 表示关闭吸附）
  snapGrid: number;
  
  // 创建/关闭窗口
  createWindow: (options: CreateWindowOptions) => string;
  closeWindow: (id: string) => void;
  closeAllWindows: () => void;
  closeOtherWindows: (keepId: string) => void;
  
  // 窗口控制
  minimizeWindow: (id: string) => void;
  maximizeWindow: (id: string) => void;
  restoreWindow: (id: string) => void;
  toggleAlwaysOnTop: (id: string) => void;
  focusWindow: (id: string | null) => void;
  
  // 窗口移动/调整
  updatePosition: (id: string, position: { x: number; y: number }) => void;
  updateSize: (id: string, size: { width: number; height: number }) => void;
  moveWindowTo: (id: string, position: 'center' | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right') => void;
  
  // 窗口层级
  bringToFront: (id: string) => void;
  sendToBack: (id: string) => void;
  
  // 布局管理
  arrangeWindows: (config: LayoutConfig) => void;
  cascadeWindows: () => void;
  tileWindows: () => void;
  stackWindows: () => void;
  
  // 数据更新
  updateWindowData: (id: string, data: Record<string, unknown>) => void;
  updateWindowTitle: (id: string, title: string) => void;
  
  // 设置
  setShowTaskbar: (show: boolean) => void;
  setSnapGrid: (size: number) => void;
  
  // 获取窗口
  getWindowById: (id: string) => WindowInstance | undefined;
  getWindowsByType: (type: WindowContentType) => WindowInstance[];
  getVisibleWindows: () => WindowInstance[];
  getMinimizedWindows: () => WindowInstance[];
}

// 计算新窗口位置（避免重叠）
function calculateNewPosition(existingWindows: WindowInstance[]): { x: number; y: number } {
  const offset = 30;
  const count = existingWindows.filter(w => !w.isMinimized).length;
  return {
    x: 100 + (count * offset) % 300,
    y: 100 + (count * offset) % 300,
  };
}

// 吸附到网格
function snapToGrid(value: number, gridSize: number): number {
  if (gridSize <= 0) return value;
  return Math.round(value / gridSize) * gridSize;
}

export const useWindowStore = create<WindowState>()(
  persist(
    (set, get) => ({
      windows: [],
      focusedWindowId: null,
      windowOrder: [],
      showTaskbar: true,
      snapGrid: 8,

      createWindow: (options) => {
        const id = generateId();
        const existingWindows = get().windows;
        const position = options.position || calculateNewPosition(existingWindows);
        const size = options.size || { ...DEFAULT_SIZE };
        
        // 应用网格吸附
        const snapGrid = get().snapGrid;
        const snappedPosition = {
          x: snapToGrid(position.x, snapGrid),
          y: snapToGrid(position.y, snapGrid),
        };

        const windowInstance: WindowInstance = {
          id,
          title: options.title || 'Untitled',
          contentType: options.contentType,
          data: options.data || {},
          isMinimized: false,
          isMaximized: false,
          isAlwaysOnTop: options.isAlwaysOnTop || false,
          isFocused: true,
          position: snappedPosition,
          size,
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };

        set((state) => ({
          windows: [...state.windows, windowInstance],
          focusedWindowId: id,
          windowOrder: [...state.windowOrder.filter(wid => wid !== id), id],
        }));

        return id;
      },

      closeWindow: (id) => {
        set((state) => ({
          windows: state.windows.filter((w) => w.id !== id),
          focusedWindowId: state.focusedWindowId === id 
            ? state.windowOrder.filter(wid => wid !== id).pop() || null
            : state.focusedWindowId,
          windowOrder: state.windowOrder.filter((wid) => wid !== id),
        }));
      },

      closeAllWindows: () => {
        set({
          windows: [],
          focusedWindowId: null,
          windowOrder: [],
        });
      },

      closeOtherWindows: (keepId) => {
        set((state) => ({
          windows: state.windows.filter((w) => w.id === keepId),
          focusedWindowId: keepId,
          windowOrder: [keepId],
        }));
      },

      minimizeWindow: (id) => {
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id 
              ? { 
                  ...w, 
                  isMinimized: true, 
                  prevState: w.prevState || { position: w.position, size: w.size },
                  updatedAt: Date.now() 
                }
              : w
          ),
          focusedWindowId: state.focusedWindowId === id 
            ? state.windowOrder.filter(wid => wid !== id && !state.windows.find(w => w.id === wid)?.isMinimized).pop() || null
            : state.focusedWindowId,
        }));
      },

      maximizeWindow: (id) => {
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id && !w.isMaximized
              ? { 
                  ...w, 
                  isMaximized: true,
                  prevState: { position: w.position, size: w.size },
                  position: { x: 0, y: 0 },
                  size: { width: window.innerWidth, height: window.innerHeight },
                  updatedAt: Date.now() 
                }
              : w
          ),
        }));
      },

      restoreWindow: (id) => {
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id
              ? { 
                  ...w, 
                  isMinimized: false,
                  isMaximized: false,
                  position: w.prevState?.position || w.position,
                  size: w.prevState?.size || w.size,
                  isFocused: true,
                  updatedAt: Date.now() 
                }
              : w
          ),
          focusedWindowId: id,
        }));
      },

      toggleAlwaysOnTop: (id) => {
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id ? { ...w, isAlwaysOnTop: !w.isAlwaysOnTop } : w
          ),
        }));
      },

      focusWindow: (id) => {
        if (id === null) {
          set({ focusedWindowId: null });
          return;
        }
        
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id ? { ...w, isFocused: true } : { ...w, isFocused: false }
          ),
          focusedWindowId: id,
          windowOrder: [...state.windowOrder.filter(wid => wid !== id), id],
        }));
      },

      updatePosition: (id, position) => {
        const snapGrid = get().snapGrid;
        const snappedPosition = {
          x: snapToGrid(position.x, snapGrid),
          y: snapToGrid(position.y, snapGrid),
        };
        
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id ? { ...w, position: snappedPosition, updatedAt: Date.now() } : w
          ),
        }));
      },

      updateSize: (id, size) => {
        const constrainedSize = {
          width: Math.max(MIN_SIZE.width, size.width),
          height: Math.max(MIN_SIZE.height, size.height),
        };
        
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id ? { ...w, size: constrainedSize, updatedAt: Date.now() } : w
          ),
        }));
      },

      moveWindowTo: (id, position) => {
        const screenWidth = window.innerWidth;
        const screenHeight = window.innerHeight;
        const windowInstance = get().windows.find(w => w.id === id);
        if (!windowInstance) return;

        let newPosition = { x: 0, y: 0 };
        const padding = 10;

        switch (position) {
          case 'center':
            newPosition = {
              x: (screenWidth - windowInstance.size.width) / 2,
              y: (screenHeight - windowInstance.size.height) / 2,
            };
            break;
          case 'top-left':
            newPosition = { x: padding, y: padding };
            break;
          case 'top-right':
            newPosition = { x: screenWidth - windowInstance.size.width - padding, y: padding };
            break;
          case 'bottom-left':
            newPosition = { x: padding, y: screenHeight - windowInstance.size.height - padding };
            break;
          case 'bottom-right':
            newPosition = { 
              x: screenWidth - windowInstance.size.width - padding, 
              y: screenHeight - windowInstance.size.height - padding 
            };
            break;
        }

        get().updatePosition(id, newPosition);
      },

      bringToFront: (id) => {
        set((state) => ({
          windowOrder: [...state.windowOrder.filter(wid => wid !== id), id],
          focusedWindowId: id,
        }));
      },

      sendToBack: (id) => {
        set((state) => ({
          windowOrder: [id, ...state.windowOrder.filter(wid => wid !== id)],
        }));
      },

      arrangeWindows: (config) => {
        switch (config.type) {
          case 'cascade':
            get().cascadeWindows();
            break;
          case 'tile':
            get().tileWindows();
            break;
          case 'stack':
            get().stackWindows();
            break;
        }
      },

      cascadeWindows: () => {
        const visibleWindows = get().getVisibleWindows();
        const offset = 40;
        
        visibleWindows.forEach((w, index) => {
          get().updatePosition(w.id, {
            x: 50 + (index * offset),
            y: 50 + (index * offset),
          });
        });
      },

      tileWindows: () => {
        const visibleWindows = get().getVisibleWindows();
        const count = visibleWindows.length;
        if (count === 0) return;

        const cols = Math.ceil(Math.sqrt(count));
        const rows = Math.ceil(count / cols);
        const screenWidth = window.innerWidth;
        const screenHeight = window.innerHeight;
        const padding = 10;

        const windowWidth = (screenWidth - padding * (cols + 1)) / cols;
        const windowHeight = (screenHeight - padding * (rows + 1)) / rows;

        visibleWindows.forEach((w, index) => {
          const col = index % cols;
          const row = Math.floor(index / cols);
          
          get().updatePosition(w.id, {
            x: padding + col * (windowWidth + padding),
            y: padding + row * (windowHeight + padding),
          });
          get().updateSize(w.id, { width: windowWidth, height: windowHeight });
        });
      },

      stackWindows: () => {
        const visibleWindows = get().getVisibleWindows();
        const screenWidth = window.innerWidth;
        const screenHeight = window.innerHeight;
        
        visibleWindows.forEach((w) => {
          get().updatePosition(w.id, {
            x: (screenWidth - w.size.width) / 2,
            y: (screenHeight - w.size.height) / 2,
          });
        });
      },

      updateWindowData: (id, data) => {
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id ? { ...w, data: { ...w.data, ...data }, updatedAt: Date.now() } : w
          ),
        }));
      },

      updateWindowTitle: (id, title) => {
        set((state) => ({
          windows: state.windows.map((w) =>
            w.id === id ? { ...w, title, updatedAt: Date.now() } : w
          ),
        }));
      },

      setShowTaskbar: (show) => {
        set({ showTaskbar: show });
      },

      setSnapGrid: (size) => {
        set({ snapGrid: size });
      },

      getWindowById: (id) => {
        return get().windows.find((w) => w.id === id);
      },

      getWindowsByType: (type) => {
        return get().windows.filter((w) => w.contentType === type);
      },

      getVisibleWindows: () => {
        return get().windows.filter((w) => !w.isMinimized);
      },

      getMinimizedWindows: () => {
        return get().windows.filter((w) => w.isMinimized);
      },
    }),
    {
      name: 'pmcenter-window-storage',
      partialize: (state) => ({
        showTaskbar: state.showTaskbar,
        snapGrid: state.snapGrid,
      }),
    }
  )
);
