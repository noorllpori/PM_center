import type { MouseEvent, PointerEvent, ReactNode } from "react";
import { ExternalLink } from "lucide-react";
import { useExternalFileDrag } from "./useExternalFileDrag";

interface ExternalDragHandleProps {
  resolvePaths: () => string[];
  className?: string;
  iconClassName?: string;
  title?: string;
  children?: ReactNode;
}

function stopInteraction(
  event: MouseEvent<HTMLButtonElement> | PointerEvent<HTMLButtonElement>,
) {
  event.preventDefault();
  event.stopPropagation();
}

export function ExternalDragHandle({
  resolvePaths,
  className = "",
  iconClassName = "h-3.5 w-3.5",
  title = "拖到系统文件夹或外部程序",
  children,
}: ExternalDragHandleProps) {
  const { supported, handlePointerDown } = useExternalFileDrag(resolvePaths);

  if (!supported) {
    return null;
  }

  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      draggable={false}
      className={className}
      onPointerDown={handlePointerDown}
      onDragStart={stopInteraction}
      onClick={stopInteraction}
      onDoubleClick={stopInteraction}
    >
      {children ?? (
        <span className="pointer-events-none" draggable={false}>
          <ExternalLink className={iconClassName} />
        </span>
      )}
    </button>
  );
}
