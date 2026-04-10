import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MoveConflictDialog } from './MoveConflictDialog';
import {
  buildRenamedFileName,
  compactDraggedPaths,
  getFileNameFromPath,
  isSameOrDescendantPath,
  joinPath,
} from './dragDrop';
import { useProjectStoreShallow } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';

interface ConflictResolution {
  action: 'overwrite' | 'rename' | 'cancel';
  renameName?: string;
}

interface ConflictState {
  isOpen: boolean;
  sourceName: string;
  targetLabel: string;
  renameName: string;
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
    renameName: '',
  });

  const resolverRef = useRef<((choice: ConflictResolution) => void) | null>(null);

  useEffect(() => {
    return () => {
      resolverRef.current?.({ action: 'cancel' });
      resolverRef.current = null;
    };
  }, []);

  const buildSuggestedRename = useCallback(async (sourceName: string, targetDir: string) => {
    for (let index = 1; ; index += 1) {
      const candidate = buildRenamedFileName(sourceName, index);
      const exists = await invoke<boolean>('path_exists', {
        path: joinPath(targetDir, candidate),
      });
      if (!exists) {
        return candidate;
      }
    }
  }, []);

  const requestConflictChoice = useCallback(async (sourceName: string, targetLabel: string, targetDir: string) => {
    const renameName = await buildSuggestedRename(sourceName, targetDir);

    return new Promise<ConflictResolution>((resolve) => {
      resolverRef.current = resolve;
      setConflictState({
        isOpen: true,
        sourceName,
        targetLabel,
        renameName,
      });
    });
  }, [buildSuggestedRename]);

  const resolveConflictChoice = useCallback((choice: ConflictResolution) => {
    resolverRef.current?.(choice);
    resolverRef.current = null;
    setConflictState({
      isOpen: false,
      sourceName: '',
      targetLabel: '',
      renameName: '',
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

      let strategy: 'error' | 'overwrite' = 'error';
      let targetName: string | undefined;

      while (true) {
        try {
          const finalPath = await invoke<string>('move_project_entry', {
            projectPath,
            source: sourcePath,
            target: targetDir,
            conflictStrategy: strategy,
            targetName,
          });

          applyMovedPath(sourcePath, finalPath);
          successCount += 1;

          if (strategy === 'overwrite') {
            overwriteCount += 1;
          } else if (targetName) {
            renameCount += 1;
          }

          break;
        } catch (error) {
          const errorMessage = String(error);

          if (errorMessage.startsWith(CONFLICT_ERROR_PREFIX)) {
            const choice = await requestConflictChoice(
              targetName || getFileNameFromPath(sourcePath),
              targetLabel,
              targetDir,
            );

            if (choice.action === 'cancel') {
              skippedCount += 1;
              break;
            }

            if (choice.action === 'overwrite') {
              strategy = 'overwrite';
              targetName = undefined;
            } else {
              strategy = 'error';
              targetName = choice.renameName?.trim();
            }
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
      renameValue={conflictState.renameName}
      onRenameValueChange={(renameName) =>
        setConflictState((state) => ({
          ...state,
          renameName,
        }))
      }
      actionLabel="移动"
      renameButtonText="重命名移动"
      overwriteButtonText="覆盖移动"
      onOverwrite={() => resolveConflictChoice({ action: 'overwrite' })}
      onRename={() =>
        resolveConflictChoice({
          action: 'rename',
          renameName: conflictState.renameName,
        })
      }
      onCancel={() => resolveConflictChoice({ action: 'cancel' })}
    />
  );

  return {
    movePathsToDirectory,
    conflictDialog,
  };
}
