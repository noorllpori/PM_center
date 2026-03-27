import { useState, useEffect } from 'react';
import { useSettingsStore } from '../stores/settingsStore';
import { scanProjectsRoot, createProject, ScannedProject } from '../api/projects';
import { invoke } from '@tauri-apps/api/core';
import { Dialog, ConfirmDialog, AlertDialog } from './Dialog';
import { SettingsPanel } from './SettingsPanel';
import pmcLogo from '../assets/pmc-logo.png';
import { 
  Folder, FolderOpen, Plus, Clock, X, Trash2, Settings, 
  RefreshCw, FolderPlus, ChevronRight, EyeOff, Eye
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';

// 格式化时间
function formatTime(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  
  if (diff < 60 * 60 * 1000) {
    const minutes = Math.floor(diff / (60 * 1000));
    return minutes < 1 ? '刚刚' : `${minutes}分钟前`;
  }
  
  if (diff < 24 * 60 * 60 * 1000) {
    const hours = Math.floor(diff / (60 * 60 * 1000));
    return `${hours}小时前`;
  }
  
  return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

interface WelcomeScreenProps {
  onOpenProject: (path: string) => void;
}

export function WelcomeScreen({ onOpenProject }: WelcomeScreenProps) {
  const { 
    recentProjects, 
    projectsRootDir,
    ignoredProjects,
    loadSettings, 
    removeRecentProject,
    clearAllRecentProjects,
    setProjectsRootDir,
    ignoreProject,
    unignoreProject,
    clearIgnoredProjects,
  } = useSettingsStore();
  
  const [scannedProjects, setScannedProjects] = useState<ScannedProject[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [newProjectName, setNewProjectName] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [activeTab, setActiveTab] = useState<'recent' | 'projects'>('projects');
  const [showIgnoredList, setShowIgnoredList] = useState(false);
  
  // 弹窗状态
  const [confirmDialog, setConfirmDialog] = useState<{
    isOpen: boolean;
    title: string;
    message: string;
    onConfirm: () => void;
  }>({ isOpen: false, title: '', message: '', onConfirm: () => {} });
  
  const [alertDialog, setAlertDialog] = useState<{
    isOpen: boolean;
    title: string;
    message: string;
  }>({ isOpen: false, title: '提示', message: '' });

  // 处理项目点击（支持未初始化的项目）
  const handleProjectClick = async (project: ScannedProject) => {
    if (project.hasPmCenter) {
      // 已初始化，直接打开
      onOpenProject(project.path);
    } else {
      // 未初始化，弹出确认对话框
      setConfirmDialog({
        isOpen: true,
        title: '初始化项目',
        message: `项目 "${project.name}" 未初始化，是否现在初始化？`,
        onConfirm: async () => {
          try {
            await invoke('init_project', { projectPath: project.path });
            // 刷新列表
            await scanProjects();
            // 打开项目
            onOpenProject(project.path);
          } catch (err) {
            setAlertDialog({
              isOpen: true,
              title: '初始化失败',
              message: String(err),
            });
          }
        },
      });
    }
  };
  
  // 处理忽略项目
  const handleIgnoreProject = async (project: ScannedProject) => {
    setConfirmDialog({
      isOpen: true,
      title: '忽略项目',
      message: `忽略 "${project.name}"？\n被忽略的项目将不再显示在项目列表中，除非手动导入。`,
      onConfirm: async () => {
        await ignoreProject(project.path);
        // 刷新列表
        await scanProjects();
      },
    });
  };

  // 加载设置
  useEffect(() => {
    loadSettings();
  }, []);

  // 扫描项目（过滤掉被忽略的）
  const scanProjects = async () => {
    if (!projectsRootDir) return;
    
    setIsScanning(true);
    try {
      const projects = await scanProjectsRoot(projectsRootDir);
      // 过滤掉被忽略的项目
      const filtered = projects.filter(p => !ignoredProjects.includes(p.path));
      setScannedProjects(filtered);
    } catch (err) {
      console.error('扫描项目失败:', err);
    } finally {
      setIsScanning(false);
    }
  };

  // 有项目根目录时自动扫描
  useEffect(() => {
    if (projectsRootDir) {
      scanProjects();
    }
  }, [projectsRootDir, ignoredProjects]);

  // 选择项目根目录
  const handleSelectRootDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: '选择项目根目录',
      });
      
      if (selected && typeof selected === 'string') {
        await setProjectsRootDir(selected);
      }
    } catch (error) {
      console.error('选择目录失败:', error);
    }
  };

  // 清除项目根目录
  const handleClearRootDir = async () => {
    await setProjectsRootDir(null);
    setScannedProjects([]);
  };

  // 创建新项目
  const handleCreateProject = async () => {
    if (!projectsRootDir || !newProjectName.trim()) return;
    
    setIsCreating(true);
    try {
      const projectPath = await createProject(projectsRootDir, newProjectName.trim());
    setShowCreateDialog(false);
    setNewProjectName('');
      onOpenProject(projectPath);
    } catch (err) {
      setAlertDialog({
        isOpen: true,
        title: '创建失败',
        message: String(err),
      });
    } finally {
      setIsCreating(false);
    }
  };

  // 打开其他目录的项目
  const handleOpenOtherProject = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: '手动导入单个项目',
      });
      
      if (selected && typeof selected === 'string') {
        onOpenProject(selected);
      }
    } catch (error) {
      console.error('打开项目失败:', error);
    }
  };

  return (
    <div className="flex-1 flex items-center justify-center p-8 overflow-auto">
      <div className="w-full max-w-4xl">
        {/* Logo 和标题 */}
        <div className="mb-8">
          <div className="flex items-start justify-between gap-4">
            <div className="flex-1 text-center">
              <img
                src={pmcLogo}
                alt="PM Center"
                className="w-20 h-20 mx-auto mb-4 object-contain"
              />
              <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-100 mb-1">
                PM Center
              </h1>
              <p className="text-sm text-gray-500 dark:text-gray-400">
                项目管理与渲染工作流工具
              </p>
            </div>
            <button
              onClick={() => setShowSettings(true)}
              className="flex items-center gap-2 px-3 py-2 rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
              title="全局设置"
            >
              <Settings className="w-4 h-4" />
              <span className="text-sm">全局设置</span>
            </button>
          </div>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* 左侧：项目目录设置 */}
          <div className="lg:col-span-1 space-y-4">
            {/* 项目根目录卡片 */}
            <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 p-4">
              <div className="flex items-center justify-between mb-3">
                <h3 className="font-medium text-gray-900 dark:text-gray-100 flex items-center gap-2">
                  <Settings className="w-4 h-4" />
                  项目目录
                </h3>
              </div>
              
              {projectsRootDir ? (
                <div className="space-y-3">
                  <div className="p-2 bg-blue-50 dark:bg-blue-900/20 rounded-lg">
                    <p className="text-xs text-gray-500 dark:text-gray-400 mb-1">当前目录</p>
                    <p className="text-sm text-gray-900 dark:text-gray-100 break-all">
                      {projectsRootDir}
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <button
                      onClick={handleSelectRootDir}
                      className="flex-1 px-3 py-1.5 text-xs bg-gray-100 dark:bg-gray-700 
                                 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg
                                 text-gray-700 dark:text-gray-300 transition-colors"
                    >
                      更换
                    </button>
                    <button
                      onClick={handleClearRootDir}
                      className="px-3 py-1.5 text-xs text-red-600 hover:bg-red-50 
                                 dark:hover:bg-red-900/20 rounded-lg transition-colors"
                    >
                      清除
                    </button>
                  </div>
                </div>
              ) : (
                <div className="text-center py-4">
                  <p className="text-sm text-gray-500 dark:text-gray-400 mb-3">
                    设置项目根目录以管理多个项目
                  </p>
                  <button
                    onClick={handleSelectRootDir}
                    className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white 
                               rounded-lg text-sm transition-colors"
                  >
                    选择目录
                  </button>
                </div>
              )}
            </div>

            {/* 快速操作 */}
            <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 p-4">
              <h3 className="font-medium text-gray-900 dark:text-gray-100 mb-3">
                快速操作
              </h3>
              {!projectsRootDir && (
                <p className="text-xs text-gray-500 dark:text-gray-400 mb-3 leading-5">
                  不设置项目根目录时，也可以直接手动导入并打开单个项目。
                </p>
              )}
              <div className="space-y-2">
                {projectsRootDir && (
                  <button
                    onClick={() => setShowCreateDialog(true)}
                    className="w-full flex items-center gap-2 px-3 py-2 
                               bg-blue-50 dark:bg-blue-900/20 hover:bg-blue-100 dark:hover:bg-blue-900/30
                               text-blue-600 dark:text-blue-400 rounded-lg text-sm transition-colors"
                  >
                    <FolderPlus className="w-4 h-4" />
                    创建新项目
                  </button>
                )}
                <button
                  onClick={handleOpenOtherProject}
                  className="w-full flex items-center gap-2 px-3 py-2 
                             bg-gray-50 dark:bg-gray-700/50 hover:bg-gray-100 dark:hover:bg-gray-700
                             text-gray-700 dark:text-gray-300 rounded-lg text-sm transition-colors"
                >
                  <FolderOpen className="w-4 h-4" />
                  手动导入单个项目
                </button>
                {/* 已忽略项目按钮（仅在有被忽略项目时显示） */}
                {ignoredProjects.length > 0 && (
                  <button
                    onClick={() => setShowIgnoredList(true)}
                    className="w-full flex items-center gap-2 px-3 py-2 
                               bg-orange-50 dark:bg-orange-900/10 hover:bg-orange-100 dark:hover:bg-orange-900/20
                               text-orange-600 dark:text-orange-400 rounded-lg text-sm transition-colors"
                  >
                    <EyeOff className="w-4 h-4" />
                    已忽略项目 ({ignoredProjects.length})
                  </button>
                )}
              </div>
            </div>
          </div>

          {/* 右侧：项目列表 */}
          <div className="lg:col-span-2 h-[480px]">
            <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 h-full flex flex-col">
              {/* 标签切换 */}
              <div className="flex border-b border-gray-200 dark:border-gray-700 flex-shrink-0">
                {projectsRootDir && (
                  <button
                    onClick={() => setActiveTab('projects')}
                    className={`flex-1 px-4 py-3 text-sm font-medium transition-colors
                      ${activeTab === 'projects'
                        ? 'text-blue-600 border-b-2 border-blue-600'
                        : 'text-gray-600 hover:text-gray-900 dark:hover:text-gray-100'
                      }`}
                  >
                    项目列表 ({scannedProjects.length})
                  </button>
                )}
                <button
                  onClick={() => setActiveTab('recent')}
                  className={`flex-1 px-4 py-3 text-sm font-medium transition-colors
                    ${activeTab === 'recent'
                      ? 'text-blue-600 border-b-2 border-blue-600'
                      : 'text-gray-600 hover:text-gray-900 dark:hover:text-gray-100'
                    }`}
                >
                  最近打开 ({recentProjects.length})
                </button>
              </div>

              {/* 内容区 */}
              <div className="p-4 h-[calc(100%-49px)] overflow-y-auto">
                {activeTab === 'projects' && projectsRootDir ? (
                  // 项目列表
                  <div className="space-y-2 min-h-[100px]">
                    {isScanning ? (
                      <div className="flex items-center justify-center py-12 text-gray-400">
                        <RefreshCw className="w-5 h-5 animate-spin mr-2" />
                        扫描中...
                      </div>
                    ) : scannedProjects.length === 0 ? (
                      <div className="text-center py-12 text-gray-400">
                        <Folder className="w-12 h-12 mx-auto mb-3 opacity-50" />
                        <p className="text-sm">该目录下暂无项目</p>
                        <p className="text-xs mt-1 opacity-70">
                          点击"创建新项目"或"手动导入单个项目"
                        </p>
                      </div>
                    ) : (
                      scannedProjects.map((project) => (
                        <div
                          key={project.path}
                          onClick={() => handleProjectClick(project)}
                          className={`group flex items-center gap-3 p-3 rounded-lg border transition-all
                            ${project.hasPmCenter
                              ? 'bg-white dark:bg-gray-800 border-gray-200 dark:border-gray-700 hover:border-blue-300 dark:hover:border-blue-700 hover:shadow-sm cursor-pointer'
                              : 'bg-gray-50 dark:bg-gray-800/50 border-gray-200 dark:border-gray-700 hover:border-yellow-300 dark:hover:border-yellow-700 hover:shadow-sm cursor-pointer'
                            }`}
                        >
                          <div className={`w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0
                            ${project.hasPmCenter
                              ? 'bg-blue-50 dark:bg-blue-900/30'
                              : 'bg-yellow-50 dark:bg-yellow-900/20'
                            }`}
                          >
                            <Folder className={`w-5 h-5 ${project.hasPmCenter ? 'text-blue-500' : 'text-yellow-600'}`} />
                          </div>
                          
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-2">
                              <p className="font-medium text-sm text-gray-900 dark:text-gray-100 truncate">
                                {project.name}
                              </p>
                              {!project.hasPmCenter && (
                                <span className="text-xs px-1.5 py-0.5 bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-400 rounded">
                                  未初始化
                                </span>
                              )}
                            </div>
                            <p className="text-xs text-gray-500 dark:text-gray-400 truncate">
                              {project.path}
                            </p>
                          </div>

                          <div className="flex items-center gap-1">
                            {/* 忽略按钮 */}
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                handleIgnoreProject(project);
                              }}
                              className="opacity-0 group-hover:opacity-100 p-1.5
                                         text-gray-400 hover:text-orange-500 hover:bg-orange-50 dark:hover:bg-orange-900/20
                                         rounded-lg transition-all"
                              title="忽略此项目"
                            >
                              <EyeOff className="w-4 h-4" />
                            </button>
                            
                            <ChevronRight className={`w-4 h-4 ${project.hasPmCenter ? 'text-gray-400' : 'text-yellow-400'}`} />
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                ) : (
                  // 最近打开列表
                  <div className="space-y-2">
                    {recentProjects.length === 0 ? (
                      <div className="text-center py-12 text-gray-400">
                        <Clock className="w-12 h-12 mx-auto mb-3 opacity-50" />
                        <p className="text-sm">暂无最近打开的项目</p>
                      </div>
                    ) : (
                      recentProjects.map((project) => (
                        <div
                          key={project.path}
                          onClick={() => onOpenProject(project.path)}
                          className="group flex items-center gap-3 p-3 rounded-lg
                                     bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700
                                     hover:border-blue-300 dark:hover:border-blue-700
                                     hover:shadow-sm transition-all cursor-pointer"
                        >
                          <div className="w-10 h-10 rounded-lg bg-blue-50 dark:bg-blue-900/30 
                                          flex items-center justify-center flex-shrink-0">
                            <Folder className="w-5 h-5 text-blue-500" />
                          </div>
                          
                          <div className="flex-1 min-w-0">
                            <p className="font-medium text-sm text-gray-900 dark:text-gray-100 truncate">
                              {project.name}
                            </p>
                            <p className="text-xs text-gray-500 dark:text-gray-400 truncate">
                              {project.path}
                            </p>
                          </div>
                          
                          <div className="flex items-center gap-2">
                            <span className="text-xs text-gray-400 whitespace-nowrap">
                              {formatTime(project.openedAt)}
                            </span>
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                removeRecentProject(project.path);
                              }}
                              className="opacity-0 group-hover:opacity-100 p-1 
                                         text-gray-400 hover:text-red-500 transition-opacity"
                            >
                              <X className="w-4 h-4" />
                            </button>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* 确认对话框 */}
      <ConfirmDialog
        isOpen={confirmDialog.isOpen}
        onClose={() => setConfirmDialog({ ...confirmDialog, isOpen: false })}
        onConfirm={confirmDialog.onConfirm}
        title={confirmDialog.title}
        message={confirmDialog.message}
        type="warning"
      />

      {/* 提示对话框 */}
      <AlertDialog
        isOpen={alertDialog.isOpen}
        onClose={() => setAlertDialog({ ...alertDialog, isOpen: false })}
        title={alertDialog.title}
        message={alertDialog.message}
      />

      <SettingsPanel
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
        defaultScope="global"
      />

      {/* 已忽略项目列表对话框 */}
      <Dialog
        isOpen={showIgnoredList}
        onClose={() => setShowIgnoredList(false)}
        title="已忽略的项目"
        size="md"
        footer={
          <button
            onClick={() => setShowIgnoredList(false)}
            className="px-4 py-2 text-sm bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg"
          >
            关闭
          </button>
        }
      >
        {ignoredProjects.length === 0 ? (
          <div className="text-center py-8 text-gray-400">
            <EyeOff className="w-12 h-12 mx-auto mb-3 opacity-50" />
            <p className="text-sm">暂无被忽略的项目</p>
          </div>
        ) : (
          <div className="space-y-2 max-h-[300px] overflow-y-auto">
            {ignoredProjects.map((path) => (
              <div
                key={path}
                className="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-800 rounded-lg"
              >
                <Folder className="w-5 h-5 text-gray-400 flex-shrink-0" />
                <p className="flex-1 text-sm text-gray-700 dark:text-gray-300 truncate">
                  {path}
                </p>
                <button
                  onClick={() => unignoreProject(path)}
                  className="p-1.5 text-gray-400 hover:text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors"
                  title="恢复显示"
                >
                  <Eye className="w-4 h-4" />
                </button>
              </div>
            ))}
          </div>
        )}
      </Dialog>

      {/* 创建新项目对话框（使用新组件） */}
      <Dialog
        isOpen={showCreateDialog}
        onClose={() => {
          setShowCreateDialog(false);
          setNewProjectName('');
        }}
        title="创建新项目"
        size="sm"
        footer={
          <>
            <button
              onClick={() => {
                setShowCreateDialog(false);
                setNewProjectName('');
              }}
              className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
            >
              取消
            </button>
            <button
              onClick={handleCreateProject}
              disabled={!newProjectName.trim() || isCreating}
              className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 text-white rounded-lg"
            >
              {isCreating ? '创建中...' : '创建'}
            </button>
          </>
        }
      >
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              项目名称
            </label>
            <input
              type="text"
              value={newProjectName}
              onChange={(e) => setNewProjectName(e.target.value)}
              placeholder="输入项目名称"
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100"
              onKeyDown={(e) => e.key === 'Enter' && handleCreateProject()}
              autoFocus
            />
          </div>
          <div className="p-3 bg-gray-50 dark:bg-gray-800 rounded-lg">
            <p className="text-xs text-gray-500 dark:text-gray-400">将在以下位置创建项目：</p>
            <p className="text-sm text-gray-900 dark:text-gray-100 break-all mt-1">
              {projectsRootDir}/{newProjectName}
            </p>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
