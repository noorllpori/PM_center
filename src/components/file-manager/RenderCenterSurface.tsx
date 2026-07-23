import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  AlertCircle,
  Archive,
  Check,
  ChevronRight,
  CirclePause,
  CirclePlay,
  Clock3,
  FolderOpen,
  Gauge,
  Layers3,
  ListRestart,
  LoaderCircle,
  Pause,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  Square,
  Trash2,
  X,
} from 'lucide-react';
import { useProjectStoreShallow } from '../../stores/projectStore';
import { useRenderStore, initRenderEventListeners } from '../../stores/renderStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { HelpAssistant } from '../ui/HelpAssistant';
import { ProjectFilePickerDialog, type ProjectFilePickerTarget } from './ProjectFilePickerDialog';
import type {
  CreateRenderBatchRequest,
  RenderFrame,
  RenderJob,
  RenderJobDetail,
  RenderPreset,
  RenderSceneInfo,
  RenderSourceInfo,
} from '../../types/render';

type CenterView = 'queue' | 'results' | 'presets';

const EMPTY_RENDER_JOBS: RenderJob[] = [];
const EMPTY_SOURCE_PATHS: string[] = [];

const STATUS_LABELS: Record<string, string> = {
  pending: '等待中', starting: '正在启动', running: '渲染中', pausing: '正在暂停', paused: '已暂停',
  cancelling: '正在取消', cancelled: '已取消', completed: '已完成', failed: '失败',
  skipped: '已跳过',
};

const STATUS_TONES: Record<string, string> = {
  pending: 'text-amber-600 bg-amber-50 dark:bg-amber-950/30',
  starting: 'text-blue-600 bg-blue-50 dark:bg-blue-950/30',
  running: 'text-blue-600 bg-blue-50 dark:bg-blue-950/30',
  pausing: 'text-orange-600 bg-orange-50 dark:bg-orange-950/30',
  paused: 'text-gray-600 bg-gray-100 dark:text-gray-300 dark:bg-gray-800',
  cancelling: 'text-orange-600 bg-orange-50 dark:bg-orange-950/30',
  cancelled: 'text-gray-600 bg-gray-100 dark:text-gray-300 dark:bg-gray-800',
  completed: 'text-emerald-600 bg-emerald-50 dark:bg-emerald-950/30',
  failed: 'text-red-600 bg-red-50 dark:bg-red-950/30',
};

function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function formatBatchTimestamp(date: Date) {
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function formatDuration(ms: number | null) {
  if (!ms) return '-';
  if (ms < 1000) return `${ms} ms`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds} 秒`;
  return `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
}

function StatusBadge({ status }: { status: string }) {
  return (
    <span className={`inline-flex h-6 items-center rounded px-2 text-xs font-medium ${STATUS_TONES[status] || STATUS_TONES.paused}`}>
      {status === 'running' && <LoaderCircle className="mr-1 h-3 w-3 animate-spin" />}
      {STATUS_LABELS[status] || status}
    </span>
  );
}

