import { convertFileSrc } from '@tauri-apps/api/core';
import { Pause, Play, SkipBack, SkipForward } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import type { ImageSequenceInfo } from '../../types';

interface ImageSequencePlayerSurfaceProps {
  title: string;
  sequence: ImageSequenceInfo;
}

function clampFps(value: number) {
  if (!Number.isFinite(value)) {
    return 12;
  }
  return Math.min(60, Math.max(1, Math.round(value)));
}

export function ImageSequencePlayerSurface({
  title,
  sequence,
}: ImageSequencePlayerSurfaceProps) {
  const [frameIndex, setFrameIndex] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [fps, setFps] = useState(12);
  const frames = sequence.frames;
  const currentFrame = frames[frameIndex] ?? frames[0] ?? null;
  const currentSource = useMemo(
    () => (currentFrame ? convertFileSrc(currentFrame.path) : null),
    [currentFrame],
  );

  useEffect(() => {
    setFrameIndex(0);
    setIsPlaying(false);
  }, [sequence.virtual_path]);

  useEffect(() => {
    if (!isPlaying || frames.length <= 1) {
      return;
    }

    const intervalId = window.setInterval(() => {
      setFrameIndex((current) => (current + 1) % frames.length);
    }, 1000 / clampFps(fps));

    return () => window.clearInterval(intervalId);
  }, [fps, frames.length, isPlaying]);

  const goPrevious = () => {
    setFrameIndex((current) => (current - 1 + frames.length) % frames.length);
  };

  const goNext = () => {
    setFrameIndex((current) => (current + 1) % frames.length);
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-neutral-950 text-white">
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-white/10 px-4 py-3">
        <div className="min-w-0">
          <h2 className="truncate text-base font-semibold">{title}</h2>
          <p className="text-xs text-white/55">
            {sequence.start_frame}-{sequence.end_frame} · {sequence.frame_count} 帧
            {sequence.missing_count > 0 ? ` · 缺 ${sequence.missing_count} 帧` : ''}
          </p>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={goPrevious}
            disabled={frames.length <= 1}
            className="inline-flex h-9 w-9 items-center justify-center rounded-md bg-white/10 text-white transition hover:bg-white/15 disabled:cursor-not-allowed disabled:opacity-40"
            title="上一帧"
          >
            <SkipBack className="h-4 w-4" />
          </button>
          <button
            type="button"
            onClick={() => setIsPlaying((value) => !value)}
            disabled={frames.length <= 1}
            className="inline-flex h-9 w-9 items-center justify-center rounded-md bg-blue-600 text-white transition hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-40"
            title={isPlaying ? '暂停' : '播放'}
          >
            {isPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
          </button>
          <button
            type="button"
            onClick={goNext}
            disabled={frames.length <= 1}
            className="inline-flex h-9 w-9 items-center justify-center rounded-md bg-white/10 text-white transition hover:bg-white/15 disabled:cursor-not-allowed disabled:opacity-40"
            title="下一帧"
          >
            <SkipForward className="h-4 w-4" />
          </button>

          <label className="ml-2 flex items-center gap-2 text-xs text-white/70">
            FPS
            <input
              type="number"
              min={1}
              max={60}
              value={fps}
              onChange={(event) => setFps(clampFps(Number(event.target.value)))}
              className="h-9 w-16 rounded-md border border-white/10 bg-white/10 px-2 text-sm text-white outline-none focus:border-blue-400"
            />
          </label>
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 items-center justify-center bg-black">
          {currentSource ? (
            <img
              src={currentSource}
              alt={currentFrame?.name || title}
              className="max-h-full max-w-full object-contain"
              draggable={false}
            />
          ) : (
            <div className="text-sm text-white/45">没有可预览帧</div>
          )}
        </div>

        <aside className="hidden w-64 shrink-0 border-l border-white/10 bg-neutral-900/95 md:flex md:flex-col">
          <div className="border-b border-white/10 px-3 py-2 text-xs text-white/60">
            当前 {frameIndex + 1}/{frames.length}
            {currentFrame ? ` · 帧 ${currentFrame.frame}` : ''}
          </div>
          <div className="min-h-0 flex-1 overflow-auto py-1">
            {frames.map((frame, index) => (
              <button
                key={`${frame.frame}-${frame.path}`}
                type="button"
                onClick={() => setFrameIndex(index)}
                className={`flex w-full min-w-0 items-center justify-between gap-2 px-3 py-2 text-left text-xs transition ${
                  index === frameIndex
                    ? 'bg-blue-600/25 text-blue-100'
                    : 'text-white/70 hover:bg-white/8 hover:text-white'
                }`}
              >
                <span className="truncate">{frame.name}</span>
                <span className="shrink-0 text-white/45">{frame.frame}</span>
              </button>
            ))}
          </div>
        </aside>
      </div>
    </div>
  );
}

