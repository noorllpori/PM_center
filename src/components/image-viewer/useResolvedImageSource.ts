import { useEffect, useState } from 'react';
import { readFile } from '@tauri-apps/plugin-fs';
import { getImageExtension, getImageMimeType, isPsdExtension } from './imageViewerUtils';

interface PsdPreviewDocument {
  width?: number;
  height?: number;
  canvas?: HTMLCanvasElement;
  children?: PsdPreviewLayer[];
  imageResources?: {
    thumbnail?: HTMLCanvasElement;
  };
}

interface PsdPreviewLayer {
  canvas?: HTMLCanvasElement;
  children?: PsdPreviewLayer[];
  hidden?: boolean;
  opacity?: number;
  left?: number;
  top?: number;
}

function isDirectBrowserSource(source: string) {
  return /^(asset:|blob:|data:|https?:|http:\/\/asset\.localhost)/i.test(source);
}

function toExactArrayBuffer(bytes: Uint8Array) {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
}

function canvasToBlob(canvas: HTMLCanvasElement) {
  return new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) {
        resolve(blob);
        return;
      }

      reject(new Error('无法生成 PSD 预览图像。'));
    }, 'image/png');
  });
}

async function resolvePsdPreviewSource(source: string) {
  const bytes = await readFile(source);
  const { readPsd } = await import('ag-psd');
  const buffer = toExactArrayBuffer(bytes);
  let psd = readPsd(buffer, {
    skipLayerImageData: true,
  }) as PsdPreviewDocument;

  let previewCanvas: HTMLCanvasElement | null | undefined = psd.canvas ?? psd.imageResources?.thumbnail;
  if (!previewCanvas) {
    psd = readPsd(buffer) as PsdPreviewDocument;
    previewCanvas = psd.canvas ?? psd.imageResources?.thumbnail ?? composePsdPreview(psd);
  }

  if (!previewCanvas) {
    throw new Error('PSD 文件缺少可用的合成图或缩略图，可能未启用兼容预览。');
  }

  const blob = await canvasToBlob(previewCanvas);
  return URL.createObjectURL(blob);
}

async function resolveRasterSource(source: string) {
  const bytes = await readFile(source);
  const blob = new Blob([bytes], {
    type: getImageMimeType(source),
  });

  return URL.createObjectURL(blob);
}

function composePsdPreview(psd: PsdPreviewDocument) {
  if (!psd.width || !psd.height || !psd.children?.length) {
    return null;
  }

  const canvas = document.createElement('canvas');
  canvas.width = psd.width;
  canvas.height = psd.height;

  const context = canvas.getContext('2d');
  if (!context) {
    return null;
  }

  drawPsdLayers(context, psd.children);
  return canvas;
}

function drawPsdLayers(context: CanvasRenderingContext2D, layers: PsdPreviewLayer[]) {
  for (const layer of [...layers].reverse()) {
    if (layer.hidden) {
      continue;
    }

    if (layer.children?.length) {
      drawPsdLayers(context, layer.children);
      continue;
    }

    if (!layer.canvas) {
      continue;
    }

    context.save();
    context.globalAlpha = typeof layer.opacity === 'number' ? layer.opacity : 1;
    context.drawImage(layer.canvas, layer.left ?? 0, layer.top ?? 0);
    context.restore();
  }
}

export function useResolvedImageSource(source: string) {
  const [resolvedSourceState, setResolvedSourceState] = useState<{
    source: string;
    value: string | null;
  }>({
    source: '',
    value: null,
  });
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    let isActive = true;
    let objectUrl: string | null = null;

    async function loadSource() {
      if (!source) {
        setResolvedSourceState({
          source: '',
          value: null,
        });
        setErrorMessage('没有可显示的图片路径。');
        setIsLoading(false);
        return;
      }

      if (isDirectBrowserSource(source)) {
        setResolvedSourceState({
          source,
          value: source,
        });
        setErrorMessage(null);
        setIsLoading(false);
        return;
      }

      setIsLoading(true);
      setErrorMessage(null);
      setResolvedSourceState({
        source,
        value: null,
      });

      try {
        const extension = getImageExtension(source);
        objectUrl = isPsdExtension(extension)
          ? await resolvePsdPreviewSource(source)
          : await resolveRasterSource(source);

        if (!isActive) {
          return;
        }

        setResolvedSourceState({
          source,
          value: objectUrl,
        });
      } catch (error) {
        if (!isActive) {
          return;
        }

        setResolvedSourceState({
          source,
          value: null,
        });
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

  const resolvedSource = resolvedSourceState.source === source
    ? resolvedSourceState.value
    : null;

  return {
    resolvedSource,
    isLoading,
    errorMessage,
  };
}
