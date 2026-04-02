import { useEffect, useMemo } from 'react';
import { TextEditorSurface } from './TextEditorSurface';

function getFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function isStandaloneTextEditorRoute(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  const searchParams = new URLSearchParams(window.location.search);
  return searchParams.get('view') === 'text-editor';
}

export function StandaloneTextEditorPage() {
  const searchParams = useMemo(() => new URLSearchParams(window.location.search), []);
  const sourcePath = searchParams.get('path') || '';
  const title = searchParams.get('title') || (sourcePath ? getFileNameFromPath(sourcePath) : '文本编辑器');

  useEffect(() => {
    document.title = title;
  }, [title]);

  if (!sourcePath) {
    return (
      <div className="flex h-screen items-center justify-center bg-white p-6 text-center text-gray-500">
        <div>
          <p className="text-base font-medium text-gray-800">没有收到要打开的文件路径</p>
          <p className="mt-2 text-sm text-gray-500">请从文件列表按住 Ctrl 双击文本文件重新打开。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen bg-white">
      <TextEditorSurface
        title={title}
        filePath={sourcePath}
      />
    </div>
  );
}
