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
  Copy,
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
  Pencil,
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
  UpdateRenderJobRequest,
} from '../../types/render';

type CenterView = 'queue' | 'results' | 'presets';

type FrameContextMenu = {
  x: number;
  y: number;
  frameNumbers: number[];
};

type FrameMarquee = {
  pointerId: number;
  startX: number;
  startY: number;
  currentX: number;
  currentY: number;
  baseSelection: Set<number>;
};

type RerenderConfirmation = {
  frameNumbers: number[];
};

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
    if (!projectPath) return false;
    setBusyAction(label);
    try {
      await invoke(command, { projectPath, ...payload });
      await refresh();
      if (selectedJobId) await loadDetail(selectedJobId);
      return true;
    } catch (error) {
      showToast({ title: `${label}失败`, message: String(error), tone: 'error' });
      return false;
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
    <div onContextMenu={(event) => event.preventDefault()} className="flex h-full min-h-0 flex-col bg-white text-gray-900 dark:bg-gray-950 dark:text-gray-100">
      <header className="flex min-h-[64px] flex-wrap items-center gap-3 border-b border-gray-200 px-4 py-3 dark:border-gray-800">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Layers3 className="h-5 w-5 text-orange-500" />
            <h2 className="truncate text-base font-semibold">渲染与批处理</h2>
            <HelpAssistant
              title="渲染中心怎么用"
              text={[
                '先点击“新建批次”，选择 Blender 版本并添加一个或多个 .blend 文件。每个文件会成为独立作业。',
                '在作业中设置场景、帧范围、帧多开、分辨率和格式后加入队列；在右侧选中任务可查看帧、日志和预览。',
                '任务创建后可用右上角铅笔修改设置；正在渲染时先暂停，当前运行帧结束后再编辑。',
              ]}
              placement="bottom-start"
              width={360}
            />
            <span className="truncate text-xs text-gray-500">{projectName}</span>
          </div>
          <p className="mt-0.5 truncate text-xs text-gray-500">本机队列 · {activeCount} 个活动作业 · 作业并发 {concurrency}</p>
        </div>
        <div className="flex items-center gap-1.5">
          <label className="flex h-8 items-center gap-2 rounded border border-gray-200 px-2 text-xs dark:border-gray-700" title="同时运行的渲染作业数；每个作业内的帧多开单独设置">
            <Gauge className="h-3.5 w-3.5 text-gray-500" />
            <span>作业并发</span>
            <HelpAssistant
              title="作业并发与帧多开"
              text={[
                '作业并发表示同时启动多少个独立渲染任务。',
                '帧多开在每个任务内另行设置，表示该任务同时启动多少个 Blender 进程来渲染不同帧。',
                '实际 Blender 进程数量可能接近“作业并发 × 帧多开”。显存紧张或大场景建议先保持 1 开。',
              ]}
              placement="bottom-end"
              width={350}
            />
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
          <p className="mt-1 truncate text-xs text-gray-500">{fileName(job.blendPath)} · {job.frameStart}-{job.frameEnd} · {job.parallelism} 开</p>
        </div>
        <ChevronRight className="mt-1 h-4 w-4 text-gray-400" />
      </div>
      <div className="mt-3 h-1.5 overflow-hidden rounded bg-gray-200 dark:bg-gray-800"><div className={`h-full ${job.status === 'failed' ? 'bg-red-500' : 'bg-blue-500'}`} style={{ width: `${Math.min(100, job.progress)}%` }} /></div>
      <div className="mt-1.5 flex justify-between gap-3 text-[11px] text-gray-500"><span>{job.completedFrames} 完成 · {job.failedFrames} 失败 · {job.skippedFrames} 跳过</span><span>{Math.round(job.progress)}%</span></div>
      {showLivePerformance && <div className="mt-1.5 flex items-center gap-3 text-[10px] tabular-nums text-gray-500"><span className="inline-flex items-center gap-1"><Cpu className="h-3 w-3" />{job.cpuUsage.toFixed(1)}%</span><span className="inline-flex items-center gap-1"><MemoryStick className="h-3 w-3" />{formatMemory(job.memoryBytes)}</span></div>}
    </button>
  );
}

