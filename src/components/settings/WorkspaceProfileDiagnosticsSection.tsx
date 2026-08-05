import { useState } from 'react';
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Layers3,
  Loader2,
  RefreshCw,
  ShieldAlert,
} from 'lucide-react';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import { ConfirmDialog } from '../Dialog';

const STATUS_META = {
  ready: {
    label: '可用',
    className: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300',
  },
  blocked: {
    label: '被依赖阻止',
    className: 'bg-amber-100 text-amber-700 dark:bg-amber-950/50 dark:text-amber-300',
  },
  invalid: {
    label: '文件无效',
    className: 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-300',
  },
} as const;

function formatDate(timestamp: number) {
  return timestamp
    ? new Date(timestamp).toLocaleString('zh-CN', { hour12: false })
    : '-';
}

export function WorkspaceProfileDiagnosticsSection() {
  const snapshot = useWorkspaceProfileStore((state) => state.snapshot);
  const isLoading = useWorkspaceProfileStore((state) => state.isLoading);
  const isSwitching = useWorkspaceProfileStore((state) => state.isSwitching);
  const error = useWorkspaceProfileStore((state) => state.error);
  const switchPreview = useWorkspaceProfileStore((state) => state.switchPreview);
  const switchMessage = useWorkspaceProfileStore((state) => state.switchMessage);
  const refresh = useWorkspaceProfileStore((state) => state.refresh);
  const previewSwitch = useWorkspaceProfileStore((state) => state.previewSwitch);
  const switchProfile = useWorkspaceProfileStore((state) => state.switchProfile);
  const clearSwitchPreview = useWorkspaceProfileStore((state) => state.clearSwitchPreview);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const currentSummary = snapshot?.profiles.find((profile) => profile.current) ?? null;
  const currentProfile = snapshot?.currentProfile ?? null;

  const confirmationMessage = switchPreview
    ? [
        `切换到“${switchPreview.targetProfileName}”后，将启用 ${switchPreview.modulesToEnable.length} 个模块、停止 ${switchPreview.modulesToDisable.length} 个模块。`,
        switchPreview.resourcesToRelease > 0
          ? `将释放 ${switchPreview.resourcesToRelease} 个后台资源，相关页面会自动撤下。`
          : '没有正在登记的后台资源需要释放。',
        '模块切换或状态提交失败时会自动恢复原 Profile。',
      ].join('\n')
    : '';

  return (
    <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-indigo-100 text-indigo-700 dark:bg-indigo-950/50 dark:text-indigo-300">
            <Layers3 className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">装配方案运行时</h4>
            <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
              Profile 统一管理模块与快捷栏；切换中断或运行时偏差会在启动时自动恢复。
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={() => void refresh()}
          disabled={isLoading || isSwitching}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
          title="刷新装配方案诊断"
        >
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {error ? (
        <div className="mt-3 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span className="whitespace-pre-wrap break-all">{error}</span>
        </div>
      ) : null}

      {switchMessage ? (
        <div className="mt-3 flex items-center gap-2 rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-xs text-emerald-700 dark:border-emerald-900/50 dark:bg-emerald-950/30 dark:text-emerald-300">
          <CheckCircle2 className="h-4 w-4 shrink-0" />
          <span>{switchMessage}</span>
        </div>
      ) : null}

      {snapshot?.lastRecovery ? (
        <div className="mt-3 flex items-start gap-2 rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-xs text-blue-700 dark:border-blue-900/50 dark:bg-blue-950/30 dark:text-blue-300">
          <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" />
          <span>
            {snapshot.lastRecovery.message} · {formatDate(snapshot.lastRecovery.recoveredAt)}
          </span>
        </div>
      ) : null}

      {snapshot?.pendingSwitch ? (
        <div className="mt-3 flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-300">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span>切换完成标记尚未落盘，重启后会继续恢复，不会重复切换模块。</span>
        </div>
      ) : null}

      {snapshot && currentProfile ? (
        <>
          <div className="mt-3 grid grid-cols-2 gap-px overflow-hidden rounded-md border border-gray-200 bg-gray-200 sm:grid-cols-4 dark:border-gray-700 dark:bg-gray-700">
            {[
              ['方案数量', snapshot.profiles.length],
              ['当前模块', currentProfile.enabledModules?.length ?? 0],
              ['固定工具', currentProfile.shellLayout?.pinnedTools?.length ?? 0],
              ['修订', currentProfile.revision ?? 0],
            ].map(([label, value]) => (
              <div key={String(label)} className="bg-white px-3 py-2.5 dark:bg-gray-900">
                <p className="text-[11px] text-gray-500 dark:text-gray-400">{label}</p>
                <p className="mt-1 text-base font-semibold text-gray-900 dark:text-gray-100">{value}</p>
              </div>
            ))}
          </div>

          <div className="mt-3 space-y-2">
            {snapshot.profiles.map((profile) => {
              const status = STATUS_META[profile.status];
              return (
                <div
                  key={profile.id}
                  className={`flex flex-wrap items-center gap-3 rounded-md border px-3 py-3 ${
                    profile.current
                      ? 'border-indigo-200 bg-indigo-50/60 dark:border-indigo-900/60 dark:bg-indigo-950/20'
                      : 'border-gray-200 dark:border-gray-700'
                  }`}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium text-gray-900 dark:text-gray-100">{profile.name}</span>
                      {profile.current ? (
                        <span className="rounded bg-indigo-100 px-1.5 py-0.5 text-[11px] font-medium text-indigo-700 dark:bg-indigo-950/60 dark:text-indigo-300">
                          当前
                        </span>
                      ) : null}
                      <span className={`rounded px-1.5 py-0.5 text-[11px] font-medium ${status.className}`}>
                        {status.label}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{profile.description}</p>
                    <p className="mt-1 font-mono text-[11px] text-gray-400">
                      {profile.enabledModuleCount} 模块 · {profile.pinnedToolCount} 固定工具 · r{profile.revision}
                    </p>
                    {profile.issues.map((issue) => (
                      <p key={issue} className="mt-1 text-xs text-amber-700 dark:text-amber-300">{issue}</p>
                    ))}
                  </div>
                  <button
                    type="button"
                    onClick={() => void previewSwitch(profile.id)}
                    disabled={profile.status !== 'ready' || isLoading || isSwitching}
                    className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
                  >
                    {isLoading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ArrowRight className="h-3.5 w-3.5" />}
                    查看影响
                  </button>
                </div>
              );
            })}
          </div>

          {switchPreview ? (
            <div className="mt-3 rounded-md border border-indigo-200 bg-indigo-50/40 p-3 dark:border-indigo-900/60 dark:bg-indigo-950/15">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold text-gray-900 dark:text-gray-100">
                    切换预览 · {switchPreview.targetProfileName}
                  </p>
                  <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                    {switchPreview.noChanges ? '当前状态已经与目标一致。' : '预览不会修改模块、页面或快捷栏。'}
                  </p>
                </div>
                <span className={`rounded px-2 py-1 text-xs font-medium ${
                  switchPreview.canSwitch
                    ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300'
                    : 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-300'
                }`}>
                  {switchPreview.canSwitch ? '可以切换' : '存在阻塞'}
                </span>
              </div>

              <div className="mt-3 grid grid-cols-3 gap-2 text-center">
                <div className="rounded-md bg-white px-2 py-2 dark:bg-gray-900">
                  <p className="text-[11px] text-gray-500">启用模块</p>
                  <p className="mt-1 text-sm font-semibold text-emerald-600">{switchPreview.modulesToEnable.length}</p>
                </div>
                <div className="rounded-md bg-white px-2 py-2 dark:bg-gray-900">
                  <p className="text-[11px] text-gray-500">停止模块</p>
                  <p className="mt-1 text-sm font-semibold text-amber-600">{switchPreview.modulesToDisable.length}</p>
                </div>
                <div className="rounded-md bg-white px-2 py-2 dark:bg-gray-900">
                  <p className="text-[11px] text-gray-500">释放资源</p>
                  <p className="mt-1 text-sm font-semibold text-gray-800 dark:text-gray-100">{switchPreview.resourcesToRelease}</p>
                </div>
              </div>

              {switchPreview.modulesToEnable.length > 0 ? (
                <p className="mt-3 text-xs text-gray-600 dark:text-gray-300">
                  启用：{switchPreview.modulesToEnable.map((module) => module.name).join('、')}
                </p>
              ) : null}
              {switchPreview.modulesToDisable.length > 0 ? (
                <p className="mt-2 text-xs text-gray-600 dark:text-gray-300">
                  停止：{switchPreview.modulesToDisable.map((module) => module.name).join('、')}
                </p>
              ) : null}
              {(switchPreview.pinnedToolsAdded.length > 0 || switchPreview.pinnedToolsRemoved.length > 0) ? (
                <p className="mt-2 break-all text-xs text-gray-600 dark:text-gray-300">
                  快捷栏：+{switchPreview.pinnedToolsAdded.length} / -{switchPreview.pinnedToolsRemoved.length}
                </p>
              ) : null}
              {switchPreview.contributionsToClose.length > 0 ? (
                <p className="mt-2 text-xs text-amber-700 dark:text-amber-300">
                  将撤下 {switchPreview.contributionsToClose.length} 个页面或表面贡献。
                </p>
              ) : null}

              {switchPreview.issues.length > 0 ? (
                <div className="mt-3 space-y-1.5">
                  {switchPreview.issues.map((issue) => (
                    <div
                      key={`${issue.code}:${issue.moduleId ?? issue.contributionId ?? issue.message}`}
                      className={`flex items-start gap-2 rounded-md px-2.5 py-2 text-xs ${
                        issue.severity === 'error'
                          ? 'bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-300'
                          : issue.severity === 'warning'
                            ? 'bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-300'
                            : 'bg-blue-50 text-blue-700 dark:bg-blue-950/30 dark:text-blue-300'
                      }`}
                    >
                      <ShieldAlert className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                      <span>{issue.message}</span>
                    </div>
                  ))}
                </div>
              ) : null}

              <div className="mt-3 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={clearSwitchPreview}
                  disabled={isSwitching}
                  className="rounded-md px-3 py-1.5 text-xs text-gray-600 hover:bg-white disabled:opacity-50 dark:text-gray-300 dark:hover:bg-gray-900"
                >
                  收起
                </button>
                <button
                  type="button"
                  onClick={() => setConfirmOpen(true)}
                  disabled={!switchPreview.canSwitch || switchPreview.noChanges || isSwitching}
                  className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-3 py-1.5 text-xs text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {isSwitching ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
                  应用此方案
                </button>
              </div>
            </div>
          ) : null}

          <div className="mt-3 grid gap-x-4 gap-y-2 text-xs text-gray-500 sm:grid-cols-2 dark:text-gray-400">
            <p>迁移来源：{snapshot.migration.source} · {snapshot.migration.sourceVersion}</p>
            <p>迁移时间：{formatDate(snapshot.migration.createdAt)}</p>
            <p className="break-all sm:col-span-2">方案目录：{snapshot.repositoryPath}</p>
            <p className="break-all sm:col-span-2">运行时状态：{snapshot.statePath}</p>
            <p className="break-all sm:col-span-2">切换日志：{snapshot.journalPath}</p>
          </div>
        </>
      ) : (
        <div className="mt-3 rounded-md bg-gray-50 px-3 py-3 text-xs text-gray-500 dark:bg-gray-800/60 dark:text-gray-400">
          {isLoading ? '正在建立当前配置的装配方案快照...' : '装配方案运行时尚未返回数据。'}
        </div>
      )}

      <ConfirmDialog
        isOpen={confirmOpen}
        onClose={() => setConfirmOpen(false)}
        onConfirm={() => {
          if (switchPreview) {
            void switchProfile(switchPreview.targetProfileId);
          }
        }}
        title="切换装配方案"
        message={confirmationMessage}
        confirmText="确认切换"
        cancelText="取消"
        type="warning"
      />
    </section>
  );
}
