import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { CircleHelp } from 'lucide-react';

export type HelpAssistantPlacement =
  | 'top'
  | 'top-start'
  | 'top-end'
  | 'right'
  | 'right-start'
  | 'right-end'
  | 'bottom'
  | 'bottom-start'
  | 'bottom-end'
  | 'left'
  | 'left-start'
  | 'left-end';

export interface HelpAssistantImage {
  src: string;
  alt?: string;
}

export interface HelpAssistantVideo {
  src: string;
  poster?: string;
}

interface HelpAssistantProps {
  title?: string;
  text?: string | string[];
  images?: Array<string | HelpAssistantImage>;
  videos?: Array<string | HelpAssistantVideo>;
  placement?: HelpAssistantPlacement;
  width?: number | string;
  ariaLabel?: string;
  children?: ReactNode;
  className?: string;
}

interface PopoverPosition {
  left: number;
  top: number;
  ready: boolean;
}

const VIEWPORT_MARGIN = 8;
const POPOVER_GAP = 8;

function normalizeImages(items: Array<string | HelpAssistantImage>) {
  return items.flatMap((item) => {
    if (typeof item === 'string') {
      const src = item.trim();
      return src ? [{ src }] : [];
    }
    const src = item.src.trim();
    return src ? [{ ...item, src }] : [];
  });
}

function normalizeVideos(items: Array<string | HelpAssistantVideo>) {
  return items.flatMap((item) => {
    if (typeof item === 'string') {
      const src = item.trim();
      return src ? [{ src }] : [];
    }
    const src = item.src.trim();
    return src ? [{ ...item, src }] : [];
  });
}

function calculatePosition(
  trigger: DOMRect,
  panel: DOMRect,
  placement: HelpAssistantPlacement,
): Omit<PopoverPosition, 'ready'> {
  const [preferredSide, alignment = 'center'] = placement.split('-') as [
    'top' | 'right' | 'bottom' | 'left',
    'start' | 'end' | 'center',
  ];

  const coordinates = (side: 'top' | 'right' | 'bottom' | 'left') => {
    let left = trigger.left + (trigger.width - panel.width) / 2;
    let top = trigger.top + (trigger.height - panel.height) / 2;

    if (side === 'top') top = trigger.top - panel.height - POPOVER_GAP;
    if (side === 'bottom') top = trigger.bottom + POPOVER_GAP;
    if (side === 'left') left = trigger.left - panel.width - POPOVER_GAP;
    if (side === 'right') left = trigger.right + POPOVER_GAP;

    if (side === 'top' || side === 'bottom') {
      if (alignment === 'start') left = trigger.left;
      if (alignment === 'end') left = trigger.right - panel.width;
    } else {
      if (alignment === 'start') top = trigger.top;
      if (alignment === 'end') top = trigger.bottom - panel.height;
    }
    return { left, top };
  };

  let side = preferredSide;
  const preferred = coordinates(side);
  if (side === 'top' && preferred.top < VIEWPORT_MARGIN) side = 'bottom';
  else if (side === 'bottom' && preferred.top + panel.height > window.innerHeight - VIEWPORT_MARGIN) side = 'top';
  else if (side === 'left' && preferred.left < VIEWPORT_MARGIN) side = 'right';
  else if (side === 'right' && preferred.left + panel.width > window.innerWidth - VIEWPORT_MARGIN) side = 'left';

  const result = coordinates(side);
  return {
    left: Math.max(VIEWPORT_MARGIN, Math.min(result.left, window.innerWidth - panel.width - VIEWPORT_MARGIN)),
    top: Math.max(VIEWPORT_MARGIN, Math.min(result.top, window.innerHeight - panel.height - VIEWPORT_MARGIN)),
  };
}

