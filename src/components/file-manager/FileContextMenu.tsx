import { useEffect, useRef, useState } from 'react';
import { FileInfo } from '../../types';
import { FolderOpen, Trash2, Copy, FileEdit, Scissors, ClipboardCopy, ExternalLink, Info, FileInput } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useClipboardStore } from '../../stores/clipboardStore';

interface ContextMenuProps {
  file: FileInfo;
  x: number;
  y: number;
  currentPath: string;
  onClose: () => void;
  onRefresh?: () => void;
}

export function FileContextMenu({ file, x, y, currentPath, onClose, onRefresh }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const { item: clipboardItem, cut, copy, paste, hasItem } = useClipboardStore();
  const [showPropertyModal, setShowPropertyModal] = useState(false);
  const [fileProperty, setFileProperty] = useState<FileProperty | null>(null);

  // 点击外部关闭菜单
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEsc);

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEsc);
    };
  }, [onClose]);

  // 打开文件/文件夹
  const handleOpen = async () => {
    try {
      if (file.is_dir) {
        await invoke('open_path', { path: file.path });
      } else {
        await invoke('open_file', { path: file.path });
      }
    } catch (error) {
      console.error('Failed to open:', error);
    }
    onClose();
  };

  // 在资源管理器中显示
  const handleReveal = async () => {
    try {
      await invoke('reveal_in_explorer', { path: file.path });
    } catch (error) {
      console.error('Failed to reveal:', error);
    }
    onClose();
  };

  // 复制路径
  const handleCopyPath = async () => {
    try {
      await navigator.clipboard.writeText(file.path);
    } catch (error) {
      console.error('Failed to copy path:', error);
    }
    onClose();
  };

  // 复制文件名
  const handleCopyName = async () => {
    try {
      await navigator.clipboard.writeText(file.name);
    } catch (error) {
      console.error('Failed to copy name:', error);
    }
    onClose();
  };

  // 剪切
  const handleCut = () => {
    cut(file.path, file.name);
    onClose();
  };

  // 复制到剪贴板
  const handleCopyToClipboard = () => {
    copy(file.path, file.name);
    onClose();
  };

  // 粘贴
  const handlePaste = async () => {
    const targetDir = file.is_dir ? file.path : currentPath;
    const success = await paste(targetDir);
    if (success) {
      onRefresh?.();
    }
    onClose();
  };

  // 删除文件
  const handleDelete = async () => {
    if (!confirm(`确定要删除 "${file.name}" 吗？`)) {
      onClose();
      return;
    }

    try {
      await invoke('delete_file', { path: file.path });
      onRefresh?.();
    } catch (error) {
      console.error('Failed to delete:', error);
      alert('删除失败: ' + error);
    }
    onClose();
  };

  // 重命名
  const handleRename = async () => {
    const newName = prompt('新名称:', file.name);
    if (newName && newName !== file.name) {
      try {
        await invoke('rename_project_entry', { 
          path: file.path, 
          newName 
        });
        onRefresh?.();
      } catch (error) {
        console.error('Failed to rename:', error);
        const message = String(error).startsWith('PM_CONFLICT:')
          ? '目标位置已存在同名文件'
          : `重命名失败: ${error}`;
        alert(message);
      }
    }
    onClose();
  };

  // 查看属性
  const handleProperty = async () => {
    try {
      const property: FileProperty = await invoke('get_file_property', { path: file.path });
      setFileProperty(property);
      setShowPropertyModal(true);
    } catch (error) {
      console.error('Failed to get property:', error);
    }
    onClose();
  };

  // 计算菜单位置，防止超出视口
  const menuStyle: React.CSSProperties = {
    position: 'fixed',
    left: Math.min(x, window.innerWidth - 200),
    top: Math.min(y, window.innerHeight - 350),
    zIndex: 9999,
  };

  const canPaste = hasItem() && (file.is_dir || !!clipboardItem);
  const isCutItem = clipboardItem?.action === 'cut' && clipboardItem?.path === file.path;

  return (
    <>
      <div
        ref={menuRef}
        className="bg-white dark:bg-gray-800 rounded-lg shadow-xl border border-gray-200 dark:border-gray-700 py-1 min-w-[180px]"
        style={menuStyle}
      >
        {/* 打开 */}
        <MenuItem onClick={handleOpen} icon={<FolderOpen className="w-4 h-4" />}>
          {file.is_dir ? '打开文件夹' : '打开'}
        </MenuItem>

        {/* 在资源管理器中显示 */}
        <MenuItem onClick={handleReveal} icon={<ExternalLink className="w-4 h-4" />}>
          在资源管理器中显示
        </MenuItem>

        <MenuDivider />

        {/* 剪切 */}
        <MenuItem 
          onClick={handleCut} 
          icon={<Scissors className="w-4 h-4" />}
          active={isCutItem}
        >
          剪切
        </MenuItem>

        {/* 复制 */}
        <MenuItem onClick={handleCopyToClipboard} icon={<Copy className="w-4 h-4" />}>
          复制
        </MenuItem>

        {/* 粘贴 */}
        <MenuItem 
          onClick={handlePaste} 
          icon={<FileInput className="w-4 h-4" />}
          disabled={!canPaste}
        >
          粘贴
        </MenuItem>

        <MenuDivider />

        {/* 复制文件名 */}
        <MenuItem onClick={handleCopyName} icon={<ClipboardCopy className="w-4 h-4" />}>
          复制文件名
        </MenuItem>

        {/* 复制完整路径 */}
        <MenuItem onClick={handleCopyPath} icon={<ClipboardCopy className="w-4 h-4" />}>
          复制完整路径
        </MenuItem>

        <MenuDivider />

        {/* 重命名 */}
        <MenuItem onClick={handleRename} icon={<FileEdit className="w-4 h-4" />}>
          重命名
        </MenuItem>

        {/* 删除 */}
        <MenuItem
          onClick={handleDelete}
          icon={<Trash2 className="w-4 h-4" />}
          danger
        >
          删除
        </MenuItem>

        <MenuDivider />

        {/* 属性 */}
        <MenuItem onClick={handleProperty} icon={<Info className="w-4 h-4" />}>
          属性
        </MenuItem>
      </div>

      {/* 属性对话框 */}
      {showPropertyModal && fileProperty && (
        <PropertyModal 
          property={fileProperty} 
          onClose={() => setShowPropertyModal(false)} 
        />
      )}
    </>
  );
}

