import { useEffect, useMemo } from 'react';
import { VideoPlayerSurface } from './VideoPlayerSurface';

function getFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function isStandaloneVideoPlayerRoute(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  const searchParams = new URLSearchParams(window.location.search);
  return searchParams.get('view') === 'video-player';
}

export function StandaloneVideoPlayerPage() {
  const searchParams = useMemo(() => new URLSearchParams(window.location.search), []);
  const sourcePath = searchParams.get('path') || '';
  const title = searchParams.get('title') || (sourcePath ? getFileNameFromPath(sourcePath) : '视频播放器');

  useEffect(() => {
    document.title = title;
  }, [title]);

  if (!sourcePath) {
    return (
      <div className="flex h-screen items-center justify-center bg-[#0a0c10] p-6 text-center text-gray-300">
        <div>
          <p className="text-base font-medium">没有收到要打开的视频路径</p>
          <p className="mt-2 text-sm text-gray-400">请从文件列表重新打开视频文件。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen bg-[#0a0c10]">
      <VideoPlayerSurface
        title={title}
        source={sourcePath}
      />
    </div>
  );
}