export function HelpAssistant({
  title = '',
  text = '',
  images = [],
  videos = [],
  placement = 'top',
  width = 320,
  ariaLabel,
  children,
  className = '',
}: HelpAssistantProps) {
  const popoverId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const videoRefs = useRef<Array<HTMLVideoElement | null>>([]);
  const hideTimerRef = useRef<number | null>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [position, setPosition] = useState<PopoverPosition>({ left: 0, top: 0, ready: false });
  const textItems = (Array.isArray(text) ? text : [text]).map((item) => String(item).trim()).filter(Boolean);
  const imageItems = normalizeImages(images);
  const videoItems = normalizeVideos(videos);
  const panelWidth = typeof width === 'number' ? `${width}px` : width;

  const clearHideTimer = useCallback(() => {
    if (hideTimerRef.current !== null) {
      window.clearTimeout(hideTimerRef.current);
      hideTimerRef.current = null;
    }
  }, []);

  const show = useCallback(() => {
    clearHideTimer();
    setIsOpen(true);
  }, [clearHideTimer]);

  const scheduleHide = useCallback(() => {
    clearHideTimer();
    hideTimerRef.current = window.setTimeout(() => setIsOpen(false), 120);
  }, [clearHideTimer]);

  useLayoutEffect(() => {
    if (!isOpen || !triggerRef.current || !panelRef.current) return;
    const trigger = triggerRef.current;
    const panel = panelRef.current;
    const updatePosition = () => {
      const next = calculatePosition(trigger.getBoundingClientRect(), panel.getBoundingClientRect(), placement);
      setPosition({ ...next, ready: true });
    };

    updatePosition();
    const resizeObserver = new ResizeObserver(updatePosition);
    resizeObserver.observe(panel);
    window.addEventListener('resize', updatePosition);
    window.addEventListener('scroll', updatePosition, true);
    return () => {
      resizeObserver.disconnect();
      window.removeEventListener('resize', updatePosition);
      window.removeEventListener('scroll', updatePosition, true);
    };
  }, [isOpen, placement, panelWidth]);

  useEffect(() => {
    if (isOpen) return;
    setPosition((current) => ({ ...current, ready: false }));
    videoRefs.current.forEach((video) => video?.pause());
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen]);

  useEffect(() => () => clearHideTimer(), [clearHideTimer]);

  const popoverStyle: CSSProperties = {
    left: position.left,
    top: position.top,
    width: panelWidth,
    maxWidth: `calc(100vw - ${VIEWPORT_MARGIN * 2}px)`,
    visibility: position.ready ? 'visible' : 'hidden',
  };

  return (
    <span className={`inline-flex shrink-0 ${className}`}>
      <button
        ref={triggerRef}
        type="button"
        className="inline-flex h-4 w-4 shrink-0 cursor-help items-center justify-center rounded-full text-blue-600 transition-colors hover:bg-blue-50 hover:text-blue-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 dark:text-blue-400 dark:hover:bg-blue-950/50"
        aria-label={ariaLabel || (title ? `${title}帮助` : '帮助提示')}
        aria-expanded={isOpen}
        aria-controls={isOpen ? popoverId : undefined}
        onPointerEnter={show}
        onPointerLeave={scheduleHide}
        onFocus={show}
        onBlur={(event) => {
          if (panelRef.current?.contains(event.relatedTarget as Node | null)) return;
          scheduleHide();
        }}
        onPointerDown={(event) => {
          if (event.pointerType !== 'mouse') setIsOpen((current) => !current);
        }}
      >
        <CircleHelp className="h-3.5 w-3.5" strokeWidth={2} />
      </button>

      {isOpen && createPortal(
        <div
          ref={panelRef}
          id={popoverId}
          role="dialog"
          aria-label={title || '帮助提示'}
          style={popoverStyle}
          className="fixed z-[220] flex max-h-[min(520px,calc(100vh-16px))] flex-col gap-2.5 overflow-auto rounded-md border border-gray-200 bg-white p-3 text-left shadow-xl dark:border-gray-700 dark:bg-gray-900"
          onPointerEnter={show}
          onPointerLeave={scheduleHide}
          onFocus={show}
          onBlur={(event) => {
            if (triggerRef.current?.contains(event.relatedTarget as Node | null)) return;
            if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
            scheduleHide();
          }}
        >
          {title && <div className="text-sm font-semibold text-gray-900 dark:text-gray-100">{title}</div>}
          {textItems.length > 0 && (
            <div className="space-y-1.5 text-xs leading-5 text-gray-600 dark:text-gray-300">
              {textItems.map((item, index) => <p key={`${index}-${item}`} className="m-0 whitespace-pre-line">{item}</p>)}
            </div>
          )}
          {imageItems.length > 0 && (
            <div className="space-y-2">
              {imageItems.map((item, index) => (
                <img
                  key={`${index}-${item.src}`}
                  src={item.src}
                  alt={item.alt || title || '提示图片'}
                  loading="lazy"
                  className="block max-h-[220px] w-full rounded-md bg-gray-50 object-contain dark:bg-gray-950"
                />
              ))}
            </div>
          )}
          {videoItems.length > 0 && (
            <div className="space-y-2">
              {videoItems.map((item, index) => (
                <video
                  key={`${index}-${item.src}`}
                  ref={(element) => { videoRefs.current[index] = element; }}
                  src={item.src}
                  poster={item.poster}
                  controls
                  muted
                  playsInline
                  preload="metadata"
                  className="block max-h-[220px] w-full rounded-md bg-gray-950 object-contain"
                />
              ))}
            </div>
          )}
          {children}
        </div>,
        document.body,
      )}
    </span>
  );
}