// 菜单项组件
function MenuItem({
  children,
  onClick,
  icon,
  disabled = false,
  danger = false,
  active = false,
}: {
  children: React.ReactNode;
  onClick: () => void;
  icon: React.ReactNode;
  disabled?: boolean;
  danger?: boolean;
  active?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`
        w-full px-3 py-2 flex items-center gap-2 text-sm
        ${disabled 
          ? 'text-gray-400 cursor-not-allowed' 
          : danger 
            ? 'text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20' 
            : active
              ? 'text-blue-600 bg-blue-50 dark:bg-blue-900/20'
              : 'text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700'
        }
        transition-colors
      `}
    >
      <span className={danger ? 'text-red-500' : active ? 'text-blue-500' : 'text-gray-500 dark:text-gray-400'}>
        {icon}
      </span>
      {children}
    </button>
  );
}

// 分隔线
function MenuDivider() {
  return (
    <div className="my-1 border-t border-gray-200 dark:border-gray-700" />
  );
}

// 文件属性类型
interface FileProperty {
  name: string;
  path: string;
  size: number;
  size_formatted: string;
  is_dir: boolean;
  created: string;
  modified: string;
  accessed: string;
  readonly: boolean;
  hidden: boolean;
  extension?: string;
}

// 属性对话框
function PropertyModal({ property, onClose }: { property: FileProperty; onClose: () => void }) {
  useEffect(() => {
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleEsc);
    return () => window.removeEventListener('keydown', handleEsc);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-[10000] flex items-center justify-center bg-black/50">
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-[400px] max-w-[90vw]">
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
          <h3 className="font-medium text-gray-900 dark:text-gray-100">属性</h3>
          <button 
            onClick={onClose}
            className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-200"
          >
            ✕
          </button>
        </div>
        
        <div className="p-4 space-y-3">
          <PropertyRow label="名称" value={property.name} />
          <PropertyRow label="类型" value={property.is_dir ? '文件夹' : (property.extension?.toUpperCase() || '文件') + ' 文件'} />
          <PropertyRow label="位置" value={property.path.replace(/\\[^\\]+$/, '')} truncate />
          <PropertyRow label="大小" value={property.size_formatted} />
          <div className="border-t border-gray-200 dark:border-gray-700 my-2" />
          <PropertyRow label="创建时间" value={property.created} />
          <PropertyRow label="修改时间" value={property.modified} />
          <PropertyRow label="访问时间" value={property.accessed} />
          <div className="border-t border-gray-200 dark:border-gray-700 my-2" />
          <div className="flex gap-4">
            <label className="flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300">
              <input type="checkbox" checked={property.readonly} disabled className="rounded" />
              只读
            </label>
            <label className="flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300">
              <input type="checkbox" checked={property.hidden} disabled className="rounded" />
              隐藏
            </label>
          </div>
        </div>

        <div className="flex justify-end px-4 py-3 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50 rounded-b-lg">
          <button
            onClick={onClose}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm"
          >
            确定
          </button>
        </div>
      </div>
    </div>
  );
}

function PropertyRow({ label, value, truncate = false }: { label: string; value: string; truncate?: boolean }) {
  return (
    <div className="flex">
      <span className="w-20 text-sm text-gray-500 dark:text-gray-400 flex-shrink-0">{label}</span>
      <span className={`text-sm text-gray-900 dark:text-gray-100 ${truncate ? 'truncate' : ''}`} title={value}>
        {value}
      </span>
    </div>
  );
}
