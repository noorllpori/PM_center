import { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Boxes,
  Blocks,
  FileCog,
  LogOut,
  RefreshCcw,
  ShieldCheck,
  SlidersHorizontal,
} from 'lucide-react';
import { ConfirmDialog, Dialog } from '../Dialog';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import { CapabilityDiagnosticsSection } from './CapabilityDiagnosticsSection';
import { ComponentRuntimeDiagnosticsSection } from './ComponentRuntimeDiagnosticsSection';
import { FileRoutingSettingsSection } from './FileRoutingSettingsSection';
import { ModuleDiagnosticsSection } from './ModuleDiagnosticsSection';
import { WorkspaceProfileDiagnosticsSection } from './WorkspaceProfileDiagnosticsSection';

type RecoveryPage = 'profiles' | 'modules' | 'components' | 'routing' | 'capabilities';

const RECOVERY_PAGES = [
  { id: 'profiles', label: '装配方案', icon: SlidersHorizontal },
  { id: 'modules', label: '模块与组件', icon: Boxes },
  { id: 'components', label: '组件运行时', icon: Blocks },
  { id: 'routing', label: '文件打开方式', icon: FileCog },
  { id: 'capabilities', label: '权限诊断', icon: ShieldCheck },
] as const;

export function RecoverySettingsPanel({
  isOpen,
  onClose,
}: {
  isOpen: boolean;
  onClose: () => void;
}) {
  const [activePage, setActivePage] = useState<RecoveryPage>('profiles');
  const [restoreConfirmOpen, setRestoreConfirmOpen] = useState(false);
  const [exitConfirmOpen, setExitConfirmOpen] = useState(false);
  const [isPreparingRestore, setIsPreparingRestore] = useState(false);
  const snapshot = useWorkspaceProfileStore((state) => state.snapshot);
  const switchPreview = useWorkspaceProfileStore((state) => state.switchPreview);
  const error = useWorkspaceProfileStore((state) => state.error);
  const previewSwitch = useWorkspaceProfileStore((state) => state.previewSwitch);
  const switchProfile = useWorkspaceProfileStore((state) => state.switchProfile);
  const isSwitching = useWorkspaceProfileStore((state) => state.isSwitching);

  const restoreMessage = useMemo(() => {
    if (!switchPreview) return '';
    return [
      `将切换到“${switchPreview.targetProfileName}”。`,
      `启用 ${switchPreview.modulesToEnable.length} 个模块，停止 ${switchPreview.modulesToDisable.length} 个模块。`,
      `快捷栏增加 ${switchPreview.pinnedToolsAdded.length} 项，移除 ${switchPreview.pinnedToolsRemoved.length} 项。`,
      switchPreview.resourcesToRelease > 0
        ? `将释放 ${switchPreview.resourcesToRelease} 个后台资源。`
        : '没有后台资源需要释放。',
      '自定义 Profile、组件数据和普通设置不会被删除。',
    ].join('\n');
  }, [switchPreview]);

  const prepareDefaultRestore = async () => {
    const defaultProfileId = snapshot?.defaultProfileId;
    if (!defaultProfileId) return;
    setIsPreparingRestore(true);
    await previewSwitch(defaultProfileId);
    const nextPreview = useWorkspaceProfileStore.getState().switchPreview;
    if (nextPreview && nextPreview.targetProfileId === defaultProfileId && nextPreview.canSwitch) {
      setRestoreConfirmOpen(true);
    }
    setIsPreparingRestore(false);
  };

  return (
    <>
      <Dialog
        isOpen={isOpen}
        onClose={onClose}
        title="恢复设置"
        size="2xl"
        contentClassName="min-h-0 overflow-hidden p-0"
      >
        <div className="flex h-[72vh] min-h-[500px] max-h-[780px] min-w-0 flex-col md:flex-row">
          <aside className="flex shrink-0 flex-row items-center gap-1 overflow-x-auto border-b border-gray-200 bg-gray-50/80 p-3 dark:border-gray-800 dark:bg-gray-950/40 md:w-52 md:flex-col md:items-stretch md:border-b-0 md:border-r">
            <div className="hidden px-2 pb-2 md:block">
              <p className="text-xs font-medium text-gray-500 dark:text-gray-400">不可停用的恢复内核</p>
              <p className="mt-1 text-[11px] leading-5 text-gray-400">普通设置中心关闭或装配错误时仍可使用。</p>
            </div>
            {RECOVERY_PAGES.map((page) => {
              const Icon = page.icon;
              const active = page.id === activePage;
              return (
                <button
                  key={page.id}
                  type="button"
                  onClick={() => setActivePage(page.id)}
                  className={`flex h-10 shrink-0 items-center gap-2 rounded-md px-2.5 text-sm transition-colors md:h-auto md:py-2 ${
                    active
                      ? 'bg-blue-50 font-medium text-blue-700 dark:bg-blue-950/50 dark:text-blue-300'
                      : 'text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800'
                  }`}
                >
                  <Icon className="h-4 w-4" />
                  {page.label}
                </button>
              );
            })}

            <div className="ml-auto flex shrink-0 items-center gap-1 md:ml-0 md:mt-auto md:flex-col md:items-stretch md:border-t md:border-gray-200 md:pt-3 dark:md:border-gray-800">
              <button
                type="button"
                onClick={() => void prepareDefaultRestore()}
                disabled={!snapshot || isPreparingRestore || isSwitching}
                className="flex items-center gap-2 rounded-md px-2.5 py-2 text-sm text-amber-700 hover:bg-amber-50 disabled:opacity-50 dark:text-amber-300 dark:hover:bg-amber-950/30"
              >
                <RefreshCcw className={`h-4 w-4 ${isPreparingRestore ? 'animate-spin' : ''}`} />
                恢复默认装配
              </button>
              <button
                type="button"
                onClick={() => setExitConfirmOpen(true)}
                className="flex items-center gap-2 rounded-md px-2.5 py-2 text-sm text-red-600 hover:bg-red-50 dark:text-red-300 dark:hover:bg-red-950/30"
              >
                <LogOut className="h-4 w-4" />
                退出程序
              </button>
            </div>
          </aside>

          <main className="min-h-0 min-w-0 flex-1 overflow-y-auto bg-gray-50/40 p-4 dark:bg-gray-950/20 sm:p-5">
            {error ? (
              <div className="mb-4 whitespace-pre-wrap break-all rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300">
                {error}
              </div>
            ) : null}
            {activePage === 'profiles' ? <WorkspaceProfileDiagnosticsSection /> : null}
            {activePage === 'modules' ? <ModuleDiagnosticsSection /> : null}
            {activePage === 'components' ? <ComponentRuntimeDiagnosticsSection /> : null}
            {activePage === 'routing' ? <FileRoutingSettingsSection /> : null}
            {activePage === 'capabilities' ? <CapabilityDiagnosticsSection /> : null}
          </main>
        </div>
      </Dialog>

      <ConfirmDialog
        isOpen={restoreConfirmOpen}
        onClose={() => setRestoreConfirmOpen(false)}
        onConfirm={() => {
          if (switchPreview) void switchProfile(switchPreview.targetProfileId);
        }}
        title="恢复默认装配"
        message={restoreMessage}
        confirmText="确认恢复"
        cancelText="取消"
        type="warning"
      />
      <ConfirmDialog
        isOpen={exitConfirmOpen}
        onClose={() => setExitConfirmOpen(false)}
        onConfirm={() => void invoke('exit_app')}
        title="退出程序"
        message="将关闭全部窗口并停止托盘、任务、渲染和网络后台资源。"
        confirmText="确认退出"
        cancelText="取消"
        type="danger"
      />
    </>
  );
}
