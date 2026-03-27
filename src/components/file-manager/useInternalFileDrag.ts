import { useCallback, useEffect, useRef } from 'react';
import type { DragEvent, SyntheticEvent } from 'react';
import { compactDraggedPaths, resolveInternalDragPaths, setFileDragData } from './dragDrop';
import { useFileDragStore } from '../../stores/fileDragStore';

const CLICK_SUPPRESSION_MS = 250;

export function useInternalFileDrag() {
  const draggedPaths = useFileDragStore((state) => state.draggedPaths);
  const startDrag = useFileDragStore((state) => state.startDrag);
  const clearDrag = useFileDragStore((state) => state.clearDrag);

  const draggingElementRef = useRef<HTMLElement | null>(null);
  const suppressInteractionsUntilRef = useRef(0);

  const startInternalDrag = useCallback(<T extends HTMLElement>(event: DragEvent<T>, sourcePaths: string[]) => {
    const compactPaths = compactDraggedPaths(sourcePaths);

    startDrag(compactPaths);
    setFileDragData(event.dataTransfer, compactPaths);

    const dragElement = event.currentTarget;
    draggingElementRef.current?.classList.remove('dragging');
    dragElement.classList.add('dragging');
    draggingElementRef.current = dragElement;

    if (typeof event.dataTransfer.setDragImage === 'function') {
      const rect = dragElement.getBoundingClientRect();
      const offsetX = Math.max(12, Math.round(event.clientX - rect.left));
      const offsetY = Math.max(12, Math.round(event.clientY - rect.top));
      event.dataTransfer.setDragImage(dragElement, offsetX, offsetY);
    }
  }, [startDrag]);

  const finishInternalDrag = useCallback(() => {
    suppressInteractionsUntilRef.current = Date.now() + CLICK_SUPPRESSION_MS;
    draggingElementRef.current?.classList.remove('dragging');
    draggingElementRef.current = null;
    clearDrag();
  }, [clearDrag]);

  const suppressInteraction = useCallback((event: SyntheticEvent<HTMLElement>) => {
    if (Date.now() >= suppressInteractionsUntilRef.current) {
      return false;
    }

    event.preventDefault();
    event.stopPropagation();
    return true;
  }, []);

  const getDraggedPathsFromDataTransfer = useCallback((dataTransfer: DataTransfer | null) => {
    return resolveInternalDragPaths(dataTransfer, draggedPaths);
  }, [draggedPaths]);

  useEffect(() => {
    return () => {
      draggingElementRef.current?.classList.remove('dragging');
      draggingElementRef.current = null;
    };
  }, []);

  return {
    draggedPaths,
    startInternalDrag,
    finishInternalDrag,
    suppressInteraction,
    getDraggedPathsFromDataTransfer,
  };
}
