import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { readFile } from '@tauri-apps/plugin-fs';
import { Image as ImageIcon, Maximize, RefreshCw, RotateCw, ZoomIn, ZoomOut } from 'lucide-react';
import { TransformComponent, TransformWrapper, type ReactZoomPanPinchContentRef } from 'react-zoom-pan-pinch';
import { getImageMimeType } from './imageViewerUtils';

interface ImageViewerSurfaceProps {
  title: string;
  source: string;
  showTitleInToolbar?: boolean;
}

const MIN_SCALE = 0.05;
const MAX_SCALE = 16;
const BUTTON_ZOOM_STEP = 0.18;
const WHEEL_SMOOTH_STEP = 0.0012;

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function isDirectBrowserSource(source: string): boolean {
  return /^(asset:|blob:|data:|https?:|http:\/\/asset\.localhost)/i.test(source);
}

function useResolvedImageSource(source: string) {
  const [resolvedSource, setResolvedSource] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    let isActive = true;
    let objectUrl: string | null = null;

    async function loadSource() {
      if (!source) {
        setResolvedSource(null);
        setErrorMessage('没有可显示的图片路径。');
        setIsLoading(false);
        return;
      }

      if (isDirectBrowserSource(source)) {
        setResolvedSource(source);
        setErrorMessage(null);
        setIsLoading(false);
        return;
      }

      setIsLoading(true);
      setErrorMessage(null);

      try {
        const bytes = await readFile(source);
        if (!isActive) {
          return;
        }

        const blob = new Blob([bytes], {
          type: getImageMimeType(source),
        });
        objectUrl = URL.createObjectURL(blob);
        setResolvedSource(objectUrl);
      } catch (error) {
        if (!isActive) {
          return;
        }

        setResolvedSource(null);
        setErrorMessage(`读取图片失败：${String(error)}`);
      } finally {
        if (isActive) {
          setIsLoading(false);
        }
      }
    }

    void loadSource();

    return () => {
      isActive = false;
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl);
      }
    };
  }, [source]);

  return {
    resolvedSource,
    isLoading,
    errorMessage,
  };
}

