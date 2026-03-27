import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useLauncherStore } from '../stores/launcherStore';
import { Rocket, Plus, X, Edit2, Trash2, Save, FolderOpen, Play } from 'lucide-react';

// 启动软件
async function launchSoftware(path: string) {
  try {
    await invoke('launch_program', { path });
  } catch (error) {
    console.error('Failed to launch:', error);
    alert('启动失败：' + error);
  }
}

interface LauncherProps {
  isOpen: boolean;
  onClose: () => void;
}

export function LauncherPanel({ isOpen, onClose }: LauncherProps) {
  const { items, loadItems, addItem, removeItem, updateItem } = useLauncherStore();
  const [isEditing, setIsEditing] = useState(false);
  const [editingItem, setEditingItem] = useState<{ id?: string; name: string; path: string }>({
    name: '',
    path: '',
  });
  const [icons, setIcons] = useState<Record<string, string>>({});

  // 加载配置和图标
  useEffect(() => {
    if (isOpen) {
      loadItems();
      loadIcons();
    }
  }, [isOpen]);

  // 加载所有软件图标
  const loadIcons = async () => {
    const iconMap: Record<string, string> = {};
    for (const item of items) {
      try {
        const icon = await invoke<string | null>('extract_icon', { path: item.path });
        if (icon) {
          iconMap[item.id] = icon;
        }
      } catch (e) {
        console.log('Failed to load icon for', item.name);
      }
    }
    setIcons(iconMap);
  };

  // 选择文件
  const selectFile = async () => {
    const selected = await open({
      multiple: false,
      filters: [
        { name: 'Executable', extensions: ['exe', 'bat', 'cmd'] },
        { name: 'All Files', extensions: ['*'] },
      ],
    });
    if (selected && typeof selected === 'string') {
      setEditingItem((prev) => ({ ...prev, path: selected }));
      // 自动提取文件名
      if (!editingItem.name) {
        const fileName = selected.split(/[\\/]/).pop()?.replace('.exe', '') || '';
        setEditingItem((prev) => ({ ...prev, name: fileName }));
      }
      // 预览图标
      try {
        const icon = await invoke<string | null>('extract_icon', { path: selected });
        if (icon) {
          setIcons((prev) => ({ ...prev, preview: icon }));
        }
      } catch (e) {}
    }
  };

  // 保存编辑
  const handleSave = async () => {
    if (!editingItem.name.trim() || !editingItem.path.trim()) {
      alert('请填写名称和路径');
      return;
    }

    if (editingItem.id) {
      await updateItem(editingItem.id, {
        name: editingItem.name,
        path: editingItem.path,
      });
    } else {
      await addItem({
        name: editingItem.name,
        path: editingItem.path,
      });
    }

    setIsEditing(false);
    setEditingItem({ name: '', path: '' });
    setIcons((prev) => {
      const { preview, ...rest } = prev;
      return rest;
    });
    await loadIcons();
  };

  // 开始编辑
  const startEdit = (item?: typeof editingItem) => {
    if (item) {
      setEditingItem(item);
    } else {
      setEditingItem({ name: '', path: '' });
    }
    setIsEditing(true);
  };

  // 删除
  const handleDelete = async (id: string) => {
    if (confirm('确定删除这个快捷启动吗？')) {
      await removeItem(id);
      setIcons((prev) => {
        const { [id]: _, ...rest } = prev;
        return rest;
      });
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* 背景遮罩 */}
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />

      {/* 面板 */}
      <div className="relative w-[800px] max-w-[90vw] max-h-[85vh] bg-white dark:bg-gray-800 rounded-2xl shadow-2xl overflow-hidden">
        {/* 头部 */}
        <div className="flex items-center justify-between px-6 py-4 bg-gradient-to-r from-blue-500 to-purple-600">
          <h2 className="text-xl font-bold text-white flex items-center gap-3">
            <Rocket className="w-6 h-6" />
            快捷启动
          </h2>
          <div className="flex items-center gap-2">
            {!isEditing && (
              <button
                onClick={() => startEdit()}
                className="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium
                           text-white bg-white/20 hover:bg-white/30 rounded-lg transition-colors"
              >
                <Plus className="w-4 h-4" />
                添加软件
              </button>
            )}
            <button
              onClick={onClose}
              className="p-2 text-white/80 hover:text-white hover:bg-white/20 rounded-lg transition-colors"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* 内容 */}
        <div className="p-6 overflow-auto max-h-[70vh]">
          {isEditing ? (
            // 编辑表单
            <div className="max-w-md mx-auto space-y-4">
              <div className="text-center mb-6">
                <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
                  {editingItem.id ? '编辑软件' : '添加软件'}
                </h3>
              </div>

              {/* 图标预览 */}
              {(icons.preview || (editingItem.id && icons[editingItem.id])) && (
                <div className="flex justify-center mb-4">
                  <img
                    src={icons.preview || icons[editingItem.id!]}
                    alt="Icon"
                    className="w-16 h-16 object-contain"
                  />
                </div>
              )}

              <div>
                <label className="text-sm font-medium text-gray-700 dark:text-gray-300 block mb-2">
                  名称
                </label>
                <input
                  type="text"
                  value={editingItem.name}
                  onChange={(e) =>
                    setEditingItem((prev) => ({ ...prev, name: e.target.value }))
                  }
                  className="w-full px-4 py-3 text-base border border-gray-300 dark:border-gray-600 
                             rounded-xl bg-white dark:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
                  placeholder="例如：Blender"
                  autoFocus
                />
              </div>

              <div>
                <label className="text-sm font-medium text-gray-700 dark:text-gray-300 block mb-2">
                  路径
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={editingItem.path}
                    onChange={(e) =>
                      setEditingItem((prev) => ({ ...prev, path: e.target.value }))
                    }
                    className="flex-1 px-4 py-3 text-base border border-gray-300 dark:border-gray-600 
                               rounded-xl bg-white dark:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="选择可执行文件..."
                  />
                  <button
                    onClick={selectFile}
                    className="px-4 py-3 text-gray-600 hover:text-gray-900 
                               hover:bg-gray-100 dark:text-gray-400 dark:hover:text-gray-100 dark:hover:bg-gray-700
                               rounded-xl border border-gray-300 dark:border-gray-600 transition-colors"
                  >
                    <FolderOpen className="w-5 h-5" />
                  </button>
                </div>
              </div>

              <div className="flex gap-3 pt-4">
                <button
                  onClick={handleSave}
                  className="flex-1 flex items-center justify-center gap-2 px-6 py-3
                             bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-xl transition-colors"
                >
                  <Save className="w-5 h-5" />
                  保存
                </button>
                <button
                  onClick={() => {
                    setIsEditing(false);
                    setEditingItem({ name: '', path: '' });
                  }}
                  className="px-6 py-3 text-gray-600 hover:text-gray-900 
                             hover:bg-gray-100 dark:text-gray-400 dark:hover:text-gray-100 dark:hover:bg-gray-700
                             font-medium rounded-xl transition-colors"
                >
                  取消
                </button>
              </div>
            </div>
          ) : items.length === 0 ? (
            // 空状态
            <div className="text-center py-16 text-gray-400">
              <Rocket className="w-20 h-20 mx-auto mb-6 opacity-50" />
              <p className="text-lg mb-2">暂无快捷启动</p>
              <p className="text-sm mb-6">添加常用软件，一键快速启动</p>
              <button
                onClick={() => startEdit()}
                className="px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-xl transition-colors"
              >
                添加第一个软件
              </button>
            </div>
          ) : (
            // 软件网格
            <div className="grid grid-cols-4 sm:grid-cols-5 md:grid-cols-6 lg:grid-cols-8 gap-4">
              {items.map((item) => (
                <div
                  key={item.id}
                  className="group relative flex flex-col items-center p-3 rounded-2xl
                             border border-gray-200 dark:border-gray-700
                             hover:border-blue-300 dark:hover:border-blue-700
                             hover:shadow-lg hover:shadow-blue-500/10
                             transition-all cursor-pointer
                             bg-gray-50 dark:bg-gray-800/50"
                  onDoubleClick={() => {
                    launchSoftware(item.path);
                    onClose();
                  }}
                  title={`双击启动：${item.name}`}
                >
                  {/* 操作按钮 - 始终显示 */}
                  <div className="absolute top-1 right-1 flex gap-0.5">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        startEdit({ id: item.id, name: item.name, path: item.path });
                      }}
                      className="p-1 text-gray-400 hover:text-blue-600 hover:bg-blue-50 
                                 dark:hover:bg-blue-900/30 rounded-md transition-colors"
                      title="编辑"
                    >
                      <Edit2 className="w-3 h-3" />
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete(item.id);
                      }}
                      className="p-1 text-gray-400 hover:text-red-600 hover:bg-red-50 
                                 dark:hover:bg-red-900/30 rounded-md transition-colors"
                      title="删除"
                    >
                      <Trash2 className="w-3 h-3" />
                    </button>
                  </div>

                  {/* 图标 */}
                  <div className="w-16 h-16 mb-3 flex items-center justify-center">
                    {icons[item.id] ? (
                      <img
                        src={icons[item.id]}
                        alt={item.name}
                        className="w-14 h-14 object-contain drop-shadow-sm"
                      />
                    ) : (
                      <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-blue-400 to-purple-500 
                                      flex items-center justify-center text-white text-2xl font-bold">
                        {item.name.charAt(0).toUpperCase()}
                      </div>
                    )}
                  </div>

                  {/* 名称 - 支持两行显示 */}
                  <p className="text-xs text-center text-gray-800 dark:text-gray-200 font-medium 
                                line-clamp-2 break-words w-full leading-tight min-h-[2rem]">
                    {item.name}
                  </p>

                  {/* 启动提示 - 仅悬停时显示，不遮挡按钮 */}
                  <div className="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 
                                  bg-blue-500/0 rounded-2xl transition-opacity pointer-events-none"
                       style={{ marginTop: '24px' }}>
                    <Play className="w-8 h-8 text-white" />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* 底部提示 */}
        {!isEditing && items.length > 0 && (
          <div className="px-6 py-4 bg-gray-50 dark:bg-gray-800/50 text-sm text-gray-500 text-center border-t border-gray-200 dark:border-gray-700">
            双击图标启动软件，按 ESC 关闭面板
          </div>
        )}
      </div>
    </div>
  );
}

// 工具栏按钮
export function LauncherButton() {
  const [isOpen, setIsOpen] = useState(false);
  const { loadItems } = useLauncherStore();

  // 监听快捷键 Ctrl+W
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === 'w') {
        e.preventDefault();
        setIsOpen(true);
        loadItems();
      }
      if (e.key === 'Escape' && isOpen) {
        setIsOpen(false);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, loadItems]);

  return (
    <>
      <button
        onClick={() => {
          setIsOpen(true);
          loadItems();
        }}
        className="flex items-center gap-1.5 px-3 py-1.5 text-sm
                   bg-gradient-to-r from-blue-500 to-purple-600 
                   hover:from-blue-600 hover:to-purple-700
                   text-white rounded-lg shadow-sm
                   transition-all hover:shadow-md"
        title="快捷启动 (Ctrl+W)"
      >
        <Rocket className="w-4 h-4" />
        <span className="hidden sm:inline">启动</span>
        <span className="text-xs opacity-70">Ctrl+W</span>
      </button>

      <LauncherPanel isOpen={isOpen} onClose={() => setIsOpen(false)} />
    </>
  );
}
