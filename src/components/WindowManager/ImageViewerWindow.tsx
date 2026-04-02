import { ImageIcon } from 'lucide-react';
import { WindowFrame } from './WindowFrame';
import type { WindowInstance } from '../../stores/windowStore';
import { ImageViewerSurface } from '../image-viewer/ImageViewerSurface';

interface ImageViewerWindowProps {
  windowInstance: WindowInstance;
}

export function ImageViewerWindow({ windowInstance }: ImageViewerWindowProps) {
  const { data, title } = windowInstance;
  const imageUrl = data.imageUrl as string;

  const headerContent = (
    <div className="flex items-center gap-2">
      <ImageIcon className="w-4 h-4 text-green-500" />
      <span className="text-sm font-medium truncate max-w-[200px]">{title}</span>
    </div>
  );

  return (
    <WindowFrame
      windowInstance={windowInstance}
      headerContent={headerContent}
    >
      <ImageViewerSurface
        title={title}
        source={imageUrl}
        showTitleInToolbar={false}
      />
    </WindowFrame>
  );
}

// 创建图像查看器窗口的辅助函数
export function createImageViewerWindow(
  createWindow: (options: import('../../stores/windowStore').CreateWindowOptions) => string,
  options: {
    title?: string;
    imageUrl: string;
  }
) {
  const { title, imageUrl } = options;
  
  // 从 URL 或标题获取文件名
  const fileName = title || imageUrl.split('/').pop() || 'Image';

  return createWindow({
    title: fileName,
    contentType: 'image-viewer',
    size: { width: 900, height: 700 },
    data: {
      imageUrl,
    },
  });
}
