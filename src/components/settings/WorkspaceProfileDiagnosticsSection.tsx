import { useState } from 'react';
import { open, save as saveDialog } from '@tauri-apps/plugin-dialog';
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Copy,
  Download,
  FileArchive,
  FolderOpen,
  Layers3,
  Loader2,
  PackageOpen,
  Pencil,
  Plus,
  RefreshCw,
  ShieldAlert,
  Trash2,
  Upload,
} from 'lucide-react';
import {
  exportWorkspacePackage,
  exportWorkspaceProfilePackage,
  getWorkspaceProfileDocument,
  importWorkspacePackage,
  inspectWorkspacePackage,
  inspectWorkspaceProfilePackage,
} from '../../api/workspaceProfiles';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import { usePythonEnvStore } from '../../stores/pythonEnvStore';
import { useSettingsStore } from '../../stores/settingsStore';
import type {
  ProfilePackageImportPreview,
  WorkspacePackageImportPreview,
  WorkspaceProfileRuntimeCommandError,
} from '../../types/workspaceProfileRuntime';
import { ConfirmDialog, Dialog, InputDialog } from '../Dialog';
import { WorkspaceProfileEditorDialog } from './WorkspaceProfileEditorDialog';

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

const COMPONENT_DISTRIBUTION_LABEL = {
  bundled: '随安装包',
  marketplace: '商城',
  local: '本地包',
} as const;

const COMPONENT_ROLE_LABEL = {
  service: '服务',
  feature: '功能',
  data: '资料',
} as const;

const COMPONENT_UI_LABEL = {
  none: '无界面',
  hosted: '宿主界面',
  contributed: '组件界面',
} as const;

const AUTOMATIC_TOOL_MAPPING = '__automatic__';

function toolDisplayName(tool: string) {
  if (tool.endsWith('.blender')) return 'Blender';
  if (tool.endsWith('.ffmpeg')) return 'FFmpeg';
  if (tool.endsWith('.ffprobe')) return 'FFprobe';
  if (tool.endsWith('.python')) return 'Python';
  return tool;
}

function formatDate(timestamp: number) {
  return timestamp
    ? new Date(timestamp).toLocaleString('zh-CN', { hour12: false })
    : '-';
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}

function packageErrorMessage(error: unknown) {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object') {
    const typed = error as WorkspaceProfileRuntimeCommandError;
    return [typed.message, ...(typed.details ?? []), typed.path]
      .filter(Boolean)
      .join('\n');
  }
  return String(error);
}

function safePackageFileName(name: string) {
  const safe = name.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_').trim();
  return `${safe || 'nexora-profile'}.pmc-profile`;
}

function safeWorkspaceFileName(name: string) {
  const safe = name.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_').trim();
  return `${safe || 'nexora-workspace'}.pmc-workspace`;
}

