import { useState, useRef, useEffect } from 'react';
import { WindowFrame } from './WindowFrame';
import { useWindowStore, type WindowInstance } from '../../stores/windowStore';
import { ZoomIn, ZoomOut, RotateCw, Maximize, Image as ImageIcon } from 'lucide-react';

interface ImageViewerWindowProps {
  windowInstance: WindowInstance;
}

export function ImageViewerWindow({ windowInstance }: ImageViewerWindowProps) {
  const { updateWindowData, updateSize: updateWindowSize, updatePosition } = useWindowStore();
  const [scale, setScale] = useState(1);
  const [rotation, setRotation] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const containerRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  
  const { id, data, title } = windowInstance;
  const imageUrl = data.imageUrl as string;
  const originalWidth = data.originalWidth as number | undefined;
  const originalHeight = data.originalHeight as number | undefined;

  // 适应窗口大小
  const fitToWindow = () => {
    if (!containerRef.current || !originalWidth || !originalHeight) return;
    
    const container = containerRef.current;
    const containerWidth = container.clientWidth - 40;
    const containerHeight = container.clientHeight - 40;
    
    const scaleX = containerWidth / originalWidth;
    const scaleY = containerHeight / originalHeight;
    const newScale = Math.min(scaleX, scaleY, 1);
    
    setScale(newScale);
    setOffset({ x: 0, y: 0 });
  };

  // 实际大小
  const actualSize = () => {
    setScale(1);
    setOffset({ x: 0, y: 0 });
  };

  // 放大
  const zoomIn = () => {
    setScale(s => Math.min(s * 1.25, 5));
  };

  // 缩小
  const zoomOut = () => {
    setScale(s => Math.max(s / 1.25, 0.1));
  };

  // 旋转
  const rotate = () => {
    setRotation(r => (r + 90) % 360);
  };

  // 拖拽图片
  const handleMouseDown = (e: React.MouseEvent) => {
    if (scale <= 1) return;
    setIsDragging(true);
    setDragStart({ x: e.clientX - offset.x, y: e.clientY - offset.y });
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isDragging) return;
    setOffset({
      x: e.clientX - dragStart.x,
      y: e.clientY - dragStart.y,
    });
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  // 滚轮缩放
  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setScale(s => Math.max(0.1, Math.min(5, s * delta)));
  };

  // 初始适应
  useEffect(() => {
    if (imageUrl) {
      const img = new Image();
      img.onload = () => {
        updateWindowData(id, {
          originalWidth: img.naturalWidth,
          originalHeight: img.naturalHeight,
        });
      };
      img.src = imageUrl;
    }
  }, [imageUrl, id, updateWindowData]);

  // 工具栏
  const toolbarContent = (
    <>
      <button
        onClick={zoomOut}
        className="p-1.5 text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 rounded"
        title="缩小"
      >
        <ZoomOut className="w-4 h-4" />
      </button>
      
      <span className="text-xs text-gray-500 min-w-[60px] text-center">
        {Math.round(scale * 100)}%
      </span>
      
      <button
        onClick={zoomIn}
        className="p-1.5 text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 rounded"
        title="放大"
      >
        <ZoomIn className="w-4 h-4" />
      </button>

      <div className="w-px h-4 bg-gray-300 dark:bg-gray-600 mx-1" />

      <button
        onClick={fitToWindow}
        className="p-1.5 text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 rounded"
        title="适应窗口"
      >
        <Maximize className="w-4 h-4" />
      </button>

      <button
        onClick={actualSize}
        className="px-2 py-1 text-xs text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 rounded"
        title="实际大小"
      >
        1:1
      </button>

      <div className="w-px h-4 bg-gray-300 dark:bg-gray-600 mx-1" />

      <button
        onClick={rotate}
        className="p-1.5 text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 rounded"
        title="旋转"
      >
        <RotateCw className="w-4 h-4" />
      </button>

      <div className="flex-1" />

      {originalWidth && originalHeight && (
        <span className="text-xs text-gray-400">
          {originalWidth} × {originalHeight}
        </span>
      )}
    </>
  );

  // 自定义头部
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
      toolbarContent={toolbarContent}
    >
      <div
        ref={containerRef}
        className="w-full h-full bg-gray-900 flex items-center justify-center overflow-hidden cursor-grab active:cursor-grabbing"
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onWheel={handleWheel}
      >
        {imageUrl ? (
          <img
            ref={imageRef}
            src={imageUrl}
            alt={title}
            className="max-w-none transition-transform duration-100"
            style={{
              transform: `translate(${offset.x}px, ${offset.y}px) rotate(${rotation}deg) scale(${scale})`,
              transformOrigin: 'center center',
            }}
            draggable={false}
          />
        ) : (
          <div className="text-gray-500 text-center">
            <ImageIcon className="w-16 h-16 mx-auto mb-2 opacity-30" />
            <p>无法加载图片</p>
          </div>
        )}
      </div>
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