export function RenderCenterSurface({ isActive }: { isActive: boolean }) {
  const { projectPath, projectName } = useProjectStoreShallow((state) => ({
    projectPath: state.projectPath,
    projectName: state.projectName,
  }));
  const showToast = useUiStore((state) => state.showToast);
  const jobs = useRenderStore((state) => projectPath ? state.jobsByProject[projectPath] || EMPTY_RENDER_JOBS : EMPTY_RENDER_JOBS);
  const isLoading = useRenderStore((state) => projectPath ? state.loadingProjects[projectPath] : false);
  const refreshProject = useRenderStore((state) => state.refreshProject);
  const pendingSources = useRenderStore((state) => projectPath ? state.pendingSourcesByProject[projectPath] || EMPTY_SOURCE_PATHS : EMPTY_SOURCE_PATHS);
  const consumePendingSources = useRenderStore((state) => state.consumePendingSources);
  const [view, setView] = useState<CenterView>('queue');
  const [selectedJobId, setSelectedJobId] = useState<string | null>(null);
  const [detail, setDetail] = useState<RenderJobDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const [initialSources, setInitialSources] = useState<string[]>([]);
  const [presets, setPresets] = useState<RenderPreset[]>([]);
  const [concurrency, setConcurrency] = useState(1);
  const [busyAction, setBusyAction] = useState<string | null>(null);

  const visibleJobs = useMemo(() => jobs.filter((job) =>
    view === 'results' ? ['completed', 'failed', 'cancelled'].includes(job.status) : !job.archived,
  ), [jobs, view]);
  const activeCount = jobs.filter((job) => ['pending', 'starting', 'running', 'pausing', 'cancelling'].includes(job.status)).length;
  const completedCount = jobs.filter((job) => job.status === 'completed').length;
  const failedCount = jobs.filter((job) => job.status === 'failed').length;

  const refresh = useCallback(async () => {
    if (!projectPath) return;
    await refreshProject(projectPath, view === 'results');
    const [settings, nextPresets] = await Promise.all([
      invoke<{ concurrency: number }>('get_render_scheduler_settings', { projectPath }),
      invoke<RenderPreset[]>('list_render_presets', { projectPath }),
    ]);
    setConcurrency(settings.concurrency);
    setPresets(nextPresets);
  }, [projectPath, refreshProject, view]);

  const loadDetail = useCallback(async (jobId: string) => {
    if (!projectPath) return;
    setDetailLoading(true);
    try {
      setDetail(await invoke<RenderJobDetail>('get_render_job', { projectPath, jobId }));
    } finally {
      setDetailLoading(false);
    }
  }, [projectPath]);

  useEffect(() => { void initRenderEventListeners(); }, []);
  useEffect(() => { if (projectPath && isActive) void refresh(); }, [isActive, projectPath, refresh]);
  useEffect(() => {
    if (!projectPath || !isActive || pendingSources.length === 0) return;
    setInitialSources(consumePendingSources(projectPath));
    setShowCreate(true);
  }, [consumePendingSources, isActive, pendingSources, projectPath]);
  useEffect(() => {
    if (!selectedJobId || !jobs.some((job) => job.id === selectedJobId)) {
      const first = visibleJobs[0]?.id || null;
      setSelectedJobId(first);
      setDetail(null);
      if (first) void loadDetail(first);
    }
  }, [jobs, loadDetail, selectedJobId, visibleJobs]);
  useEffect(() => {
    if (!selectedJobId) return;
    const unlisten = listen<{ jobId: string }>('pm-center:render-job-progress', ({ payload }) => {
      if (payload?.jobId === selectedJobId) void loadDetail(selectedJobId);
    });
    return () => { void unlisten.then((dispose) => dispose()); };
  }, [loadDetail, selectedJobId]);

  const runAction = async (label: string, command: string, payload: Record<string, unknown>) => {
    if (!projectPath) return;
    setBusyAction(label);
    try {
      await invoke(command, { projectPath, ...payload });
      await refresh();
      if (selectedJobId) await loadDetail(selectedJobId);
    } catch (error) {
      showToast({ title: `${label}失败`, message: String(error), tone: 'error' });
    } finally {
      setBusyAction(null);
    }
  };

  const selectJob = (job: RenderJob) => {
    setSelectedJobId(job.id);
    void loadDetail(job.id);
  };

  if (!projectPath) {
    return <div className="flex h-full items-center justify-center text-sm text-gray-500">项目已关闭</div>;
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-white text-gray-900 dark:bg-gray-950 dark:text-gray-100">
      <header className="flex min-h-[64px] flex-wrap items-center gap-3 border-b border-gray-200 px-4 py-3 dark:border-gray-800">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Layers3 className="h-5 w-5 text-orange-500" />
            <h2 className="truncate text-base font-semibold">渲染与批处理</h2>
            <span className="truncate text-xs text-gray-500">{projectName}</span>
          </div>
          <p className="mt-0.5 truncate text-xs text-gray-500">本机队列 · {activeCount} 个活动作业 · 并发 {concurrency}</p>
        </div>
        <div className="flex items-center gap-1.5">
          <label className="flex h-8 items-center gap-2 rounded border border-gray-200 px-2 text-xs dark:border-gray-700" title="本机 Blender 并发数">
            <Gauge className="h-3.5 w-3.5 text-gray-500" />
            <span>并发</span>
            <select
              value={concurrency}
              onChange={async (event) => {
                const value = Number(event.target.value);
                setConcurrency(value);
                await invoke('set_render_scheduler_settings', { projectPath, settings: { concurrency: value } });
              }}
              className="bg-transparent text-xs outline-none"
            >
              {[1,2,3,4,5,6,7,8].map((value) => <option key={value} value={value}>{value}</option>)}
            </select>
          </label>
          <button className="icon-button h-8 w-8 p-0" onClick={() => void refresh()} title="刷新">
            <RefreshCw className={`mx-auto h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </button>
          <button
            className="flex h-8 items-center gap-1.5 rounded bg-gray-900 px-3 text-xs font-medium text-white hover:bg-black dark:bg-white dark:text-gray-900"
            onClick={() => setShowCreate(true)}
          >
            <Plus className="h-4 w-4" /> 新建批次
          </button>
        </div>
      </header>

      <div className="grid min-h-0 flex-1 grid-cols-[156px_minmax(320px,0.8fr)_minmax(360px,1.2fr)] max-[900px]:grid-cols-[132px_minmax(260px,1fr)]">
        <nav className="border-r border-gray-200 p-2 dark:border-gray-800">
          <NavButton icon={<Clock3 />} label="队列" count={activeCount} active={view === 'queue'} onClick={() => setView('queue')} />
          <NavButton icon={<Check />} label="结果" count={completedCount + failedCount} active={view === 'results'} onClick={() => setView('results')} />
          <NavButton icon={<Settings2 />} label="预设" count={presets.length} active={view === 'presets'} onClick={() => setView('presets')} />
          <div className="mt-4 border-t border-gray-200 pt-3 dark:border-gray-800">
            <button className="flex w-full items-center gap-2 rounded px-2 py-2 text-left text-xs text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-900" onClick={() => void runAction('暂停队列', 'pause_render_queue', {})}>
              <Pause className="h-4 w-4" /> 暂停队列
            </button>
            <button className="flex w-full items-center gap-2 rounded px-2 py-2 text-left text-xs text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-900" onClick={() => void runAction('继续队列', 'resume_render_queue', {})}>
              <Play className="h-4 w-4" /> 继续队列
            </button>
          </div>
        </nav>

        <main className="min-h-0 overflow-auto border-r border-gray-200 dark:border-gray-800">
          {view === 'presets' ? (
            <PresetList presets={presets} projectPath={projectPath} onChanged={refresh} />
          ) : visibleJobs.length === 0 ? (
            <div className="flex h-full min-h-[280px] flex-col items-center justify-center px-6 text-center text-gray-500">
              <Layers3 className="mb-3 h-9 w-9 text-gray-300" />
              <p className="text-sm font-medium text-gray-700 dark:text-gray-300">{view === 'queue' ? '队列为空' : '还没有渲染结果'}</p>
              {view === 'queue' && <button className="mt-3 text-xs text-blue-600 hover:underline" onClick={() => setShowCreate(true)}>创建第一个渲染批次</button>}
            </div>
          ) : (
            visibleJobs.map((job) => (
              <JobRow key={job.id} job={job} selected={job.id === selectedJobId} onClick={() => selectJob(job)} />
            ))
          )}
        </main>

        <aside className="min-h-0 overflow-hidden max-[900px]:col-span-2 max-[900px]:hidden">
          {view === 'presets' ? (
            <div className="flex h-full items-center justify-center px-8 text-center text-sm text-gray-500">预设可在新建批次时套用；全局预设对所有项目可见。</div>
          ) : detailLoading && !detail ? (
            <div className="flex h-full items-center justify-center"><LoaderCircle className="h-5 w-5 animate-spin text-gray-400" /></div>
          ) : detail ? (
            <JobDetailPane
              detail={detail}
              busy={Boolean(busyAction)}
              onAction={(label, command, payload = {}) => runAction(label, command, { jobId: detail.job.id, ...payload })}
            />
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-gray-500">选择一个作业查看帧和日志</div>
          )}
        </aside>
      </div>

      {showCreate && (
        <CreateBatchDialog
          projectPath={projectPath}
          presets={presets}
          initialSources={initialSources}
          onClose={() => setShowCreate(false)}
          onCreated={async () => { setShowCreate(false); setView('queue'); await refresh(); }}
        />
      )}
    </div>
  );
}

function NavButton({ icon, label, count, active, onClick }: { icon: React.ReactElement; label: string; count: number; active: boolean; onClick: () => void }) {
  return (
    <button onClick={onClick} className={`mb-1 flex h-9 w-full items-center gap-2 rounded px-2 text-xs ${active ? 'bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900' : 'text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-900'}`}>
      <span className="[&>svg]:h-4 [&>svg]:w-4">{icon}</span><span className="flex-1 text-left">{label}</span><span className="tabular-nums opacity-70">{count}</span>
    </button>
  );
}

function JobRow({ job, selected, onClick }: { job: RenderJob; selected: boolean; onClick: () => void }) {
  return (
    <button onClick={onClick} className={`block w-full border-b border-gray-100 px-4 py-3 text-left transition-colors dark:border-gray-800 ${selected ? 'bg-blue-50 dark:bg-blue-950/20' : 'hover:bg-gray-50 dark:hover:bg-gray-900/60'}`}>
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2"><p className="truncate text-sm font-medium">{job.name}</p><StatusBadge status={job.status} /></div>
          <p className="mt-1 truncate text-xs text-gray-500">{fileName(job.blendPath)} · {job.frameStart}-{job.frameEnd}</p>
        </div>
        <ChevronRight className="mt-1 h-4 w-4 text-gray-400" />
      </div>
      <div className="mt-3 h-1.5 overflow-hidden rounded bg-gray-200 dark:bg-gray-800"><div className={`h-full ${job.status === 'failed' ? 'bg-red-500' : 'bg-blue-500'}`} style={{ width: `${Math.min(100, job.progress)}%` }} /></div>
      <div className="mt-1.5 flex justify-between text-[11px] text-gray-500"><span>{job.completedFrames} 完成 · {job.failedFrames} 失败 · {job.skippedFrames} 跳过</span><span>{Math.round(job.progress)}%</span></div>
    </button>
  );
}

function JobDetailPane({ detail, busy, onAction }: { detail: RenderJobDetail; busy: boolean; onAction: (label: string, command: string, payload?: Record<string, unknown>) => Promise<void> }) {
  const { job, frames, logTail } = detail;
  const canPause = ['pending', 'starting', 'running'].includes(job.status);
  const canResume = ['paused', 'failed', 'cancelled'].includes(job.status);
  const failedFrames = frames.filter((frame) => frame.status === 'failed').map((frame) => frame.frame);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-gray-200 px-4 py-3 dark:border-gray-800">
        <div className="flex items-start justify-between gap-3"><div className="min-w-0"><h3 className="truncate text-sm font-semibold">{job.name}</h3><p className="mt-1 truncate text-xs text-gray-500">{job.outputDir}</p></div><StatusBadge status={job.status} /></div>
        <div className="mt-3 flex flex-wrap gap-1.5">
          {canPause && <IconAction title="暂停" icon={<CirclePause />} disabled={busy} onClick={() => onAction('暂停作业', 'pause_render_job')} />}
          {canResume && <IconAction title="继续" icon={<CirclePlay />} disabled={busy} onClick={() => onAction('继续作业', 'resume_render_job')} />}
          {!['completed','cancelled'].includes(job.status) && <IconAction title="取消" icon={<Square />} disabled={busy} onClick={() => onAction('取消作业', 'cancel_render_job')} />}
          {failedFrames.length > 0 && <IconAction title="重试失败帧" icon={<RotateCcw />} disabled={busy} onClick={() => onAction('重试失败帧', 'retry_render_frames', { frames: failedFrames })} />}
          <IconAction title="打开输出目录" icon={<FolderOpen />} disabled={busy} onClick={() => onAction('打开输出目录', 'open_render_output', { path: job.outputDir })} />
          {!['running','pausing','cancelling'].includes(job.status) && <IconAction title={job.archived ? '取消归档' : '归档'} icon={<Archive />} disabled={busy} onClick={() => onAction('归档作业', 'archive_render_job', { archived: !job.archived })} />}
        </div>
        {job.error && <p className="mt-2 flex items-start gap-1.5 text-xs text-red-600"><AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />{job.error}</p>}
      </div>
      <div className="grid grid-cols-4 border-b border-gray-200 dark:border-gray-800">
        {[['总帧', job.totalFrames], ['完成', job.completedFrames], ['失败', job.failedFrames], ['当前', job.currentFrame ?? '-']].map(([label,value]) => <div key={label} className="border-r border-gray-100 px-3 py-2 last:border-r-0 dark:border-gray-800"><div className="text-[10px] text-gray-500">{label}</div><div className="mt-0.5 text-sm font-semibold tabular-nums">{value}</div></div>)}
      </div>
      <div className="grid min-h-0 flex-1 grid-rows-[minmax(150px,0.9fr)_minmax(160px,1.1fr)]">
        <div className="min-h-0 overflow-auto border-b border-gray-200 dark:border-gray-800">
          <div className="sticky top-0 grid grid-cols-[64px_82px_64px_1fr] bg-gray-50 px-3 py-1.5 text-[10px] font-medium text-gray-500 dark:bg-gray-900"><span>帧</span><span>状态</span><span>耗时</span><span>输出</span></div>
          {frames.map((frame) => <FrameRow key={frame.frame} frame={frame} />)}
        </div>
        <div className="min-h-0 overflow-auto bg-gray-950 p-3 font-mono text-[11px] leading-5 text-gray-300">
          {logTail.length ? logTail.map((line,index) => <div key={`${index}-${line}`} className="break-all">{line}</div>) : <span className="text-gray-600">尚无日志</span>}
        </div>
      </div>
    </div>
  );
}

function FrameRow({ frame }: { frame: RenderFrame }) {
  return <div className="grid grid-cols-[64px_82px_64px_1fr] border-t border-gray-100 px-3 py-1.5 text-[11px] dark:border-gray-900"><span className="tabular-nums">{frame.frame}</span><span className={frame.status === 'failed' ? 'text-red-600' : frame.status === 'completed' ? 'text-emerald-600' : 'text-gray-500'}>{STATUS_LABELS[frame.status] || frame.status}</span><span className="text-gray-500">{formatDuration(frame.durationMs)}</span><span className="truncate text-gray-500" title={frame.outputPath}>{fileName(frame.outputPath)}</span></div>;
}

function IconAction({ title, icon, disabled, onClick }: { title: string; icon: React.ReactElement; disabled: boolean; onClick: () => void }) {
  return <button title={title} disabled={disabled} onClick={onClick} className="flex h-8 w-8 items-center justify-center rounded border border-gray-200 text-gray-600 hover:bg-gray-100 disabled:opacity-40 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800 [&>svg]:h-4 [&>svg]:w-4">{icon}</button>;
}

function PresetList({ presets, projectPath, onChanged }: { presets: RenderPreset[]; projectPath: string; onChanged: () => Promise<void> }) {
  return <div>{presets.length === 0 ? <div className="p-6 text-center text-sm text-gray-500">暂无预设，可在新建批次中保存当前设置。</div> : presets.map((preset) => <div key={preset.id} className="flex items-center gap-3 border-b border-gray-100 px-4 py-3 dark:border-gray-800"><Save className="h-4 w-4 text-gray-400" /><div className="min-w-0 flex-1"><p className="truncate text-sm font-medium">{preset.name}</p><p className="text-xs text-gray-500">{preset.scope === 'global' ? '全局预设' : '项目预设'}</p></div><button title="删除预设" className="h-8 w-8 p-0 text-gray-400 hover:text-red-600" onClick={async () => { await invoke('delete_render_preset', { projectPath, id: preset.id, scope: preset.scope }); await onChanged(); }}><Trash2 className="mx-auto h-4 w-4" /></button></div>)}</div>;
}

interface EditableJob {
  path: string;
  scenes: RenderSceneInfo[];
  sceneName: string;
  frameStart: number;
  frameEnd: number;
  frameStep: number;
  resolutionPercentage: number;
  engine: string;
  outputFormat: string;
  error: string | null;
}

type RenderFilePickerTarget = 'blend' | 'outputRoot' | 'preHook' | 'postHook';

function CreateBatchDialog({ projectPath, presets, initialSources, onClose, onCreated }: { projectPath: string; presets: RenderPreset[]; initialSources: string[]; onClose: () => void; onCreated: () => Promise<void> }) {
  const blenderDefault = useSettingsStore((state) => state.toolPaths.blender) || '';
  const blenderInstallations = useSettingsStore((state) => state.blenderInstallations);
  const availableBlenders = useMemo(
    () => blenderInstallations.filter((installation) => installation.status === 'ready'),
    [blenderInstallations],
  );
  const showToast = useUiStore((state) => state.showToast);
  const [namePrefix, setNamePrefix] = useState('渲染批次');
  const [batchTimestamp] = useState(() => formatBatchTimestamp(new Date()));
  const [blenderPath, setBlenderPath] = useState(() => (
    availableBlenders.some((installation) => installation.path === blenderDefault)
      ? blenderDefault
      : availableBlenders[0]?.path || ''
  ));
  const [outputRoot, setOutputRoot] = useState('');
  const [preHook, setPreHook] = useState('');
  const [postHook, setPostHook] = useState('');
  const [forceOverwrite, setForceOverwrite] = useState(false);
  const [maxRetries, setMaxRetries] = useState(2);
  const [jobs, setJobs] = useState<EditableJob[]>([]);
  const [inspecting, setInspecting] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [presetName, setPresetName] = useState('');
  const [presetScope, setPresetScope] = useState<'project' | 'global'>('project');
  const [filePickerTarget, setFilePickerTarget] = useState<RenderFilePickerTarget | null>(null);

  useEffect(() => {
    if (availableBlenders.some((installation) => installation.path === blenderPath)) return;
    const preferred = availableBlenders.find((installation) => installation.path === blenderDefault);
    setBlenderPath(preferred?.path || availableBlenders[0]?.path || '');
  }, [availableBlenders, blenderDefault, blenderPath]);

  const inspectPaths = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    if (!blenderPath) { showToast({ title: '未配置 Blender', message: '请先选择 Blender 可执行文件。', tone: 'warning' }); return; }
    setInspecting(true);
    try {
      const inspected = await invoke<RenderSourceInfo[]>('inspect_render_sources', { blenderPath, sources: paths.map((path) => ({ path })) });
      setJobs((current) => [...current, ...inspected.map(toEditableJob).filter((next) => !current.some((item) => item.path === next.path))]);
    } catch (error) {
      showToast({ title: '读取 Blender 场景失败', message: String(error), tone: 'error' });
    } finally { setInspecting(false); }
  }, [blenderPath, showToast]);

  useEffect(() => {
    if (initialSources.length > 0) void inspectPaths(initialSources);
  }, [initialSources, inspectPaths]);

  const updateJob = (index: number, patch: Partial<EditableJob>) => setJobs((items) => items.map((item, current) => current === index ? { ...item, ...patch } : item));
  const handleFilePickerSelection = async (paths: string[]) => {
    if (filePickerTarget === 'blend') await inspectPaths(paths);
    if (filePickerTarget === 'outputRoot') setOutputRoot(paths[0] || '');
    if (filePickerTarget === 'preHook') setPreHook(paths[0] || '');
    if (filePickerTarget === 'postHook') setPostHook(paths[0] || '');
  };
  const projectFilePickerTarget: ProjectFilePickerTarget = filePickerTarget === 'outputRoot' ? 'directory' : 'file';
  const filePickerExtensions = filePickerTarget === 'blend' ? ['blend'] : filePickerTarget === 'preHook' || filePickerTarget === 'postHook' ? ['py'] : [];
  const filePickerTitle = filePickerTarget === 'blend' ? '选择 Blender 文件' : filePickerTarget === 'outputRoot' ? '选择输出目录' : filePickerTarget === 'preHook' ? '选择前置脚本' : '选择后置脚本';
  const applyPreset = (presetId: string) => {
    const settings = presets.find((preset) => preset.id === presetId)?.settings;
    if (!settings) return;
    if (typeof settings.outputRoot === 'string') setOutputRoot(settings.outputRoot);
    if (typeof settings.forceOverwrite === 'boolean') setForceOverwrite(settings.forceOverwrite);
    if (typeof settings.maxRetries === 'number') setMaxRetries(settings.maxRetries);
    if (typeof settings.outputFormat === 'string') setJobs((items) => items.map((item) => ({ ...item, outputFormat: settings.outputFormat as string })));
    if (typeof settings.resolutionPercentage === 'number') setJobs((items) => items.map((item) => ({ ...item, resolutionPercentage: settings.resolutionPercentage as number })));
  };
  const savePreset = async () => {
    if (!presetName.trim()) return;
    await invoke('save_render_preset', { projectPath, name: presetName.trim(), scope: presetScope, settings: { outputRoot, forceOverwrite, maxRetries, outputFormat: jobs[0]?.outputFormat || 'PNG', resolutionPercentage: jobs[0]?.resolutionPercentage || 100 } });
    setPresetName('');
    showToast({ title: '预设已保存', message: presetScope === 'global' ? '所有项目均可使用' : '仅当前项目使用', tone: 'success' });
  };
  const submit = async () => {
    const validJobs = jobs.filter((job) => !job.error && job.sceneName && job.frameEnd >= job.frameStart);
    if (!namePrefix.trim() || !blenderPath || !validJobs.length) return;
    setSubmitting(true);
    try {
      const request: CreateRenderBatchRequest = { name: `${namePrefix.trim()} ${batchTimestamp}`, blenderPath, outputRoot: outputRoot || null, preHook: preHook || null, postHook: postHook || null, forceOverwrite, maxRetries, jobs: validJobs.map((job) => ({ blendPath: job.path, sceneName: job.sceneName, frameStart: job.frameStart, frameEnd: job.frameEnd, frameStep: job.frameStep, resolutionPercentage: job.resolutionPercentage, engine: job.engine || null, outputFormat: job.outputFormat })) };
      await invoke('create_render_batch', { projectPath, request });
      showToast({ title: '渲染批次已加入队列', message: `${validJobs.length} 个作业`, tone: 'success' });
      await onCreated();
    } catch (error) { showToast({ title: '创建渲染批次失败', message: String(error), tone: 'error' }); }
    finally { setSubmitting(false); }
  };

  return (
    <div className="fixed inset-0 z-[80] flex justify-end bg-black/45" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <div className="flex h-full w-[760px] max-w-[96vw] flex-col bg-white shadow-2xl dark:bg-gray-950">
        <div className="flex min-h-[58px] items-center justify-between border-b border-gray-200 px-5 dark:border-gray-800"><div><h3 className="text-base font-semibold">新建渲染批次</h3><p className="text-xs text-gray-500">设置仅在 Blender 内存中生效，不修改源文件</p></div><button title="关闭" className="h-8 w-8 p-0" onClick={onClose}><X className="mx-auto h-4 w-4" /></button></div>
        <div className="min-h-0 flex-1 overflow-auto p-5">
          <div className="grid grid-cols-2 gap-3 max-[620px]:grid-cols-1">
            <Field label="批次名称">
              <span className="flex h-9 min-w-0 overflow-hidden rounded border border-gray-300 dark:border-gray-700">
                <input
                  value={namePrefix}
                  onChange={(event) => setNamePrefix(event.target.value)}
                  className="min-w-[96px] flex-1 bg-transparent px-2 text-xs outline-none"
                  aria-label="批次名称前缀"
                />
                <span className="flex shrink-0 items-center border-l border-gray-200 bg-gray-50 px-2 text-xs tabular-nums text-gray-500 dark:border-gray-700 dark:bg-gray-900">
                  {batchTimestamp}
                </span>
              </span>
            </Field>
            <Field label="套用预设"><select defaultValue="" onChange={(e) => applyPreset(e.target.value)}><option value="">不使用预设</option>{presets.map((preset) => <option key={preset.id} value={preset.id}>{preset.name} · {preset.scope === 'global' ? '全局' : '项目'}</option>)}</select></Field>
            <Field label={<span className="inline-flex items-center gap-1">Blender 版本 *<HelpAssistant title="Blender 版本" text={["版本来自“设置 > Blender 版本管理”。", '场景读取和队列渲染都会使用当前选中的可执行文件。']} placement="right" /></span>}>
              <select value={blenderPath} onChange={(event) => setBlenderPath(event.target.value)}>
                {availableBlenders.length === 0 && <option value="">请先在设置中添加可用 Blender</option>}
                {availableBlenders.map((installation) => (
                  <option key={installation.path} value={installation.path}>
                    {installation.version ? `Blender ${installation.version}` : 'Blender'} · {installation.path}
                  </option>
                ))}
              </select>
            </Field>
            <PathField label="输出根目录（默认项目 renders）" value={outputRoot} onChange={setOutputRoot} onBrowse={() => setFilePickerTarget('outputRoot')} />
            <div className="grid grid-cols-2 gap-3"><Field label="失败重试"><input type="number" min={0} max={10} value={maxRetries} onChange={(e) => setMaxRetries(Number(e.target.value))} /></Field><label className="mt-6 flex h-9 items-center gap-2 text-xs"><input type="checkbox" checked={forceOverwrite} onChange={(e) => setForceOverwrite(e.target.checked)} className="h-4 w-4" />强制覆盖已有帧</label></div>
            <PathField label="前置脚本（PMC Python）" value={preHook} onChange={setPreHook} onBrowse={() => setFilePickerTarget('preHook')} />
            <PathField label="后置脚本（PMC Python）" value={postHook} onChange={setPostHook} onBrowse={() => setFilePickerTarget('postHook')} />
          </div>
          <div className="mt-5 flex items-center justify-between border-b border-gray-200 pb-2 dark:border-gray-800"><div><h4 className="text-sm font-semibold">源文件与场景</h4><p className="text-xs text-gray-500">每个文件选择一个场景并生成独立作业</p></div><button disabled={inspecting || !blenderPath} onClick={() => setFilePickerTarget('blend')} className="flex h-8 items-center gap-1.5 rounded border border-gray-300 px-3 text-xs disabled:opacity-50 dark:border-gray-700">{inspecting ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}添加 .blend</button></div>
          {jobs.length === 0 ? <div className="flex h-32 items-center justify-center text-sm text-gray-500">选择一个或多个 Blender 文件开始</div> : <div>{jobs.map((job,index) => <EditableJobRow key={job.path} job={job} onChange={(patch) => updateJob(index, patch)} onRemove={() => setJobs((items) => items.filter((_,current) => current !== index))} />)}</div>}
          <div className="mt-5 border-t border-gray-200 pt-4 dark:border-gray-800"><div className="flex flex-wrap items-end gap-2"><Field label="保存当前通用设置为预设"><input value={presetName} onChange={(e) => setPresetName(e.target.value)} placeholder="预设名称" /></Field><select value={presetScope} onChange={(e) => setPresetScope(e.target.value as 'project'|'global')} className="h-9"><option value="project">项目</option><option value="global">全局</option></select><button disabled={!presetName.trim()} onClick={() => void savePreset()} className="flex h-9 items-center gap-1.5 rounded border border-gray-300 px-3 text-xs disabled:opacity-40 dark:border-gray-700"><Save className="h-4 w-4" />保存预设</button></div></div>
        </div>
        <div className="flex min-h-[60px] items-center justify-between border-t border-gray-200 px-5 dark:border-gray-800"><span className="text-xs text-gray-500">{jobs.filter((job) => !job.error).length} 个有效作业</span><div className="flex gap-2"><button className="h-9 rounded px-4 text-xs" onClick={onClose}>取消</button><button disabled={submitting || !jobs.some((job) => !job.error) || !blenderPath} onClick={() => void submit()} className="flex h-9 items-center gap-1.5 rounded bg-gray-900 px-4 text-xs font-medium text-white disabled:opacity-40 dark:bg-white dark:text-gray-900">{submitting ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}加入队列</button></div></div>
      </div>
      <ProjectFilePickerDialog
        isOpen={filePickerTarget !== null}
        projectPath={projectPath}
        title={filePickerTitle}
        target={projectFilePickerTarget}
        selectionMode={filePickerTarget === 'blend' ? 'multiple' : 'single'}
        extensions={filePickerExtensions}
        onClose={() => setFilePickerTarget(null)}
        onSelect={handleFilePickerSelection}
      />
    </div>
  );
}

function toEditableJob(source: RenderSourceInfo): EditableJob {
  const scene = source.scenes[0];
  return { path: source.path, scenes: source.scenes, sceneName: scene?.name || '', frameStart: scene?.frameStart ?? 1, frameEnd: scene?.frameEnd ?? 250, frameStep: 1, resolutionPercentage: 100, engine: scene?.engine || '', outputFormat: scene?.outputFormat || 'PNG', error: source.error };
}

function EditableJobRow({ job, onChange, onRemove }: { job: EditableJob; onChange: (patch: Partial<EditableJob>) => void; onRemove: () => void }) {
  const scene = job.scenes.find((item) => item.name === job.sceneName);
  return <div className="border-b border-gray-100 py-3 dark:border-gray-800"><div className="flex items-center gap-2"><span className="min-w-0 flex-1 truncate text-xs font-medium" title={job.path}>{fileName(job.path)}</span>{job.error && <span className="text-xs text-red-600">读取失败</span>}<button title="移除" onClick={onRemove} className="h-7 w-7 p-0 text-gray-400 hover:text-red-600"><X className="mx-auto h-3.5 w-3.5" /></button></div>{job.error ? <p className="mt-1 text-xs text-red-600">{job.error}</p> : <div className="mt-2 grid grid-cols-[minmax(120px,1fr)_80px_80px_62px_90px_100px] gap-2 max-[680px]:grid-cols-3"><Field label="场景"><select value={job.sceneName} onChange={(e) => { const next=job.scenes.find((item)=>item.name===e.target.value); onChange({ sceneName:e.target.value, frameStart:next?.frameStart ?? job.frameStart, frameEnd:next?.frameEnd ?? job.frameEnd, engine:next?.engine ?? job.engine, outputFormat:next?.outputFormat ?? job.outputFormat }); }}>{job.scenes.map((item)=><option key={item.name}>{item.name}</option>)}</select></Field><Field label="起始"><input type="number" value={job.frameStart} onChange={(e)=>onChange({frameStart:Number(e.target.value)})} /></Field><Field label="结束"><input type="number" value={job.frameEnd} onChange={(e)=>onChange({frameEnd:Number(e.target.value)})} /></Field><Field label="步长"><input type="number" min={1} value={job.frameStep} onChange={(e)=>onChange({frameStep:Math.max(1,Number(e.target.value))})} /></Field><Field label={<span className="inline-flex items-center gap-1">分辨率 %<HelpAssistant title="渲染分辨率比例" text={["按场景原始分辨率的百分比渲染。", '降低比例可以加快预览渲染，正式输出通常使用 100%。']} images={[{ src: '/help_media/渲染像素比.jpg', alt: '不同渲染清晰度的对比' }]} placement="top" /></span>}><input type="number" min={1} max={100} value={job.resolutionPercentage} onChange={(e)=>onChange({resolutionPercentage:Number(e.target.value)})} /></Field><Field label="格式"><select value={job.outputFormat} onChange={(e)=>onChange({outputFormat:e.target.value})}><option>PNG</option><option>JPEG</option><option>OPEN_EXR</option><option>TIFF</option><option>WEBP</option></select></Field><div className="col-span-full text-[10px] text-gray-500">{scene?.resolutionX} × {scene?.resolutionY} · {scene?.fps} fps · {job.engine}</div></div>}</div>;
}

function Field({ label, children }: { label: React.ReactNode; children: React.ReactNode }) {
  return <label className="block min-w-0"><span className="mb-1 flex h-4 min-w-0 items-center text-[11px] font-medium text-gray-600 dark:text-gray-400">{label}</span><span className="block [&>input]:h-9 [&>input]:w-full [&>input]:rounded [&>input]:border [&>input]:border-gray-300 [&>input]:bg-transparent [&>input]:px-2 [&>input]:text-xs [&>select]:h-9 [&>select]:w-full [&>select]:rounded [&>select]:border [&>select]:border-gray-300 [&>select]:bg-transparent [&>select]:px-2 [&>select]:text-xs dark:[&>input]:border-gray-700 dark:[&>select]:border-gray-700">{children}</span></label>;
}

function PathField({ label, value, onChange, onBrowse, required = false }: { label: string; value: string; onChange: (value: string) => void; onBrowse: () => void; required?: boolean }) {
  return <Field label={`${label}${required ? ' *' : ''}`}><span className="flex h-9 overflow-hidden rounded border border-gray-300 dark:border-gray-700"><input value={value} onChange={(e) => onChange(e.target.value)} className="min-w-0 flex-1 bg-transparent px-2 text-xs outline-none" /><button title="浏览" onClick={onBrowse} className="h-9 w-9 shrink-0 border-l border-gray-300 p-0 dark:border-gray-700"><FolderOpen className="mx-auto h-4 w-4" /></button></span></Field>;
}
