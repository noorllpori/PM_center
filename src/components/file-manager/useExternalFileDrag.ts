import { useCallback, useEffect, useMemo, useRef } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import {
  startExternalFileDrag,
  supportsExternalFileDrag,
} from "../../api/externalDrag";
import { useUiStore } from "../../stores/uiStore";
import { compactDraggedPaths } from "./dragDrop";

const POINTER_DRAG_THRESHOLD = 8;

interface TrackingState {
  onPointerMove: (event: PointerEvent) => void;
  onPointerUp: () => void;
  onPointerCancel: () => void;
}

export function useExternalFileDrag(resolvePaths: () => string[]) {
  const showToast = useUiStore((state) => state.showToast);
  const pendingRef = useRef(false);
  const trackingRef = useRef<TrackingState | null>(null);
  const supported = useMemo(() => supportsExternalFileDrag(), []);

  const clearPointerTracking = useCallback(() => {
    if (!trackingRef.current) {
      return;
    }

    window.removeEventListener(
      "pointermove",
      trackingRef.current.onPointerMove,
      true,
    );
    window.removeEventListener(
      "pointerup",
      trackingRef.current.onPointerUp,
      true,
    );
    window.removeEventListener(
      "pointercancel",
      trackingRef.current.onPointerCancel,
      true,
    );
    trackingRef.current = null;
  }, []);

  useEffect(() => {
    return () => {
      clearPointerTracking();
    };
  }, [clearPointerTracking]);

  const beginExternalDrag = useCallback(async () => {
    if (pendingRef.current) {
      return;
    }

    const paths = compactDraggedPaths(
      resolvePaths().filter((path): path is string => Boolean(path)),
    );
    if (paths.length === 0) {
      return;
    }

    pendingRef.current = true;

    try {
      const result = await startExternalFileDrag(paths);
      if (result.status === "unsupported") {
        showToast({
          title: "外部拖出不可用",
          message: "当前平台不支持把文件拖到系统或外部程序。",
          tone: "warning",
        });
      }
    } catch (error) {
      showToast({
        title: "外部拖出失败",
        message: String(error),
        tone: "error",
      });
    } finally {
      pendingRef.current = false;
    }
  }, [resolvePaths, showToast]);

  const handlePointerDown = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (!supported || event.button !== 0 || pendingRef.current) {
        return;
      }

      event.preventDefault();
      event.stopPropagation();

      clearPointerTracking();

      const startX = event.clientX;
      const startY = event.clientY;

      const onPointerMove = (moveEvent: PointerEvent) => {
        const deltaX = moveEvent.clientX - startX;
        const deltaY = moveEvent.clientY - startY;
        const distance = Math.hypot(deltaX, deltaY);

        if (distance < POINTER_DRAG_THRESHOLD) {
          return;
        }

        clearPointerTracking();
        void beginExternalDrag();
      };

      const onPointerUp = () => {
        clearPointerTracking();
      };

      const onPointerCancel = () => {
        clearPointerTracking();
      };

      trackingRef.current = {
        onPointerMove,
        onPointerUp,
        onPointerCancel,
      };

      window.addEventListener("pointermove", onPointerMove, true);
      window.addEventListener("pointerup", onPointerUp, true);
      window.addEventListener("pointercancel", onPointerCancel, true);
    },
    [beginExternalDrag, clearPointerTracking, supported],
  );

  return {
    supported,
    handlePointerDown,
  };
}