export function WorkspaceProfileDiagnosticsSection() {
  const snapshot = useWorkspaceProfileStore((state) => state.snapshot);
  const isLoading = useWorkspaceProfileStore((state) => state.isLoading);
  const isSwitching = useWorkspaceProfileStore((state) => state.isSwitching);
  const isMutating = useWorkspaceProfileStore((state) => state.isMutating);
  const error = useWorkspaceProfileStore((state) => state.error);
  const switchPreview = useWorkspaceProfileStore((state) => state.switchPreview);
  const switchMessage = useWorkspaceProfileStore((state) => state.switchMessage);
  const refresh = useWorkspaceProfileStore((state) => state.refresh);
  const previewSwitch = useWorkspaceProfileStore((state) => state.previewSwitch);
  const switchProfile = useWorkspaceProfileStore((state) => state.switchProfile);
  const createProfile = useWorkspaceProfileStore((state) => state.createProfile);
  const importProfilePackage = useWorkspaceProfileStore((state) => state.importProfilePackage);
  const deleteProfile = useWorkspaceProfileStore((state) => state.deleteProfile);
  const clearSwitchPreview = useWorkspaceProfileStore((state) => state.clearSwitchPreview);
  const toolPaths = useSettingsStore((state) => state.toolPaths);
  const blenderInstallations = useSettingsStore((state) => state.blenderInstallations);
  const pythonEnvs = usePythonEnvStore((state) => state.envs);
  const selectedPythonEnvId = usePythonEnvStore((state) => state.selectedEnvId);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [createDialog, setCreateDialog] = useState<{
    kind: 'blank' | 'copy';
    sourceProfileId?: string;
    sourceDescription?: string;
  } | null>(null);
  const [newProfileName, setNewProfileName] = useState('');
  const [editorProfileId, setEditorProfileId] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null);
  const [importPreview, setImportPreview] = useState<ProfilePackageImportPreview | null>(null);
  const [importPackageKind, setImportPackageKind] = useState<'profile' | 'workspace'>('profile');
  const [importName, setImportName] = useState('');
  const [importToolMappings, setImportToolMappings] = useState<Record<string, string>>({});
  const [importPathMappings, setImportPathMappings] = useState<Record<string, string>>({});
  const [importBindingPresetId, setImportBindingPresetId] = useState('');
  const [packageBusy, setPackageBusy] = useState<string | null>(null);
  const [packageNotice, setPackageNotice] = useState<string | null>(null);
  const [packageError, setPackageError] = useState<string | null>(null);
  const currentSummary = snapshot?.profiles.find((profile) => profile.current) ?? null;
  const currentProfile = snapshot?.currentProfile ?? null;
  const toolCandidates = (tool: string) => {
    const candidates: Array<{ path: string; label: string }> = [];
    if (tool.endsWith('.blender')) {
      if (toolPaths.blender) candidates.push({ path: toolPaths.blender, label: '当前 Blender' });
      blenderInstallations.forEach((installation) => {
        candidates.push({
          path: installation.path,
          label: installation.version ? `Blender ${installation.version}` : 'Blender',
        });
      });
    } else if (tool.endsWith('.ffmpeg') && toolPaths.ffmpeg) {
      candidates.push({ path: toolPaths.ffmpeg, label: '当前 FFmpeg' });
    } else if (tool.endsWith('.ffprobe') && toolPaths.ffprobe) {
      candidates.push({ path: toolPaths.ffprobe, label: '当前 FFprobe' });
    } else if (tool.endsWith('.python')) {
      pythonEnvs.forEach((environment) => {
        candidates.push({
          path: environment.path,
          label: `${environment.name}${environment.version ? ` · ${environment.version}` : ''}`,
        });
      });
    }
    const seen = new Set<string>();
    return candidates.filter((candidate) => {
      const key = candidate.path.toLowerCase().replace(/\\/g, '/');
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  };
  const preferredToolPath = (tool: string) => {
    if (tool.endsWith('.blender')) return toolPaths.blender ?? '';
    if (tool.endsWith('.ffmpeg')) return toolPaths.ffmpeg ?? '';
    if (tool.endsWith('.ffprobe')) return toolPaths.ffprobe ?? '';
    if (tool.endsWith('.python')) {
      return pythonEnvs.find((environment) => environment.id === selectedPythonEnvId)?.path
        ?? pythonEnvs.find((environment) => environment.isEmbedded)?.path
        ?? '';
    }
    return '';
  };
  const mappingsReady = importPreview
    ? importPreview.toolAliases.every((alias) => !alias.required || Boolean(importToolMappings[alias.id]))
      && importPreview.pathVariables.every((variable) => !variable.required || Boolean(importPathMappings[variable.id]))
    : false;

  const confirmationMessage = switchPreview
    ? [
        `切换到“${switchPreview.targetProfileName}”后，将启用 ${switchPreview.modulesToEnable.length} 个模块、停止 ${switchPreview.modulesToDisable.length} 个模块。`,
        switchPreview.resourcesToRelease > 0
          ? `将释放 ${switchPreview.resourcesToRelease} 个后台资源，相关页面会自动撤下。`
          : '没有正在登记的后台资源需要释放。',
        '模块切换或状态提交失败时会自动恢复原 Profile。',
      ].join('\n')
    : '';

  const openCreateDialog = (
    kind: 'blank' | 'copy',
    sourceProfileId?: string,
    sourceName?: string,
    sourceDescription?: string,
  ) => {
    setCreateDialog({ kind, sourceProfileId, sourceDescription });
    setNewProfileName(kind === 'copy' ? `${sourceName || '装配方案'} 副本` : '新装配方案');
  };

  const createAndEditProfile = async () => {
    if (!createDialog || !newProfileName.trim()) return;
    try {
      const result = await createProfile({
        name: newProfileName.trim(),
        description: createDialog.kind === 'copy'
          ? createDialog.sourceDescription || '从现有装配方案复制，可独立修改。'
          : '从空白状态创建的装配方案。',
        sourceProfileId: createDialog.sourceProfileId ?? null,
      });
      setCreateDialog(null);
      setEditorProfileId(result.profile.id);
    } catch {
      // The store keeps the structured backend error visible in this section.
    }
  };

  const editProfile = (profile: {
    id: string;
  }) => {
    setEditorProfileId(profile.id);
  };

  const exportProfile = async (profileId: string, profileName: string) => {
    setPackageError(null);
    setPackageNotice(null);
    setPackageBusy(`export:${profileId}`);
    try {
      const destinationPath = await saveDialog({
        title: '导出 Nexora 装配方案',
        defaultPath: safePackageFileName(profileName),
        filters: [{ name: 'Nexora 装配方案', extensions: ['pmc-profile'] }],
      });
      if (!destinationPath) return;
      const result = await exportWorkspaceProfilePackage({ profileId, destinationPath });
      setPackageNotice(`已导出“${profileName}” · ${formatBytes(result.sizeBytes)}`);
    } catch (exportError) {
      setPackageError(packageErrorMessage(exportError));
    } finally {
      setPackageBusy(null);
    }
  };

  const exportWorkspace = async (profileId: string, profileName: string) => {
    setPackageError(null);
    setPackageNotice(null);
    setPackageBusy(`workspace:${profileId}`);
    try {
      const destinationPath = await saveDialog({
        title: '导出 Nexora 装配空间',
        defaultPath: safeWorkspaceFileName(profileName),
        filters: [{ name: 'Nexora 装配空间', extensions: ['pmc-workspace'] }],
      });
      if (!destinationPath) return;
      const profile = await getWorkspaceProfileDocument(profileId);
      const openSurfaceIds = (profile.surfaces ?? []).map((surface) => surface.id);
      const homeSurfaceId = profile.shellLayout?.home;
      const activeSurfaceId = homeSurfaceId
        && openSurfaceIds.includes(homeSurfaceId)
        ? homeSurfaceId
        : null;
      const result = await exportWorkspacePackage({
        profileId,
        destinationPath,
        variables: {},
        openSurfaceIds,
        activeSurfaceId,
      });
      setPackageNotice(`已导出装配空间“${profileName}” · ${formatBytes(result.sizeBytes)}`);
    } catch (exportError) {
      setPackageError(packageErrorMessage(exportError));
    } finally {
      setPackageBusy(null);
    }
  };

  const inspectImportPackage = async () => {
    setPackageError(null);
    setPackageNotice(null);
    try {
      const packagePath = await open({
        title: '选择 Nexora 装配方案包',
        multiple: false,
        directory: false,
        filters: [{ name: 'Nexora 装配方案', extensions: ['pmc-profile'] }],
      });
      if (!packagePath || Array.isArray(packagePath)) return;
      setPackageBusy('inspect');
      const preview = await inspectWorkspaceProfilePackage(packagePath);
      setImportPackageKind('profile');
      setImportPreview(preview);
      setImportName(preview.suggestedName);
      setImportToolMappings(Object.fromEntries(
        preview.toolAliases.map((alias) => [alias.id, preferredToolPath(alias.tool)]),
      ));
      setImportPathMappings({});
      setImportBindingPresetId('');
    } catch (inspectError) {
      setPackageError(packageErrorMessage(inspectError));
    } finally {
      setPackageBusy(null);
    }
  };

  const inspectImportWorkspace = async () => {
    setPackageError(null);
    setPackageNotice(null);
    try {
      const packagePath = await open({
        title: '选择 Nexora 装配空间',
        multiple: false,
        directory: false,
        filters: [{ name: 'Nexora 装配空间', extensions: ['pmc-workspace'] }],
      });
      if (!packagePath || Array.isArray(packagePath)) return;
      setPackageBusy('inspect-workspace');
      const preview = await inspectWorkspacePackage(packagePath);
      setImportPackageKind('workspace');
      setImportPreview(preview);
      setImportName(preview.suggestedName);
      setImportToolMappings(Object.fromEntries(
        preview.toolAliases.map((alias) => [alias.id, preferredToolPath(alias.tool)]),
      ));
      setImportPathMappings({});
      setImportBindingPresetId('');
    } catch (inspectError) {
      setPackageError(packageErrorMessage(inspectError));
    } finally {
      setPackageBusy(null);
    }
  };

  const importPackage = async () => {
    if (!importPreview?.canImport || !importName.trim()) return;
    setPackageBusy('import');
    setPackageError(null);
    try {
      const request = {
        packagePath: importPreview.packagePath,
        name: importName.trim(),
        toolMappings: importPreview.toolAliases.flatMap((alias) => {
          const value = importToolMappings[alias.id];
          if (!value) return [];
          return [{
            id: alias.id,
            mode: value === AUTOMATIC_TOOL_MAPPING ? 'automatic' as const : 'path' as const,
            path: value === AUTOMATIC_TOOL_MAPPING ? null : value,
          }];
        }),
        pathMappings: importPreview.pathVariables.flatMap((variable) => {
          const path = importPathMappings[variable.id];
          return path ? [{ id: variable.id, mode: 'path' as const, path }] : [];
        }),
      };
      const importedName = importPackageKind === 'workspace'
        ? (await importWorkspacePackage(request)).mutation.profile.name
        : (await importProfilePackage(request)).profile.name;
      if (importPackageKind === 'workspace') {
        await refresh();
      }
      setImportPreview(null);
      setImportToolMappings({});
      setImportPathMappings({});
      setImportBindingPresetId('');
      setPackageNotice(`已导入“${importedName}”，当前运行方案未改变。`);
    } catch (importError) {
      setPackageError(packageErrorMessage(importError));
    } finally {
      setPackageBusy(null);
    }
  };

  const deleteSelectedProfile = async (target: { id: string; name: string }) => {
    setPackageNotice(null);
    try {
      await deleteProfile(target.id);
      if (editorProfileId === target.id) {
        setEditorProfileId(null);
      }
      setPackageNotice(`已删除装配方案“${target.name}”。`);
    } catch {
      // The store keeps the structured backend error visible in this section.
    }
  };

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
        <div className="flex flex-wrap items-center justify-end gap-1.5">
          <button
            type="button"
            onClick={() => void inspectImportWorkspace()}
            disabled={isLoading || isSwitching || isMutating || Boolean(packageBusy) || Boolean(snapshot?.pendingSwitch)}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            {packageBusy === 'inspect-workspace' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <FileArchive className="h-3.5 w-3.5" />}
            导入空间
          </button>
          <button
            type="button"
            onClick={() => void inspectImportPackage()}
            disabled={isLoading || isSwitching || isMutating || Boolean(packageBusy) || Boolean(snapshot?.pendingSwitch)}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            {packageBusy === 'inspect' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Upload className="h-3.5 w-3.5" />}
            导入方案
          </button>
          <button
            type="button"
            onClick={() => openCreateDialog('blank')}
            disabled={isLoading || isSwitching || isMutating || Boolean(snapshot?.pendingSwitch)}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            <Plus className="h-3.5 w-3.5" />
            新建方案
          </button>
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={isLoading || isSwitching || isMutating}
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
            title="刷新装配方案诊断"
          >
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {error ? (
        <div className="mt-3 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span className="whitespace-pre-wrap break-all">{error}</span>
        </div>
      ) : null}

      {packageError ? (
        <div className="mt-3 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span className="whitespace-pre-wrap break-all">{packageError}</span>
        </div>
      ) : null}

      {packageNotice ? (
        <div className="mt-3 flex items-center gap-2 rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-xs text-emerald-700 dark:border-emerald-900/50 dark:bg-emerald-950/30 dark:text-emerald-300">
          <CheckCircle2 className="h-4 w-4 shrink-0" />
          <span>{packageNotice}</span>
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
          <div className="mt-3 grid grid-cols-2 gap-px overflow-hidden rounded-md border border-gray-200 bg-gray-200 lg:grid-cols-5 dark:border-gray-700 dark:bg-gray-700">
            {[
              ['方案数量', snapshot.profiles.length],
              ['当前模块', currentProfile.enabledModules?.length ?? 0],
              ['有效组件', snapshot.components.filter((component) => component.effectiveEnabled).length],
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
                      {profile.enabledModuleCount} 模块 · {profile.effectiveComponentCount} 组件 · {profile.pinnedToolCount} 固定工具 · r{profile.revision}
                    </p>
                    {profile.issues.map((issue) => (
                      <p key={issue} className="mt-1 text-xs text-amber-700 dark:text-amber-300">{issue}</p>
                    ))}
                  </div>
                  <div className="flex flex-wrap items-center justify-end gap-1.5">
                    <button
                      type="button"
                      onClick={() => void exportWorkspace(profile.id, profile.name)}
                      disabled={profile.status !== 'ready' || isLoading || isSwitching || isMutating || Boolean(packageBusy)}
                      className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
                      title="导出 Profile、变量声明和页面骨架"
                    >
                      {packageBusy === `workspace:${profile.id}` ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <FileArchive className="h-3.5 w-3.5" />}
                      空间
                    </button>
                    <button
                      type="button"
                      onClick={() => void exportProfile(profile.id, profile.name)}
                      disabled={profile.status !== 'ready' || isLoading || isSwitching || isMutating || Boolean(packageBusy)}
                      className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
                    >
                      {packageBusy === `export:${profile.id}` ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Download className="h-3.5 w-3.5" />}
                      导出
                    </button>
                    <button
                      type="button"
                      onClick={() => editProfile(profile)}
                      disabled={profile.status !== 'ready' || isLoading || isSwitching || isMutating || Boolean(snapshot.pendingSwitch)}
                      className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
                      title={profile.current ? '编辑当前装配方案，保存后立即应用' : '编辑装配方案'}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                      编辑
                    </button>
                    <button
                      type="button"
                      onClick={() => openCreateDialog('copy', profile.id, profile.name, profile.description)}
                      disabled={profile.status !== 'ready' || isLoading || isSwitching || isMutating || Boolean(snapshot.pendingSwitch)}
                      className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
                    >
                      <Copy className="h-3.5 w-3.5" />
                      复制
                    </button>
                    {!profile.current && (
                      profile.id.startsWith('local.profile-') || profile.id === 'local.current-pm-center'
                    ) ? (
                      <button
                        type="button"
                        onClick={() => setDeleteTarget({ id: profile.id, name: profile.name })}
                        disabled={isLoading || isSwitching || isMutating || Boolean(snapshot.pendingSwitch)}
                        className="inline-flex h-8 items-center gap-1.5 rounded-md border border-red-200 bg-white px-2.5 text-xs text-red-600 hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-red-900/60 dark:bg-gray-900 dark:text-red-300 dark:hover:bg-red-950/30"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        删除
                      </button>
                    ) : null}
                    <button
                      type="button"
                      onClick={() => void previewSwitch(profile.id)}
                      disabled={profile.status !== 'ready' || isLoading || isSwitching || isMutating}
                      className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-200 bg-white px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
                    >
                      {isLoading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ArrowRight className="h-3.5 w-3.5" />}
                      查看影响
                    </button>
                  </div>
                </div>
              );
            })}
          </div>

          <div className="mt-4 border-t border-gray-200 pt-4 dark:border-gray-700">
            <div className="flex flex-wrap items-start justify-between gap-2">
              <div className="flex min-w-0 items-start gap-2.5">
                <PackageOpen className="mt-0.5 h-4 w-4 shrink-0 text-gray-500" />
                <div>
                  <p className="text-sm font-semibold text-gray-900 dark:text-gray-100">已安装组件</p>
                  <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                    “随安装包”只表示初始来源；组件仍可由后续组件管理器卸载、重装或升级。
                  </p>
                </div>
              </div>
              <span className="rounded bg-gray-100 px-2 py-1 text-xs text-gray-600 dark:bg-gray-800 dark:text-gray-300">
                {snapshot.components.length} 个
              </span>
            </div>

            <div className="mt-3 divide-y divide-gray-100 overflow-hidden rounded-md border border-gray-200 dark:divide-gray-800 dark:border-gray-700">
              {snapshot.components.map((component) => (
                <div key={component.id} className="px-3 py-3">
                  <div className="flex flex-wrap items-start gap-2">
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-1.5">
                        <span className="text-sm font-medium text-gray-900 dark:text-gray-100">{component.name}</span>
                        <span className="font-mono text-[11px] text-gray-400">{component.id} · v{component.version}</span>
                      </div>
                      <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{component.description}</p>
                    </div>
                    <span className={`rounded px-1.5 py-0.5 text-[11px] font-medium ${
                      component.effectiveEnabled
                        ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300'
                        : 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300'
                    }`}>
                      {component.effectiveEnabled ? '当前生效' : '当前未启用'}
                    </span>
                  </div>
                  <div className="mt-2 flex flex-wrap gap-1.5 text-[11px]">
                    <span className="rounded bg-gray-100 px-1.5 py-0.5 text-gray-600 dark:bg-gray-800 dark:text-gray-300">
                      {COMPONENT_DISTRIBUTION_LABEL[component.distribution]}
                    </span>
                    <span className="rounded bg-gray-100 px-1.5 py-0.5 text-gray-600 dark:bg-gray-800 dark:text-gray-300">
                      {COMPONENT_ROLE_LABEL[component.role]} · {COMPONENT_UI_LABEL[component.uiMode]}
                    </span>
                    <span className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-gray-600 dark:bg-gray-800 dark:text-gray-300">
                      {component.runtime}
                    </span>
                    {component.explicitEnabled ? (
                      <span className="rounded bg-indigo-100 px-1.5 py-0.5 text-indigo-700 dark:bg-indigo-950/50 dark:text-indigo-300">
                        Profile 显式选择
                      </span>
                    ) : null}
                    {component.requiredByModules.length > 0 ? (
                      <span
                        className="rounded bg-blue-100 px-1.5 py-0.5 text-blue-700 dark:bg-blue-950/50 dark:text-blue-300"
                        title={component.requiredByModules.join('、')}
                      >
                        {component.requiredByModules.length} 个模块依赖
                      </span>
                    ) : null}
                    {component.requiredByComponents.length > 0 ? (
                      <span
                        className="rounded bg-violet-100 px-1.5 py-0.5 text-violet-700 dark:bg-violet-950/50 dark:text-violet-300"
                        title={component.requiredByComponents.join('、')}
                      >
                        {component.requiredByComponents.length} 个组件依赖
                      </span>
                    ) : null}
                  </div>
                </div>
              ))}
              {snapshot.components.length === 0 ? (
                <p className="px-3 py-4 text-xs text-gray-500 dark:text-gray-400">当前没有登记已安装组件。</p>
              ) : null}
            </div>
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
      <ConfirmDialog
        isOpen={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={() => {
          if (deleteTarget) {
            void deleteSelectedProfile(deleteTarget);
          }
        }}
        title="删除装配方案"
        message={deleteTarget
          ? `确定删除“${deleteTarget.name}”吗？\n\n删除后无法恢复，当前运行方案和系统恢复方案不会受影响。`
          : ''}
        confirmText="确认删除"
        cancelText="取消"
        type="danger"
      />
      <InputDialog
        isOpen={Boolean(createDialog)}
        onClose={() => setCreateDialog(null)}
        onConfirm={() => createAndEditProfile()}
        title={createDialog?.kind === 'copy' ? '复制装配方案' : '新建装配方案'}
        label="方案名称"
        value={newProfileName}
        onChange={setNewProfileName}
        confirmText={createDialog?.kind === 'copy' ? '复制并编辑' : '创建并编辑'}
        disabled={isMutating || !newProfileName.trim()}
        description="创建后不会自动切换当前运行方案；保存草稿后仍需通过“查看影响”明确应用。"
        selectOnOpen
      />
      <WorkspaceProfileEditorDialog
        isOpen={Boolean(editorProfileId)}
        profileId={editorProfileId}
        onClose={() => setEditorProfileId(null)}
      />
      <Dialog
        isOpen={Boolean(importPreview)}
        onClose={() => {
          if (packageBusy !== 'import') {
            setImportPreview(null);
            setImportToolMappings({});
            setImportPathMappings({});
            setImportBindingPresetId('');
          }
        }}
        title={importPackageKind === 'workspace' ? '导入装配空间' : '导入装配方案'}
        size="lg"
        footer={
          <>
            <button
              type="button"
              onClick={() => {
                setImportPreview(null);
                setImportToolMappings({});
                setImportPathMappings({});
                setImportBindingPresetId('');
              }}
              disabled={packageBusy === 'import'}
              className="rounded-md px-3 py-2 text-sm text-gray-600 hover:bg-gray-100 disabled:opacity-50 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              取消
            </button>
            <button
              type="button"
              onClick={() => void importPackage()}
              disabled={!importPreview?.canImport || !mappingsReady || !importName.trim() || packageBusy === 'import'}
              className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-3 py-2 text-sm text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {packageBusy === 'import' ? <Loader2 className="h-4 w-4 animate-spin" /> : <Upload className="h-4 w-4" />}
              导入为新方案
            </button>
          </>
        }
      >
        {importPreview ? (
          <div className="space-y-4">
            <div className="flex items-start gap-3 rounded-md border border-gray-200 p-3 dark:border-gray-700">
              <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-indigo-100 text-indigo-700 dark:bg-indigo-950/50 dark:text-indigo-300">
                <FileArchive className="h-4 w-4" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="text-sm font-semibold text-gray-900 dark:text-gray-100">{importPreview.profileName}</p>
                  <span className={`rounded px-1.5 py-0.5 text-[11px] font-medium ${!importPreview.canImport ? 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-300' : mappingsReady ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300' : 'bg-amber-100 text-amber-700 dark:bg-amber-950/50 dark:text-amber-300'}`}>
                    {!importPreview.canImport ? '存在阻塞' : mappingsReady ? '检查通过' : '等待本机映射'}
                  </span>
                </div>
                <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{importPreview.description || '无方案说明'}</p>
                <p className="mt-1 break-all font-mono text-[11px] text-gray-400">
                  {importPreview.packageId} · Nexora {importPreview.producerVersion} · {formatBytes(importPreview.packageSizeBytes)}
                </p>
                {importPackageKind === 'workspace' ? (
                  <p className="mt-1 text-[11px] text-indigo-600 dark:text-indigo-300">
                    包含 {(importPreview as WorkspacePackageImportPreview).openSurfaceIds.length} 个打开页面引用、{Object.keys((importPreview as WorkspacePackageImportPreview).variables).length} 个普通变量。
                  </p>
                ) : null}
              </div>
            </div>

            <div className="grid grid-cols-2 gap-px overflow-hidden rounded-md border border-gray-200 bg-gray-200 text-center sm:grid-cols-5 dark:border-gray-700 dark:bg-gray-700">
              {[
                ['模块', importPreview.moduleCount],
                ['组件', importPreview.componentCount],
                ['页面', importPreview.surfaceCount],
                ['Widget', importPreview.widgetCount],
                ['固定工具', importPreview.pinnedToolCount],
              ].map(([label, value]) => (
                <div key={String(label)} className="bg-white px-2 py-2 dark:bg-gray-900">
                  <p className="text-[11px] text-gray-500">{label}</p>
                  <p className="mt-1 text-sm font-semibold text-gray-900 dark:text-gray-100">{value}</p>
                </div>
              ))}
            </div>

            {(importPreview.toolAliases.length > 0 || importPreview.pathVariables.length > 0) ? (
              <div className="space-y-3 rounded-md border border-indigo-200 bg-indigo-50/50 p-3 dark:border-indigo-900 dark:bg-indigo-950/20">
                <div>
                  <p className="text-xs font-semibold text-indigo-900 dark:text-indigo-200">本机工具与路径映射</p>
                  <p className="mt-0.5 text-[11px] text-indigo-700/80 dark:text-indigo-300/70">
                    映射只保存在本机，不写入可分享的装配方案。
                  </p>
                </div>
                {importPreview.reusableBindingPresets.length > 0 ? (
                  <label className="block rounded-md border border-indigo-200 bg-white/70 p-2 dark:border-indigo-900 dark:bg-gray-950/40">
                    <span className="mb-1 block text-[11px] font-medium text-gray-700 dark:text-gray-300">沿用其他方案的相同选项</span>
                    <select
                      value={importBindingPresetId}
                      onChange={(event) => {
                        const presetId = event.target.value;
                        setImportBindingPresetId(presetId);
                        const preset = importPreview.reusableBindingPresets.find((item) => item.profileId === presetId);
                        if (!preset) return;
                        setImportToolMappings((values) => ({
                          ...values,
                          ...Object.fromEntries(preset.toolMappings.flatMap((mapping) => {
                            const value = mapping.mode === 'automatic' ? AUTOMATIC_TOOL_MAPPING : mapping.path;
                            return value ? [[mapping.id, value]] : [];
                          })),
                        }));
                        setImportPathMappings((values) => ({
                          ...values,
                          ...Object.fromEntries(preset.pathMappings.flatMap((mapping) => (
                            mapping.path ? [[mapping.id, mapping.path]] : []
                          ))),
                        }));
                      }}
                      disabled={packageBusy === 'import'}
                      className="h-9 w-full rounded-md border border-gray-200 bg-white px-2 text-xs dark:border-gray-700 dark:bg-gray-900"
                    >
                      <option value="">不沿用，单独设置</option>
                      {importPreview.reusableBindingPresets.map((preset) => (
                        <option key={preset.profileId} value={preset.profileId}>
                          {preset.profileName} · {preset.toolMappings.length + preset.pathMappings.length} 项可复用
                        </option>
                      ))}
                    </select>
                    <span className="mt-1 block text-[10px] text-gray-500">只复制相同工具和同名路径的当前值，不会与原方案建立关联。</span>
                  </label>
                ) : null}
                {importPreview.toolAliases.map((alias) => {
                  const candidates = toolCandidates(alias.tool);
                  const current = importToolMappings[alias.id] ?? '';
                  const hasCustomValue = current
                    && current !== AUTOMATIC_TOOL_MAPPING
                    && !candidates.some((candidate) => candidate.path === current);
                  return (
                    <div key={alias.id} className="grid gap-2 sm:grid-cols-[minmax(150px,0.75fr)_minmax(220px,1.25fr)_auto] sm:items-center">
                      <div className="min-w-0">
                        <p className="truncate text-xs font-medium text-gray-800 dark:text-gray-200">
                          {toolDisplayName(alias.tool)}
                          {alias.required ? <span className="ml-1 text-red-500">*</span> : null}
                        </p>
                        <p className="truncate font-mono text-[10px] text-gray-500" title={alias.description || alias.id}>
                          {alias.id} · {alias.versionRequirement || '*'}
                        </p>
                      </div>
                      <select
                        value={current}
                        onChange={(event) => setImportToolMappings((values) => ({
                          ...values,
                          [alias.id]: event.target.value,
                        }))}
                        disabled={packageBusy === 'import'}
                        className="h-9 min-w-0 rounded-md border border-gray-200 bg-white px-2 text-xs dark:border-gray-700 dark:bg-gray-900"
                      >
                        <option value="">请选择映射</option>
                        <option value={AUTOMATIC_TOOL_MAPPING}>使用系统自动检测</option>
                        {candidates.map((candidate) => (
                          <option key={candidate.path} value={candidate.path}>
                            {candidate.label} · {candidate.path}
                          </option>
                        ))}
                        {hasCustomValue ? <option value={current}>{current}</option> : null}
                      </select>
                      <button
                        type="button"
                        onClick={async () => {
                          const selected = await open({
                            title: `选择 ${toolDisplayName(alias.tool)} 可执行文件`,
                            multiple: false,
                            directory: false,
                          });
                          if (selected && !Array.isArray(selected)) {
                            setImportToolMappings((values) => ({ ...values, [alias.id]: selected }));
                          }
                        }}
                        disabled={packageBusy === 'import'}
                        className="inline-flex h-9 items-center justify-center gap-1 rounded-md border border-gray-200 bg-white px-2 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
                      >
                        <FolderOpen className="h-3.5 w-3.5" />
                        选择
                      </button>
                    </div>
                  );
                })}
                {importPreview.pathVariables.map((variable) => (
                  <div key={variable.id} className="grid gap-2 sm:grid-cols-[minmax(150px,0.75fr)_minmax(220px,1.25fr)_auto] sm:items-center">
                    <div className="min-w-0">
                      <p className="truncate text-xs font-medium text-gray-800 dark:text-gray-200">
                        {variable.description || variable.id}
                        {variable.required ? <span className="ml-1 text-red-500">*</span> : null}
                      </p>
                      <p className="font-mono text-[10px] text-gray-500">{variable.id} · {variable.kind === 'directory' ? '目录' : '文件'}</p>
                    </div>
                    <input
                      value={importPathMappings[variable.id] ?? ''}
                      readOnly
                      placeholder={variable.required ? '尚未选择' : '可选，不映射'}
                      className="h-9 min-w-0 rounded-md border border-gray-200 bg-white px-2 text-xs dark:border-gray-700 dark:bg-gray-900"
                    />
                    <button
                      type="button"
                      onClick={async () => {
                        const selected = await open({
                          title: variable.description || `选择 ${variable.id}`,
                          multiple: false,
                          directory: variable.kind === 'directory',
                        });
                        if (selected && !Array.isArray(selected)) {
                          setImportPathMappings((values) => ({ ...values, [variable.id]: selected }));
                        }
                      }}
                      disabled={packageBusy === 'import'}
                      className="inline-flex h-9 items-center justify-center gap-1 rounded-md border border-gray-200 bg-white px-2 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
                    >
                      <FolderOpen className="h-3.5 w-3.5" />
                      选择
                    </button>
                  </div>
                ))}
              </div>
            ) : null}

            <label className="block">
              <span className="mb-1.5 block text-xs font-medium text-gray-700 dark:text-gray-300">导入后的方案名称</span>
              <input
                value={importName}
                onChange={(event) => setImportName(event.target.value)}
                maxLength={80}
                disabled={packageBusy === 'import'}
                className="h-9 w-full rounded-md border border-gray-200 bg-white px-3 text-sm outline-none focus:border-indigo-400 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100"
              />
              <span className="mt-1 block text-[11px] text-gray-500">导入只新增方案，不会切换当前运行状态，也不会安装缺失模块或组件；本机路径单独保存。</span>
            </label>

            {(importPreview.missingModuleIds.length > 0 || importPreview.missingComponentIds.length > 0) ? (
              <div className="rounded-md bg-red-50 px-3 py-2 text-xs text-red-700 dark:bg-red-950/30 dark:text-red-300">
                {importPreview.missingModuleIds.length > 0 ? (
                  <p className="break-all">缺失模块：{importPreview.missingModuleIds.join('、')}</p>
                ) : null}
                {importPreview.missingComponentIds.length > 0 ? (
                  <p className="mt-1 break-all">缺失组件：{importPreview.missingComponentIds.join('、')}</p>
                ) : null}
              </div>
            ) : null}

            {importPreview.issues.length > 0 ? (
              <div className="space-y-1.5">
                {importPreview.issues.map((issue, index) => (
                  <div
                    key={`${issue.code}:${issue.path ?? index}`}
                    className={`rounded-md px-3 py-2 text-xs ${issue.severity === 'error' ? 'bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-300' : issue.severity === 'warning' ? 'bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-300' : 'bg-blue-50 text-blue-700 dark:bg-blue-950/30 dark:text-blue-300'}`}
                  >
                    <p className="font-medium">{issue.code}</p>
                    <p className="mt-0.5">{issue.message}</p>
                    {issue.path ? <p className="mt-0.5 break-all font-mono opacity-70">{issue.path}</p> : null}
                  </div>
                ))}
              </div>
            ) : (
              <div className="flex items-center gap-2 rounded-md bg-emerald-50 px-3 py-2 text-xs text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300">
                <CheckCircle2 className="h-4 w-4" />
                包结构、摘要、敏感数据和依赖检查均通过。
              </div>
            )}
          </div>
        ) : null}
      </Dialog>
    </section>
  );
}
