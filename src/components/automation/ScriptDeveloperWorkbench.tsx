import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open, save as saveDialog } from '@tauri-apps/plugin-dialog';
import {
  AlertTriangle,
  Braces,
  CheckCircle2,
  Code2,
  FileCode2,
  FolderOpen,
  KeyRound,
  Loader2,
  PackagePlus,
  Play,
  RefreshCw,
  Save,
  ShieldCheck,
  ShieldOff,
  Trash2,
} from 'lucide-react';
import {
  createScriptComponentTemplate,
  generateScriptSigningKey,
  listAutomationBindings,
  listScriptDevelopmentFiles,
  openScriptDevelopmentDirectoryInVSCode,
  packageScriptComponent,
  readScriptDevelopmentFile,
  reloadScriptComponent,
  removeAutomationBinding,
  saveAutomationBinding,
  saveScriptDevelopmentFile,
  trustScriptDevelopmentDirectory,
  untrustScriptDevelopmentDirectory,
  validateScriptComponent,
} from '../../api/scriptAutomation';
import { useAutomationStore } from '../../stores/automationStore';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import type {
  ScriptComponentValidation,
  ScriptDevelopmentDocument,
  ScriptDevelopmentFile,
} from '../../types/automation';
import type {
  AutomationProjectContext,
  AutomationTriggerBinding,
  JsonValue,
  ProfileAutomationBinding,
  WorkspaceProfileV1,
} from '../../types/platform';
import { Dialog } from '../Dialog';
import { ScriptSurfaceFrame } from './ScriptSurfaceFrame';

interface ScriptDeveloperWorkbenchProps {
  isOpen: boolean;
  onClose: () => void;
  projectPath?: string | null;
}

type WorkbenchSection = 'components' | 'code' | 'bindings' | 'surfaces';
type TriggerKind = AutomationTriggerBinding['kind'];

const EVENT_OPTIONS = [
  'app.started',
  'project.opened',
  'project.closed',
  'file.changed',
  'lan.file-received',
  'render.batch-created',
  'render.batch-completed',
  'render.job-completed',
  'task.completed',
  'task.failed',
];

function cloneProfile(profile: WorkspaceProfileV1): WorkspaceProfileV1 {
  return JSON.parse(JSON.stringify(profile)) as WorkspaceProfileV1;
}

function parseJson(text: string): JsonValue {
  return JSON.parse(text) as JsonValue;
}

