export type RenderJobStatus =
  | 'pending'
  | 'starting'
  | 'running'
  | 'pausing'
  | 'paused'
  | 'cancelling'
  | 'cancelled'
  | 'completed'
  | 'failed';

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
}

export interface RenderFrame {
  jobId: string;
  frame: number;
  status: string;
  attempts: number;
  outputPath: string;
  error: string | null;
  durationMs: number | null;
}

export interface RenderJobDetail {
  job: RenderJob;
  frames: RenderFrame[];
  logTail: string[];
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
  pythonPath?: string | null;
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
    resolutionX?: number | null;
    resolutionY?: number | null;
    resolutionPercentage?: number | null;
    engine?: string | null;
    outputFormat?: string | null;
  }>;
}
