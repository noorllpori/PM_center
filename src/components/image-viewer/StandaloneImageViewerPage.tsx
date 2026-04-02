import { useEffect, useMemo } from 'react';
import { ImageViewerSurface } from './ImageViewerSurface';

function getFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function isStandaloneImageViewerRoute(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  const searchParams = new URLSearchParams(window.location.search);
  return searchParams.get('view') === 'image-viewer';
}

export function StandaloneImageViewerPage() {
  const searchParams = useMemo(() => new URLSearchParams(window.location.search), []);
  const sourcePath = searchParams.get('path') || '';
  const title = searchParams.get('title') || (sourcePath ? getFileNameFromPath(sourcePath) : '图片查看器');

  useEffect(() => {
    document.title = title;
  }, [title]);

  if (!sourcePath) {
    return (
      <div className="flex h-screen items-center justify-center bg-gray-950 p-6 text-center text-gray-300">
        <div>
          <p className="text-base font-medium">没有收到要打开的图片路径</p>
          <p className="mt-2 text-sm text-gray-400">请从文件列表双击图片，或重新从主窗口打开。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen bg-gray-950">
      <ImageViewerSurface
        title={title}
        source={sourcePath}
      />
    </div>
  );
}