export function ImageViewerSurface({
  title,
  source,
  showTitleInToolbar = true,
}: ImageViewerSurfaceProps) {
  const { resolvedSource, isLoading, errorMessage: sourceErrorMessage } = useResolvedImageSource(source);
  const viewportRef = useRef<HTMLDivElement>(null);
  const transformRef = useRef<ReactZoomPanPinchContentRef | null>(null);
  const [naturalSize, setNaturalSize] = useState({ width: 0, height: 0 });
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 });
  const [rotationQuarterTurns, setRotationQuarterTurns] = useState(0);
  const [currentScale, setCurrentScale] = useState(1);
  const [viewMode, setViewMode] = useState<'fit' | 'custom'>('fit');
  const [renderErrorMessage, setRenderErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }

    const updateSize = () => {
      setViewportSize({
        width: viewport.clientWidth,
        height: viewport.clientHeight,
      });
    };

    updateSize();

    const observer = new ResizeObserver(updateSize);
    observer.observe(viewport);

    return () => {
      observer.disconnect();
    };
  }, []);

  useEffect(() => {
    setNaturalSize({ width: 0, height: 0 });
    setRotationQuarterTurns(0);
    setCurrentScale(1);
    setViewMode('fit');
    setRenderErrorMessage(null);
  }, [source]);

  useEffect(() => {
    if (!resolvedSource) {
      return;
    }

    let isActive = true;
    const image = new Image();

    image.onload = () => {
      if (!isActive) {
        return;
      }

      setNaturalSize({
        width: image.naturalWidth,
        height: image.naturalHeight,
      });
      setRenderErrorMessage(null);
    };

    image.onerror = () => {
      if (!isActive) {
        return;
      }

      setNaturalSize({ width: 0, height: 0 });
      setRenderErrorMessage('图片解码失败，无法显示预览。');
    };

    image.src = resolvedSource;

    return () => {
      isActive = false;
    };
  }, [resolvedSource]);

  const normalizedQuarterTurns = useMemo(
    () => ((rotationQuarterTurns % 4) + 4) % 4,
    [rotationQuarterTurns],
  );
  const rotationDegrees = normalizedQuarterTurns * 90;

  const rotatedNaturalSize = useMemo(() => {
    if (normalizedQuarterTurns % 2 === 0) {
      return naturalSize;
    }

    return {
      width: naturalSize.height,
      height: naturalSize.width,
    };
  }, [naturalSize, normalizedQuarterTurns]);

  const fitScale = useMemo(() => {
    if (
      viewportSize.width <= 0 ||
      viewportSize.height <= 0 ||
      rotatedNaturalSize.width <= 0 ||
      rotatedNaturalSize.height <= 0
    ) {
      return 1;
    }

    const scaleX = viewportSize.width / rotatedNaturalSize.width;
    const scaleY = viewportSize.height / rotatedNaturalSize.height;

    return clamp(Math.min(scaleX, scaleY), MIN_SCALE, MAX_SCALE);
  }, [rotatedNaturalSize.height, rotatedNaturalSize.width, viewportSize.height, viewportSize.width]);

  useEffect(() => {
    if (
      viewMode !== 'fit' ||
      !transformRef.current ||
      rotatedNaturalSize.width <= 0 ||
      rotatedNaturalSize.height <= 0
    ) {
      return;
    }

    requestAnimationFrame(() => {
      transformRef.current?.centerView(fitScale, 0);
      setCurrentScale(fitScale);
    });
  }, [fitScale, rotatedNaturalSize.height, rotatedNaturalSize.width, viewMode]);

  const handleFitToWindow = useCallback(() => {
    setViewMode('fit');
  }, []);

  const handleActualSize = useCallback(() => {
    setViewMode('custom');
    transformRef.current?.centerView(1, 0);
  }, []);

  const handleZoomIn = useCallback(() => {
    setViewMode('custom');
    transformRef.current?.zoomIn(BUTTON_ZOOM_STEP, 0);
  }, []);

  const handleZoomOut = useCallback(() => {
    setViewMode('custom');
    transformRef.current?.zoomOut(BUTTON_ZOOM_STEP, 0);
  }, []);

  const handleRotate = useCallback(() => {
    setRotationQuarterTurns((currentRotation) => (currentRotation + 1) % 4);
    setViewMode('fit');
  }, []);

  const handleDoubleClick = useCallback(() => {
    if (Math.abs(currentScale - fitScale) < 0.01 || viewMode === 'fit') {
      setViewMode('custom');
      transformRef.current?.centerView(1, 0);
      return;
    }

    setViewMode('fit');
  }, [currentScale, fitScale, viewMode]);

  const statusText = useMemo(() => {
    if (isLoading) {
      return '加载中...';
    }

    if (sourceErrorMessage || renderErrorMessage) {
      return '预览失败';
    }

    if (naturalSize.width > 0 && naturalSize.height > 0) {
      return `${naturalSize.width} x ${naturalSize.height}`;
    }

    return '等待加载';
  }, [isLoading, naturalSize.height, naturalSize.width, renderErrorMessage, sourceErrorMessage]);

  const effectiveErrorMessage = sourceErrorMessage || renderErrorMessage;

  return (
    <div className="flex h-full w-full min-w-0 flex-col bg-white dark:bg-gray-900">
      <div className="flex items-center gap-2 border-b border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-800/80">
        {showTitleInToolbar && (
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium text-gray-800 dark:text-gray-100">{title}</p>
            <p className="hidden text-xs text-gray-500 dark:text-gray-400 md:block">
              滚轮缩放，左键拖动，双击切换适应窗口和 1:1
            </p>
          </div>
        )}

        <button
          onClick={handleZoomOut}
          className="rounded p-1.5 text-gray-600 transition-colors hover:bg-gray-200 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-700 dark:hover:text-gray-100"
          title="缩小"
        >
          <ZoomOut className="h-4 w-4" />
        </button>

        <span className="min-w-[64px] text-center text-xs text-gray-500 dark:text-gray-400">
          {Math.round(currentScale * 100)}%
        </span>

        <button
          onClick={handleZoomIn}
          className="rounded p-1.5 text-gray-600 transition-colors hover:bg-gray-200 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-700 dark:hover:text-gray-100"
          title="放大"
        >
          <ZoomIn className="h-4 w-4" />
        </button>

        <div className="h-4 w-px bg-gray-300 dark:bg-gray-600" />

        <button
          onClick={handleFitToWindow}
          className="rounded p-1.5 text-gray-600 transition-colors hover:bg-gray-200 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-700 dark:hover:text-gray-100"
          title="适应窗口"
        >
          <Maximize className="h-4 w-4" />
        </button>

        <button
          onClick={handleActualSize}
          className="rounded px-2 py-1 text-xs text-gray-600 transition-colors hover:bg-gray-200 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-700 dark:hover:text-gray-100"
          title="实际大小"
        >
          1:1
        </button>

        <button
          onClick={handleRotate}
          className="rounded p-1.5 text-gray-600 transition-colors hover:bg-gray-200 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-700 dark:hover:text-gray-100"
          title="顺时针旋转"
        >
          <RotateCw className="h-4 w-4" />
        </button>

        <div className="flex-1" />

        <span className="min-w-[120px] text-right text-xs text-gray-500 dark:text-gray-400">
          {statusText}
        </span>
      </div>

      <div
        ref={viewportRef}
        className="relative flex-1 overflow-hidden bg-gray-950 p-[2px]"
        style={{
          backgroundImage:
            'linear-gradient(45deg, rgba(255,255,255,0.04) 25%, transparent 25%), linear-gradient(-45deg, rgba(255,255,255,0.04) 25%, transparent 25%), linear-gradient(45deg, transparent 75%, rgba(255,255,255,0.04) 75%), linear-gradient(-45deg, transparent 75%, rgba(255,255,255,0.04) 75%)',
          backgroundSize: '24px 24px',
          backgroundPosition: '0 0, 0 12px, 12px -12px, -12px 0',
        }}
      >
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-center text-gray-300">
            <div>
              <RefreshCw className="mx-auto mb-3 h-10 w-10 animate-spin opacity-80" />
              <p className="text-sm">正在加载图片...</p>
            </div>
          </div>
        ) : effectiveErrorMessage ? (
          <div className="flex h-full items-center justify-center p-6 text-center text-gray-300">
            <div className="max-w-sm">
              <ImageIcon className="mx-auto mb-3 h-14 w-14 opacity-40" />
              <p className="text-sm font-medium">无法显示这张图片</p>
              <p className="mt-2 break-all text-xs text-gray-400">{effectiveErrorMessage}</p>
            </div>
          </div>
        ) : resolvedSource && naturalSize.width > 0 && naturalSize.height > 0 ? (
          <TransformWrapper
            key={`${resolvedSource}-${rotationQuarterTurns}`}
            ref={transformRef}
            minScale={MIN_SCALE}
            maxScale={MAX_SCALE}
            limitToBounds={false}
            centerOnInit
            centerZoomedOut
            smooth
            doubleClick={{ disabled: true }}
            panning={{
              allowLeftClickPan: true,
              velocityDisabled: true,
              wheelPanning: false,
            }}
            wheel={{
              smoothStep: WHEEL_SMOOTH_STEP,
            }}
            alignmentAnimation={{
              disabled: true,
            }}
            velocityAnimation={{
              disabled: true,
            }}
            onWheelStart={() => setViewMode('custom')}
            onPanningStart={() => setViewMode('custom')}
            onPinchingStart={() => setViewMode('custom')}
            onTransformed={(_, state) => {
              setCurrentScale(state.scale);
            }}
          >
            <TransformComponent
              wrapperStyle={{
                width: '100%',
                height: '100%',
              }}
              contentStyle={{
                width: `${rotatedNaturalSize.width}px`,
                height: `${rotatedNaturalSize.height}px`,
              }}
              wrapperProps={{
                onDoubleClick: handleDoubleClick,
              }}
            >
              <div
                className="relative"
                style={{
                  width: rotatedNaturalSize.width,
                  height: rotatedNaturalSize.height,
                }}
              >
                <img
                  src={resolvedSource}
                  alt={title}
                  className="pointer-events-none absolute left-1/2 top-1/2 block select-none rounded-lg shadow-2xl shadow-black/30"
                  style={{
                    width: naturalSize.width,
                    height: naturalSize.height,
                    maxWidth: 'none',
                    transform: `translate(-50%, -50%) rotate(${rotationDegrees}deg)`,
                    transformOrigin: 'center center',
                  }}
                  draggable={false}
                  onError={() => setRenderErrorMessage('图片渲染失败，可能是当前格式暂不支持。')}
                />
              </div>
            </TransformComponent>
          </TransformWrapper>
        ) : (
          <div className="flex h-full items-center justify-center text-center text-gray-400">
            <div>
              <ImageIcon className="mx-auto mb-3 h-14 w-14 opacity-40" />
              <p className="text-sm">暂无可显示的图片</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