function JobDetailPane({ detail, busy, onAction }: { detail: RenderJobDetail; busy: boolean; onAction: (label: string, command: string, payload?: Record<string, unknown>) => Promise<boolean> }) {
  const { job, frames, logTail } = detail;
  const performanceSamples = detail.performanceSamples || [];
  const showToast = useUiStore((state) => state.showToast);
  const [logExpanded, setLogExpanded] = useState(false);
  const [showPerformance, setShowPerformance] = useState(false);
  const [selectedFrameNumbers, setSelectedFrameNumbers] = useState<Set<number>>(() => new Set());
  const [frameSelectionAnchor, setFrameSelectionAnchor] = useState<number | null>(null);
  const [frameContextMenu, setFrameContextMenu] = useState<FrameContextMenu | null>(null);
  const [frameMarquee, setFrameMarquee] = useState<FrameMarquee | null>(null);
  const [rerenderConfirmation, setRerenderConfirmation] = useState<RerenderConfirmation | null>(null);
  const [skipConfirmation, setSkipConfirmation] = useState<number[] | null>(null);
  const frameListRef = useRef<HTMLDivElement>(null);
  const [previewFrameNumber, setPreviewFrameNumber] = useState<number | null>(null);
  const [showSettingsEditor, setShowSettingsEditor] = useState(false);
  const canPause = ['pending', 'starting', 'running'].includes(job.status);
  const canResume = ['paused', 'failed', 'cancelled'].includes(job.status);
  const canEdit = !['starting', 'running', 'pausing', 'cancelling'].includes(job.status);
  const failedFrames = frames.filter((frame) => frame.status === 'failed').map((frame) => frame.frame);
  const runningFrames = frames.filter((frame) => frame.status === 'running').map((frame) => frame.frame);
  const previewableFrames = useMemo(
    () => frames.filter((frame) => ['completed', 'skipped'].includes(frame.status) && Boolean(frame.outputPath.trim())),
    [frames],
  );
  const previewableFrameNumbers = useMemo(
    () => new Set(previewableFrames.map((frame) => frame.frame)),
    [previewableFrames],
  );
  const skippableFrameNumbers = useMemo(
    () => new Set(frames.filter((frame) => ['pending', 'failed'].includes(frame.status)).map((frame) => frame.frame)),
    [frames],
  );
  const smoothedEta = useSmoothedEta(job, detail.eta);
  const completionEstimate = formatCompletionEstimate(smoothedEta);
  const summaryItems: Array<{ label: string; value: string | number; detail: string }> = [
    { label: '总帧', value: job.totalFrames, detail: '' },
    { label: '完成', value: job.completedFrames, detail: '' },
    { label: '失败', value: job.failedFrames, detail: '' },
    { label: '当前', value: runningFrames.length > 3 ? `${runningFrames[0]} 等 ${runningFrames.length} 帧` : runningFrames.join(', ') || job.currentFrame || '-', detail: runningFrames.join(', ') },
    { label: '多开', value: `${job.parallelism} 开`, detail: '' },
    { label: '预计完成', value: completionEstimate.value, detail: completionEstimate.detail },
  ];

  useEffect(() => {
    setLogExpanded(false);
    setShowPerformance(false);
    setSelectedFrameNumbers(new Set());
    setFrameSelectionAnchor(null);
    setFrameContextMenu(null);
    setFrameMarquee(null);
    setRerenderConfirmation(null);
    setSkipConfirmation(null);
    setPreviewFrameNumber(null);
    setShowSettingsEditor(false);
  }, [job.id]);

  useEffect(() => {
    setSelectedFrameNumbers((current) => {
      const next = new Set([...current].filter((frameNumber) => frames.some((frame) => frame.frame === frameNumber)));
      return next.size === current.size ? current : next;
    });
    if (previewFrameNumber !== null && !previewableFrames.some((frame) => frame.frame === previewFrameNumber)) {
      setPreviewFrameNumber(null);
    }
  }, [frames, previewFrameNumber, previewableFrames]);

  useEffect(() => {
    if (!frameContextMenu) return;
    const closeMenu = (event: PointerEvent) => {
      if ((event.target as HTMLElement).closest('[data-frame-context-menu]')) return;
      setFrameContextMenu(null);
    };
    const handleKeyDown = (event: KeyboardEvent) => { if (event.key === 'Escape') setFrameContextMenu(null); };
    window.addEventListener('pointerdown', closeMenu);
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('pointerdown', closeMenu);
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [frameContextMenu]);

  const selectFrame = useCallback((frameNumber: number, event: React.PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    const frameIndex = frames.findIndex((frame) => frame.frame === frameNumber);
    const additive = event.ctrlKey || event.metaKey;
    if (event.shiftKey && frameSelectionAnchor !== null) {
      const anchorIndex = frames.findIndex((frame) => frame.frame === frameSelectionAnchor);
      if (anchorIndex >= 0 && frameIndex >= 0) {
        const [from, to] = anchorIndex < frameIndex ? [anchorIndex, frameIndex] : [frameIndex, anchorIndex];
        setSelectedFrameNumbers(new Set(frames.slice(from, to + 1).map((frame) => frame.frame)));
      }
      return;
    }
    const nextSelection = additive ? new Set(selectedFrameNumbers) : new Set<number>();
    if (additive && nextSelection.has(frameNumber)) nextSelection.delete(frameNumber);
    else nextSelection.add(frameNumber);
    setSelectedFrameNumbers(nextSelection);
    setFrameSelectionAnchor(frameNumber);
    setFrameContextMenu(null);
    if (!event.shiftKey) {
      setFrameMarquee({
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        currentX: event.clientX,
        currentY: event.clientY,
        baseSelection: additive ? nextSelection : new Set(),
      });
      event.currentTarget.setPointerCapture(event.pointerId);
    }
  }, [frameSelectionAnchor, frames, selectedFrameNumbers]);

  const updateMarquee = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (!frameMarquee || event.pointerId !== frameMarquee.pointerId) return;
    const currentX = event.clientX;
    const currentY = event.clientY;
    setFrameMarquee((current) => current ? { ...current, currentX, currentY } : current);
    if (Math.abs(currentX - frameMarquee.startX) < 4 && Math.abs(currentY - frameMarquee.startY) < 4) return;
    const left = Math.min(frameMarquee.startX, currentX);
    const right = Math.max(frameMarquee.startX, currentX);
    const top = Math.min(frameMarquee.startY, currentY);
    const bottom = Math.max(frameMarquee.startY, currentY);
    const selected = new Set(frameMarquee.baseSelection);
    frameListRef.current?.querySelectorAll<HTMLElement>('[data-render-frame]').forEach((element) => {
      const bounds = element.getBoundingClientRect();
      if (bounds.left < right && bounds.right > left && bounds.top < bottom && bounds.bottom > top) {
        selected.add(Number(element.dataset.renderFrame));
      }
    });
    setSelectedFrameNumbers(selected);
  }, [frameMarquee]);

  const finishMarquee = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (!frameMarquee || event.pointerId !== frameMarquee.pointerId) return;
    setFrameMarquee(null);
  }, [frameMarquee]);

  const openFrameContextMenu = useCallback((frameNumber: number, event: React.MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    const frameNumbers = selectedFrameNumbers.has(frameNumber) ? [...selectedFrameNumbers] : [frameNumber];
    if (!selectedFrameNumbers.has(frameNumber)) {
      setSelectedFrameNumbers(new Set(frameNumbers));
      setFrameSelectionAnchor(frameNumber);
    }
    setFrameContextMenu({
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - 210)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - 220)),
      frameNumbers,
    });
  }, [selectedFrameNumbers]);

  const requestRerenderSelectedFrames = useCallback((frameNumbers: number[]) => {
    if (!frameNumbers.length) return;
    setFrameContextMenu(null);
    setRerenderConfirmation({ frameNumbers });
  }, []);

  const confirmRerenderSelectedFrames = useCallback(async () => {
    const frameNumbers = rerenderConfirmation?.frameNumbers;
    if (!frameNumbers?.length) return;
    setRerenderConfirmation(null);
    await onAction('重新渲染所选帧', 'retry_render_frames', { frames: frameNumbers });
  }, [onAction, rerenderConfirmation]);

  const requestSkipSelectedFrames = useCallback((frameNumbers: number[]) => {
    const skippableFrames = frameNumbers.filter((frameNumber) => skippableFrameNumbers.has(frameNumber));
    if (!skippableFrames.length) return;
    setFrameContextMenu(null);
    setSkipConfirmation(skippableFrames);
  }, [skippableFrameNumbers]);

  const confirmSkipSelectedFrames = useCallback(async () => {
    if (!skipConfirmation?.length) return;
    const frameNumbers = skipConfirmation;
    setSkipConfirmation(null);
    await onAction('跳过所选帧', 'skip_render_frames', { frames: frameNumbers });
  }, [onAction, skipConfirmation]);

  const copySelectedPaths = useCallback(async (frameNumbers: number[]) => {
    const paths = frames.filter((frame) => frameNumbers.includes(frame.frame)).map((frame) => frame.outputPath).filter(Boolean);
    if (!paths.length) return;
    try {
      await navigator.clipboard.writeText(paths.join('\n'));
      showToast({ title: '已复制输出路径', message: `${paths.length} 个路径已复制到剪贴板`, tone: 'success' });
    } catch (error) {
      showToast({ title: '复制输出路径失败', message: String(error), tone: 'error' });
    }
    setFrameContextMenu(null);
  }, [frames, showToast]);

  const marqueeStyle = useMemo(() => {
    if (!frameMarquee || !frameListRef.current) return null;
    const bounds = frameListRef.current.getBoundingClientRect();
    const left = Math.min(frameMarquee.startX, frameMarquee.currentX) - bounds.left + frameListRef.current.scrollLeft;
    const top = Math.min(frameMarquee.startY, frameMarquee.currentY) - bounds.top + frameListRef.current.scrollTop;
    return {
      left,
      top,
      width: Math.abs(frameMarquee.currentX - frameMarquee.startX),
      height: Math.abs(frameMarquee.currentY - frameMarquee.startY),
    };
  }, [frameMarquee]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-gray-200 px-3 py-2 dark:border-gray-800">
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1"><div className="flex items-center gap-2"><h3 className="truncate text-sm font-semibold">{job.name}</h3><StatusBadge status={job.status} /><HelpAssistant title="管理这个任务" text={['单击帧行可选中；按 Ctrl/Cmd 多选，按 Shift 选择连续范围，也可拖拽框选。右键显示批量操作。', '双击已完成帧可预览；铅笔按钮用于修改场景、帧范围、帧多开、分辨率和格式。运行中的任务需先暂停。', '右键可重新渲染或跳过所选帧，均需先暂停并确认；跳过只标记等待中/失败帧，不删除已有输出。']} placement="bottom-start" width={340} /></div><p className="mt-0.5 truncate text-[11px] text-gray-500" title={job.outputDir}>{job.outputDir}</p></div>
          <div className="flex shrink-0 items-center gap-1">
          {canPause && <IconAction title="暂停" icon={<CirclePause />} disabled={busy} onClick={() => onAction('暂停作业', 'pause_render_job')} />}
          {canResume && <IconAction title="继续" icon={<CirclePlay />} disabled={busy} onClick={() => onAction('继续作业', 'resume_render_job')} />}
          {!['completed','cancelled'].includes(job.status) && <IconAction title="取消" icon={<Square />} disabled={busy} onClick={() => onAction('取消作业', 'cancel_render_job')} />}
          {failedFrames.length > 0 && <IconAction title="重试失败帧" icon={<RotateCcw />} disabled={busy} onClick={() => onAction('重试失败帧', 'retry_render_frames', { frames: failedFrames })} />}
          <IconAction title={canEdit ? '编辑任务设置' : '请先暂停任务再修改设置'} icon={<Pencil />} disabled={busy || !canEdit} onClick={() => setShowSettingsEditor(true)} />
          <IconAction title="打开输出目录" icon={<FolderOpen />} disabled={busy} onClick={() => onAction('打开输出目录', 'open_render_output', { path: job.outputDir })} />
          {!['running','pausing','cancelling'].includes(job.status) && <IconAction title={job.archived ? '取消归档' : '归档'} icon={<Archive />} disabled={busy} onClick={() => onAction('归档作业', 'archive_render_job', { archived: !job.archived })} />}
          </div>
        </div>
        {job.error && <p className="mt-1.5 flex items-start gap-1.5 text-[11px] text-red-600"><AlertCircle className="mt-0.5 h-3 w-3 shrink-0" />{job.error}</p>}
      </div>
      <div className="grid grid-cols-6 border-b border-gray-200 dark:border-gray-800">
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
      <div ref={frameListRef} className="relative min-h-0 flex-1 overflow-auto" onPointerMove={updateMarquee} onPointerUp={finishMarquee} onPointerCancel={finishMarquee}>
        <div className="sticky top-0 z-10 grid grid-cols-[64px_82px_64px_1fr] bg-gray-50 px-3 py-1.5 text-[10px] font-medium text-gray-500 dark:bg-gray-900"><span>帧{selectedFrameNumbers.size > 0 ? ` · 已选 ${selectedFrameNumbers.size}` : ''}</span><span>状态</span><span>耗时</span><span>输出</span></div>
        {frames.map((frame) => {
          const previewable = previewableFrameNumbers.has(frame.frame);
          return (
            <FrameRow
              key={frame.frame}
              frame={frame}
              selected={selectedFrameNumbers.has(frame.frame)}
              previewable={previewable}
              onPointerDown={(event) => selectFrame(frame.frame, event)}
              onContextMenu={(event) => openFrameContextMenu(frame.frame, event)}
              onPreview={() => {
                setSelectedFrameNumbers(new Set([frame.frame]));
                setFrameSelectionAnchor(frame.frame);
                if (previewable) setPreviewFrameNumber(frame.frame);
              }}
            />
          );
        })}
        {marqueeStyle && <div aria-hidden="true" className="pointer-events-none absolute z-20 border border-blue-500 bg-blue-500/15" style={marqueeStyle} />}
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
      {showSettingsEditor && (
        <EditRenderJobDialog
          detail={detail}
          onClose={() => setShowSettingsEditor(false)}
          onSave={(request) => onAction('修改任务设置', 'update_render_job', { request })}
        />
      )}
      {previewFrameNumber !== null && (
        <RenderFramePreview
          jobName={job.name}
          frames={previewableFrames}
          currentFrameNumber={previewFrameNumber}
          onFrameChange={(frameNumber) => {
            setPreviewFrameNumber(frameNumber);
            setSelectedFrameNumbers(new Set([frameNumber]));
            setFrameSelectionAnchor(frameNumber);
          }}
          onClose={() => setPreviewFrameNumber(null)}
        />
      )}
      {frameContextMenu && (
        <div data-frame-context-menu role="menu" className="fixed z-[120] w-52 overflow-hidden rounded border border-gray-200 bg-white py-1 shadow-xl dark:border-gray-700 dark:bg-gray-900" style={{ left: frameContextMenu.x, top: frameContextMenu.y }}>
          <div className="border-b border-gray-100 px-3 py-1.5 text-[10px] text-gray-500 dark:border-gray-800">已选 {frameContextMenu.frameNumbers.length} 帧</div>
          <button type="button" role="menuitem" disabled={!frameContextMenu.frameNumbers.some((frameNumber) => previewableFrameNumbers.has(frameNumber))} onClick={() => {
            const frame = previewableFrames.find((item) => frameContextMenu.frameNumbers.includes(item.frame));
            if (frame) setPreviewFrameNumber(frame.frame);
            setFrameContextMenu(null);
          }} className="flex h-8 w-full items-center gap-2 px-3 text-left text-xs hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-gray-800"><ImageIcon className="h-3.5 w-3.5" />预览所选帧</button>
          <button type="button" role="menuitem" disabled={busy || !canEdit} title={canEdit ? '强制重新渲染所选帧' : '请先暂停任务再重新渲染'} onClick={() => requestRerenderSelectedFrames(frameContextMenu.frameNumbers)} className="flex h-8 w-full items-center gap-2 px-3 text-left text-xs hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-gray-800"><RotateCcw className="h-3.5 w-3.5" />重新渲染所选帧</button>
          <button type="button" role="menuitem" disabled={busy || !canEdit || !frameContextMenu.frameNumbers.some((frameNumber) => skippableFrameNumbers.has(frameNumber))} title={canEdit ? '跳过等待中或失败的所选帧' : '请先暂停任务再跳过帧'} onClick={() => requestSkipSelectedFrames(frameContextMenu.frameNumbers)} className="flex h-8 w-full items-center gap-2 px-3 text-left text-xs hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-gray-800"><CirclePause className="h-3.5 w-3.5" />跳过所选帧</button>
          <button type="button" role="menuitem" onClick={() => void onAction('打开输出目录', 'open_render_output', { path: job.outputDir }).then(() => setFrameContextMenu(null))} className="flex h-8 w-full items-center gap-2 px-3 text-left text-xs hover:bg-gray-100 dark:hover:bg-gray-800"><FolderOpen className="h-3.5 w-3.5" />打开输出目录</button>
          <button type="button" role="menuitem" onClick={() => void copySelectedPaths(frameContextMenu.frameNumbers)} className="flex h-8 w-full items-center gap-2 px-3 text-left text-xs hover:bg-gray-100 dark:hover:bg-gray-800"><Copy className="h-3.5 w-3.5" />复制输出路径</button>
          <button type="button" role="menuitem" onClick={() => { setSelectedFrameNumbers(new Set()); setFrameSelectionAnchor(null); setFrameContextMenu(null); }} className="flex h-8 w-full items-center gap-2 px-3 text-left text-xs text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"><X className="h-3.5 w-3.5" />取消选择</button>
        </div>
      )}
      {rerenderConfirmation && (
        <div className="fixed inset-0 z-[130] flex items-center justify-center bg-black/45 p-4" role="dialog" aria-modal="true" aria-label="确认重新渲染">
          <div className="w-full max-w-md overflow-hidden rounded-md border border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-950">
            <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-3 dark:border-gray-800"><AlertCircle className="h-5 w-5 text-amber-500" /><div><h4 className="text-sm font-semibold">确认重新渲染</h4><p className="mt-0.5 text-xs text-gray-500">将强制重新渲染所选的 {rerenderConfirmation.frameNumbers.length} 帧，现有输出会被覆盖。</p></div></div>
            <div className="flex justify-end gap-2 px-4 py-3"><button type="button" onClick={() => setRerenderConfirmation(null)} className="h-8 rounded px-3 text-xs hover:bg-gray-100 dark:hover:bg-gray-800">取消</button><button type="button" onClick={() => void confirmRerenderSelectedFrames()} className="h-8 rounded bg-red-600 px-3 text-xs font-medium text-white hover:bg-red-500">确认重新渲染</button></div>
          </div>
        </div>
      )}
      {skipConfirmation && (
        <div className="fixed inset-0 z-[130] flex items-center justify-center bg-black/45 p-4" role="dialog" aria-modal="true" aria-label="确认跳过帧">
          <div className="w-full max-w-md overflow-hidden rounded-md border border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-950">
            <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-3 dark:border-gray-800"><CirclePause className="h-5 w-5 text-amber-500" /><div><h4 className="text-sm font-semibold">确认跳过帧</h4><p className="mt-0.5 text-xs text-gray-500">将把所选的 {skipConfirmation.length} 帧标记为已跳过，不会删除已有输出文件。</p></div></div>
            <div className="flex justify-end gap-2 px-4 py-3"><button type="button" onClick={() => setSkipConfirmation(null)} className="h-8 rounded px-3 text-xs hover:bg-gray-100 dark:hover:bg-gray-800">取消</button><button type="button" onClick={() => void confirmSkipSelectedFrames()} className="h-8 rounded bg-amber-600 px-3 text-xs font-medium text-white hover:bg-amber-500">确认跳过</button></div>
          </div>
        </div>
      )}
    </div>
  );
}

