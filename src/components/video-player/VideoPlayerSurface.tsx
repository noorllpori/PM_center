import { useMemo } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Film, PlayCircle } from 'lucide-react';

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
    <div className="flex h-full w-full min-w-0 flex-col bg-[#0a0c10] text-white">
      <div className="flex items-center gap-2 border-b border-white/10 bg-white/5 px-3 py-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <PlayCircle className="h-4 w-4 shrink-0 text-red-400" />
            <p className="truncate text-sm font-medium text-white/95">
              {showTitleInToolbar ? title : '视频播放'}
            </p>
          </div>
          <p className="mt-0.5 truncate text-xs text-white/45">
            {source}
          </p>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 items-center justify-center p-4">
        <div className="flex h-full w-full items-center justify-center overflow-hidden rounded-2xl border border-white/10 bg-black shadow-2xl">
          <video
            src={resolvedSource}
            className="h-full w-full bg-black object-contain"
            controls
            preload="metadata"
          />
        </div>
      </div>
    </div>
  );
}
