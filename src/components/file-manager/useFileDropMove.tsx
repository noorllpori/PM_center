import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MoveConflictDialog } from './MoveConflictDialog';
import { compactDraggedPaths, getFileNameFromPath, isSameOrDescendantPath } from './dragDrop';
import { useProjectStoreShallow } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';

type ConflictChoice = 'overwrite' | 'rename' | 'cancel';

interface ConflictState {
  isOpen: boolean;
  sourceName: string;
  targetLabel: string;
}

const CONFLICT_ERROR_PREFIX = 'PM_CONFLICT:';

export function useFileDropMove(onAfterMove: () => Promise<void> | void) {
  const showToast = useUiStore((state) => state.showToast);
  const { projectPath, applyMovedPath } = useProjectStoreShallow((state) => ({
    projectPath: state.projectPath,
    applyMovedPath: state.applyMovedPath,
  }));

  const [conflictState, setConflictState] = useState<ConflictState>({
    isOpen: false,
    sourceName: '',
    targetLabel: '',
  });

  const resolverRef = useRef<((choice: ConflictChoice) => void) | null>(null);

  useEffect(() => {
    return () => {
      resolverRef.current?.('cancel');
      resolverRef.current = null;
    };
  }, []);

  const requestConflictChoice = useCallback((sourceName: string, targetLabel: string) => {
    return new Promise<ConflictChoice>((resolve) => {
      resolverRef.current = resolve;
      setConflictState({
        isOpen: true,
        sourceName,
        targetLabel,
      });
    });
  }, []);

  const resolveConflictChoice = useCallback((choice: ConflictChoice) => {
    resolverRef.current?.(choice);
    resolverRef.current = null;
    setConflictState({
      isOpen: false,
      sourceName: '',
      targetLabel: '',
    });
  }, []);

  const movePathsToDirectory = useCallback(async (sourcePaths: string[], targetDir: string, targetLabel: string) => {
    const compactPaths = compactDraggedPaths(sourcePaths);
    if (!projectPath) {
      return;
    }

    let successCount = 0;
    let overwriteCount = 0;
    let renameCount = 0;
    let skippedCount = 0;
    let failedCount = 0;

    for (const sourcePath of compactPaths) {
      if (isSameOrDescendantPath(targetDir, sourcePath)) {
        skippedCount += 1;
        continue;
      }

      let strategy: 'error' | 'overwrite' | 'rename' = 'error';

      while (true) {
        try {
          const finalPath = await invoke<string>('move_project_entry', {
            projectPath,
            source: sourcePath,
            target: targetDir,
            conflictStrategy: strategy,
          });

          applyMovedPath(sourcePath, finalPath);
          successCount += 1;

          if (strategy === 'overwrite') {
            overwriteCount += 1;
          } else if (strategy === 'rename') {
            renameCount += 1;
          }

          break;
        } catch (error) {
          const errorMessage = String(error);

          if (errorMessage.startsWith(CONFLICT_ERROR_PREFIX)) {
            const choice = await requestConflictChoice(getFileNameFromPath(sourcePath), targetLabel);

            if (choice === 'cancel') {
              skippedCount += 1;
              break;
            }

            strategy = choice;
            continue;
          }

          failedCount += 1;
          console.error('Move failed:', error);
          break;
        }
      }
    }

    await onAfterMove();

    if (successCount === 0 && skippedCount === 0 && failedCount === 0) {
      return;
    }

    const summaryParts = [];
    if (successCount > 0) summaryParts.push(`成功 ${successCount} 个`);
    if (overwriteCount > 0) summaryParts.push(`覆盖 ${overwriteCount} 个`);
    if (renameCount > 0) summaryParts.push(`重命名 ${renameCount} 个`);
    if (skippedCount > 0) summaryParts.push(`跳过 ${skippedCount} 个`);
    if (failedCount > 0) summaryParts.push(`失败 ${failedCount} 个`);

    showToast({
      title: failedCount > 0 ? (successCount > 0 ? '移动部分完成' : '移动失败') : '移动完成',
      message: `${summaryParts.join('，')}，目标目录：${targetLabel}`,
      tone: failedCount > 0 ? (successCount > 0 ? 'warning' : 'error') : 'success',
    });
  }, [applyMovedPath, onAfterMove, projectPath, requestConflictChoice, showToast]);

  const conflictDialog = (
    <MoveConflictDialog
      isOpen={conflictState.isOpen}
      sourceName={conflictState.sourceName}
      targetLabel={conflictState.targetLabel}
      onOverwrite={() => resolveConflictChoice('overwrite')}
      onRename={() => resolveConflictChoice('rename')}
      onCancel={() => resolveConflictChoice('cancel')}
    />
  );

  return {
    movePathsToDirectory,
    conflictDialog,
  };
}
