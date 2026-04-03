import { useEffect, useMemo, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Film, Loader2, PlayCircle } from 'lucide-react';

interface VideoPlayerSurfaceProps {
  title: string;
  source: string;
  showTitleInToolbar?: boolean;
}

function resolveVideoSource(source: string) {
  if (!source) {
    return '';
  }

  if (/^(asset|https?|data|blob):/i.test(source)) {
    return source;
  }

  return convertFileSrc(source);
}

export function VideoPlayerSurface({
  title,
  source,
  showTitleInToolbar = true,
}: VideoPlayerSurfaceProps) {
  const resolvedSource = useMemo(() => resolveVideoSource(source), [source]);
  const [isReady, setIsReady] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    setIsReady(false);
    setErrorMessage(null);
  }, [resolvedSource]);

  if (!resolvedSource) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-black/95 text-gray-300">
        <div className="text-center">
          <Film className="mx-auto h-10 w-10 text-gray-500" />
          <p className="mt-3 text-sm font-medium">没有可播放的视频路径</p>
        </div>
      </div>
    );
  }

  return (
    <div className="group relative flex h-full w-full min-w-0 flex-col bg-[#0a0c10] text-white">
      <div className="flex min-h-0 flex-1 items-center justify-center p-3">
        <div className="relative flex h-full w-full items-center justify-center overflow-hidden rounded-xl border border-white/10 bg-black shadow-2xl">
          <div
            className={`pointer-events-none absolute left-3 right-3 top-3 z-10 transition-all duration-200 ${
              showTitleInToolbar ? 'opacity-0 group-hover:opacity-100 translate-y-0 group-hover:translate-y-0' : 'hidden'
            }`}
          >
            <div className="rounded-xl border border-white/10 bg-black/60 px-3 py-2 backdrop-blur-md">
              <div className="flex items-center gap-2">
                <PlayCircle className="h-4 w-4 shrink-0 text-red-400" />
                <p className="truncate text-sm font-medium text-white/95">
                  {title}
                </p>
              </div>
              <p className="mt-0.5 truncate text-xs text-white/55">
                {source}
              </p>
            </div>
          </div>

          {!isReady && !errorMessage && (
            <div className="pointer-events-none absolute inset-0 z-[5] flex items-center justify-center bg-black/45 backdrop-blur-[1px]">
              <div className="rounded-xl border border-white/10 bg-black/60 px-4 py-3 text-center shadow-lg">
                <Loader2 className="mx-auto h-5 w-5 animate-spin text-white/80" />
                <p className="mt-2 text-sm font-medium text-white/90">正在读取视频...</p>
                <p className="mt-1 max-w-[60vw] truncate text-xs text-white/45">{title}</p>
              </div>
            </div>
          )}

          {errorMessage && (
            <div className="absolute inset-0 z-[5] flex items-center justify-center bg-black/60 p-6 text-center">
              <div>
                <Film className="mx-auto h-9 w-9 text-white/50" />
                <p className="mt-3 text-sm font-medium text-white/90">视频无法播放</p>
                <p className="mt-2 max-w-[60vw] break-all text-xs text-white/55">{errorMessage}</p>
              </div>
            </div>
          )}

          <video
            src={resolvedSource}
            className="h-full w-full bg-black object-contain"
            controls
            preload="metadata"
            playsInline
            onLoadedMetadata={() => {
              setIsReady(true);
              setErrorMessage(null);
            }}
            onCanPlay={() => {
              setIsReady(true);
              setErrorMessage(null);
            }}
            onError={() => {
              setIsReady(false);
              setErrorMessage('浏览器内核暂时无法解码这个视频，或视频元数据读取失败。');
            }}
          />
        </div>
      </div>
    </div>
  );
}
