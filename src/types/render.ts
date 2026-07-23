export type RenderJobStatus =
  | 'pending'
  | 'starting'
  | 'running'
  | 'pausing'
  | 'paused'
  | 'cancelling'
  | 'cancelled'
  | 'completed'
  | 'failed'
  | 'attention';

export type RenderExecutionMode = 'persistent' | 'isolated';
export type RenderFrameOrderMode = 'dynamic' | 'strict';

export interface RenderSceneInfo {
  name: string;
  frameStart: number;
  frameEnd: number;
  resolutionX: number;
  resolutionY: number;
  fps: number;
  engine: string;
  outputFormat: string;
}

export interface RenderSourceInfo {
  path: string;
  scenes: RenderSceneInfo[];
  error: string | null;
}

export interface RenderJob {
  id: string;
  batchId: string;
  projectPath: string;
  name: string;
  blendPath: string;
  sceneName: string;
  status: RenderJobStatus;
  frameStart: number;
  frameEnd: number;
  frameStep: number;
  parallelism: number;
  effectiveParallelism: number;
  readyWorkers: number;
  executionMode: RenderExecutionMode;
  frameOrderMode: RenderFrameOrderMode;
  totalFrames: number;
  completedFrames: number;
  failedFrames: number;
  skippedFrames: number;
  currentFrame: number | null;
  progress: number;
  outputDir: string;
  blenderPath: string;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
  error: string | null;
  archived: boolean;
  cpuUsage: number;
  memoryBytes: number;
  peakCpuUsage: number;
  peakMemoryBytes: number;
  performanceUpdatedAt: number | null;
  position: number;
  batchName: string;
  batchStatus: string;
  batchPosition: number;
  attentionCode: string | null;
}

export interface RenderFrame {
  jobId: string;
  frame: number;
  status: string;
  attempts: number;
  outputPath: string;
  error: string | null;
  durationMs: number | null;
  renderDurationMs: number | null;
  workerId: string | null;
  claimToken: string | null;
  updatedAt: number;
}

export interface RenderWorkerState {
  workerId: string;
  ordinal: number;
  pid: number | null;
  state: 'starting' | 'ready' | 'rendering' | 'failed' | 'stopped' | string;
  currentFrame: number | null;
  startupMs: number | null;
  error: string | null;
  updatedAt: number;
}

export interface RenderStartupStats {
  requestedWorkers: number;
  readyWorkers: number;
  averageStartupMs: number | null;
}

export interface RenderEta {
  status: 'calibrating' | 'estimating' | 'paused' | 'unavailable' | 'completed' | string;
  estimatedFinishAt: number | null;
  remainingMs: number | null;
  sampleCount: number;
  confidence: 'none' | 'low' | 'medium' | 'high' | string;
}

export interface RenderPerformanceSample {
  sampledAt: number;
  cpuUsage: number;
  memoryBytes: number;
}

export interface RenderJobDetail {
  job: RenderJob;
  settings: RenderJobSettings;
  frames: RenderFrame[];
  logTail: string[];
  performanceSamples: RenderPerformanceSample[];
  eta: RenderEta;
  workers: RenderWorkerState[];
  startup: RenderStartupStats;
}

export interface RenderJobSettings {
  sceneName: string;
  frameStart: number;
  frameEnd: number;
  frameStep: number;
  parallelism: number;
  executionMode: RenderExecutionMode;
  frameOrderMode: RenderFrameOrderMode;
  resolutionPercentage: number;
  engine: string | null;
  outputFormat: string;
}

export type UpdateRenderJobRequest = RenderJobSettings;

export interface RenderSchedulerSettings {
  concurrency: number;
  maxBlenderProcesses: number;
}

export interface RenderPreset {
  id: string;
  name: string;
  scope: 'project' | 'global' | string;
  settings: Record<string, unknown>;
  createdAt: number;
  updatedAt: number;
}

export interface CreateRenderBatchRequest {
  name: string;
  blenderPath: string;
  outputRoot?: string | null;
  preHook?: string | null;
  postHook?: string | null;
  forceOverwrite: boolean;
  maxRetries: number;
  jobs: Array<{
    blendPath: string;
    sceneName: string;
    frameStart: number;
    frameEnd: number;
    frameStep: number;
    parallelism: number;
    executionMode: RenderExecutionMode;
    frameOrderMode: RenderFrameOrderMode;
    resolutionX?: number | null;
    resolutionY?: number | null;
    resolutionPercentage?: number | null;
    engine?: string | null;
    outputFormat?: string | null;
  }>;
}

export type RenderVideoPackageFormat = 'mp4' | 'mov' | 'webm';

export interface RenderBatchPackageRequest {
  fps: number;
  format: RenderVideoPackageFormat;
  ffmpegPath: string | null;
}

export interface RenderBatchPackageOutput {
  jobId: string;
  jobName: string;
  outputPath: string;
  /** Frames with no readable image on disk; the generated video uses black placeholders. */
  missingFrames: number[];
}

export interface RenderBatchPackageResult {
  outputDir: string;
  outputs: RenderBatchPackageOutput[];
}