function EditRenderJobDialog({ detail, onClose, onSave }: { detail: RenderJobDetail; onClose: () => void; onSave: (request: UpdateRenderJobRequest) => Promise<boolean> }) {
  const { job, settings } = detail;
  const showToast = useUiStore((state) => state.showToast);
  const [form, setForm] = useState<UpdateRenderJobRequest>(() => ({ ...settings }));
  const [source, setSource] = useState<RenderSourceInfo | null>(null);
  const [inspecting, setInspecting] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void invoke<RenderSourceInfo[]>('inspect_render_sources', {
      blenderPath: job.blenderPath,
      sources: [{ path: job.blendPath }],
    }).then((items) => {
      if (cancelled) return;
      const next = items[0] || null;
      setSource(next);
      if (next?.error) {
        showToast({ title: '读取 Blender 场景失败', message: next.error, tone: 'error' });
      }
    }).catch((error) => {
      if (!cancelled) showToast({ title: '读取 Blender 场景失败', message: String(error), tone: 'error' });
    }).finally(() => {
      if (!cancelled) setInspecting(false);
    });
    return () => { cancelled = true; };
  }, [job.blendPath, job.blenderPath, showToast]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => { if (event.key === 'Escape' && !saving) onClose(); };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, saving]);

  const scenes = source?.scenes || [];
  const sceneOptions = scenes.some((scene) => scene.name === form.sceneName)
    ? scenes
    : [{ name: form.sceneName, frameStart: form.frameStart, frameEnd: form.frameEnd, resolutionX: 0, resolutionY: 0, fps: 0, engine: form.engine || '', outputFormat: form.outputFormat }, ...scenes];
  const selectedScene = sceneOptions.find((scene) => scene.name === form.sceneName);
  const imageSettingsChanged = form.sceneName !== settings.sceneName
    || form.resolutionPercentage !== settings.resolutionPercentage
    || form.engine !== settings.engine
    || form.outputFormat !== settings.outputFormat;
  const frameLayoutChanged = form.frameStart !== settings.frameStart
    || form.frameEnd !== settings.frameEnd
    || form.frameStep !== settings.frameStep;
  const valid = form.sceneName.trim()
    && form.frameEnd >= form.frameStart
    && form.frameStep >= 1
    && form.parallelism >= 1
    && form.parallelism <= 8
    && form.resolutionPercentage >= 1
    && form.resolutionPercentage <= 100;

  const save = async () => {
    if (!valid || saving) return;
    if (imageSettingsChanged && job.completedFrames > 0
      && !confirm('画面设置已修改，范围内已完成的帧会按新设置重新渲染；仅修改帧范围不会重渲染已有结果。确定继续吗？')) return;
    setSaving(true);
    try {
      if (await onSave(form)) onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/50 p-4" onMouseDown={(event) => { if (event.target === event.currentTarget && !saving) onClose(); }}>
      <div role="dialog" aria-modal="true" aria-label="编辑渲染任务设置" className="flex w-full max-w-3xl flex-col overflow-hidden rounded-md border border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-950">
        <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-3 dark:border-gray-800">
          <Pencil className="h-4 w-4 text-blue-600" />
          <div className="min-w-0 flex-1"><div className="flex items-center gap-1.5"><h3 className="truncate text-sm font-semibold">编辑任务设置</h3><HelpAssistant title="修改已有任务" text={['保存后会自动刷新任务的帧列表。', '只调整帧范围、步长或帧多开时，会保留范围内已有结果，只把缺失帧按帧号加入队列。', '修改场景、渲染引擎、分辨率或格式会改变画面内容，因此会重新渲染范围内的帧；旧输出文件不会自动删除。']} placement="bottom-start" width={340} /></div><p className="truncate text-[11px] text-gray-500" title={job.blendPath}>{fileName(job.blendPath)}</p></div>
          <button type="button" onClick={onClose} disabled={saving} title="关闭" className="flex h-8 w-8 items-center justify-center rounded hover:bg-gray-100 disabled:opacity-40 dark:hover:bg-gray-800"><X className="h-4 w-4" /></button>
        </div>
        <div className="p-4">
          <div className="grid grid-cols-[minmax(150px,1fr)_74px_74px_64px_82px_94px_100px] gap-2 max-[720px]:grid-cols-3">
            <Field label={<span className="inline-flex items-center gap-1">场景{inspecting && <LoaderCircle className="h-3 w-3 animate-spin" />}<HelpAssistant title="切换场景" text={['列表由当前 .blend 文件读取。切换场景会带入该场景默认的起止帧、渲染引擎和格式。', '保存后会把现有帧重新排队。']} placement="top-start" /></span>}>
              <select
                value={form.sceneName}
                onChange={(event) => {
                  const scene = sceneOptions.find((item) => item.name === event.target.value);
                  setForm((current) => ({
                    ...current,
                    sceneName: event.target.value,
                    frameStart: scene?.frameStart ?? current.frameStart,
                    frameEnd: scene?.frameEnd ?? current.frameEnd,
                    engine: scene?.engine || null,
                    outputFormat: scene?.outputFormat || current.outputFormat,
                  }));
                }}
              >
                {sceneOptions.map((scene) => <option key={scene.name} value={scene.name}>{scene.name}</option>)}
              </select>
            </Field>
            <Field label={<span className="inline-flex items-center gap-1">起始<HelpAssistant title="帧范围与步长" text={['起始和结束决定要渲染的帧号；步长为 2 时会渲染 1、3、5 等帧。', '调整范围会保留范围内已完成的帧，只把缺失帧从小到大补入队列；移出范围的记录会移除，但已有输出文件会保留。']} placement="top-start" /></span>}><input type="number" value={form.frameStart} onChange={(event) => setForm((current) => ({ ...current, frameStart: Number(event.target.value) }))} /></Field>
            <Field label="结束"><input type="number" value={form.frameEnd} onChange={(event) => setForm((current) => ({ ...current, frameEnd: Number(event.target.value) }))} /></Field>
            <Field label="步长"><input type="number" min={1} value={form.frameStep} onChange={(event) => setForm((current) => ({ ...current, frameStep: Math.max(1, Number(event.target.value)) }))} /></Field>
            <Field label={<span className="inline-flex items-center gap-1">帧多开<HelpAssistant title="帧多开" text={["为当前任务同时启动多个 Blender 进程，每个进程领取不同帧。", "大场景或显存紧张时建议设为 1。"]} placement="top" /></span>}><select value={form.parallelism} onChange={(event) => setForm((current) => ({ ...current, parallelism: Number(event.target.value) }))}>{[1,2,3,4,5,6,7,8].map((value) => <option key={value} value={value}>{value} 开</option>)}</select></Field>
            <Field label={<span className="inline-flex items-center gap-1">分辨率 %<HelpAssistant title="分辨率比例" text={['按场景原始分辨率的百分比渲染。100% 为正式输出，较低比例可用于快速预览。', '改变比例会重新渲染已有帧。']} placement="top" /></span>}><input type="number" min={1} max={100} value={form.resolutionPercentage} onChange={(event) => setForm((current) => ({ ...current, resolutionPercentage: Number(event.target.value) }))} /></Field>
            <Field label={<span className="inline-flex items-center gap-1">格式<HelpAssistant title="输出格式" text={['PNG 适合常规交付；JPEG 文件更小；OPEN_EXR 常用于后期合成。', '切换格式会生成新的输出扩展名，并重新渲染受影响帧。']} placement="top-end" /></span>}><select value={form.outputFormat} onChange={(event) => setForm((current) => ({ ...current, outputFormat: event.target.value }))}><option>PNG</option><option>JPEG</option><option>OPEN_EXR</option><option>TIFF</option><option>WEBP</option></select></Field>
          </div>
          <div className="mt-2 flex min-h-5 items-center gap-2 text-[11px] text-gray-500">
            {selectedScene && selectedScene.resolutionX > 0 && <span>{selectedScene.resolutionX} × {selectedScene.resolutionY} · {selectedScene.fps} fps · {form.engine || '-'}</span>}
            {imageSettingsChanged && <span className="ml-auto text-amber-600">画面设置已变化，现有帧将重新排队</span>}
            {!imageSettingsChanged && frameLayoutChanged && <span className="ml-auto text-blue-600">保留已有结果，补充新范围内缺失帧</span>}
          </div>
        </div>
        <div className="flex min-h-[58px] items-center justify-end gap-2 border-t border-gray-200 px-4 dark:border-gray-800">
          <button type="button" onClick={onClose} disabled={saving} className="h-9 rounded px-4 text-xs disabled:opacity-40">取消</button>
          <button type="button" onClick={() => void save()} disabled={!valid || saving} className="flex h-9 items-center gap-1.5 rounded bg-gray-900 px-4 text-xs font-medium text-white disabled:opacity-40 dark:bg-white dark:text-gray-900">{saving ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}保存设置</button>
        </div>
      </div>
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

function FrameRow({ frame, selected, previewable, onPointerDown, onContextMenu, onPreview }: { frame: RenderFrame; selected: boolean; previewable: boolean; onPointerDown: (event: React.PointerEvent<HTMLButtonElement>) => void; onContextMenu: (event: React.MouseEvent<HTMLButtonElement>) => void; onPreview: () => void }) {
  return (
    <button
      type="button"
      data-render-frame={frame.frame}
      aria-pressed={selected}
      onPointerDown={onPointerDown}
      onContextMenu={onContextMenu}
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
  parallelism: number;
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
    if (typeof settings.parallelism === 'number') setJobs((items) => items.map((item) => ({ ...item, parallelism: Math.min(8, Math.max(1, settings.parallelism as number)) })));
  };
  const savePreset = async () => {
    if (!presetName.trim()) return;
    await invoke('save_render_preset', { projectPath, name: presetName.trim(), scope: presetScope, settings: { outputRoot, forceOverwrite, maxRetries, outputFormat: jobs[0]?.outputFormat || 'PNG', resolutionPercentage: jobs[0]?.resolutionPercentage || 100, parallelism: jobs[0]?.parallelism || 1 } });
    setPresetName('');
    showToast({ title: '预设已保存', message: presetScope === 'global' ? '所有项目均可使用' : '仅当前项目使用', tone: 'success' });
  };
  const submit = async () => {
    const validJobs = jobs.filter((job) => !job.error && job.sceneName && job.frameEnd >= job.frameStart);
    if (!namePrefix.trim() || !blenderPath || !validJobs.length) return;
    setSubmitting(true);
    try {
      const request: CreateRenderBatchRequest = { name: `${namePrefix.trim()} ${batchTimestamp}`, blenderPath, outputRoot: outputRoot || null, preHook: preHook || null, postHook: postHook || null, forceOverwrite, maxRetries, jobs: validJobs.map((job) => ({ blendPath: job.path, sceneName: job.sceneName, frameStart: job.frameStart, frameEnd: job.frameEnd, frameStep: job.frameStep, parallelism: job.parallelism, resolutionPercentage: job.resolutionPercentage, engine: job.engine || null, outputFormat: job.outputFormat })) };
      await invoke('create_render_batch', { projectPath, request });
      showToast({ title: '渲染批次已加入队列', message: `${validJobs.length} 个作业`, tone: 'success' });
      await onCreated();
    } catch (error) { showToast({ title: '创建渲染批次失败', message: String(error), tone: 'error' }); }
    finally { setSubmitting(false); }
  };

  return (
    <div className="fixed inset-0 z-[80] flex justify-end bg-black/45" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <div className="flex h-full w-[760px] max-w-[96vw] flex-col bg-white shadow-2xl dark:bg-gray-950">
        <div className="flex min-h-[58px] items-center justify-between border-b border-gray-200 px-5 dark:border-gray-800"><div><div className="flex items-center gap-1.5"><h3 className="text-base font-semibold">新建渲染批次</h3><HelpAssistant title="创建渲染批次" text={['1. 选择已在设置中登记的 Blender 版本。', '2. 点击“添加 .blend”，从当前项目或系统文件选择器加入一个或多个文件。', '3. 为每个文件设置场景、帧范围、帧多开、分辨率和格式，最后点击“加入队列”。', '这些设置只作用于渲染进程内存，不会保存或改写源 .blend 文件。']} placement="bottom-start" width={350} /></div><p className="text-xs text-gray-500">设置仅在 Blender 内存中生效，不修改源文件</p></div><button title="关闭" className="h-8 w-8 p-0" onClick={onClose}><X className="mx-auto h-4 w-4" /></button></div>
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
            <div className="grid grid-cols-2 gap-3"><Field label={<span className="inline-flex items-center gap-1">失败重试<HelpAssistant title="失败重试" text={['单帧失败后会等待再尝试，最多按这里的次数重试。', '适合临时资源加载失败；场景或插件报错时应先修复错误再继续。']} placement="top-start" /></span>}><input type="number" min={0} max={10} value={maxRetries} onChange={(e) => setMaxRetries(Number(e.target.value))} /></Field><label className="mt-6 flex h-9 items-center gap-1.5 text-xs"><input type="checkbox" checked={forceOverwrite} onChange={(e) => setForceOverwrite(e.target.checked)} className="h-4 w-4" />强制覆盖已有帧<HelpAssistant title="强制覆盖" text={['关闭时，已有有效图片会被跳过，适合断点续渲。', '开启后会重新写入同名输出文件，适合正式重渲。']} placement="top-end" /></label></div>
            <PathField label="前置脚本（PMC Python）" value={preHook} onChange={setPreHook} onBrowse={() => setFilePickerTarget('preHook')} />
            <PathField label="后置脚本（PMC Python）" value={postHook} onChange={setPostHook} onBrowse={() => setFilePickerTarget('postHook')} />
          </div>
          <div className="mt-5 flex items-center justify-between border-b border-gray-200 pb-2 dark:border-gray-800"><div><div className="flex items-center gap-1.5"><h4 className="text-sm font-semibold">源文件与场景</h4><HelpAssistant title="添加源文件" text={['可以一次选择多个 .blend。PMC 会用上方 Blender 版本读取每个文件的场景信息。', '每个 .blend 只选择一个场景，并在加入队列后生成一个独立任务；不同文件可设不同帧多开数量。']} placement="top-start" width={330} /></div><p className="text-xs text-gray-500">每个文件选择一个场景并生成独立作业</p></div><button disabled={inspecting || !blenderPath} onClick={() => setFilePickerTarget('blend')} className="flex h-8 items-center gap-1.5 rounded border border-gray-300 px-3 text-xs disabled:opacity-50 dark:border-gray-700">{inspecting ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}添加 .blend</button></div>
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
  return { path: source.path, scenes: source.scenes, sceneName: scene?.name || '', frameStart: scene?.frameStart ?? 1, frameEnd: scene?.frameEnd ?? 250, frameStep: 1, parallelism: 1, resolutionPercentage: 100, engine: scene?.engine || '', outputFormat: scene?.outputFormat || 'PNG', error: source.error };
}

function EditableJobRow({ job, onChange, onRemove }: { job: EditableJob; onChange: (patch: Partial<EditableJob>) => void; onRemove: () => void }) {
  const scene = job.scenes.find((item) => item.name === job.sceneName);
  return <div className="border-b border-gray-100 py-3 dark:border-gray-800"><div className="flex items-center gap-2"><span className="min-w-0 flex-1 truncate text-xs font-medium" title={job.path}>{fileName(job.path)}</span>{job.error && <span className="text-xs text-red-600">读取失败</span>}<button title="移除" onClick={onRemove} className="h-7 w-7 p-0 text-gray-400 hover:text-red-600"><X className="mx-auto h-3.5 w-3.5" /></button></div>{job.error ? <p className="mt-1 text-xs text-red-600">{job.error}</p> : <div className="mt-2 grid grid-cols-[minmax(116px,1fr)_68px_68px_56px_72px_88px_90px] gap-2 max-[680px]:grid-cols-3"><Field label="场景"><select value={job.sceneName} onChange={(e) => { const next=job.scenes.find((item)=>item.name===e.target.value); onChange({ sceneName:e.target.value, frameStart:next?.frameStart ?? job.frameStart, frameEnd:next?.frameEnd ?? job.frameEnd, engine:next?.engine ?? job.engine, outputFormat:next?.outputFormat ?? job.outputFormat }); }}>{job.scenes.map((item)=><option key={item.name}>{item.name}</option>)}</select></Field><Field label="起始"><input type="number" value={job.frameStart} onChange={(e)=>onChange({frameStart:Number(e.target.value)})} /></Field><Field label="结束"><input type="number" value={job.frameEnd} onChange={(e)=>onChange({frameEnd:Number(e.target.value)})} /></Field><Field label="步长"><input type="number" min={1} value={job.frameStep} onChange={(e)=>onChange({frameStep:Math.max(1,Number(e.target.value))})} /></Field><Field label={<span className="inline-flex items-center gap-1">帧多开<HelpAssistant title="帧多开" text={["为当前作业同时启动多个 Blender 进程，每个进程领取不同帧。", "大场景或显存紧张时建议设为 1；较小场景可逐步增加。"]} placement="top" /></span>}><select value={job.parallelism} onChange={(e)=>onChange({parallelism:Number(e.target.value)})}>{[1,2,3,4,5,6,7,8].map((value)=><option key={value} value={value}>{value} 开</option>)}</select></Field><Field label={<span className="inline-flex items-center gap-1">分辨率 %<HelpAssistant title="渲染分辨率比例" text={["按场景原始分辨率的百分比渲染。", '降低比例可以加快预览渲染，正式输出通常使用 100%。']} images={[{ src: '/help_media/渲染像素比.jpg', alt: '不同渲染清晰度的对比' }]} placement="top" /></span>}><input type="number" min={1} max={100} value={job.resolutionPercentage} onChange={(e)=>onChange({resolutionPercentage:Number(e.target.value)})} /></Field><Field label="格式"><select value={job.outputFormat} onChange={(e)=>onChange({outputFormat:e.target.value})}><option>PNG</option><option>JPEG</option><option>OPEN_EXR</option><option>TIFF</option><option>WEBP</option></select></Field><div className="col-span-full text-[10px] text-gray-500">{scene?.resolutionX} × {scene?.resolutionY} · {scene?.fps} fps · {job.engine}</div></div>}</div>;
}

function Field({ label, children }: { label: React.ReactNode; children: React.ReactNode }) {
  return <label className="block min-w-0"><span className="mb-1 flex h-4 min-w-0 items-center text-[11px] font-medium text-gray-600 dark:text-gray-400">{label}</span><span className="block [&>input]:h-9 [&>input]:w-full [&>input]:rounded [&>input]:border [&>input]:border-gray-300 [&>input]:bg-transparent [&>input]:px-2 [&>input]:text-xs [&>select]:h-9 [&>select]:w-full [&>select]:rounded [&>select]:border [&>select]:border-gray-300 [&>select]:bg-transparent [&>select]:px-2 [&>select]:text-xs dark:[&>input]:border-gray-700 dark:[&>select]:border-gray-700">{children}</span></label>;
}

function PathField({ label, value, onChange, onBrowse, required = false }: { label: string; value: string; onChange: (value: string) => void; onBrowse: () => void; required?: boolean }) {
  return <Field label={`${label}${required ? ' *' : ''}`}><span className="flex h-9 overflow-hidden rounded border border-gray-300 dark:border-gray-700"><input value={value} onChange={(e) => onChange(e.target.value)} className="min-w-0 flex-1 bg-transparent px-2 text-xs outline-none" /><button title="浏览" onClick={onBrowse} className="h-9 w-9 shrink-0 border-l border-gray-300 p-0 dark:border-gray-700"><FolderOpen className="mx-auto h-4 w-4" /></button></span></Field>;
}