export function ScriptDeveloperWorkbench({ isOpen, onClose, projectPath }: ScriptDeveloperWorkbenchProps) {
  const snapshot = useAutomationStore((state) => state.snapshot);
  const initialize = useAutomationStore((state) => state.initialize);
  const refreshAutomation = useAutomationStore((state) => state.refresh);
  const startRun = useAutomationStore((state) => state.startRun);
  const profileSnapshot = useWorkspaceProfileStore((state) => state.snapshot);
  const refreshProfiles = useWorkspaceProfileStore((state) => state.refresh);
  const saveCurrentProfile = useWorkspaceProfileStore((state) => state.saveCurrentProfile);
  const [section, setSection] = useState<WorkbenchSection>('components');
  const [sourcePath, setSourcePath] = useState('');
  const [validation, setValidation] = useState<ScriptComponentValidation | null>(null);
  const [files, setFiles] = useState<ScriptDevelopmentFile[]>([]);
  const [activeDocument, setActiveDocument] = useState<ScriptDevelopmentDocument | null>(null);
  const [editorContent, setEditorContent] = useState('');
  const [selectedComponentId, setSelectedComponentId] = useState('');
  const [selectedCommand, setSelectedCommand] = useState('');
  const [runInput, setRunInput] = useState('{}');
  const [bindings, setBindings] = useState<ProfileAutomationBinding[]>([]);
  const [editingBindingId, setEditingBindingId] = useState<string | null>(null);
  const [triggerKind, setTriggerKind] = useState<TriggerKind>('manual');
  const [eventName, setEventName] = useState('file.changed');
  const [cron, setCron] = useState('0 9 * * 1-5');
  const [projectContext, setProjectContext] = useState<AutomationProjectContext>('none');
  const [projectVariable, setProjectVariable] = useState('');
  const [bindingInput, setBindingInput] = useState('{}');
  const [selectedSurfaceId, setSelectedSurfaceId] = useState('');
  const [templateParent, setTemplateParent] = useState('');
  const [templateId, setTemplateId] = useState('local.script-example');
  const [templateName, setTemplateName] = useState('脚本组件示例');
  const [templateSurface, setTemplateSurface] = useState(true);
  const [signingKeyPath, setSigningKeyPath] = useState('');
  const [publisherId, setPublisherId] = useState('local.publisher');
  const [publisherName, setPublisherName] = useState('Local Publisher');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const currentProfile = profileSnapshot?.currentProfile ?? null;
  const components = snapshot?.availableComponents ?? [];
  const selectedComponent = components.find((item) => item.componentId === selectedComponentId) ?? components[0] ?? null;
  const selectedSurface = selectedComponent?.surfaces.find((item) => item.id === selectedSurfaceId)
    ?? selectedComponent?.surfaces[0]
    ?? null;
  const declaredEventOptions = useMemo(
    () => EVENT_OPTIONS.filter((event) => selectedComponent?.events.includes(event)),
    [selectedComponent],
  );

  useEffect(() => {
    if (!isOpen) return;
    void Promise.all([initialize(), refreshProfiles()]);
  }, [initialize, isOpen, refreshProfiles]);

  useEffect(() => {
    if (!isOpen || !currentProfile) return;
    void listAutomationBindings(currentProfile.id)
      .then(setBindings)
      .catch((nextError) => setError(String(nextError)));
  }, [currentProfile?.id, currentProfile?.revision, isOpen]);

  useEffect(() => {
    if (!selectedComponent) {
      setSelectedComponentId('');
      setSelectedCommand('');
      setSelectedSurfaceId('');
      return;
    }
    if (selectedComponentId !== selectedComponent.componentId) {
      setSelectedComponentId(selectedComponent.componentId);
    }
    if (!selectedComponent.commands.some((command) => command.command === selectedCommand)) {
      setSelectedCommand(selectedComponent.commands[0]?.command ?? '');
    }
    if (!selectedComponent.surfaces.some((surface) => surface.id === selectedSurfaceId)) {
      setSelectedSurfaceId(selectedComponent.surfaces[0]?.id ?? '');
    }
  }, [selectedCommand, selectedComponent, selectedComponentId, selectedSurfaceId]);

  useEffect(() => {
    if (triggerKind !== 'event' || declaredEventOptions.includes(eventName)) return;
    setEventName(declaredEventOptions[0] ?? '');
  }, [declaredEventOptions, eventName, triggerKind]);

  const runAction = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await action();
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  };

  const chooseSource = async () => {
    const selected = await open({ directory: true, multiple: false, title: '选择脚本组件开发目录' });
    if (typeof selected === 'string') {
      setSourcePath(selected);
      await inspectSource(selected);
    }
  };

  const inspectSource = async (path = sourcePath) => {
    if (!path.trim()) return;
    await runAction(async () => {
      const report = await validateScriptComponent(path);
      setValidation(report);
      const nextFiles = report.valid ? await listScriptDevelopmentFiles(path) : [];
      setFiles(nextFiles);
      setMessage(report.valid ? '组件合同与入口校验通过。' : null);
      if (nextFiles.length > 0) {
        await loadFile(path, nextFiles[0].path);
      }
    });
  };

  const loadFile = async (path: string, relativePath: string) => {
    const document = await readScriptDevelopmentFile(path, relativePath);
    setActiveDocument(document);
    setEditorContent(document.content);
  };

  const saveFile = async () => {
    if (!activeDocument || !sourcePath) return;
    await runAction(async () => {
      const saved = await saveScriptDevelopmentFile({
        sourcePath,
        relativePath: activeDocument.path,
        content: editorContent,
        expectedContentDigest: activeDocument.contentDigest,
      });
      setActiveDocument(saved);
      setEditorContent(saved.content);
      setValidation(await validateScriptComponent(sourcePath));
      setMessage(`已保存 ${saved.path}`);
    });
  };

  const trustSource = async () => {
    await runAction(async () => {
      await trustScriptDevelopmentDirectory(sourcePath);
      setValidation(await validateScriptComponent(sourcePath));
      await refreshAutomation();
      setMessage('开发目录已信任。Python 仍拥有当前 Windows 用户权限。');
    });
  };

  const untrustSource = async () => {
    await runAction(async () => {
      await untrustScriptDevelopmentDirectory(sourcePath);
      setValidation(await validateScriptComponent(sourcePath));
      await refreshAutomation();
      setMessage('已解除开发目录信任；已安装副本和历史记录不自动删除。');
    });
  };

  const reloadSource = async () => {
    const componentId = validation?.manifest?.id;
    if (!componentId) return;
    await runAction(async () => {
      await reloadScriptComponent(componentId);
      await Promise.all([refreshProfiles(), refreshAutomation()]);
      setMessage(`已从开发目录热重载 ${componentId}`);
    });
  };

  const enableInProfile = async () => {
    const componentId = validation?.manifest?.id;
    const profile = useWorkspaceProfileStore.getState().snapshot?.currentProfile;
    if (!componentId || !profile) return;
    await runAction(async () => {
      const next = cloneProfile(profile);
      const existing = next.enabledComponents ?? [];
      if (!existing.some((item) => item.id === componentId)) {
        next.enabledComponents = [...existing, {
          id: componentId,
          versionRequirement: `^${validation?.manifest?.version ?? '0.1.0'}`,
        }];
      }
      await saveCurrentProfile({ profile: next, expectedRevision: profile.revision ?? 1 });
      await refreshAutomation();
      setMessage('组件已加入当前装配方案的有效组件闭包。');
    });
  };

  const createTemplate = async () => {
    await runAction(async () => {
      const created = await createScriptComponentTemplate({
        parentPath: templateParent,
        componentId: templateId,
        name: templateName,
        includeSurface: templateSurface,
      });
      setSourcePath(created);
      await inspectSource(created);
      setSection('code');
      setMessage(`模板已创建：${created}`);
    });
  };

  const openSourceInVSCode = async () => {
    await runAction(async () => {
      await openScriptDevelopmentDirectoryInVSCode(sourcePath);
      setMessage('已在 VS Code 中打开开发目录。');
    });
  };

  const runSelectedCommand = async () => {
    if (!selectedComponent || !selectedCommand) return;
    await runAction(async () => {
      const run = await startRun({
        componentId: selectedComponent.componentId,
        command: selectedCommand,
        input: parseJson(runInput),
        projectPath,
      });
      setMessage(`运行已进入任务中心：${run.id}`);
    });
  };

  const resetBindingForm = () => {
    setEditingBindingId(null);
    setTriggerKind('manual');
    setEventName('file.changed');
    setCron('0 9 * * 1-5');
    setProjectContext('none');
    setProjectVariable('');
    setBindingInput('{}');
  };

  const editBinding = (binding: ProfileAutomationBinding) => {
    setEditingBindingId(binding.id);
    setSelectedComponentId(binding.componentId);
    setSelectedCommand(binding.command);
    setTriggerKind(binding.trigger.kind);
    if (binding.trigger.kind === 'event') setEventName(binding.trigger.event);
    if (binding.trigger.kind === 'schedule') setCron(binding.trigger.cron);
    setProjectContext(binding.projectContext ?? 'none');
    setProjectVariable(binding.projectVariable ?? '');
    setBindingInput(JSON.stringify(binding.input ?? {}, null, 2));
  };

  const saveBinding = async () => {
    if (!currentProfile || !selectedComponent || !selectedCommand) return;
    const trigger: AutomationTriggerBinding = triggerKind === 'event'
      ? { kind: 'event', event: eventName }
      : triggerKind === 'schedule'
        ? { kind: 'schedule', cron }
        : { kind: 'manual' };
    const binding: ProfileAutomationBinding = {
      id: editingBindingId ?? `automation-${Date.now().toString(36)}`,
      componentId: selectedComponent.componentId,
      command: selectedCommand,
      trigger,
      enabled: true,
      projectContext,
      projectVariable: projectContext === 'profile-variable' ? projectVariable : undefined,
      input: parseJson(bindingInput),
    };
    await runAction(async () => {
      await saveAutomationBinding({
        profileId: currentProfile.id,
        expectedRevision: currentProfile.revision ?? 1,
        binding,
      });
      await Promise.all([refreshProfiles(), refreshAutomation()]);
      const latest = useWorkspaceProfileStore.getState().snapshot?.currentProfile;
      setBindings(latest?.automationBindings ?? []);
      resetBindingForm();
      setMessage('自动化绑定已保存；保存不会立即执行脚本。');
    });
  };

  const deleteBinding = async (bindingId: string) => {
    if (!currentProfile) return;
    await runAction(async () => {
      await removeAutomationBinding({
        profileId: currentProfile.id,
        expectedRevision: currentProfile.revision ?? 1,
        bindingId,
      });
      await refreshProfiles();
      setBindings(useWorkspaceProfileStore.getState().snapshot?.currentProfile.automationBindings ?? []);
      if (editingBindingId === bindingId) resetBindingForm();
      setMessage('绑定已移除，运行历史仍保留。');
    });
  };

  const chooseTemplateParent = async () => {
    const selected = await open({ directory: true, multiple: false, title: '选择组件模板父目录' });
    if (typeof selected === 'string') setTemplateParent(selected);
  };

  const chooseSigningKey = async () => {
    const selected = await open({
      multiple: false,
      title: '选择 Ed25519 签名私钥',
      filters: [{ name: 'Nexora 签名私钥', extensions: ['json'] }],
    });
    if (typeof selected === 'string') setSigningKeyPath(selected);
  };

  const createSigningKey = async () => {
    const selected = await saveDialog({
      title: '创建 Ed25519 签名私钥',
      defaultPath: 'nexora-publisher-key.json',
      filters: [{ name: 'Nexora 签名私钥', extensions: ['json'] }],
    });
    if (typeof selected !== 'string') return;
    await runAction(async () => {
      const result = await generateScriptSigningKey(selected);
      setSigningKeyPath(result.path);
      setMessage(`签名私钥已创建，公钥：${result.publicKey}`);
    });
  };

  const packageSource = async () => {
    const manifest = validation?.manifest;
    if (!manifest || !sourcePath || !signingKeyPath) return;
    const selected = await saveDialog({
      title: '生成 Nexora 组件包',
      defaultPath: `${manifest.id}-${manifest.version}.pmc-pack`,
      filters: [{ name: 'Nexora 组件包', extensions: ['pmc-pack'] }],
    });
    if (typeof selected !== 'string') return;
    await runAction(async () => {
      const result = await packageScriptComponent({
        sourcePath,
        destinationPath: selected,
        keyPath: signingKeyPath,
        publisherId,
        publisherName,
        license: 'NOASSERTION',
      });
      setMessage(`已生成 ${result.componentId} ${result.componentVersion}：${result.destinationPath}`);
    });
  };

  const sourceInstalled = Boolean(
    validation?.manifest?.id
    && profileSnapshot?.components.some((item) => item.id === validation.manifest?.id),
  );
  const sourceEnabled = Boolean(
    validation?.manifest?.id
    && profileSnapshot?.currentProfile.enabledComponents?.some((item) => item.id === validation.manifest?.id),
  );
  const editorDirty = Boolean(activeDocument && editorContent !== activeDocument.content);

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onClose}
      title="脚本开发者工作台"
      size="2xl"
      contentClassName="flex h-[720px] min-h-0 overflow-hidden p-0"
    >
      <aside className="flex w-48 shrink-0 flex-col border-r border-gray-200 bg-gray-50 p-3 dark:border-gray-700 dark:bg-gray-950">
        <div className="mb-3 rounded-md border border-amber-200 bg-amber-50 p-2 text-[11px] leading-4 text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-300">
          Python 组件是受信任代码。Capability 约束宿主接口，但不限制 Python 标准库的 Windows 用户权限。
        </div>
        {([
          ['components', PackagePlus, '组件与运行'],
          ['code', Code2, '代码编辑'],
          ['bindings', Braces, '触发绑定'],
          ['surfaces', FileCode2, '页面预览'],
        ] as const).map(([id, Icon, label]) => (
          <button key={id} type="button" onClick={() => setSection(id)} className={`mb-1 flex items-center gap-2 rounded-md px-3 py-2 text-sm ${section === id ? 'bg-blue-100 text-blue-700 dark:bg-blue-950/50 dark:text-blue-300' : 'text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800'}`}>
            <Icon className="h-4 w-4" />{label}
          </button>
        ))}
        <div className="mt-auto text-[11px] text-gray-400">
          <p>{snapshot?.availableComponents.length ?? 0} 个有效脚本组件</p>
          <p>{snapshot?.trustedDevelopmentDirectories.length ?? 0} 个受信任目录</p>
          <p>旧 Workflow 合同：仅兼容解析</p>
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {(message || error) ? (
          <div className={`mx-4 mt-4 rounded-md border px-3 py-2 text-xs ${error ? 'border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300' : 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/30 dark:text-emerald-300'}`}>{error || message}</div>
        ) : null}

        {section === 'components' ? (
          <div className="min-h-0 flex-1 overflow-auto p-4">
            <div className="grid gap-4 lg:grid-cols-2">
              <section className="rounded-md border border-gray-200 p-4 dark:border-gray-700">
                <div className="flex items-center justify-between">
                  <div><h3 className="text-sm font-semibold">开发目录</h3><p className="text-xs text-gray-500">校验、信任、安装和热重载目录组件。</p></div>
                  <button type="button" onClick={() => void chooseSource()} className="rounded-md border border-gray-300 p-2 text-gray-600 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800" title="选择开发目录"><FolderOpen className="h-4 w-4" /></button>
                </div>
                <div className="mt-3 flex gap-2">
                  <input value={sourcePath} onChange={(event) => setSourcePath(event.target.value)} placeholder="包含 component.json 的目录" className="min-w-0 flex-1 rounded-md border border-gray-300 bg-white px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900" />
                  <button type="button" disabled={busy || !sourcePath} onClick={() => void inspectSource()} className="rounded-md border border-gray-300 px-3 text-xs disabled:opacity-50 dark:border-gray-700"><RefreshCw className={`h-4 w-4 ${busy ? 'animate-spin' : ''}`} /></button>
                </div>
                {validation ? (
                  <div className="mt-3 space-y-2 text-xs">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className={`inline-flex items-center gap-1 rounded px-2 py-1 ${validation.valid ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300' : 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-300'}`}>{validation.valid ? <CheckCircle2 className="h-3.5 w-3.5" /> : <AlertTriangle className="h-3.5 w-3.5" />}{validation.valid ? '合同有效' : '合同无效'}</span>
                      <span className="rounded bg-gray-100 px-2 py-1 text-gray-600 dark:bg-gray-800 dark:text-gray-300">{validation.trusted ? '目录已信任' : '目录未信任'}</span>
                      {sourceInstalled ? <span className="rounded bg-blue-100 px-2 py-1 text-blue-700 dark:bg-blue-950/50 dark:text-blue-300">已安装</span> : null}
                      {sourceEnabled ? <span className="rounded bg-violet-100 px-2 py-1 text-violet-700 dark:bg-violet-950/50 dark:text-violet-300">当前装配已启用</span> : null}
                    </div>
                    <p className="font-medium text-gray-800 dark:text-gray-200">{validation.manifest?.name ?? '未读取清单'} <span className="font-mono text-gray-400">{validation.manifest?.id}</span></p>
                    {validation.errors.map((item) => <p key={item} className="text-red-600">{item}</p>)}
                    {validation.warnings.map((item) => <p key={item} className="text-amber-600 dark:text-amber-400">{item}</p>)}
                    <div className="flex flex-wrap gap-2 pt-1">
                      {!validation.trusted ? <button type="button" disabled={!validation.valid || busy} onClick={() => void trustSource()} className="inline-flex items-center gap-1 rounded-md bg-blue-600 px-2.5 py-1.5 text-white disabled:opacity-50"><ShieldCheck className="h-3.5 w-3.5" />信任目录</button> : <button type="button" disabled={busy} onClick={() => void untrustSource()} className="inline-flex items-center gap-1 rounded-md border border-gray-300 px-2.5 py-1.5 dark:border-gray-700"><ShieldOff className="h-3.5 w-3.5" />解除信任</button>}
                      <button type="button" disabled={!validation.valid || !validation.trusted || busy} onClick={() => void reloadSource()} className="rounded-md border border-gray-300 px-2.5 py-1.5 disabled:opacity-50 dark:border-gray-700">{sourceInstalled ? '热重载安装副本' : '安装开发组件'}</button>
                      <button type="button" disabled={!sourceInstalled || sourceEnabled || busy} onClick={() => void enableInProfile()} className="rounded-md border border-gray-300 px-2.5 py-1.5 disabled:opacity-50 dark:border-gray-700">加入当前装配</button>
                      <button type="button" disabled={busy || !sourcePath} onClick={() => void openSourceInVSCode()} className="inline-flex items-center gap-1 rounded-md border border-gray-300 px-2.5 py-1.5 disabled:opacity-50 dark:border-gray-700" title="用 Visual Studio Code 打开组件开发目录"><Code2 className="h-3.5 w-3.5" />VS Code</button>
                      <button type="button" onClick={() => void invoke('show_in_folder', { path: sourcePath })} className="rounded-md border border-gray-300 px-2.5 py-1.5 dark:border-gray-700">打开目录</button>
                    </div>
                  </div>
                ) : <p className="mt-3 text-xs text-gray-400">选择开发目录后显示清单、摘要、依赖和信任状态。</p>}
              </section>

              <section className="rounded-md border border-gray-200 p-4 dark:border-gray-700">
                <h3 className="text-sm font-semibold">创建 Python 组件模板</h3>
                <p className="mt-1 text-xs text-gray-500">生成正式组件目录、SDK 示例和可选隔离页面。</p>
                <div className="mt-3 space-y-2">
                  <div className="flex gap-2"><input value={templateParent} onChange={(event) => setTemplateParent(event.target.value)} placeholder="模板父目录" className="min-w-0 flex-1 rounded-md border border-gray-300 px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900" /><button type="button" onClick={() => void chooseTemplateParent()} className="rounded-md border border-gray-300 p-2 dark:border-gray-700"><FolderOpen className="h-4 w-4" /></button></div>
                  <input value={templateId} onChange={(event) => setTemplateId(event.target.value)} placeholder="组件 ID，如 studio.example" className="w-full rounded-md border border-gray-300 px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900" />
                  <input value={templateName} onChange={(event) => setTemplateName(event.target.value)} placeholder="组件名称" className="w-full rounded-md border border-gray-300 px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900" />
                  <label className="flex items-center gap-2 text-xs text-gray-600 dark:text-gray-300"><input type="checkbox" checked={templateSurface} onChange={(event) => setTemplateSurface(event.target.checked)} />包含 HTML/CSS/JavaScript 隔离页面</label>
                  <button type="button" disabled={busy || !templateParent || !templateId || !templateName} onClick={() => void createTemplate()} className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-2 text-xs text-white disabled:opacity-50"><PackagePlus className="h-4 w-4" />创建模板</button>
                </div>
              </section>
            </div>

            <section className="mt-4 rounded-md border border-gray-200 p-4 dark:border-gray-700">
              <div className="flex items-start justify-between gap-4">
                <div><h3 className="text-sm font-semibold">签名与打包</h3><p className="mt-1 text-xs text-gray-500">使用 Nexora 内置 Ed25519 链路生成可安装的 .pmc-pack；私钥不会写入组件目录。</p></div>
                <button type="button" onClick={() => void createSigningKey()} className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-gray-300 px-2.5 py-1.5 text-xs dark:border-gray-700"><KeyRound className="h-3.5 w-3.5" />新建私钥</button>
              </div>
              <div className="mt-3 grid gap-2 md:grid-cols-2">
                <label className="text-xs text-gray-500">发布者 ID<input value={publisherId} onChange={(event) => setPublisherId(event.target.value)} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900" /></label>
                <label className="text-xs text-gray-500">发布者名称<input value={publisherName} onChange={(event) => setPublisherName(event.target.value)} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900" /></label>
                <label className="text-xs text-gray-500 md:col-span-2">签名私钥<div className="mt-1 flex gap-2"><input value={signingKeyPath} onChange={(event) => setSigningKeyPath(event.target.value)} placeholder="仓库外的 Ed25519 私钥 JSON" className="min-w-0 flex-1 rounded-md border border-gray-300 px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900" /><button type="button" onClick={() => void chooseSigningKey()} className="rounded-md border border-gray-300 p-2 dark:border-gray-700"><FolderOpen className="h-4 w-4" /></button></div></label>
              </div>
              <button type="button" disabled={busy || !validation?.valid || !validation.trusted || !signingKeyPath || !publisherId.trim() || !publisherName.trim()} onClick={() => void packageSource()} className="mt-3 inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-2 text-xs text-white disabled:opacity-50"><PackagePlus className="h-4 w-4" />生成签名组件包</button>
            </section>

            <section className="mt-4 rounded-md border border-gray-200 p-4 dark:border-gray-700">
              <div className="flex flex-wrap items-end gap-3">
                <label className="min-w-[220px] flex-1 text-xs text-gray-500">有效脚本组件<select value={selectedComponent?.componentId ?? ''} onChange={(event) => setSelectedComponentId(event.target.value)} className="mt-1 w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-100">{components.map((component) => <option key={component.componentId} value={component.componentId}>{component.componentName} · {component.componentVersion}</option>)}</select></label>
                <label className="min-w-[180px] flex-1 text-xs text-gray-500">命令<select value={selectedCommand} onChange={(event) => setSelectedCommand(event.target.value)} className="mt-1 w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-100">{selectedComponent?.commands.map((command) => <option key={command.id} value={command.command}>{command.name} · {command.command}</option>)}</select></label>
                <button type="button" disabled={!selectedCommand || busy || !snapshot?.running} onClick={() => void runSelectedCommand()} className="inline-flex h-9 items-center gap-1.5 rounded-md bg-emerald-600 px-3 text-xs text-white disabled:opacity-50"><Play className="h-4 w-4" />调试运行</button>
              </div>
              <textarea value={runInput} onChange={(event) => setRunInput(event.target.value)} className="mt-3 h-28 w-full resize-none rounded-md border border-gray-300 bg-gray-950 p-3 font-mono text-xs text-gray-100 dark:border-gray-700" spellCheck={false} />
            </section>
          </div>
        ) : null}

        {section === 'code' ? (
          <div className="flex min-h-0 flex-1 overflow-hidden p-4">
            <aside className="w-56 shrink-0 overflow-auto rounded-l-md border border-r-0 border-gray-200 dark:border-gray-700">
              <div className="border-b border-gray-200 p-2 text-xs font-medium text-gray-500 dark:border-gray-700">{sourcePath || '未选择开发目录'}</div>
              {files.map((file) => <button key={file.path} type="button" onClick={() => void runAction(() => loadFile(sourcePath, file.path))} className={`block w-full border-b border-gray-100 px-3 py-2 text-left text-xs dark:border-gray-800 ${activeDocument?.path === file.path ? 'bg-blue-50 text-blue-700 dark:bg-blue-950/30 dark:text-blue-300' : 'hover:bg-gray-50 dark:hover:bg-gray-800'}`}><span className="block truncate">{file.path}</span><span className="text-[10px] text-gray-400">{Math.ceil(file.sizeBytes / 1024)} KiB</span></button>)}
            </aside>
            <section className="flex min-w-0 flex-1 flex-col rounded-r-md border border-gray-200 dark:border-gray-700">
              <div className="flex items-center justify-between gap-3 border-b border-gray-200 px-3 py-2 dark:border-gray-700"><span className="truncate text-xs font-medium">{activeDocument?.path ?? '选择文件'}</span><div className="flex shrink-0 items-center gap-2"><button type="button" disabled={busy || !sourcePath} onClick={() => void openSourceInVSCode()} className="inline-flex items-center gap-1.5 rounded-md border border-gray-300 px-2.5 py-1.5 text-xs disabled:opacity-40 dark:border-gray-700" title="建议使用 VS Code 进行完整编辑"><Code2 className="h-3.5 w-3.5" />VS Code</button><button type="button" disabled={!editorDirty || busy} onClick={() => void saveFile()} className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-2.5 py-1.5 text-xs text-white disabled:opacity-40"><Save className="h-3.5 w-3.5" />保存</button></div></div>
              {activeDocument ? <textarea value={editorContent} onChange={(event) => setEditorContent(event.target.value)} className="min-h-0 flex-1 resize-none bg-gray-950 p-4 font-mono text-xs leading-5 text-gray-100 outline-none" spellCheck={false} /> : <div className="flex flex-1 items-center justify-center text-sm text-gray-400">先在“组件与运行”中选择有效开发目录</div>}
            </section>
          </div>
        ) : null}

        {section === 'bindings' ? (
          <div className="grid min-h-0 flex-1 grid-cols-[320px_minmax(0,1fr)] overflow-hidden p-4">
            <aside className="overflow-auto rounded-l-md border border-r-0 border-gray-200 dark:border-gray-700">
              <div className="border-b border-gray-200 p-3 dark:border-gray-700"><p className="text-sm font-medium">当前 Profile 绑定</p><p className="text-xs text-gray-500">{currentProfile?.name ?? '未加载 Profile'} · 保存不自动执行</p></div>
              {bindings.length ? bindings.map((binding) => <div key={binding.id} className={`border-b border-gray-100 p-3 dark:border-gray-800 ${editingBindingId === binding.id ? 'bg-blue-50 dark:bg-blue-950/20' : ''}`}><button type="button" onClick={() => editBinding(binding)} className="block w-full text-left"><p className="truncate text-sm font-medium">{binding.componentId}</p><p className="mt-0.5 truncate text-xs text-gray-500">{binding.command} · {binding.trigger.kind}</p></button><button type="button" onClick={() => void deleteBinding(binding.id)} className="mt-2 inline-flex items-center gap-1 text-[11px] text-red-500"><Trash2 className="h-3 w-3" />移除</button></div>) : <p className="p-4 text-sm text-gray-400">暂无绑定</p>}
            </aside>
            <section className="overflow-auto rounded-r-md border border-gray-200 p-4 dark:border-gray-700">
              <div className="grid gap-3 md:grid-cols-2">
                <label className="text-xs text-gray-500">组件<select value={selectedComponent?.componentId ?? ''} onChange={(event) => setSelectedComponentId(event.target.value)} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900">{components.map((component) => <option key={component.componentId} value={component.componentId}>{component.componentName}</option>)}</select></label>
                <label className="text-xs text-gray-500">命令<select value={selectedCommand} onChange={(event) => setSelectedCommand(event.target.value)} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900">{selectedComponent?.commands.map((command) => <option key={command.id} value={command.command}>{command.name}</option>)}</select></label>
                <label className="text-xs text-gray-500">触发方式<select value={triggerKind} onChange={(event) => setTriggerKind(event.target.value as TriggerKind)} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900"><option value="manual">手动</option><option value="event">应用事件</option><option value="schedule">五段式 cron</option></select></label>
                {triggerKind === 'event' ? <label className="text-xs text-gray-500">事件<select value={eventName} onChange={(event) => setEventName(event.target.value)} disabled={!declaredEventOptions.length} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm disabled:opacity-50 dark:border-gray-700 dark:bg-gray-900">{declaredEventOptions.map((event) => <option key={event}>{event}</option>)}</select>{!declaredEventOptions.length ? <span className="mt-1 block text-[11px] text-amber-600 dark:text-amber-400">该组件没有声明可绑定事件。</span> : null}</label> : null}
                {triggerKind === 'schedule' ? <label className="text-xs text-gray-500">Cron<input value={cron} onChange={(event) => setCron(event.target.value)} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 font-mono text-sm dark:border-gray-700 dark:bg-gray-900" /></label> : null}
                <label className="text-xs text-gray-500">项目上下文<select value={projectContext} onChange={(event) => setProjectContext(event.target.value as AutomationProjectContext)} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900"><option value="none">全局，无项目</option><option value="active-project">手动执行时的活动项目</option><option value="event-project">事件携带项目</option><option value="each-open-project">每个已打开项目</option><option value="profile-variable">Profile 路径变量</option></select></label>
                {projectContext === 'profile-variable' ? <label className="text-xs text-gray-500">变量名<input value={projectVariable} onChange={(event) => setProjectVariable(event.target.value)} className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900" /></label> : null}
              </div>
              <label className="mt-3 block text-xs text-gray-500">输入映射 JSON<textarea value={bindingInput} onChange={(event) => setBindingInput(event.target.value)} className="mt-1 h-48 w-full resize-none rounded-md border border-gray-300 bg-gray-950 p-3 font-mono text-xs text-gray-100 dark:border-gray-700" spellCheck={false} /></label>
              <div className="mt-3 flex gap-2"><button type="button" disabled={busy || !selectedCommand || (triggerKind === 'event' && !eventName)} onClick={() => void saveBinding()} className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-2 text-xs text-white disabled:opacity-50"><Save className="h-4 w-4" />{editingBindingId ? '更新绑定' : '保存绑定'}</button>{editingBindingId ? <button type="button" onClick={resetBindingForm} className="rounded-md border border-gray-300 px-3 py-2 text-xs dark:border-gray-700">新建绑定</button> : null}</div>
            </section>
          </div>
        ) : null}

        {section === 'surfaces' ? (
          <div className="flex min-h-0 flex-1 flex-col p-4">
            <div className="mb-3 flex gap-3"><select value={selectedComponent?.componentId ?? ''} onChange={(event) => setSelectedComponentId(event.target.value)} className="min-w-0 flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900">{components.filter((component) => component.surfaces.length).map((component) => <option key={component.componentId} value={component.componentId}>{component.componentName}</option>)}</select><select value={selectedSurface?.id ?? ''} onChange={(event) => setSelectedSurfaceId(event.target.value)} className="min-w-0 flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900">{selectedComponent?.surfaces.map((surface) => <option key={surface.id} value={surface.id}>{surface.name}</option>)}</select></div>
            <div className="min-h-0 flex-1 overflow-hidden rounded-md border border-gray-200 bg-white dark:border-gray-700">{selectedComponent && selectedSurface ? <ScriptSurfaceFrame componentId={selectedComponent.componentId} surfaceId={selectedSurface.id} projectPath={projectPath} /> : <div className="flex h-full items-center justify-center text-sm text-gray-400">当前有效组件没有 scriptSurfaces</div>}</div>
          </div>
        ) : null}

        {busy ? <div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-white/40 dark:bg-gray-900/40"><Loader2 className="h-6 w-6 animate-spin text-blue-600" /></div> : null}
      </main>
    </Dialog>
  );
}
