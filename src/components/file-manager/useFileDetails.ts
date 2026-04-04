import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useSettingsStore, type ToolPaths } from '../../stores/settingsStore';
import type { FileDetailsResponse, FileInfo } from '../../types';

const MAX_CACHE_ENTRIES = 200;

const fileDetailsCache = new Map<string, FileDetailsResponse>();
const pendingRequests = new Map<string, Promise<FileDetailsResponse>>();

function buildCacheKey(file: FileInfo, toolPaths: ToolPaths) {
  return JSON.stringify({
    path: file.path,
    modified: file.modified,
    size: file.size,
    ffprobe: toolPaths.ffprobe,
    blender: toolPaths.blender,
  });
}

function touchCacheEntry(key: string, value: FileDetailsResponse) {
  if (fileDetailsCache.has(key)) {
    fileDetailsCache.delete(key);
  }

  fileDetailsCache.set(key, value);

  if (fileDetailsCache.size > MAX_CACHE_ENTRIES) {
    const oldestKey = fileDetailsCache.keys().next().value;
    if (oldestKey) {
      fileDetailsCache.delete(oldestKey);
    }
  }
}

function getCachedDetails(key: string) {
  const cached = fileDetailsCache.get(key);
  if (!cached) {
    return null;
  }

  touchCacheEntry(key, cached);
  return cached;
}

async function requestFileDetails(
  file: FileInfo,
  view: 'panel' | 'dialog',
  toolPaths: ToolPaths,
  forceRefresh = false,
) {
  const cacheKey = buildCacheKey(file, toolPaths);

  if (!forceRefresh) {
    const cached = getCachedDetails(cacheKey);
    if (cached) {
      return cached;
    }

    const pending = pendingRequests.get(cacheKey);
    if (pending) {
      return pending;
    }
  } else {
    fileDetailsCache.delete(cacheKey);
  }

  const request = invoke<FileDetailsResponse>('get_file_details', {
    path: file.path,
    view,
    toolPaths,
    forceRefresh,
  })
    .then((result) => {
      touchCacheEntry(cacheKey, result);
      return result;
    })
    .finally(() => {
      if (pendingRequests.get(cacheKey) === request) {
        pendingRequests.delete(cacheKey);
      }
    });

  pendingRequests.set(cacheKey, request);
  return request;
}

export function clearFileDetailsCache() {
  fileDetailsCache.clear();
  pendingRequests.clear();
}

export function useFileDetails(file: FileInfo | null, view: 'panel' | 'dialog') {
  const toolPaths = useSettingsStore((state) => state.toolPaths);
  const [details, setDetails] = useState<FileDetailsResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const requestIdRef = useRef(0);

  const cacheKey = useMemo(() => {
    if (!file) {
      return null;
    }

    return buildCacheKey(file, toolPaths);
  }, [file, toolPaths]);

  useEffect(() => {
    const requestId = ++requestIdRef.current;

    if (!file || !cacheKey) {
      setDetails(null);
      setErrorMessage(null);
      setIsLoading(false);
      setIsRefreshing(false);
      return;
    }

    const cached = getCachedDetails(cacheKey);
    if (cached) {
      setDetails(cached);
      setErrorMessage(null);
      setIsLoading(false);
      setIsRefreshing(false);
      return;
    }

    setDetails(null);
    setErrorMessage(null);
    setIsLoading(true);
    setIsRefreshing(false);

    requestFileDetails(file, view, toolPaths)
      .then((result) => {
        if (requestId !== requestIdRef.current) {
          return;
        }

        setDetails(result);
        setErrorMessage(null);
      })
      .catch((error) => {
        if (requestId !== requestIdRef.current) {
          return;
        }

        setDetails(null);
        setErrorMessage(String(error));
      })
      .finally(() => {
        if (requestId !== requestIdRef.current) {
          return;
        }

        setIsLoading(false);
      });
  }, [cacheKey, file, toolPaths, view]);

  const refresh = useCallback(async () => {
    if (!file) {
      return;
    }

    const requestId = ++requestIdRef.current;
    const hasExistingDetails = details !== null;

    setErrorMessage(null);
    setIsLoading(false);
    setIsRefreshing(true);

    try {
      const result = await requestFileDetails(file, view, toolPaths, true);
      if (requestId !== requestIdRef.current) {
        return;
      }

      setDetails(result);
      setErrorMessage(null);
    } catch (error) {
      if (requestId !== requestIdRef.current) {
        return;
      }

      if (!hasExistingDetails) {
        setDetails(null);
      }

      setErrorMessage(
        hasExistingDetails
          ? `刷新失败，当前显示缓存信息：${String(error)}`
          : String(error),
      );
    } finally {
      if (requestId !== requestIdRef.current) {
        return;
      }

      setIsRefreshing(false);
    }
  }, [details, file, toolPaths, view]);

  return {
    details,
    isLoading,
    isRefreshing,
    errorMessage,
    refresh,
  };
}
