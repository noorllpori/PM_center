import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  AlertCircle,
  Activity,
  Archive,
  Check,
  ChevronLeft,
  ChevronDown,
  ChevronRight,
  CirclePause,
  CirclePlay,
  Clock3,
  Cpu,
  FolderOpen,
  Gauge,
  Image as ImageIcon,
  Layers3,
  ListRestart,
  LoaderCircle,
  MemoryStick,
  Maximize2,
  Minimize2,
  Pause,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  Square,
  Terminal,
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
  RenderEta,
  RenderFrame,
  RenderJob,
  RenderJobDetail,
  RenderPerformanceSample,
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

function formatMemory(bytes: number) {
  if (!bytes) return '-';
  if (bytes < 1024 * 1024 * 1024) return `${Math.round(bytes / (1024 * 1024))} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function formatRemainingDuration(ms: number) {
  if (ms < 60_000) return `${Math.max(1, Math.ceil(ms / 1000))} 秒`;
  const minutes = Math.max(1, Math.round(ms / 60_000));
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  return remainder ? `${hours} 小时 ${remainder} 分` : `${hours} 小时`;
}

interface SmoothedEta extends RenderEta {
  jobId: string;
}

function useSmoothedEta(job: RenderJob, rawEta?: RenderEta) {
  const previousRef = useRef<SmoothedEta | null>(null);
  const inputKeyRef = useRef('');
  const fallback: RenderEta = {
    status: job.status === 'completed' ? 'completed' : job.status === 'paused' ? 'paused' : ['failed', 'cancelled'].includes(job.status) ? 'unavailable' : 'calibrating',
    estimatedFinishAt: null,
    remainingMs: null,
    sampleCount: job.completedFrames,
    confidence: 'none',
  };
  const next = rawEta || fallback;
  const inputKey = `${job.id}:${next.status}:${next.estimatedFinishAt}:${next.remainingMs}:${next.sampleCount}:${next.confidence}`;
  const previous = previousRef.current;

  if (inputKeyRef.current === inputKey && previous) {
    return previous.status === 'estimating' && previous.estimatedFinishAt !== null
      ? { ...previous, remainingMs: Math.max(0, previous.estimatedFinishAt - Date.now()) }
      : previous;
  }
  inputKeyRef.current = inputKey;

  if (next.status !== 'estimating' || next.estimatedFinishAt === null) {
    const resolved = { ...next, jobId: job.id };
    previousRef.current = resolved;
    return resolved;
  }
  if (!previous || previous.jobId !== job.id || previous.status !== 'estimating' || previous.estimatedFinishAt === null) {
    const resolved = { ...next, jobId: job.id };
    previousRef.current = resolved;
    return resolved;
  }

  const rawDelta = next.estimatedFinishAt - previous.estimatedFinishAt;
  const alpha = next.confidence === 'high' ? 0.2 : next.confidence === 'medium' ? 0.35 : 0.55;
  const maxShift = Math.max(30_000, (next.remainingMs || 0) * 0.2);
  let shift = Math.max(-maxShift, Math.min(maxShift, rawDelta * alpha));
  if (next.sampleCount === previous.sampleCount && rawDelta <= 0) shift = 0;
  let estimatedFinishAt = previous.estimatedFinishAt + shift;
  if (estimatedFinishAt <= Date.now()) {
    estimatedFinishAt = Math.max(
      Date.now() + 5_000,
      Date.now() + (next.remainingMs || 0) * 0.75,
      previous.estimatedFinishAt + Math.max(1_000, rawDelta * 0.12),
    );
  }
  const resolved: SmoothedEta = {
    ...next,
    jobId: job.id,
    estimatedFinishAt,
    remainingMs: Math.max(0, estimatedFinishAt - Date.now()),
  };
  previousRef.current = resolved;
  return resolved;
}

function formatCompletionEstimate(eta: RenderEta) {
  if (eta.status === 'completed') return { value: '已完成', detail: '' };
  if (eta.status === 'paused') return { value: '已暂停', detail: '' };
  if (eta.status === 'unavailable') return { value: '-', detail: '' };
  if (eta.status !== 'estimating' || eta.estimatedFinishAt === null || eta.remainingMs === null) {
    return { value: '校准中', detail: eta.sampleCount === 1 ? '还需 1 帧' : '等待样本' };
  }
  const confidenceLabel = eta.confidence === 'high' ? '稳定' : eta.confidence === 'medium' ? '校准中' : '初步';
  return {
    value: new Date(eta.estimatedFinishAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    detail: `约剩 ${formatRemainingDuration(eta.remainingMs)} · ${confidenceLabel}`,
  };
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
  const showLivePerformance = ['starting', 'running', 'pausing', 'cancelling'].includes(job.status)
    && job.performanceUpdatedAt !== null;
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
      <div className="mt-1.5 flex justify-between gap-3 text-[11px] text-gray-500"><span>{job.completedFrames} 完成 · {job.failedFrames} 失败 · {job.skippedFrames} 跳过</span><span>{Math.round(job.progress)}%</span></div>
      {showLivePerformance && <div className="mt-1.5 flex items-center gap-3 text-[10px] tabular-nums text-gray-500"><span className="inline-flex items-center gap-1"><Cpu className="h-3 w-3" />{job.cpuUsage.toFixed(1)}%</span><span className="inline-flex items-center gap-1"><MemoryStick className="h-3 w-3" />{formatMemory(job.memoryBytes)}</span></div>}
    </button>
  );
}

function JobDetailPane({ detail, busy, onAction }: { detail: RenderJobDetail; busy: boolean; onAction: (label: string, command: string, payload?: Record<string, unknown>) => Promise<void> }) {
  const { job, frames, logTail } = detail;
  const performanceSamples = detail.performanceSamples || [];
  const [logExpanded, setLogExpanded] = useState(false);
  const [showPerformance, setShowPerformance] = useState(false);
  const [selectedFrameNumber, setSelectedFrameNumber] = useState<number | null>(null);
  const [previewFrameNumber, setPreviewFrameNumber] = useState<number | null>(null);
  const canPause = ['pending', 'starting', 'running'].includes(job.status);
  const canResume = ['paused', 'failed', 'cancelled'].includes(job.status);
  const failedFrames = frames.filter((frame) => frame.status === 'failed').map((frame) => frame.frame);
  const previewableFrames = useMemo(
    () => frames.filter((frame) => ['completed', 'skipped'].includes(frame.status) && Boolean(frame.outputPath.trim())),
    [frames],
  );
  const previewableFrameNumbers = useMemo(
    () => new Set(previewableFrames.map((frame) => frame.frame)),
    [previewableFrames],
  );
  const smoothedEta = useSmoothedEta(job, detail.eta);
  const completionEstimate = formatCompletionEstimate(smoothedEta);
  const summaryItems: Array<{ label: string; value: string | number; detail: string }> = [
    { label: '总帧', value: job.totalFrames, detail: '' },
    { label: '完成', value: job.completedFrames, detail: '' },
    { label: '失败', value: job.failedFrames, detail: '' },
    { label: '当前', value: job.currentFrame ?? '-', detail: '' },
    { label: '预计完成', value: completionEstimate.value, detail: completionEstimate.detail },
  ];

  useEffect(() => {
    setLogExpanded(false);
    setShowPerformance(false);
    setSelectedFrameNumber(null);
    setPreviewFrameNumber(null);
  }, [job.id]);

  useEffect(() => {
    if (selectedFrameNumber !== null && !frames.some((frame) => frame.frame === selectedFrameNumber)) {
      setSelectedFrameNumber(null);
    }
    if (previewFrameNumber !== null && !previewableFrames.some((frame) => frame.frame === previewFrameNumber)) {
      setPreviewFrameNumber(null);
    }
  }, [frames, previewFrameNumber, previewableFrames, selectedFrameNumber]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-gray-200 px-3 py-2 dark:border-gray-800">
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1"><div className="flex items-center gap-2"><h3 className="truncate text-sm font-semibold">{job.name}</h3><StatusBadge status={job.status} /></div><p className="mt-0.5 truncate text-[11px] text-gray-500" title={job.outputDir}>{job.outputDir}</p></div>
          <div className="flex shrink-0 items-center gap-1">
          {canPause && <IconAction title="暂停" icon={<CirclePause />} disabled={busy} onClick={() => onAction('暂停作业', 'pause_render_job')} />}
          {canResume && <IconAction title="继续" icon={<CirclePlay />} disabled={busy} onClick={() => onAction('继续作业', 'resume_render_job')} />}
          {!['completed','cancelled'].includes(job.status) && <IconAction title="取消" icon={<Square />} disabled={busy} onClick={() => onAction('取消作业', 'cancel_render_job')} />}
          {failedFrames.length > 0 && <IconAction title="重试失败帧" icon={<RotateCcw />} disabled={busy} onClick={() => onAction('重试失败帧', 'retry_render_frames', { frames: failedFrames })} />}
          <IconAction title="打开输出目录" icon={<FolderOpen />} disabled={busy} onClick={() => onAction('打开输出目录', 'open_render_output', { path: job.outputDir })} />
          {!['running','pausing','cancelling'].includes(job.status) && <IconAction title={job.archived ? '取消归档' : '归档'} icon={<Archive />} disabled={busy} onClick={() => onAction('归档作业', 'archive_render_job', { archived: !job.archived })} />}
          </div>
        </div>
        {job.error && <p className="mt-1.5 flex items-start gap-1.5 text-[11px] text-red-600"><AlertCircle className="mt-0.5 h-3 w-3 shrink-0" />{job.error}</p>}
      </div>
      <div className="grid grid-cols-5 border-b border-gray-200 dark:border-gray-800">
        {summaryItems.map(({ label, value, detail: detailText }) => <div key={label} className="min-w-0 border-r border-gray-100 px-2.5 py-1.5 last:border-r-0 dark:border-gray-800"><div className="text-[9px] text-gray-500">{label}</div><div className="truncate text-xs font-semibold tabular-nums" title={detailText || String(value)}>{value}</div>{detailText && <div className="truncate text-[9px] text-gray-500" title={detailText}>{detailText}</div>}</div>)}
      </div>
      <div className="grid grid-cols-[minmax(96px,1.2fr)_repeat(4,minmax(58px,1fr))] border-b border-gray-200 bg-gray-50/70 dark:border-gray-800 dark:bg-gray-900/50">
        <div className="min-w-0 px-2.5 py-1.5 text-[9px] text-gray-500">
          <div className="truncate font-medium">性能监测 · Blender</div>
          <div className="truncate">{job.performanceUpdatedAt ? new Date(job.performanceUpdatedAt).toLocaleTimeString() : '等待采样'}</div>
        </div>
        <PerformanceValue icon={<Cpu />} label="CPU" value={job.performanceUpdatedAt ? `${job.cpuUsage.toFixed(1)}%` : '-'} onClick={() => setShowPerformance(true)} />
        <PerformanceValue icon={<MemoryStick />} label="内存" value={formatMemory(job.memoryBytes)} onClick={() => setShowPerformance(true)} />
        <PerformanceValue icon={<Cpu />} label="峰值 CPU" value={job.performanceUpdatedAt ? `${job.peakCpuUsage.toFixed(1)}%` : '-'} />
        <PerformanceValue icon={<MemoryStick />} label="峰值内存" value={formatMemory(job.peakMemoryBytes)} />
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <div className="sticky top-0 grid grid-cols-[64px_82px_64px_1fr] bg-gray-50 px-3 py-1.5 text-[10px] font-medium text-gray-500 dark:bg-gray-900"><span>帧</span><span>状态</span><span>耗时</span><span>输出</span></div>
        {frames.map((frame) => {
          const previewable = previewableFrameNumbers.has(frame.frame);
          return (
            <FrameRow
              key={frame.frame}
              frame={frame}
              selected={selectedFrameNumber === frame.frame}
              previewable={previewable}
              onSelect={() => setSelectedFrameNumber(frame.frame)}
              onPreview={() => {
                setSelectedFrameNumber(frame.frame);
                if (previewable) setPreviewFrameNumber(frame.frame);
              }}
            />
          );
        })}
      </div>
      <div className={`flex shrink-0 flex-col border-t border-gray-200 dark:border-gray-800 ${logExpanded ? 'h-[38%] min-h-[140px] max-h-[320px]' : 'h-8'}`}>
        <button type="button" onClick={() => setLogExpanded((expanded) => !expanded)} className="flex h-8 shrink-0 items-center gap-2 bg-gray-100 px-3 text-[10px] font-medium text-gray-600 hover:bg-gray-200 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800">
          <Terminal className="h-3.5 w-3.5" />
          <span>任务日志</span>
          <span className="text-gray-400">{logTail.length} 行</span>
          <ChevronDown className={`ml-auto h-3.5 w-3.5 transition-transform ${logExpanded ? 'rotate-180' : ''}`} />
        </button>
        {logExpanded && <div className="min-h-0 flex-1 overflow-auto bg-gray-950 p-3 font-mono text-[11px] leading-5 text-gray-300">
          {logTail.length ? logTail.map((line,index) => <div key={`${index}-${line}`} className="break-all">{line}</div>) : <span className="text-gray-600">尚无日志</span>}
        </div>}
      </div>
      {showPerformance && <PerformanceChartDialog job={job} samples={performanceSamples} onClose={() => setShowPerformance(false)} />}
      {previewFrameNumber !== null && (
        <RenderFramePreview
          jobName={job.name}
          frames={previewableFrames}
          currentFrameNumber={previewFrameNumber}
          onFrameChange={(frameNumber) => {
            setPreviewFrameNumber(frameNumber);
            setSelectedFrameNumber(frameNumber);
          }}
          onClose={() => setPreviewFrameNumber(null)}
        />
      )}
    </div>
  );
}

function PerformanceValue({ icon, label, value, onClick }: { icon: React.ReactElement; label: string; value: string; onClick?: () => void }) {
  const content = <><div className="flex items-center gap-1 text-[9px] text-gray-500 [&>svg]:h-3 [&>svg]:w-3">{icon}{label}{onClick && <Activity className="ml-auto opacity-50" />}</div><div className="truncate text-xs font-semibold tabular-nums" title={value}>{value}</div></>;
  if (onClick) return <button type="button" onClick={onClick} title={`查看${label}曲线`} className="min-w-0 border-l border-gray-200 px-2 py-1.5 text-left hover:bg-gray-100 dark:border-gray-800 dark:hover:bg-gray-800">{content}</button>;
  return <div className="min-w-0 border-l border-gray-200 px-2 py-1.5 dark:border-gray-800">{content}</div>;
}

function PerformanceChartDialog({ job, samples, onClose }: { job: RenderJob; samples: RenderPerformanceSample[]; onClose: () => void }) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  const latest = samples[samples.length - 1];
  const memoryMax = Math.max(job.peakMemoryBytes, ...samples.map((sample) => sample.memoryBytes), 1);
  const timeRange = samples.length > 1
    ? `${new Date(samples[0].sampledAt).toLocaleTimeString()} - ${new Date(samples[samples.length - 1].sampledAt).toLocaleTimeString()}`
    : '等待采样';

  return (
    <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/50 p-4" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <div role="dialog" aria-modal="true" aria-label="任务性能曲线" className="flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-md border border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-950">
        <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-3 dark:border-gray-800">
          <Activity className="h-5 w-5 text-blue-600" />
          <div className="min-w-0 flex-1"><h3 className="truncate text-sm font-semibold">任务性能曲线</h3><p className="truncate text-[11px] text-gray-500">{job.name} · 最近 {samples.length} 个采样 · {timeRange}</p></div>
          <button type="button" onClick={onClose} title="关闭" className="flex h-8 w-8 items-center justify-center rounded hover:bg-gray-100 dark:hover:bg-gray-800"><X className="h-4 w-4" /></button>
        </div>
        <div className="min-h-0 overflow-auto p-4">
          <div className="mb-4 grid grid-cols-4 border border-gray-200 dark:border-gray-800">
            <ChartSummary label="当前 CPU" value={latest ? `${latest.cpuUsage.toFixed(1)}%` : '-'} />
            <ChartSummary label="峰值 CPU" value={`${job.peakCpuUsage.toFixed(1)}%`} />
            <ChartSummary label="当前内存" value={latest ? formatMemory(latest.memoryBytes) : '-'} />
            <ChartSummary label="峰值内存" value={formatMemory(job.peakMemoryBytes)} />
          </div>
          <WaveformChart title="CPU 使用率" color="#2563eb" samples={samples} value={(sample) => sample.cpuUsage} maxValue={100} formatValue={(value) => `${value.toFixed(0)}%`} />
          <div className="mt-5"><WaveformChart title="内存占用" color="#059669" samples={samples} value={(sample) => sample.memoryBytes} maxValue={memoryMax} formatValue={(value) => formatMemory(value)} /></div>
        </div>
      </div>
    </div>
  );
}

function ChartSummary({ label, value }: { label: string; value: string }) {
  return <div className="min-w-0 border-r border-gray-200 px-3 py-2 last:border-r-0 dark:border-gray-800"><div className="text-[10px] text-gray-500">{label}</div><div className="truncate text-sm font-semibold tabular-nums" title={value}>{value}</div></div>;
}

function WaveformChart({ title, color, samples, value, maxValue, formatValue }: { title: string; color: string; samples: RenderPerformanceSample[]; value: (sample: RenderPerformanceSample) => number; maxValue: number; formatValue: (value: number) => string }) {
  const width = 720;
  const height = 176;
  const left = 48;
  const right = 12;
  const top = 12;
  const bottom = 24;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const safeMax = Math.max(maxValue, 1);
  const points = samples.map((sample, index) => ({
    x: left + (samples.length === 1 ? plotWidth / 2 : (index / (samples.length - 1)) * plotWidth),
    y: top + plotHeight - (Math.min(value(sample), safeMax) / safeMax) * plotHeight,
    sample,
  }));
  const linePath = points.map((point, index) => `${index ? 'L' : 'M'} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`).join(' ');
  const areaPath = points.length ? `${linePath} L ${points[points.length - 1].x.toFixed(2)} ${top + plotHeight} L ${points[0].x.toFixed(2)} ${top + plotHeight} Z` : '';

  return (
    <section>
      <div className="mb-2 flex items-center justify-between"><h4 className="text-xs font-medium">{title}</h4><span className="text-[10px] text-gray-500">2 秒采样 · 最近 10 分钟</span></div>
      <div className="relative h-44 w-full overflow-hidden border border-gray-200 bg-gray-50 dark:border-gray-800 dark:bg-gray-900/50">
        {samples.length === 0 ? <div className="flex h-full items-center justify-center text-xs text-gray-500">暂无性能采样</div> : (
          <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" className="h-full w-full" aria-label={`${title}波形图`}>
            {[0, 0.25, 0.5, 0.75, 1].map((ratio) => {
              const y = top + plotHeight * ratio;
              const tickValue = safeMax * (1 - ratio);
              return <g key={ratio}><line x1={left} y1={y} x2={width - right} y2={y} stroke="currentColor" strokeOpacity="0.12" vectorEffect="non-scaling-stroke" /><text x={left - 6} y={y + 3} textAnchor="end" className="fill-gray-400 text-[9px]">{formatValue(tickValue)}</text></g>;
            })}
            <path d={areaPath} fill={color} opacity="0.1" />
            <path d={linePath} fill="none" stroke={color} strokeWidth="2" vectorEffect="non-scaling-stroke" />
            {points.map((point) => <circle key={point.sample.sampledAt} cx={point.x} cy={point.y} r="2" fill={color}><title>{`${new Date(point.sample.sampledAt).toLocaleTimeString()} · ${formatValue(value(point.sample))}`}</title></circle>)}
            <text x={left} y={height - 7} className="fill-gray-400 text-[9px]">{new Date(samples[0].sampledAt).toLocaleTimeString()}</text>
            <text x={width - right} y={height - 7} textAnchor="end" className="fill-gray-400 text-[9px]">{new Date(samples[samples.length - 1].sampledAt).toLocaleTimeString()}</text>
          </svg>
        )}
      </div>
    </section>
  );
}

function FrameRow({ frame, selected, previewable, onSelect, onPreview }: { frame: RenderFrame; selected: boolean; previewable: boolean; onSelect: () => void; onPreview: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onSelect}
      onDoubleClick={onPreview}
      title={previewable ? `双击预览帧 ${frame.frame}` : `帧 ${frame.frame} 暂无可预览输出`}
      className={`grid h-8 w-full grid-cols-[62px_82px_64px_1fr] items-center border-l-2 border-t px-2.5 text-left text-[11px] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-blue-500 dark:border-t-gray-900 ${
        selected
          ? 'border-l-blue-600 border-t-blue-200 bg-blue-100 text-blue-950 shadow-[inset_0_0_0_1px_rgba(37,99,235,0.22)] dark:border-l-blue-400 dark:border-t-blue-900 dark:bg-blue-950/70 dark:text-blue-100'
          : 'border-l-transparent border-t-gray-100 hover:bg-gray-50 dark:hover:bg-gray-900/70'
      }`}
    >
      <span className="font-medium tabular-nums">{frame.frame}</span>
      <span className={frame.status === 'failed' ? 'text-red-600' : frame.status === 'completed' ? 'text-emerald-600 dark:text-emerald-400' : selected ? 'text-blue-700 dark:text-blue-300' : 'text-gray-500'}>{STATUS_LABELS[frame.status] || frame.status}</span>
      <span className={selected ? 'text-blue-700 dark:text-blue-300' : 'text-gray-500'}>{formatDuration(frame.durationMs)}</span>
      <span className={`flex min-w-0 items-center gap-1.5 ${selected ? 'text-blue-800 dark:text-blue-200' : 'text-gray-500'}`}>
        {previewable && <ImageIcon className="h-3 w-3 shrink-0" />}
        <span className="truncate" title={frame.outputPath}>{fileName(frame.outputPath)}</span>
      </span>
    </button>
  );
}

const PREVIEW_FPS_OPTIONS = [1, 2, 4, 8, 12, 24];

function RenderFramePreview({ jobName, frames, currentFrameNumber, onFrameChange, onClose }: { jobName: string; frames: RenderFrame[]; currentFrameNumber: number; onFrameChange: (frameNumber: number) => void; onClose: () => void }) {
  const previewRef = useRef<HTMLDivElement>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [fps, setFps] = useState(8);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [isImageLoading, setIsImageLoading] = useState(true);
  const [imageError, setImageError] = useState(false);
  const currentIndex = Math.max(0, frames.findIndex((frame) => frame.frame === currentFrameNumber));
  const currentFrame = frames[currentIndex] || null;
  const currentSource = useMemo(
    () => currentFrame ? convertFileSrc(currentFrame.outputPath) : '',
    [currentFrame],
  );

  const goPrevious = useCallback(() => {
    if (currentIndex > 0) onFrameChange(frames[currentIndex - 1].frame);
  }, [currentIndex, frames, onFrameChange]);

  const goNext = useCallback(() => {
    if (currentIndex < frames.length - 1) onFrameChange(frames[currentIndex + 1].frame);
  }, [currentIndex, frames, onFrameChange]);

  const toggleFullscreen = useCallback(async () => {
    try {
      if (document.fullscreenElement) {
        await document.exitFullscreen();
        return;
      }
      await previewRef.current?.requestFullscreen();
    } catch {
      setIsFullscreen(false);
    }
  }, []);

  const closePreview = useCallback(() => {
    if (document.fullscreenElement === previewRef.current) void document.exitFullscreen();
    onClose();
  }, [onClose]);

  useEffect(() => {
    const handleFullscreenChange = () => setIsFullscreen(document.fullscreenElement === previewRef.current);
    document.addEventListener('fullscreenchange', handleFullscreenChange);
    return () => document.removeEventListener('fullscreenchange', handleFullscreenChange);
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'ArrowLeft') {
        event.preventDefault();
        goPrevious();
      } else if (event.key === 'ArrowRight') {
        event.preventDefault();
        goNext();
      } else if (event.key === ' ') {
        event.preventDefault();
        setIsPlaying((playing) => !playing);
      } else if (event.key.toLowerCase() === 'f') {
        event.preventDefault();
        void toggleFullscreen();
      } else if (event.key === 'Escape' && !document.fullscreenElement) {
        closePreview();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [closePreview, goNext, goPrevious, toggleFullscreen]);

  useEffect(() => {
    if (!isPlaying) return;
    if (frames.length <= 1 || currentIndex >= frames.length - 1) {
      setIsPlaying(false);
      return;
    }
    const timer = window.setTimeout(() => {
      onFrameChange(frames[currentIndex + 1].frame);
    }, 1000 / fps);
    return () => window.clearTimeout(timer);
  }, [currentIndex, fps, frames, isPlaying, onFrameChange]);

  useEffect(() => {
    setIsImageLoading(true);
    setImageError(false);
  }, [currentSource]);

  useEffect(() => {
    [frames[currentIndex - 1], frames[currentIndex + 1]].forEach((frame) => {
      if (!frame) return;
      const image = new Image();
      image.src = convertFileSrc(frame.outputPath);
    });
  }, [currentIndex, frames]);

  return (
    <div ref={previewRef} role="dialog" aria-modal="true" aria-label="渲染帧预览" className="fixed inset-0 z-[110] flex min-h-0 flex-col bg-neutral-950 text-white">
      <div className="flex min-h-12 shrink-0 flex-wrap items-center gap-2 border-b border-white/10 bg-neutral-950 px-3 py-2">
        <ImageIcon className="h-4 w-4 shrink-0 text-blue-400" />
        <div className="min-w-[140px] flex-1">
          <p className="truncate text-sm font-medium">{jobName}</p>
          <p className="truncate text-[10px] text-white/50">帧 {currentFrame?.frame ?? '-'} · {currentIndex + 1}/{frames.length}</p>
        </div>
        <div className="flex max-w-full flex-wrap items-center justify-end gap-1">
          <button type="button" onClick={goPrevious} disabled={currentIndex <= 0} title="上一帧" className="flex h-8 w-8 items-center justify-center rounded hover:bg-white/10 disabled:opacity-30"><ChevronLeft className="h-4 w-4" /></button>
          <button type="button" onClick={() => setIsPlaying((playing) => !playing)} disabled={frames.length <= 1} title={isPlaying ? '暂停测试播放' : '测试播放'} className={`flex h-8 items-center gap-1.5 rounded px-2.5 text-xs ${isPlaying ? 'bg-blue-600 hover:bg-blue-500' : 'bg-white/10 hover:bg-white/15'} disabled:opacity-30`}>
            {isPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
            <span>{isPlaying ? '暂停' : '测试播放'}</span>
          </button>
          <button type="button" onClick={goNext} disabled={currentIndex >= frames.length - 1} title="下一帧" className="flex h-8 w-8 items-center justify-center rounded hover:bg-white/10 disabled:opacity-30"><ChevronRight className="h-4 w-4" /></button>
          <label className="ml-1 flex h-8 items-center gap-1 rounded border border-white/10 px-2 text-[10px] text-white/60">
            FPS
            <select value={fps} onChange={(event) => setFps(Number(event.target.value))} className="bg-neutral-950 text-xs text-white outline-none">
              {PREVIEW_FPS_OPTIONS.map((value) => <option key={value} value={value}>{value}</option>)}
            </select>
          </label>
          <button type="button" onClick={() => void toggleFullscreen()} title={isFullscreen ? '退出全屏' : '全屏预览'} className="ml-1 flex h-8 w-8 items-center justify-center rounded hover:bg-white/10">{isFullscreen ? <Minimize2 className="h-4 w-4" /> : <Maximize2 className="h-4 w-4" />}</button>
          <button type="button" onClick={closePreview} title="关闭预览" className="flex h-8 w-8 items-center justify-center rounded hover:bg-white/10"><X className="h-4 w-4" /></button>
        </div>
      </div>

      <div className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden bg-black">
        {currentSource && !imageError ? (
          <img
            key={currentSource}
            src={currentSource}
            alt={`渲染帧 ${currentFrame?.frame ?? ''}`}
            draggable={false}
            onLoad={() => setIsImageLoading(false)}
            onError={() => { setIsImageLoading(false); setImageError(true); setIsPlaying(false); }}
            className={`max-h-full max-w-full select-none object-contain transition-opacity ${isImageLoading ? 'opacity-0' : 'opacity-100'}`}
          />
        ) : (
          <div className="max-w-md px-6 text-center text-white/55"><ImageIcon className="mx-auto mb-3 h-12 w-12 opacity-40" /><p className="text-sm">无法显示此帧</p><p className="mt-1 break-all text-[11px] text-white/35">{currentFrame?.outputPath}</p></div>
        )}
        {isImageLoading && !imageError && <div className="absolute inset-0 flex items-center justify-center"><LoaderCircle className="h-8 w-8 animate-spin text-white/55" /></div>}
        <button type="button" onClick={goPrevious} disabled={currentIndex <= 0} title="上一帧" className="absolute left-3 flex h-12 w-10 items-center justify-center rounded bg-black/45 text-white/80 backdrop-blur-sm hover:bg-black/70 disabled:hidden"><ChevronLeft className="h-6 w-6" /></button>
        <button type="button" onClick={goNext} disabled={currentIndex >= frames.length - 1} title="下一帧" className="absolute right-3 flex h-12 w-10 items-center justify-center rounded bg-black/45 text-white/80 backdrop-blur-sm hover:bg-black/70 disabled:hidden"><ChevronRight className="h-6 w-6" /></button>
      </div>

      <div className="flex h-8 shrink-0 items-center gap-3 border-t border-white/10 bg-neutral-950 px-3 text-[10px] text-white/45">
        <span className="shrink-0 tabular-nums">{currentFrame ? `${currentFrame.frame} / ${frames[frames.length - 1]?.frame}` : '-'}</span>
        <span className="min-w-0 flex-1 truncate" title={currentFrame?.outputPath}>{currentFrame?.outputPath}</span>
        <span className="shrink-0">{isPlaying ? `${fps} FPS 播放中` : '已暂停'}</span>
      </div>
    </div>
  );
}

function IconAction({ title, icon, disabled, onClick }: { title: string; icon: React.ReactElement; disabled: boolean; onClick: () => void }) {
  return <button title={title} disabled={disabled} onClick={onClick} className="flex h-7 w-7 items-center justify-center rounded border border-gray-200 text-gray-600 hover:bg-gray-100 disabled:opacity-40 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800 [&>svg]:h-3.5 [&>svg]:w-3.5">{icon}</button>;
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
