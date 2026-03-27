import { useState, useEffect } from 'react';
import { usePythonEnvStore } from '../stores/pythonEnvStore';
import { Dialog, AlertDialog } from './Dialog';
import { 
  Plus, Trash2, RefreshCw, Package, Terminal,
  ChevronDown, ChevronUp, Check, X, Code2
} from 'lucide-react';

interface PythonEnvManagerProps {
  isOpen: boolean;
  onClose: () => void;
}

export function PythonEnvManager({ isOpen, onClose }: PythonEnvManagerProps) {
  const {
    envs,
    selectedEnvId,
    isDetecting,
    isCreatingVenv,
    loadSettings,
    detectEnvs,
    selectEnv,
    createVenv,
    deleteVenv,
    installPackage,
    uninstallPackage,
    getInstalledPackages,
  } = usePythonEnvStore();

  const [showCreateVenv, setShowCreateVenv] = useState(false);
  const [venvName, setVenvName] = useState('');
  const [selectedBasePython, setSelectedBasePython] = useState('');
  
  const [expandedEnv, setExpandedEnv] = useState<string | null>(null);
  const [packages, setPackages] = useState<string[]>([]);
  const [isLoadingPackages, setIsLoadingPackages] = useState(false);
  const [newPackage, setNewPackage] = useState('');
  const [isInstalling, setIsInstalling] = useState(false);
  
  const [alertDialog, setAlertDialog] = useState({ isOpen: false, title: '', message: '' });

  // 加载设置
  useEffect(() => {
    if (isOpen) {
      loadSettings();
      if (envs.length === 0) {
        detectEnvs();
      }
    }
  }, [isOpen]);

  // 加载包列表
  useEffect(() => {
    if (expandedEnv) {
      loadPackages(expandedEnv);
    }
  }, [expandedEnv]);

  const loadPackages = async (envId: string) => {
    setIsLoadingPackages(true);
    try {
      const list = await getInstalledPackages(envId);
      setPackages(list);
    } catch (err) {
      setPackages([]);
    } finally {
      setIsLoadingPackages(false);
    }
  };

  const handleCreateVenv = async () => {
    if (!venvName.trim()) return;
    try {
      await createVenv(venvName.trim(), selectedBasePython || undefined);
      setShowCreateVenv(false);
      setVenvName('');
    } catch (err) {
      setAlertDialog({
        isOpen: true,
        title: '创建失败',
        message: String(err),
      });
    }
  };

  const handleDeleteVenv = async (id: string, name: string) => {
    if (confirm(`确定删除虚拟环境 "${name}"？\n此操作不可恢复。`)) {
      try {
        await deleteVenv(id);
      } catch (err) {
        setAlertDialog({
          isOpen: true,
          title: '删除失败',
          message: String(err),
        });
      }
    }
  };

  const handleInstallPackage = async () => {
    if (!newPackage.trim() || !expandedEnv) return;
    setIsInstalling(true);
    try {
      await installPackage(expandedEnv, newPackage.trim());
      setNewPackage('');
      await loadPackages(expandedEnv);
    } catch (err) {
      setAlertDialog({
        isOpen: true,
        title: '安装失败',
        message: String(err),
      });
    } finally {
      setIsInstalling(false);
    }
  };

  const handleUninstallPackage = async (packageName: string) => {
    if (!expandedEnv) return;
    const name = packageName.split('==')[0];
    if (confirm(`确定卸载 "${name}"？`)) {
      try {
        await uninstallPackage(expandedEnv, name);
        await loadPackages(expandedEnv);
      } catch (err) {
        setAlertDialog({
          isOpen: true,
          title: '卸载失败',
          message: String(err),
        });
      }
    }
  };

  // 获取系统 Python 列表（用于创建 venv）
  const systemPythons = envs.filter(e => e.isSystem);

  return (
    <>
      <Dialog
        isOpen={isOpen}
        onClose={onClose}
        title="Python 环境管理"
        size="lg"
        footer={
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg"
          >
            关闭
          </button>
        }
      >
        <div className="space-y-4">
          {/* 工具栏 */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <button
                onClick={detectEnvs}
                disabled={isDetecting}
                className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400 rounded-lg hover:bg-blue-100 dark:hover:bg-blue-900/30 transition-colors disabled:opacity-50"
              >
                <RefreshCw className={`w-4 h-4 ${isDetecting ? 'animate-spin' : ''}`} />
                刷新检测
              </button>
              <button
                onClick={() => setShowCreateVenv(true)}
                className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-green-50 dark:bg-green-900/20 text-green-600 dark:text-green-400 rounded-lg hover:bg-green-100 dark:hover:bg-green-900/30 transition-colors"
              >
                <Plus className="w-4 h-4" />
                新建虚拟环境
              </button>
            </div>
            
            {selectedEnvId && (
              <div className="text-sm text-gray-500">
                当前使用: <span className="font-medium text-blue-600">{envs.find(e => e.id === selectedEnvId)?.name}</span>
              </div>
            )}
          </div>

          {/* 环境列表 */}
          <div className="space-y-2 max-h-[400px] overflow-y-auto">
            {envs.length === 0 ? (
              <div className="text-center py-12 text-gray-400">
                <Code2 className="w-12 h-12 mx-auto mb-3 opacity-50" />
                <p className="text-sm">未检测到 Python 环境</p>
                <p className="text-xs mt-1">点击"刷新检测"或安装 Python</p>
              </div>
            ) : (
              envs.map((env) => (
                <div
                  key={env.id}
                  className={`border rounded-lg overflow-hidden transition-all ${
                    selectedEnvId === env.id
                      ? 'border-blue-500 bg-blue-50/50 dark:bg-blue-900/10'
                      : 'border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800'
                  }`}
                >
                  {/* 环境头部 */}
                  <div className="flex items-center gap-3 p-3">
                    <input
                      type="radio"
                      name="python-env"
                      checked={selectedEnvId === env.id}
                      onChange={() => selectEnv(env.id)}
                      className="w-4 h-4 text-blue-600"
                    />
                    
                    <div className={`w-10 h-10 rounded-lg flex items-center justify-center ${
                      env.isVenv
                        ? 'bg-green-100 dark:bg-green-900/30'
                        : 'bg-blue-100 dark:bg-blue-900/30'
                    }`}>
                      <Code2 className={`w-5 h-5 ${env.isVenv ? 'text-green-600' : 'text-blue-600'}`} />
                    </div>
                    
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-medium text-gray-900 dark:text-gray-100">
                          {env.name}
                        </span>
                        {env.isVenv && (
                          <span className="text-xs px-1.5 py-0.5 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400 rounded">
                            venv
                          </span>
                        )}
                      </div>
                      <p className="text-xs text-gray-500 truncate">{env.path}</p>
                    </div>
                    
                    {/* 展开/删除按钮 */}
                    <div className="flex items-center gap-1">
                      <button
                        onClick={() => setExpandedEnv(expandedEnv === env.id ? null : env.id)}
                        className="p-1.5 text-gray-400 hover:text-gray-600 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
                        title="管理包"
                      >
                        <Package className="w-4 h-4" />
                      </button>
                      
                      {env.isVenv && (
                        <button
                          onClick={() => handleDeleteVenv(env.id, env.name)}
                          className="p-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors"
                          title="删除"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      )}
                    </div>
                  </div>
                  
                  {/* 展开的包管理区域 */}
                  {expandedEnv === env.id && (
                    <div className="border-t border-gray-200 dark:border-gray-700 p-3 bg-gray-50/50 dark:bg-gray-900/30">
                      {/* 安装新包 */}
                      <div className="flex gap-2 mb-3">
                        <input
                          type="text"
                          value={newPackage}
                          onChange={(e) => setNewPackage(e.target.value)}
                          placeholder="输入包名 (如: requests, numpy)"
                          className="flex-1 px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800"
                          onKeyDown={(e) => e.key === 'Enter' && handleInstallPackage()}
                        />
                        <button
                          onClick={handleInstallPackage}
                          disabled={!newPackage.trim() || isInstalling}
                          className="px-3 py-1.5 text-sm bg-green-600 hover:bg-green-700 disabled:bg-gray-300 text-white rounded-lg flex items-center gap-1"
                        >
                          {isInstalling ? (
                            <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                          ) : (
                            <Plus className="w-3.5 h-3.5" />
                          )}
                          安装
                        </button>
                      </div>
                      
                      {/* 已安装包列表 */}
                      <div className="text-sm">
                        <div className="flex items-center justify-between mb-2">
                          <span className="font-medium text-gray-700 dark:text-gray-300">已安装包</span>
                          <button
                            onClick={() => loadPackages(env.id)}
                            className="text-xs text-blue-600 hover:text-blue-700 flex items-center gap-1"
                          >
                            <RefreshCw className={`w-3 h-3 ${isLoadingPackages ? 'animate-spin' : ''}`} />
                            刷新
                          </button>
                        </div>
                        
                        {isLoadingPackages ? (
                          <div className="text-center py-4 text-gray-400 text-xs">加载中...</div>
                        ) : packages.length === 0 ? (
                          <div className="text-center py-4 text-gray-400 text-xs">暂无已安装的包</div>
                        ) : (
                          <div className="max-h-[200px] overflow-y-auto space-y-1">
                            {packages.map((pkg) => {
                              const [name, version] = pkg.split('==');
                              return (
                                <div
                                  key={pkg}
                                  className="flex items-center justify-between px-2 py-1 bg-white dark:bg-gray-800 rounded"
                                >
                                  <span className="text-xs">
                                    <span className="font-medium">{name}</span>
                                    {version && <span className="text-gray-500">=={version}</span>}
                                  </span>
                                  <button
                                    onClick={() => handleUninstallPackage(pkg)}
                                    className="p-1 text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors"
                                  >
                                    <X className="w-3.5 h-3.5" />
                                  </button>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              ))
            )}
          </div>
        </div>
      </Dialog>

      {/* 创建虚拟环境对话框 */}
      <Dialog
        isOpen={showCreateVenv}
        onClose={() => {
          setShowCreateVenv(false);
          setVenvName('');
        }}
        title="新建虚拟环境"
        size="sm"
        footer={
          <>
            <button
              onClick={() => {
                setShowCreateVenv(false);
                setVenvName('');
              }}
              className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
            >
              取消
            </button>
            <button
              onClick={handleCreateVenv}
              disabled={!venvName.trim() || isCreatingVenv}
              className="px-4 py-2 text-sm bg-green-600 hover:bg-green-700 disabled:bg-gray-300 text-white rounded-lg"
            >
              {isCreatingVenv ? '创建中...' : '创建'}
            </button>
          </>
        }
      >
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
              环境名称
            </label>
            <input
              type="text"
              value={venvName}
              onChange={(e) => setVenvName(e.target.value)}
              placeholder="例如: myproject-env"
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800"
              onKeyDown={(e) => e.key === 'Enter' && handleCreateVenv()}
              autoFocus
            />
          </div>
          
          {systemPythons.length > 0 && (
            <div>
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                基础 Python 版本
              </label>
              <select
                value={selectedBasePython}
                onChange={(e) => setSelectedBasePython(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800"
              >
                <option value="">自动选择</option>
                {systemPythons.map((env) => (
                  <option key={env.id} value={env.path}>
                    {env.name}
                  </option>
                ))}
              </select>
              <p className="text-xs text-gray-500 mt-1">
                不选择则使用系统默认 Python
              </p>
            </div>
          )}
        </div>
      </Dialog>

      {/* 提示对话框 */}
      <AlertDialog
        isOpen={alertDialog.isOpen}
        onClose={() => setAlertDialog({ ...alertDialog, isOpen: false })}
        title={alertDialog.title}
        message={alertDialog.message}
      />
    </>
  );
}
