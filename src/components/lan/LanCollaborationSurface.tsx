import { useEffect, useMemo, useRef, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  ArrowLeft,
  ArrowDownToLine,
  ArrowUpFromLine,
  Building2,
  CheckCircle2,
  CircleUserRound,
  CircleStop,
  Clock3,
  ContactRound,
  Eraser,
  ExternalLink,
  FileUp,
  Files,
  FolderOpen,
  FolderUp,
  Globe2,
  History,
  ImagePlus,
  Inbox,
  MessageCircle,
  Paperclip,
  Radio,
  RefreshCw,
  Search,
  Send,
  Settings2,
  SquareArrowOutUpRight,
  Trash2,
  UserRound,
  Wifi,
  WifiOff,
  X,
} from 'lucide-react';
import { ConfirmDialog } from '../Dialog';
import { HelpAssistant } from '../ui/HelpAssistant';
import { useUiStore } from '../../stores/uiStore';
import { getInternalDragPaths, hasInternalDragData } from '../file-manager/dragDrop';
import {
  formatLanTransferBytes,
  getLanTransferStatusLabel,
  LanTransferCard,
  LanTransferIcon,
} from './LanTransferCard';
import { clipboardImageFiles, prepareBrowserFiles, type PreparedTransferFile } from './lanTransferFiles';
import {
  useLanCollaborationStore,
  type LanContact,
  type LanMessage,
  type LanProfile,
  type LanTransfer,
} from '../../stores/lanCollaborationStore';

type LanViewMode = 'messages' | 'contacts' | 'files' | 'profile';
type LanTransferFilter = 'all' | 'active' | 'completed' | 'attention';

interface LanCollaborationSurfaceProps {
  isActive?: boolean;
}

function deterministicColor(id: string) {
  const colors = [
    'bg-blue-500',
    'bg-emerald-500',
    'bg-violet-500',
    'bg-rose-500',
    'bg-amber-500',
    'bg-cyan-600',
  ];
  let value = 0;
  for (let index = 0; index < id.length; index += 1) {
    value = (value * 31 + id.charCodeAt(index)) >>> 0;
  }
  return colors[value % colors.length];
}

function avatarText(name: string) {
  return Array.from(name.trim())[0] || '?';
}

function LanAvatar({
  id,
  name,
  avatarPath,
  size = 'md',
  online,
}: {
  id: string;
  name: string;
  avatarPath?: string | null;
  size?: 'sm' | 'md' | 'lg' | 'xl';
  online?: boolean;
}) {
  const sizeClass = {
    sm: 'h-8 w-8 text-xs',
    md: 'h-10 w-10 text-sm',
    lg: 'h-12 w-12 text-base',
    xl: 'h-20 w-20 text-2xl',
  }[size];
  return (
    <div className="relative shrink-0">
      <div className={`flex ${sizeClass} items-center justify-center overflow-hidden rounded-full font-semibold text-white ${deterministicColor(id)}`}>
        {avatarPath ? (
          <img src={convertFileSrc(avatarPath)} alt="" className="h-full w-full object-cover" />
        ) : (
          avatarText(name)
        )}
      </div>
      {typeof online === 'boolean' ? (
        <span className={`absolute bottom-0 right-0 block h-2.5 w-2.5 rounded-full border-2 border-white dark:border-gray-900 ${online ? 'bg-emerald-500' : 'bg-gray-400'}`} />
      ) : null}
    </div>
  );
}

function formatClock(timestamp: number | null | undefined) {
  if (!timestamp) return '';
  const date = new Date(timestamp);
  const today = new Date();
  if (date.toDateString() === today.toDateString()) {
    return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
  }
  return date.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' });
}

function formatLastSeen(timestamp: number) {
  if (!timestamp) return '从未在线';
  const elapsed = Date.now() - timestamp;
  if (elapsed < 60_000) return '刚刚在线';
  if (elapsed < 60 * 60_000) return `${Math.floor(elapsed / 60_000)} 分钟前在线`;
  if (elapsed < 24 * 60 * 60_000) return `${Math.floor(elapsed / 3_600_000)} 小时前在线`;
  return new Date(timestamp).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
}

function lastMessageFor(conversationId: string, messages: LanMessage[]) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (messages[index].conversationId === conversationId) return messages[index];
  }
  return null;
}

function lastTransferFor(conversationId: string, transfers: LanTransfer[]) {
  for (let index = transfers.length - 1; index >= 0; index -= 1) {
    if (transfers[index].conversationId === conversationId) return transfers[index];
  }
  return null;
}

function isActiveTransfer(transfer: LanTransfer) {
  return transfer.status === 'waiting'
    || transfer.status === 'pending'
    || transfer.status === 'transferring';
}

function isAttentionTransfer(transfer: LanTransfer) {
  return transfer.status === 'failed' || transfer.status === 'rejected' || transfer.status === 'cancelled';
}

function transferStatusClass(transfer: LanTransfer) {
  if (transfer.status === 'completed') {
    return 'bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300';
  }
  if (transfer.status === 'failed' || transfer.status === 'rejected') {
    return 'bg-red-50 text-red-700 dark:bg-red-950/40 dark:text-red-300';
  }
  if (transfer.status === 'cancelled') {
    return 'bg-amber-50 text-amber-700 dark:bg-amber-950/40 dark:text-amber-300';
  }
  if (isActiveTransfer(transfer)) {
    return 'bg-blue-50 text-blue-700 dark:bg-blue-950/40 dark:text-blue-300';
  }
  return 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300';
}

type LanTimelineItem =
  | { kind: 'message'; timestamp: number; message: LanMessage }
  | { kind: 'transfer'; timestamp: number; transfer: LanTransfer };

const LOBBY_IMAGE_EXTENSIONS = /\.(?:png|jpe?g|gif|webp|bmp|tiff?|hdr|exr)$/i;
const MESSAGE_URL_PATTERN = /(?:https?:\/\/|www\.)[^\s<>"']+/gi;
const TRAILING_URL_PUNCTUATION = new Set([
  '.', ',', '!', '?', ';', ':',
  '。', '，', '！', '？', '；', '：', '、',
  ')', ']', '}', '）', '】', '》',
]);

function transferPathKey(path: string) {
  let normalized = path.replace(/\//g, '\\');
  if (normalized.startsWith('\\\\?\\UNC\\')) {
    normalized = `\\\\${normalized.slice(8)}`;
  } else if (normalized.startsWith('\\\\?\\')) {
    normalized = normalized.slice(4);
  }
  return normalized.toLocaleLowerCase();
}

function canSendPreparedImageToLobby(file: PreparedTransferFile) {
  return LOBBY_IMAGE_EXTENSIONS.test(file.name.trim());
}

function trimUrlPunctuation(value: string) {
  let link = value;
  let trailing = '';
  while (link.length > 0) {
    const character = link.slice(-1);
    if (!TRAILING_URL_PUNCTUATION.has(character)) break;
    if (character === ')' && (link.match(/\(/g)?.length || 0) >= (link.match(/\)/g)?.length || 0)) break;
    if (character === ']' && (link.match(/\[/g)?.length || 0) >= (link.match(/\]/g)?.length || 0)) break;
    if (character === '}' && (link.match(/\{/g)?.length || 0) >= (link.match(/\}/g)?.length || 0)) break;
    trailing = character + trailing;
    link = link.slice(0, -1);
  }
  return { link, trailing };
}

function normalizeMessageUrl(value: string) {
  try {
    const url = new URL(/^www\./i.test(value) ? `https://${value}` : value);
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.href : null;
  } catch {
    return null;
  }
}

interface LinkPreview {
  url: string;
  finalUrl: string;
  title: string;
  description: string | null;
  siteName: string;
  faviconDataUrl: string | null;
  imageDataUrl: string | null;
}

const linkPreviewRequests = new Map<string, Promise<LinkPreview | null>>();

function firstPreviewUrl(content: string) {
  for (const match of content.matchAll(MESSAGE_URL_PATTERN)) {
    const { link } = trimUrlPunctuation(match[0]);
    const href = normalizeMessageUrl(link);
    if (href) return href;
  }
  return null;
}

function requestLinkPreview(url: string) {
  const existing = linkPreviewRequests.get(url);
  if (existing) return existing;
  const request = invoke<LinkPreview>('get_link_preview', { url })
    .catch((error) => {
      linkPreviewRequests.delete(url);
      console.debug('LAN link preview unavailable:', error);
      return null;
    });
  if (linkPreviewRequests.size >= 256) {
    const oldestUrl = linkPreviewRequests.keys().next().value;
    if (oldestUrl) linkPreviewRequests.delete(oldestUrl);
  }
  linkPreviewRequests.set(url, request);
  return request;
}

function LinkPreviewCard({ url }: { url: string }) {
  const cardRef = useRef<HTMLDivElement | null>(null);
  const [preview, setPreview] = useState<LinkPreview | null>(null);

  useEffect(() => {
    const element = cardRef.current;
    if (!element) return;
    let cancelled = false;
    let requested = false;

    const loadPreview = () => {
      if (requested) return;
      requested = true;
      void requestLinkPreview(url).then((result) => {
        if (!cancelled && result) setPreview(result);
      });
    };

    if (typeof IntersectionObserver === 'undefined') {
      loadPreview();
      return () => {
        cancelled = true;
      };
    }

    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return;
      observer.disconnect();
      loadPreview();
    }, { rootMargin: '240px 0px' });
    observer.observe(element);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [url]);

  return (
    <div ref={cardRef} className={preview ? 'mt-2' : 'h-px'}>
      {preview ? (
        <button
          type="button"
          title="使用默认浏览器打开"
          onClick={() => {
            void openUrl(preview.finalUrl).catch((error) => {
              console.warn('Failed to open LAN link preview:', error);
            });
          }}
          className="group flex w-full max-w-[560px] flex-col overflow-hidden rounded-md border border-gray-200 bg-white text-left shadow-sm transition-colors hover:border-blue-300 hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-900 dark:hover:border-blue-700 dark:hover:bg-gray-900/80 sm:flex-row"
        >
          <span className="min-w-0 flex-1 px-3 py-2.5">
            <span className="flex items-center gap-1.5 text-[11px] text-gray-500 dark:text-gray-400">
              {preview.faviconDataUrl ? (
                <img src={preview.faviconDataUrl} alt="" className="h-4 w-4 rounded-sm object-cover" />
              ) : (
                <Globe2 className="h-3.5 w-3.5" />
              )}
              <span className="truncate">{preview.siteName}</span>
              <ExternalLink className="ml-auto h-3 w-3 shrink-0 opacity-0 transition-opacity group-hover:opacity-100" />
            </span>
            <span className="mt-1.5 line-clamp-2 block break-words text-sm font-medium leading-5 text-gray-900 dark:text-gray-100">
              {preview.title}
            </span>
            {preview.description ? (
              <span className="mt-1 line-clamp-2 block break-words text-xs leading-4 text-gray-500 dark:text-gray-400">
                {preview.description}
              </span>
            ) : null}
          </span>
          {preview.imageDataUrl ? (
            <img
              src={preview.imageDataUrl}
              alt=""
              className="order-first h-28 w-full shrink-0 border-b border-gray-200 object-cover dark:border-gray-700 sm:order-none sm:w-44 sm:self-stretch sm:border-b-0 sm:border-l"
            />
          ) : null}
        </button>
      ) : null}
    </div>
  );
}

function MessageText({ content, mine }: { content: string; mine: boolean }) {
  const parts: React.ReactNode[] = [];
  let cursor = 0;
  for (const match of content.matchAll(MESSAGE_URL_PATTERN)) {
    const index = match.index ?? 0;
    if (index > cursor) parts.push(content.slice(cursor, index));
    const raw = match[0];
    const { link, trailing } = trimUrlPunctuation(raw);
    const href = normalizeMessageUrl(link);
    if (href) {
      parts.push(
        <a
          key={`${index}:${link}`}
          href={href}
          title="使用默认浏览器打开"
          onClick={(event) => {
            event.preventDefault();
            if (event.detail > 1) return;
            void openUrl(href).catch((error) => {
              console.warn('Failed to open LAN message link:', error);
            });
          }}
          className={`cursor-pointer break-all underline decoration-1 underline-offset-2 ${mine ? 'text-blue-100 hover:text-white' : 'text-blue-600 hover:text-blue-700 dark:text-blue-300 dark:hover:text-blue-200'}`}
        >
          {link}
        </a>,
      );
      if (trailing) parts.push(trailing);
    } else {
      parts.push(raw);
    }
    cursor = index + raw.length;
  }
  if (cursor < content.length) parts.push(content.slice(cursor));
  return <>{parts}</>;
}

function NavigationButton({
  active,
  title,
  badge = 0,
  onClick,
  children,
}: {
  active: boolean;
  title: string;
  badge?: number;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className={`relative flex h-10 w-10 items-center justify-center rounded-md transition-colors ${active ? 'bg-blue-100 text-blue-700 dark:bg-blue-950/60 dark:text-blue-300' : 'text-gray-500 hover:bg-gray-200/70 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100'}`}
    >
      {children}
      {badge > 0 ? (
        <span className="absolute -right-1 -top-1 min-w-4 rounded-full bg-red-500 px-1 text-[9px] font-semibold leading-4 text-white">
          {badge > 99 ? '99+' : badge}
        </span>
      ) : null}
    </button>
  );
}

function ConversationRow({
  title,
  subtitle,
  time,
  unread,
  selected,
  avatar,
  onClick,
}: {
  title: string;
  subtitle: string;
  time?: string;
  unread: number;
  selected: boolean;
  avatar: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex w-full items-center gap-3 border-b border-gray-100 px-3 py-3 text-left transition-colors dark:border-gray-800 ${selected ? 'bg-blue-50 dark:bg-blue-950/30' : 'hover:bg-gray-50 dark:hover:bg-gray-900/70'}`}
    >
      {avatar}
      <span className="min-w-0 flex-1">
        <span className="flex items-center justify-between gap-2">
          <span className="truncate text-sm font-medium text-gray-900 dark:text-gray-100">{title}</span>
          <span className="shrink-0 text-[10px] text-gray-400">{time}</span>
        </span>
        <span className="mt-0.5 flex items-center justify-between gap-2">
          <span className="truncate text-xs text-gray-500 dark:text-gray-400">{subtitle}</span>
          {unread > 0 ? (
            <span className="min-w-5 shrink-0 rounded-full bg-red-500 px-1.5 text-center text-[10px] font-semibold leading-5 text-white">
              {unread > 99 ? '99+' : unread}
            </span>
          ) : null}
        </span>
      </span>
    </button>
  );
}

function MessageBubble({
  message,
  profile,
  senderAvatarPath,
}: {
  message: LanMessage;
  profile: LanProfile;
  senderAvatarPath?: string | null;
}) {
  const mine = message.fromId === profile.id;
  const previewUrl = firstPreviewUrl(message.content);
  return (
    <div className={`flex gap-2.5 ${mine ? 'justify-end' : 'justify-start'}`}>
      {!mine ? <LanAvatar id={message.fromId} name={message.fromName} avatarPath={senderAvatarPath} size="sm" /> : null}
      <div className={`max-w-[min(72%,680px)] ${mine ? 'items-end' : 'items-start'}`}>
        {!mine ? <p className="mb-1 px-1 text-[11px] text-gray-500">{message.fromName}</p> : null}
        <div className={`whitespace-pre-wrap break-words rounded-md px-3 py-2 text-sm leading-6 ${mine ? 'bg-blue-600 text-white' : 'bg-gray-100 text-gray-900 dark:bg-gray-800 dark:text-gray-100'}`}>
          <MessageText content={message.content} mine={mine} />
        </div>
        {previewUrl ? <LinkPreviewCard url={previewUrl} /> : null}
        <div className={`mt-1 flex items-center gap-2 px-1 text-[10px] text-gray-400 ${mine ? 'justify-end' : ''}`}>
          <span>{new Date(message.timestamp).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>
          {mine && message.deliveryStatus !== 'delivered' ? (
            <span className={message.deliveryStatus === 'failed' ? 'text-red-500' : 'text-amber-600'}>
              {message.deliveryStatus === 'failed' ? '发送失败' : '部分送达'}
            </span>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function LanFileManagementPanel({
  transfer,
  summary,
  progress,
  contactsById,
  receiveDirectory,
  busy,
  onBack,
  onOpenConversation,
  onAccept,
  onReject,
  onCancel,
  cancelling,
}: {
  transfer: LanTransfer | null;
  summary: { total: number; active: number; completed: number; totalBytes: number };
  progress?: { transferredBytes: number; totalBytes: number; bytesPerSecond: number };
  contactsById: Map<string, LanContact>;
  receiveDirectory?: string | null;
  busy: boolean;
  onBack: () => void;
  onOpenConversation: (transfer: LanTransfer) => void;
  onAccept: (transfer: LanTransfer) => void;
  onReject: (transfer: LanTransfer) => void;
  onCancel: (transfer: LanTransfer) => void;
  cancelling: boolean;
}) {
  const outgoing = transfer?.direction === 'outgoing';
  const localPath = transfer
    ? transfer.receivedPath || (outgoing ? transfer.sourcePath : null)
    : null;
  const contactName = transfer
    ? transfer.conversationId === 'lobby'
      ? '局域网大厅'
      : outgoing
        ? contactsById.get(transfer.toId)?.displayName || '未知联系人'
        : transfer.fromName
    : '';
  const autoReceivingImage = Boolean(
    transfer
      && !outgoing
      && transfer.status === 'pending'
      && transfer.totalBytes <= 100 * 1024 * 1024
      && transfer.mimeType?.startsWith('image/'),
  );
  const canRespond = Boolean(
    transfer
      && !outgoing
      && !autoReceivingImage
      && (transfer.status === 'pending' || transfer.status === 'failed' || transfer.status === 'cancelled'),
  );
  const canCancel = transfer?.status === 'transferring';
  const transferredBytes = transfer?.status === 'completed'
    ? transfer.totalBytes
    : progress?.transferredBytes || 0;
  const progressPercent = transfer && transfer.totalBytes > 0
    ? Math.min(100, Math.round((transferredBytes / transfer.totalBytes) * 100))
    : transfer?.status === 'completed' ? 100 : 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex h-16 shrink-0 items-center gap-3 border-b border-gray-200 px-4 dark:border-gray-800">
        <button type="button" onClick={onBack} className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 lg:hidden">
          <ArrowLeft className="h-4 w-4" />
        </button>
        <div className="flex h-9 w-9 items-center justify-center rounded-md bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300">
          <Files className="h-5 w-5" />
        </div>
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-sm font-semibold">文件统筹</h2>
          <p className="truncate text-xs text-gray-500">统一查看局域网收发记录与本地文件</p>
        </div>
        <button
          type="button"
          disabled={!receiveDirectory}
          onClick={() => receiveDirectory && void invoke('open_path', { path: receiveDirectory })}
          className="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-300 px-2.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-40 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-900"
        >
          <FolderOpen className="h-3.5 w-3.5" />接收目录
        </button>
      </header>

      <div className="grid shrink-0 grid-cols-2 border-b border-gray-200 bg-gray-50 dark:border-gray-800 dark:bg-gray-900/50 sm:grid-cols-4">
        {[
          { label: '传输记录', value: String(summary.total), icon: Files },
          { label: '进行中', value: String(summary.active), icon: Clock3 },
          { label: '已完成', value: String(summary.completed), icon: CheckCircle2 },
          { label: '数据总量', value: formatLanTransferBytes(summary.totalBytes), icon: ArrowDownToLine },
        ].map((item) => (
          <div key={item.label} className="flex min-w-0 items-center gap-2.5 border-r border-gray-200 px-4 py-3 last:border-r-0 dark:border-gray-800">
            <item.icon className="h-4 w-4 shrink-0 text-gray-400" />
            <div className="min-w-0">
              <p className="text-[10px] text-gray-500">{item.label}</p>
              <p className="mt-0.5 truncate text-sm font-semibold">{item.value}</p>
            </div>
          </div>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {transfer ? (
          <div className="mx-auto max-w-4xl px-5 py-5">
            {transfer.status === 'completed' && transfer.kind === 'file' && transfer.mimeType?.startsWith('image/') && localPath ? (
              <div className="mb-5 overflow-hidden rounded-md border border-gray-200 bg-gray-100 dark:border-gray-800 dark:bg-gray-900">
                <img src={convertFileSrc(localPath)} alt="" className="max-h-72 w-full object-contain" />
              </div>
            ) : null}

            <div className="flex items-start gap-3 border-b border-gray-200 pb-5 dark:border-gray-800">
              <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300">
                <LanTransferIcon kind={transfer.kind} mimeType={transfer.mimeType} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <h3 className="min-w-0 break-words text-base font-semibold">{transfer.displayName}</h3>
                  <span className={`shrink-0 rounded px-2 py-0.5 text-xs ${transferStatusClass(transfer)}`}>
                    {getLanTransferStatusLabel(transfer)}
                  </span>
                </div>
                <p className="mt-1 text-xs text-gray-500">
                  {outgoing ? <ArrowUpFromLine className="mr-1 inline h-3.5 w-3.5" /> : <ArrowDownToLine className="mr-1 inline h-3.5 w-3.5" />}
                  {outgoing ? '发送给' : '接收自'} {contactName}
                </p>
              </div>
            </div>

            <dl className="grid gap-x-8 gap-y-4 border-b border-gray-200 py-5 text-sm dark:border-gray-800 sm:grid-cols-2">
              <div><dt className="text-xs text-gray-500">类型</dt><dd className="mt-1">{transfer.kind === 'directory' ? `文件夹 · ${transfer.itemCount} 个项目` : transfer.mimeType || '普通文件'}</dd></div>
              <div><dt className="text-xs text-gray-500">大小</dt><dd className="mt-1">{formatLanTransferBytes(transfer.totalBytes)}</dd></div>
              <div><dt className="text-xs text-gray-500">创建时间</dt><dd className="mt-1">{new Date(transfer.createdAt).toLocaleString('zh-CN')}</dd></div>
              <div><dt className="text-xs text-gray-500">最近更新</dt><dd className="mt-1">{new Date(transfer.updatedAt).toLocaleString('zh-CN')}</dd></div>
            </dl>

            {isActiveTransfer(transfer) || transfer.status === 'completed' ? (
              <div className="border-b border-gray-200 py-5 dark:border-gray-800">
                <div className="flex items-center justify-between gap-4 text-xs">
                  <span className="font-medium">传输进度</span>
                  <span className="text-gray-500">
                    {formatLanTransferBytes(transferredBytes)} / {formatLanTransferBytes(transfer.totalBytes)}
                    {!outgoing && progress?.bytesPerSecond ? ` · ${formatLanTransferBytes(progress.bytesPerSecond)}/s` : ''}
                  </span>
                </div>
                <div className="mt-2 h-2 overflow-hidden rounded bg-gray-200 dark:bg-gray-800">
                  <div className="h-full bg-blue-500 transition-[width]" style={{ width: `${progressPercent}%` }} />
                </div>
              </div>
            ) : null}

            {transfer.error ? (
              <div className="border-b border-gray-200 py-5 dark:border-gray-800">
                <p className="text-xs font-medium text-red-600 dark:text-red-300">失败原因</p>
                <p className="mt-1 break-words text-sm text-red-600 dark:text-red-300">{transfer.error}</p>
              </div>
            ) : null}

            {localPath ? (
              <div className="border-b border-gray-200 py-5 dark:border-gray-800">
                <p className="text-xs text-gray-500">本地路径</p>
                <p className="mt-1 break-all text-sm">{localPath}</p>
              </div>
            ) : null}

            <div className="flex flex-wrap justify-end gap-2 pt-5">
              <button type="button" onClick={() => onOpenConversation(transfer)} className="inline-flex h-9 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-sm text-gray-700 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-900">
                <MessageCircle className="h-4 w-4" />定位到会话
              </button>
              {canRespond ? (
                <>
                  <button type="button" disabled={busy} onClick={() => onReject(transfer)} className="inline-flex h-9 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-900">
                    <X className="h-4 w-4" />拒绝
                  </button>
                  <button type="button" disabled={busy} onClick={() => onAccept(transfer)} className="inline-flex h-9 items-center gap-1.5 rounded-md bg-blue-600 px-3 text-sm text-white hover:bg-blue-700 disabled:opacity-50">
                    <ArrowDownToLine className="h-4 w-4" />{transfer.status === 'failed' || transfer.status === 'cancelled' ? '重新接收' : '接收'}
                  </button>
                </>
              ) : null}
              {canCancel ? (
                <div className="inline-flex items-center gap-1.5">
                  <HelpAssistant
                    title="中断文件传输"
                    text={[
                      '发送方或接收方都可以立即中断正在进行的传输，对方会同步显示为“已中断”。',
                      '接收端只写入临时文件或临时目录；中断后未完成内容会自动清理，不会覆盖最终目标。',
                      '已中断的接收记录可以点击“重新接收”从头开始，目前不支持从已有字节断点续传。',
                    ]}
                    placement="top-end"
                    width={340}
                  />
                  <button
                    type="button"
                    disabled={cancelling}
                    onClick={() => onCancel(transfer)}
                    className="inline-flex h-9 items-center gap-1.5 rounded-md border border-red-200 px-3 text-sm text-red-600 hover:bg-red-50 disabled:opacity-50 dark:border-red-900/60 dark:text-red-300 dark:hover:bg-red-950/30"
                  >
                    <CircleStop className="h-4 w-4" />{cancelling ? '中断中...' : '中断传输'}
                  </button>
                </div>
              ) : null}
              {transfer.status === 'completed' && localPath && transfer.kind === 'directory' ? (
                <button type="button" onClick={() => void invoke('open_path', { path: localPath })} className="inline-flex h-9 items-center gap-1.5 rounded-md bg-blue-600 px-3 text-sm text-white hover:bg-blue-700">
                  <FolderOpen className="h-4 w-4" />打开目录
                </button>
              ) : transfer.status === 'completed' && localPath ? (
                <>
                  <button type="button" onClick={() => void invoke('show_in_folder', { path: localPath })} className="inline-flex h-9 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-sm text-gray-700 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-900">
                    <FolderOpen className="h-4 w-4" />所在目录
                  </button>
                  <button type="button" onClick={() => void invoke('open_file', { path: localPath })} className="inline-flex h-9 items-center gap-1.5 rounded-md bg-blue-600 px-3 text-sm text-white hover:bg-blue-700">
                    <SquareArrowOutUpRight className="h-4 w-4" />打开文件
                  </button>
                </>
              ) : null}
            </div>
          </div>
        ) : (
          <div className="flex h-full min-h-64 flex-col items-center justify-center px-6 text-center text-gray-400">
            <Files className="h-10 w-10" />
            <p className="mt-3 text-sm font-medium text-gray-600 dark:text-gray-300">暂无文件传输记录</p>
            <p className="mt-1 text-xs">在聊天中发送或接收文件后会统一显示在这里</p>
          </div>
        )}
      </div>
    </div>
  );
}

export function LanCollaborationSurface({ isActive = true }: LanCollaborationSurfaceProps) {
  const {
    profile,
    localSettings,
    contacts,
    messages,
    transfers,
    transferProgress,
    conversations,
    unreadCount,
    service,
    navigationRequest,
    isLoading,
    error,
    initialize,
    refresh,
    updateProfile,
    updateReceiveDirectory,
    updateDiscoverySubnet,
    scanDiscoverySubnet,
    setAvatar,
    startDiscovery,
    stopDiscovery,
    sendMessage,
    offerFiles,
    respondTransfer,
    cancelTransfer,
    createTransferStagingPath,
    discardTransferStagingFile,
    markConversationRead,
    clearConversation,
    clearHistory,
    removeContact,
    requestConversationNavigation,
    clearConversationNavigation,
  } = useLanCollaborationStore();
  const showToast = useUiStore((state) => state.showToast);
  const [mode, setMode] = useState<LanViewMode>('messages');
  const [transferFilter, setTransferFilter] = useState<LanTransferFilter>('all');
  const [selectedTransferId, setSelectedTransferId] = useState<string | null>(null);
  const [selectedConversationId, setSelectedConversationId] = useState('lobby');
  const [query, setQuery] = useState('');
  const [input, setInput] = useState('');
  const [isSending, setIsSending] = useState(false);
  const [isPreparingFiles, setIsPreparingFiles] = useState(false);
  const [showAttachmentMenu, setShowAttachmentMenu] = useState(false);
  const [isFileDragOver, setIsFileDragOver] = useState(false);
  const [transferActions, setTransferActions] = useState<Set<string>>(() => new Set());
  const transferActionsRef = useRef<Set<string>>(new Set());
  const [cancellingTransfers, setCancellingTransfers] = useState<Set<string>>(() => new Set());
  const cancelledByUserRef = useRef<Set<string>>(new Set());
  const [showMobileConversation, setShowMobileConversation] = useState(false);
  const [profileName, setProfileName] = useState('');
  const [profileDepartment, setProfileDepartment] = useState('');
  const [isProfileDraftDirty, setIsProfileDraftDirty] = useState(false);
  const [isSavingProfile, setIsSavingProfile] = useState(false);
  const [isUpdatingReceiveDirectory, setIsUpdatingReceiveDirectory] = useState(false);
  const [discoverySubnet, setDiscoverySubnet] = useState('');
  const [isScanningSubnet, setIsScanningSubnet] = useState(false);
  const [confirmAction, setConfirmAction] = useState<'conversation' | 'history' | null>(null);
  const [scrollRequest, setScrollRequest] = useState(0);
  const messagesViewportRef = useRef<HTMLDivElement | null>(null);
  const attachmentMenuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    void initialize().catch((initializationError) => {
      showToast({ title: '局域网服务初始化失败', message: String(initializationError), tone: 'error' });
    });
  }, [initialize, showToast]);

  useEffect(() => {
    if (!showAttachmentMenu) return;
    const closeMenu = (event: MouseEvent) => {
      if (attachmentMenuRef.current?.contains(event.target as Node)) return;
      setShowAttachmentMenu(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setShowAttachmentMenu(false);
    };
    window.addEventListener('mousedown', closeMenu);
    window.addEventListener('keydown', closeOnEscape);
    return () => {
      window.removeEventListener('mousedown', closeMenu);
      window.removeEventListener('keydown', closeOnEscape);
    };
  }, [showAttachmentMenu]);

  useEffect(() => {
    if (!profile || isProfileDraftDirty) return;
    setProfileName(profile.displayName);
    setProfileDepartment(profile.department);
  }, [isProfileDraftDirty, profile]);

  useEffect(() => {
    setDiscoverySubnet(localSettings?.discoverySubnet || '');
  }, [localSettings?.discoverySubnet]);

  const selectedContactId = selectedConversationId.startsWith('direct:')
    ? selectedConversationId.slice('direct:'.length)
    : null;
  const selectedContact = contacts.find((contact) => contact.id === selectedContactId) || null;
  const selectedConversation = conversations.find((conversation) => conversation.id === selectedConversationId) || conversations[0] || null;
  const selectedMessages = useMemo(
    () => messages.filter((message) => message.conversationId === selectedConversationId),
    [messages, selectedConversationId],
  );
  const selectedTransfers = useMemo(() => {
    const matches = transfers.filter((transfer) => transfer.conversationId === selectedConversationId);
    if (selectedConversationId !== 'lobby') return matches;
    const byLobbyItem = new Map<string, LanTransfer>();
    for (const transfer of matches) {
      const key = transfer.lobbyItemId || transfer.id;
      const previous = byLobbyItem.get(key);
      const currentHasLocalCopy = Boolean(transfer.receivedPath || transfer.sourcePath);
      const previousHasLocalCopy = Boolean(previous?.receivedPath || previous?.sourcePath);
      if (!previous || (currentHasLocalCopy && !previousHasLocalCopy) || transfer.id === transfer.lobbyItemId) {
        byLobbyItem.set(key, transfer);
      }
    }
    return [...byLobbyItem.values()].sort((left, right) => left.createdAt - right.createdAt);
  }, [selectedConversationId, transfers]);
  const selectedTimeline = useMemo<LanTimelineItem[]>(() => [
    ...selectedMessages.map((message) => ({ kind: 'message' as const, timestamp: message.timestamp, message })),
    ...selectedTransfers.map((transfer) => ({ kind: 'transfer' as const, timestamp: transfer.createdAt, transfer })),
  ].sort((left, right) => left.timestamp - right.timestamp), [selectedMessages, selectedTransfers]);
  const contactsById = useMemo(
    () => new Map(contacts.map((contact) => [contact.id, contact] as const)),
    [contacts],
  );
  const displayTransfers = useMemo(() => {
    const byItem = new Map<string, LanTransfer>();
    for (const transfer of transfers) {
      const key = transfer.conversationId === 'lobby'
        ? `lobby:${transfer.lobbyItemId || transfer.id}`
        : `direct:${transfer.id}`;
      const previous = byItem.get(key);
      const currentHasLocalCopy = Boolean(transfer.receivedPath || transfer.sourcePath);
      const previousHasLocalCopy = Boolean(previous?.receivedPath || previous?.sourcePath);
      if (!previous
        || transfer.id === transfer.lobbyItemId
        || (currentHasLocalCopy && !previousHasLocalCopy)
        || (currentHasLocalCopy === previousHasLocalCopy && transfer.updatedAt > previous.updatedAt)) {
        byItem.set(key, transfer);
      }
    }
    return [...byItem.values()];
  }, [transfers]);
  const managedTransfers = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return [...displayTransfers]
      .filter((transfer) => {
        if (transferFilter === 'active' && !isActiveTransfer(transfer)) return false;
        if (transferFilter === 'completed' && transfer.status !== 'completed') return false;
        if (transferFilter === 'attention' && !isAttentionTransfer(transfer)) return false;
        if (!normalized) return true;
        const contactName = transfer.conversationId === 'lobby'
          ? '局域网大厅'
          : transfer.direction === 'outgoing'
            ? contactsById.get(transfer.toId)?.displayName || ''
            : transfer.fromName;
        return [
          transfer.displayName,
          transfer.sourcePath || '',
          transfer.receivedPath || '',
          contactName,
          getLanTransferStatusLabel(transfer),
        ].some((value) => value.toLocaleLowerCase().includes(normalized));
      })
      .sort((left, right) => right.updatedAt - left.updatedAt || right.createdAt - left.createdAt);
  }, [contactsById, displayTransfers, query, transferFilter]);
  const selectedManagedTransfer = managedTransfers.find((transfer) => transfer.id === selectedTransferId)
    || managedTransfers[0]
    || null;
  const transferSummary = useMemo(() => ({
    total: displayTransfers.length,
    active: displayTransfers.filter(isActiveTransfer).length,
    completed: displayTransfers.filter((transfer) => transfer.status === 'completed').length,
    totalBytes: displayTransfers.reduce((sum, transfer) => sum + Math.max(0, transfer.totalBytes || 0), 0),
  }), [displayTransfers]);

  useEffect(() => {
    if (!isActive || !selectedConversation || selectedConversation.unreadCount === 0) return;
    void markConversationRead(selectedConversation.id);
  }, [isActive, markConversationRead, selectedConversation, selectedTimeline.length]);

  useEffect(() => {
    if (!isActive) return;
    const viewport = messagesViewportRef.current;
    if (!viewport) return;
    const scrollToDestination = () => {
      const targetMessageId = navigationRequest?.conversationId === selectedConversationId
        ? navigationRequest.messageId
        : null;
      const targetTransferId = navigationRequest?.conversationId === selectedConversationId
        ? navigationRequest.transferId
        : null;
      const target = targetMessageId
        ? viewport.querySelector<HTMLElement>(`[data-lan-message-id="${CSS.escape(targetMessageId)}"]`)
        : targetTransferId
          ? viewport.querySelector<HTMLElement>(`[data-lan-transfer-id="${CSS.escape(targetTransferId)}"]`)
          : null;
      if (target) {
        target.scrollIntoView({ block: 'center' });
      } else {
        viewport.scrollTop = viewport.scrollHeight;
      }
    };
    scrollToDestination();
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      scrollToDestination();
      secondFrame = window.requestAnimationFrame(scrollToDestination);
    });
    const content = viewport.firstElementChild;
    const resizeObserver = content instanceof HTMLElement
      ? new ResizeObserver(scrollToDestination)
      : null;
    if (content instanceof HTMLElement) resizeObserver?.observe(content);
    const settleTimer = window.setTimeout(() => resizeObserver?.disconnect(), 800);
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame) window.cancelAnimationFrame(secondFrame);
      window.clearTimeout(settleTimer);
      resizeObserver?.disconnect();
    };
  }, [isActive, navigationRequest, scrollRequest, selectedConversationId, selectedTimeline.length, showMobileConversation]);

  useEffect(() => {
    if (!isActive || !navigationRequest) return;
    setSelectedConversationId(navigationRequest.conversationId);
    setMode('messages');
    setShowMobileConversation(true);
    setScrollRequest((current) => current + 1);
  }, [isActive, navigationRequest]);

  useEffect(() => {
    if (mode !== 'messages') setIsFileDragOver(false);
  }, [mode]);

  const filteredContacts = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return contacts;
    return contacts.filter((contact) => [contact.displayName, contact.department, contact.ip]
      .some((value) => value.toLocaleLowerCase().includes(normalized)));
  }, [contacts, query]);

  const departmentGroups = useMemo(() => {
    const groups = new Map<string, LanContact[]>();
    for (const contact of filteredContacts) {
      const department = contact.department.trim() || '未分组联系人';
      const items = groups.get(department) || [];
      items.push(contact);
      groups.set(department, items);
    }
    return Array.from(groups.entries())
      .map(([department, items]) => ({
        department,
        contacts: items.sort((left, right) => Number(right.online) - Number(left.online) || left.displayName.localeCompare(right.displayName, 'zh-CN')),
      }))
      .sort((left, right) => left.department === '未分组联系人' ? 1 : right.department === '未分组联系人' ? -1 : left.department.localeCompare(right.department, 'zh-CN'));
  }, [filteredContacts]);

  const selectConversation = (conversationId: string) => {
    clearConversationNavigation();
    setSelectedConversationId(conversationId);
    setMode('messages');
    setShowMobileConversation(true);
    setScrollRequest((current) => current + 1);
  };

  const openTransferConversation = (transfer: LanTransfer) => {
    requestConversationNavigation(transfer.conversationId, null, transfer.id);
  };

  const handleSend = async () => {
    const content = input.trim();
    if (!content || isSending) return;
    setIsSending(true);
    try {
      const result = await sendMessage(selectedConversationId, content);
      setInput('');
      if (result.failures.length > 0) {
        showToast({
          title: selectedConversationId === 'lobby'
            ? '大厅消息已发布'
            : result.deliveredCount > 0 ? '消息部分送达' : '消息发送失败',
          message: selectedConversationId === 'lobby'
            ? `消息已保存在本机；${result.failures.length} 台设备未即时收到，上线后会补齐。`
            : `已送达 ${result.deliveredCount}/${result.targetCount}；${result.failures.map((failure) => failure.userName).join('、')} 未收到`,
          tone: 'warning',
        });
      } else if (selectedConversationId === 'lobby' && result.targetCount === 0) {
        showToast({ title: '大厅消息已发布', message: '当前没有在线设备，设备上线后会自动同步。', tone: 'success' });
      }
    } catch (sendError) {
      showToast({ title: '消息发送失败', message: String(sendError), tone: 'error' });
    } finally {
      setIsSending(false);
    }
  };

  const requireTransferContact = (minProtocolVersion = 3, contentLabel = '文件') => {
    if (!selectedContactId || !selectedContact) {
      showToast({ title: '请选择联系人', message: '文件传输需要在联系人私聊中发起。', tone: 'warning' });
      return null;
    }
    if (!selectedContact.online) {
      showToast({ title: '联系人离线', message: '对方在线后才能接收文件传输请求。', tone: 'warning' });
      return null;
    }
    if (selectedContact.protocolVersion < minProtocolVersion) {
      showToast({
        title: '客户端版本不兼容',
        message: `对方需要升级后才能使用${contentLabel}传输。`,
        tone: 'warning',
      });
      return null;
    }
    return selectedContact;
  };

  const offerPreparedFiles = async (prepared: PreparedTransferFile[]) => {
    const isLobby = selectedConversationId === 'lobby';
    const onlineLobbyContacts = contacts.filter((contact) => contact.online);
    const lobbyContacts = onlineLobbyContacts;
    const contact = isLobby ? null : requireTransferContact();
    if (isPreparingFiles) {
      await Promise.all(prepared
        .filter((item) => item.staged)
        .map((item) => discardTransferStagingFile(item.path).catch(() => {})));
      return;
    }
    const eligible = isLobby
      ? prepared.filter(canSendPreparedImageToLobby)
      : prepared;
    const rejected = prepared.filter((item) => !eligible.includes(item));
    if (rejected.length > 0) {
      await Promise.all(rejected
        .filter((item) => item.staged)
        .map((item) => discardTransferStagingFile(item.path).catch(() => {})));
      showToast({
        title: '大厅只支持图片',
        message: `${rejected.length} 项非图片内容未发送；普通文件和文件夹请在联系人私聊中发送。`,
        tone: 'warning',
      });
    }
    if ((!isLobby && !contact) || eligible.length === 0) return;
    setIsPreparingFiles(true);
    try {
      const recipients = isLobby ? lobbyContacts.map((item) => item.id) : contact!.id;
      const result = await offerFiles(
        recipients,
        eligible.map((item) => item.path),
        isLobby ? 'lobby' : null,
      );
      const successfulPaths = new Set(result.transfers
        .map((transfer) => transfer.sourcePath ? transferPathKey(transfer.sourcePath) : null)
        .filter((path): path is string => Boolean(path)));
      await Promise.all(eligible
        .filter((item) => item.staged && !successfulPaths.has(transferPathKey(item.path)))
        .map((item) => discardTransferStagingFile(item.path).catch(() => {})));
      if (result.transfers.length > 0) {
        const deliveredLobbyContacts = new Set(result.transfers
          .filter((transfer) => transfer.toId)
          .map((transfer) => transfer.toId)).size;
        const missedLobbyContacts = Math.max(0, onlineLobbyContacts.length - deliveredLobbyContacts);
        showToast({
          title: isLobby ? '大厅图片已发布' : '传输请求已发送',
          message: isLobby
            ? deliveredLobbyContacts > 0
              ? `已保存并向 ${deliveredLobbyContacts} 台在线设备同步 ${eligible.length} 张图片${missedLobbyContacts > 0 ? `；其余设备上线后会补齐` : ''}。`
              : `已保存 ${eligible.length} 张图片，设备上线后会自动同步。`
            : `${result.transfers.length} 项内容正在等待 ${contact!.displayName} 接收。`,
          tone: 'success',
        });
      }
      if (result.failures.length > 0 && !isLobby) {
        showToast({
          title: result.transfers.length > 0 ? '部分文件未发送' : '文件发送失败',
          message: result.failures.slice(0, 3).map((failure) => `${failure.path.split(/[\\/]/).pop()}：${failure.error}`).join('；'),
          tone: result.transfers.length > 0 ? 'warning' : 'error',
        });
      }
    } catch (offerError) {
      await Promise.all(eligible
        .filter((item) => item.staged)
        .map((item) => discardTransferStagingFile(item.path).catch(() => {})));
      showToast({ title: isLobby ? '大厅图片发送失败' : '文件发送失败', message: String(offerError), tone: 'error' });
    } finally {
      setIsPreparingFiles(false);
    }
  };

  const handleChooseFiles = async () => {
    const isLobby = selectedConversationId === 'lobby';
    if (!isLobby && !requireTransferContact()) return;
    const selected = await open({
      title: isLobby ? '选择要发送到大厅的图片' : '选择要发送的文件',
      multiple: true,
      directory: false,
      filters: isLobby ? [{
        name: '图片',
        extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'tif', 'tiff', 'hdr', 'exr'],
      }] : undefined,
    });
    const paths = typeof selected === 'string' ? [selected] : selected;
    if (!paths?.length) return;
    await offerPreparedFiles(paths.map((path) => ({
      path,
      staged: false,
      name: path.split(/[\\/]/).pop() || path,
    })));
  };

  const handleChooseDirectory = async () => {
    if (selectedConversationId === 'lobby') {
      showToast({ title: '大厅只支持图片', message: '文件夹请在联系人私聊中发送。', tone: 'warning' });
      return;
    }
    if (!requireTransferContact(4, '目录')) return;
    const selected = await open({
      title: '选择要发送的文件夹',
      multiple: false,
      directory: true,
    });
    if (typeof selected !== 'string' || !selected) return;
    await offerPreparedFiles([{
      path: selected,
      staged: false,
      name: selected.split(/[\\/]/).pop() || selected,
    }]);
  };

  const handleFileDrop = async (event: React.DragEvent<HTMLElement>) => {
    event.preventDefault();
    event.stopPropagation();
    setIsFileDragOver(false);
    const isLobby = selectedConversationId === 'lobby';
    if (!isLobby && !requireTransferContact()) return;
    const internalPaths = getInternalDragPaths(event.dataTransfer);
    if (internalPaths.length > 0) {
      await offerPreparedFiles(internalPaths.map((path) => ({
        path,
        staged: false,
        name: path.split(/[\\/]/).pop() || path,
      })));
      return;
    }
    const files = Array.from(event.dataTransfer.files || []);
    const containsDirectory = Array.from(event.dataTransfer.items || []).some((item) => {
      const entry = (item as DataTransferItem & {
        webkitGetAsEntry?: () => FileSystemEntry | null;
      }).webkitGetAsEntry?.();
      return Boolean(entry?.isDirectory);
    });
    if (containsDirectory) {
      if (isLobby) {
        showToast({ title: '大厅只支持图片', message: '文件夹请在联系人私聊中发送。', tone: 'warning' });
        return;
      }
      if (!requireTransferContact(4, '目录')) return;
      const directoryPaths = files
        .map((file) => (file as File & { path?: string }).path)
        .filter((path): path is string => Boolean(path));
      if (directoryPaths.length > 0) {
        await offerPreparedFiles(directoryPaths.map((path) => ({
          path,
          staged: false,
          name: path.split(/[\\/]/).pop() || path,
        })));
        return;
      }
      showToast({ title: '无法读取文件夹路径', message: '请使用输入框旁的附件菜单选择“发送文件夹”。', tone: 'warning' });
      return;
    }
    if (files.length === 0) {
      showToast({ title: '没有可发送的内容', message: '请使用附件菜单选择文件或文件夹。', tone: 'warning' });
      return;
    }
    try {
      const filesToPrepare = isLobby
        ? files.filter((file) => file.type.startsWith('image/'))
        : files;
      if (filesToPrepare.length === 0) {
        showToast({ title: '大厅只支持图片', message: '拖入的内容中没有可发送的图片。', tone: 'warning' });
        return;
      }
      const prepared = await prepareBrowserFiles(
        filesToPrepare,
        createTransferStagingPath,
        discardTransferStagingFile,
      );
      await offerPreparedFiles(prepared);
    } catch (dropError) {
      showToast({ title: '准备拖入文件失败', message: String(dropError), tone: 'error' });
    }
  };

  const handleClipboardPaste = async (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const images = clipboardImageFiles(event.clipboardData);
    if (images.length === 0) return;
    event.preventDefault();
    if (selectedConversationId !== 'lobby' && !requireTransferContact()) return;
    try {
      const prepared = await prepareBrowserFiles(
        images,
        createTransferStagingPath,
        discardTransferStagingFile,
        true,
      );
      await offerPreparedFiles(prepared);
    } catch (pasteError) {
      showToast({ title: '准备剪贴板图片失败', message: String(pasteError), tone: 'error' });
    }
  };

  const beginTransferAction = (transferId: string) => {
    if (transferActionsRef.current.has(transferId)) return false;
    transferActionsRef.current.add(transferId);
    setTransferActions((current) => new Set(current).add(transferId));
    return true;
  };

  const endTransferAction = (transferId: string) => {
    transferActionsRef.current.delete(transferId);
    setTransferActions((current) => {
      const next = new Set(current);
      next.delete(transferId);
      return next;
    });
  };

  const handleAcceptTransfer = async (transfer: LanTransfer) => {
    if (!beginTransferAction(transfer.id)) return;
    cancelledByUserRef.current.delete(transfer.id);
    try {
      const completed = await respondTransfer(transfer.id, 'accept');
      showToast({
        title: transfer.kind === 'directory' ? '目录接收完成' : '文件接收完成',
        message: completed.receivedPath || transfer.displayName,
        tone: 'success',
      });
    } catch (acceptError) {
      const message = String(acceptError);
      if (!cancelledByUserRef.current.delete(transfer.id)) {
        showToast({
          title: message.includes('传输已中断') ? '文件传输已被对方中断' : transfer.kind === 'directory' ? '目录接收失败' : '文件接收失败',
          message,
          tone: message.includes('传输已中断') ? 'warning' : 'error',
        });
      }
    } finally {
      endTransferAction(transfer.id);
    }
  };

  const handleChooseReceiveDirectory = async () => {
    if (isUpdatingReceiveDirectory) return;
    const selected = await open({
      title: '选择局域网文件接收目录',
      multiple: false,
      directory: true,
      defaultPath: localSettings?.receiveDirectory,
    });
    if (typeof selected !== 'string') return;
    setIsUpdatingReceiveDirectory(true);
    try {
      await updateReceiveDirectory(selected);
      showToast({ title: '接收目录已更新', message: selected, tone: 'success' });
    } catch (directoryError) {
      showToast({ title: '更新接收目录失败', message: String(directoryError), tone: 'error' });
    } finally {
      setIsUpdatingReceiveDirectory(false);
    }
  };

  const handleRejectTransfer = async (transfer: LanTransfer) => {
    if (!beginTransferAction(transfer.id)) return;
    try {
      await respondTransfer(transfer.id, 'reject');
    } catch (rejectError) {
      showToast({ title: '拒绝文件失败', message: String(rejectError), tone: 'error' });
    } finally {
      endTransferAction(transfer.id);
    }
  };

  const handleSaveProfile = async () => {
    if (!profileName.trim() || isSavingProfile) return;
    setIsSavingProfile(true);
    try {
      await updateProfile(profileName, profileDepartment);
      const savedProfile = useLanCollaborationStore.getState().profile;
      if (savedProfile) {
        setProfileName(savedProfile.displayName);
        setProfileDepartment(savedProfile.department);
      }
      setIsProfileDraftDirty(false);
      showToast({ title: '个人资料已更新', message: '新名称和部门已向在线联系人广播。', tone: 'success' });
    } catch (saveError) {
      showToast({ title: '保存个人资料失败', message: String(saveError), tone: 'error' });
    } finally {
      setIsSavingProfile(false);
    }
  };

  const handleChooseAvatar = async () => {
    const selected = await open({
      title: '选择局域网头像',
      multiple: false,
      directory: false,
      filters: [{ name: '图片', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'gif'] }],
    });
    if (typeof selected !== 'string') return;
    try {
      await setAvatar(selected);
      showToast({ title: '头像已更新', message: '仅压缩后的 128×128 缩略图会在局域网中同步。', tone: 'success' });
    } catch (avatarError) {
      showToast({ title: '更新头像失败', message: String(avatarError), tone: 'error' });
    }
  };

  const handleRemoveContact = async (contact: LanContact) => {
    try {
      await removeContact(contact.id);
      if (selectedContactId === contact.id) setSelectedConversationId('lobby');
      showToast({ title: '联系人已移除', message: `${contact.displayName} 的聊天记录仍然保留。`, tone: 'success' });
    } catch (removeError) {
      showToast({ title: '移除联系人失败', message: String(removeError), tone: 'error' });
    }
  };

  const handleToggleDiscovery = async () => {
    try {
      if (service.isRunning) {
        await stopDiscovery();
      } else {
        await startDiscovery();
      }
    } catch (discoveryError) {
      showToast({ title: '切换局域网发现失败', message: String(discoveryError), tone: 'error' });
    }
  };

  const handleCancelTransfer = async (transfer: LanTransfer) => {
    if (cancellingTransfers.has(transfer.id)) return;
    cancelledByUserRef.current.add(transfer.id);
    setCancellingTransfers((current) => new Set(current).add(transfer.id));
    try {
      await cancelTransfer(transfer.id);
      showToast({
        title: '文件传输已中断',
        message: `${transfer.displayName} 的未完成传输已停止。`,
        tone: 'warning',
      });
    } catch (cancelError) {
      cancelledByUserRef.current.delete(transfer.id);
      showToast({ title: '中断文件传输失败', message: String(cancelError), tone: 'error' });
    } finally {
      setCancellingTransfers((current) => {
        const next = new Set(current);
        next.delete(transfer.id);
        return next;
      });
    }
  };

  const handleSaveAndScanSubnet = async () => {
    if (isScanningSubnet) return;
    setIsScanningSubnet(true);
    try {
      const result = await updateDiscoverySubnet(discoverySubnet);
      setDiscoverySubnet(result.discoverySubnet);
      showToast({
        title: result.discoverySubnet ? '隧道网段已保存并扫描' : '隧道网段已清除',
        message: result.discoverySubnet
          ? `已向 ${result.sentCount}/${result.targetCount} 个可用地址发送发现请求。`
          : '后续只使用原有局域网广播发现。',
        tone: 'success',
      });
    } catch (scanError) {
      showToast({ title: '保存或扫描网段失败', message: String(scanError), tone: 'error' });
    } finally {
      setIsScanningSubnet(false);
    }
  };

  const handleScanSubnet = async () => {
    if (isScanningSubnet) return;
    setIsScanningSubnet(true);
    try {
      const result = await scanDiscoverySubnet();
      showToast({
        title: '网段扫描已发送',
        message: `已向 ${result.sentCount}/${result.targetCount} 个可用地址发送发现请求。`,
        tone: 'success',
      });
    } catch (scanError) {
      showToast({ title: '扫描网段失败', message: String(scanError), tone: 'error' });
    } finally {
      setIsScanningSubnet(false);
    }
  };

  const handleRemoveAvatar = async () => {
    try {
      await setAvatar(null);
    } catch (avatarError) {
      showToast({ title: '移除头像失败', message: String(avatarError), tone: 'error' });
    }
  };

  const canSend = Boolean(
    profile
      && service.isRunning
      && (selectedConversationId === 'lobby' || selectedContact?.online),
  );
  const canTransfer = Boolean(
    profile
      && service.isRunning
      && (selectedConversationId === 'lobby'
        ? true
        : selectedContact?.online && selectedContact.protocolVersion >= 3),
  );

  if (isLoading && !profile) {
    return <div className="flex h-full items-center justify-center bg-white text-sm text-gray-500 dark:bg-gray-950">正在初始化局域网联系人...</div>;
  }

  if (!profile) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 bg-white p-6 text-center dark:bg-gray-950">
        <WifiOff className="h-9 w-9 text-red-500" />
        <p className="text-sm font-medium text-gray-900 dark:text-gray-100">局域网服务无法初始化</p>
        <p className="max-w-lg text-xs leading-5 text-gray-500">{error || '请检查应用数据目录权限后重试。'}</p>
        <button type="button" onClick={() => void initialize()} className="rounded-md bg-blue-600 px-3 py-2 text-sm text-white hover:bg-blue-700">重新初始化</button>
      </div>
    );
  }

  const middlePanel = (
    <section className={`${showMobileConversation ? 'hidden lg:flex' : 'flex'} min-w-0 w-full flex-col border-r border-gray-200 bg-white dark:border-gray-800 dark:bg-gray-950 lg:w-[310px] lg:shrink-0`}>
      <div className="border-b border-gray-200 px-3 py-3 dark:border-gray-800">
        <div className="flex items-center gap-2">
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-400" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={mode === 'contacts'
                ? '搜索联系人...'
                : mode === 'files'
                  ? '搜索文件、联系人或状态...'
                  : '搜索会话...'}
              className="h-9 w-full rounded-md border border-gray-200 bg-gray-50 pl-8 pr-8 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-100"
            />
            {query ? <button type="button" onClick={() => setQuery('')} className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400"><X className="h-4 w-4" /></button> : null}
          </div>
          <button type="button" onClick={() => void refresh()} title="刷新局域网数据" className="flex h-9 w-9 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800">
            <RefreshCw className="h-4 w-4" />
          </button>
        </div>
        {mode === 'files' ? (
          <div className="mt-2 grid grid-cols-4 gap-1 rounded-md bg-gray-100 p-1 dark:bg-gray-900">
            {([
              ['all', '全部'],
              ['active', '进行中'],
              ['completed', '已完成'],
              ['attention', '异常'],
            ] as const).map(([value, label]) => (
              <button
                key={value}
                type="button"
                onClick={() => setTransferFilter(value)}
                className={`h-7 rounded text-[11px] transition-colors ${transferFilter === value ? 'bg-white font-medium text-gray-900 shadow-sm dark:bg-gray-800 dark:text-gray-100' : 'text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200'}`}
              >
                {label}
              </button>
            ))}
          </div>
        ) : null}
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {mode === 'messages' ? (
          <>
            {conversations
              .filter((conversation) => conversation.id === 'lobby' || conversation.lastMessageAt || contacts.some((contact) => contact.id === conversation.contactId && contact.online))
              .filter((conversation) => conversation.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
              .map((conversation) => {
                const contact = conversation.contactId ? contacts.find((item) => item.id === conversation.contactId) : null;
                const lastMessage = lastMessageFor(conversation.id, messages);
                const lastTransfer = lastTransferFor(conversation.id, transfers);
                const transferIsLatest = Boolean(lastTransfer && (!lastMessage || lastTransfer.createdAt >= lastMessage.timestamp));
                return (
                  <ConversationRow
                    key={conversation.id}
                    title={conversation.title}
                    subtitle={transferIsLatest && lastTransfer
                      ? `${lastTransfer.direction === 'outgoing' ? '我：' : ''}[文件] ${lastTransfer.displayName}`
                      : lastMessage
                        ? `${lastMessage.fromId === profile.id ? '我：' : ''}${lastMessage.content}`
                        : conversation.id === 'lobby'
                          ? '所有在线联系人都能看到'
                          : contact?.online ? '在线' : formatLastSeen(contact?.lastSeen || 0)}
                    time={formatClock(conversation.lastMessageAt)}
                    unread={conversation.unreadCount}
                    selected={conversation.id === selectedConversationId}
                    onClick={() => selectConversation(conversation.id)}
                    avatar={conversation.id === 'lobby'
                      ? <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-blue-600 text-white"><Radio className="h-5 w-5" /></div>
                      : <LanAvatar id={contact?.id || conversation.id} name={contact?.displayName || conversation.title} avatarPath={contact?.avatarPath} online={contact?.online} />}
                  />
                );
              })}
          </>
        ) : mode === 'contacts' ? (
          departmentGroups.length > 0 ? departmentGroups.map((group) => (
            <div key={group.department}>
              <div className="sticky top-0 z-10 flex items-center justify-between border-b border-gray-100 bg-gray-50 px-3 py-2 text-xs font-medium text-gray-600 dark:border-gray-800 dark:bg-gray-900 dark:text-gray-300">
                <span className="flex items-center gap-1.5"><Building2 className="h-3.5 w-3.5" />{group.department}</span>
                <span>{group.contacts.filter((contact) => contact.online).length}/{group.contacts.length}</span>
              </div>
              {group.contacts.map((contact) => (
                <div key={contact.id} className="group flex items-center border-b border-gray-100 pr-2 dark:border-gray-800">
                  <button type="button" onClick={() => selectConversation(`direct:${contact.id}`)} className="flex min-w-0 flex-1 items-center gap-3 px-3 py-3 text-left hover:bg-gray-50 dark:hover:bg-gray-900/70">
                    <LanAvatar id={contact.id} name={contact.displayName} avatarPath={contact.avatarPath} online={contact.online} />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium text-gray-900 dark:text-gray-100">{contact.displayName}</span>
                      <span className="mt-0.5 block truncate text-xs text-gray-500">{contact.online ? `${contact.ip} · 在线` : formatLastSeen(contact.lastSeen)}</span>
                    </span>
                  </button>
                  {!contact.online ? (
                    <button type="button" onClick={() => void handleRemoveContact(contact)} title="移除离线联系人" className="invisible flex h-8 w-8 items-center justify-center rounded-md text-gray-400 hover:bg-red-50 hover:text-red-600 group-hover:visible dark:hover:bg-red-950/30">
                      <Trash2 className="h-4 w-4" />
                    </button>
                  ) : null}
                </div>
              ))}
            </div>
          )) : <div className="px-4 py-12 text-center text-sm text-gray-400">没有匹配的联系人</div>
        ) : mode === 'files' ? (
          managedTransfers.length > 0 ? managedTransfers.map((transfer) => {
            const outgoing = transfer.direction === 'outgoing';
            const contactName = transfer.conversationId === 'lobby'
              ? '局域网大厅'
              : outgoing
                ? contactsById.get(transfer.toId)?.displayName || '未知联系人'
                : transfer.fromName;
            const progress = transferProgress[transfer.id];
            const progressPercent = transfer.totalBytes > 0
              ? Math.min(100, Math.round(((progress?.transferredBytes || 0) / transfer.totalBytes) * 100))
              : 0;
            return (
              <button
                key={transfer.id}
                type="button"
                onClick={() => {
                  setSelectedTransferId(transfer.id);
                  setShowMobileConversation(true);
                }}
                className={`block w-full border-b border-gray-100 px-3 py-3 text-left transition-colors dark:border-gray-800 ${selectedManagedTransfer?.id === transfer.id ? 'bg-blue-50 dark:bg-blue-950/25' : 'hover:bg-gray-50 dark:hover:bg-gray-900/70'}`}
              >
                <span className="flex items-start gap-2.5">
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300">
                    <LanTransferIcon kind={transfer.kind} mimeType={transfer.mimeType} />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-2">
                      <span className="min-w-0 flex-1 truncate text-sm font-medium" title={transfer.displayName}>{transfer.displayName}</span>
                      <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] ${transferStatusClass(transfer)}`}>
                        {getLanTransferStatusLabel(transfer)}
                      </span>
                    </span>
                    <span className="mt-1 flex items-center justify-between gap-2 text-[11px] text-gray-500">
                      <span className="min-w-0 truncate">{outgoing ? '发往' : '来自'} {contactName}</span>
                      <span className="shrink-0">{formatLanTransferBytes(transfer.totalBytes)}</span>
                    </span>
                    {transfer.status === 'transferring' ? (
                      <span className="mt-2 block h-1 overflow-hidden rounded bg-gray-200 dark:bg-gray-800">
                        <span className="block h-full bg-blue-500" style={{ width: `${progressPercent}%` }} />
                      </span>
                    ) : null}
                  </span>
                </span>
              </button>
            );
          }) : (
            <div className="flex h-full min-h-48 flex-col items-center justify-center px-5 text-center text-gray-400">
              <Files className="h-8 w-8" />
              <p className="mt-2 text-sm">没有匹配的传输记录</p>
            </div>
          )
        ) : (
          <div className="space-y-1 p-2">
            <button type="button" onClick={() => void handleToggleDiscovery()} className="flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-800">
              {service.isRunning ? <WifiOff className="h-4 w-4" /> : <Wifi className="h-4 w-4" />}
              <span>{service.isRunning ? '停止局域网发现' : '开始局域网发现'}</span>
            </button>
            <button type="button" onClick={() => setConfirmAction('history')} className="flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm text-red-600 hover:bg-red-50 dark:text-red-300 dark:hover:bg-red-950/30">
              <Eraser className="h-4 w-4" />
              <span>清空全部聊天记录</span>
            </button>
          </div>
        )}
      </div>
    </section>
  );

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 bg-white text-gray-900 dark:bg-gray-950 dark:text-gray-100">
      <aside className="flex w-14 shrink-0 flex-col items-center border-r border-gray-200 bg-gray-100 py-3 dark:border-gray-800 dark:bg-gray-900">
        <LanAvatar id={profile.id} name={profile.displayName} avatarPath={profile.avatarPath} size="md" />
        <div className="mt-5 flex flex-col gap-2">
          <NavigationButton active={mode === 'messages'} title="最近消息" badge={unreadCount} onClick={() => { setQuery(''); setMode('messages'); setShowMobileConversation(false); }}><MessageCircle className="h-5 w-5" /></NavigationButton>
          <NavigationButton active={mode === 'contacts'} title="联系人" onClick={() => { setQuery(''); setMode('contacts'); setShowMobileConversation(false); }}><ContactRound className="h-5 w-5" /></NavigationButton>
          <NavigationButton active={mode === 'files'} title="文件统筹" onClick={() => { setQuery(''); setMode('files'); setShowMobileConversation(false); }}><Files className="h-5 w-5" /></NavigationButton>
        </div>
        <div className="mt-auto flex flex-col items-center gap-2">
          <span title={service.isRunning ? '发现服务运行中' : '发现服务已停止'} className={`h-2.5 w-2.5 rounded-full ${service.isRunning ? 'bg-emerald-500' : 'bg-gray-400'}`} />
          <NavigationButton active={mode === 'profile'} title="个人资料与设置" onClick={() => { setQuery(''); setMode('profile'); setShowMobileConversation(true); }}><Settings2 className="h-5 w-5" /></NavigationButton>
        </div>
      </aside>

      {middlePanel}

      <main
        className={`${showMobileConversation ? 'flex' : 'hidden lg:flex'} relative min-w-0 flex-1 flex-col bg-white dark:bg-gray-950`}
        onDragEnter={(event) => {
          if (mode !== 'messages') return;
          const isFileDrag = hasInternalDragData(event.dataTransfer)
            || Array.from(event.dataTransfer.types || []).includes('Files');
          if (!isFileDrag) return;
          event.preventDefault();
          setIsFileDragOver(true);
        }}
        onDragOver={(event) => {
          if (mode !== 'messages') return;
          const isFileDrag = hasInternalDragData(event.dataTransfer)
            || Array.from(event.dataTransfer.types || []).includes('Files');
          if (!isFileDrag) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = 'copy';
          setIsFileDragOver(true);
        }}
        onDragLeave={(event) => {
          if (event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget)) return;
          setIsFileDragOver(false);
        }}
        onDrop={(event) => {
          if (mode !== 'messages') return;
          void handleFileDrop(event);
        }}
      >
        {isFileDragOver ? (
          <div className="pointer-events-none absolute inset-3 z-50 flex items-center justify-center rounded-md border-2 border-dashed border-blue-400 bg-blue-50/95 text-blue-700 shadow-sm dark:border-blue-600 dark:bg-blue-950/90 dark:text-blue-200">
            <div className="text-center">
              <Paperclip className="mx-auto h-7 w-7" />
              <p className="mt-2 text-sm font-medium">
                {selectedConversationId === 'lobby' ? '拖到此处向大厅发送图片' : '拖到此处发送文件或文件夹'}
              </p>
              <p className="mt-1 text-xs opacity-75">
                {selectedConversationId === 'lobby' ? '图片会保存到大厅，并在设备上线后自动补齐' : '文件夹和普通文件由对方确认，图片会自动接收'}
              </p>
            </div>
          </div>
        ) : null}
        {mode === 'files' ? (
          <LanFileManagementPanel
            transfer={selectedManagedTransfer}
            summary={transferSummary}
            progress={selectedManagedTransfer ? transferProgress[selectedManagedTransfer.id] : undefined}
            contactsById={contactsById}
            receiveDirectory={localSettings?.receiveDirectory}
            busy={Boolean(selectedManagedTransfer && transferActions.has(selectedManagedTransfer.id))}
            cancelling={Boolean(selectedManagedTransfer && cancellingTransfers.has(selectedManagedTransfer.id))}
            onBack={() => setShowMobileConversation(false)}
            onOpenConversation={openTransferConversation}
            onAccept={(transfer) => void handleAcceptTransfer(transfer)}
            onReject={(transfer) => void handleRejectTransfer(transfer)}
            onCancel={(transfer) => void handleCancelTransfer(transfer)}
          />
        ) : mode === 'profile' ? (
          <div className="min-h-0 flex-1 overflow-auto">
            <div className="border-b border-gray-200 px-5 py-4 dark:border-gray-800">
              <div className="flex items-center gap-2">
                <CircleUserRound className="h-5 w-5 text-blue-600" />
                <h2 className="text-base font-semibold">个人资料与局域网状态</h2>
                <HelpAssistant
                  title="局域网协同说明"
                  text={[
                    '消息和压缩头像仅在当前局域网内传输，不经过云端，但内容未加密，请只在可信网络中使用。',
                    '私聊中的文件和文件夹在联系人确认后保存；大厅文字和图片会在设备间同步，新设备会从在线副本合并最近 30 条内容。图片使用临时路径和 BLAKE3 完整性校验。',
                    '无法互相发现时，请允许 PM Center 通过 Windows 专用网络防火墙访问 UDP 31523 和 TCP 31524。',
                  ]}
                  placement="bottom-start"
                  width={360}
                />
              </div>
            </div>
            <div className="mx-auto max-w-3xl px-5 py-6">
              <div className="flex flex-col gap-6 sm:flex-row">
                <div className="flex shrink-0 flex-col items-center gap-2">
                  <LanAvatar id={profile.id} name={profile.displayName} avatarPath={profile.avatarPath} size="xl" />
                  <button type="button" onClick={() => void handleChooseAvatar()} className="flex items-center gap-1.5 rounded-md border border-gray-300 px-2.5 py-1.5 text-xs text-gray-700 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-900">
                    <ImagePlus className="h-3.5 w-3.5" />更换头像
                  </button>
                  {profile.avatarPath ? <button type="button" onClick={() => void handleRemoveAvatar()} className="text-xs text-gray-500 hover:text-red-600">移除头像</button> : null}
                </div>
                <div className="min-w-0 flex-1 space-y-4">
                  <label className="block">
                    <span className="mb-1.5 block text-sm font-medium">显示名称</span>
                    <input
                      value={profileName}
                      onChange={(event) => {
                        setProfileName(event.target.value);
                        setIsProfileDraftDirty(true);
                      }}
                      maxLength={32}
                      className="h-10 w-full rounded-md border border-gray-300 px-3 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-gray-900"
                    />
                  </label>
                  <label className="block">
                    <span className="mb-1.5 block text-sm font-medium">部门</span>
                    <input
                      value={profileDepartment}
                      onChange={(event) => {
                        setProfileDepartment(event.target.value);
                        setIsProfileDraftDirty(true);
                      }}
                      maxLength={40}
                      placeholder="未填写时归入未分组联系人"
                      className="h-10 w-full rounded-md border border-gray-300 px-3 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-gray-900"
                    />
                  </label>
                  <div className="flex justify-end">
                    <button type="button" disabled={isSavingProfile || !profileName.trim()} onClick={() => void handleSaveProfile()} className="rounded-md bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700 disabled:opacity-50">{isSavingProfile ? '保存中...' : '保存资料'}</button>
                  </div>
                </div>
              </div>

              <div className="mt-8 border-t border-gray-200 pt-5 dark:border-gray-800">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <h3 className="text-sm font-semibold">文件接收</h3>
                    <p className="mt-1 text-xs text-gray-500">按发送者名称建立子目录；同名文件或文件夹自动编号，不覆盖已有内容。</p>
                  </div>
                  <div className="flex shrink-0 gap-2">
                    <button
                      type="button"
                      disabled={!localSettings?.receiveDirectory}
                      onClick={() => localSettings?.receiveDirectory && void invoke('open_path', { path: localSettings.receiveDirectory })}
                      className="inline-flex items-center gap-1.5 rounded-md border border-gray-300 px-2.5 py-1.5 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-900"
                    >
                      <FolderOpen className="h-3.5 w-3.5" />打开
                    </button>
                    <button
                      type="button"
                      disabled={isUpdatingReceiveDirectory}
                      onClick={() => void handleChooseReceiveDirectory()}
                      className="rounded-md bg-blue-600 px-3 py-1.5 text-xs text-white hover:bg-blue-700 disabled:opacity-50"
                    >
                      {isUpdatingReceiveDirectory ? '更新中...' : '更改位置'}
                    </button>
                  </div>
                </div>
                <p className="mt-3 break-all rounded-md bg-gray-100 px-3 py-2 text-xs text-gray-600 dark:bg-gray-900 dark:text-gray-300">
                  {localSettings?.receiveDirectory || '正在读取接收目录...'}
                </p>
                <p className="mt-2 text-xs text-gray-500">图片不需要手动确认，校验通过后会自动保存到对应联系人目录。</p>
              </div>

              <div className="mt-8 border-t border-gray-200 pt-5 dark:border-gray-800">
                <div className="flex items-center justify-between gap-4">
                  <div>
                    <div className="flex items-center gap-1.5">
                      <h3 className="text-sm font-semibold">网络诊断</h3>
                      <HelpAssistant
                        title="局域网发现方式"
                        text={[
                          'PMC 默认每 4 秒通过 UDP 31523 在当前局域网广播，收到请求的设备会单播回复；消息和文件使用 TCP 31524。',
                          'WireGuard、Tailscale 等隧道通常不转发广播，可另外配置隧道 CIDR 并手动发送一次单播发现请求。',
                          '隧道两端不必都登记：任意一端扫描成功后，接收方会记住扫描方并回复，因此双方都会建立联系人。',
                          '发现成功后只会对已知设备 IP 发送轻量保活，不会继续遍历整个隧道网段。设备或地址变化时再手动扫描一次即可。',
                        ]}
                        placement="bottom-start"
                        width={360}
                      />
                    </div>
                    <p className="mt-1 text-xs text-gray-500">UDP 31523 · TCP 31524 · {service.onlineCount} 人在线</p>
                  </div>
                  <button type="button" onClick={() => void handleToggleDiscovery()} className={`rounded-md px-3 py-2 text-sm ${service.isRunning ? 'bg-gray-100 text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-200' : 'bg-blue-600 text-white hover:bg-blue-700'}`}>
                    {service.isRunning ? '停止发现' : '开始发现'}
                  </button>
                </div>
                <dl className="mt-4 grid gap-x-6 gap-y-3 text-sm sm:grid-cols-2">
                  <div><dt className="text-xs text-gray-500">发现端口</dt><dd className="mt-0.5">{service.udpBound ? '已绑定' : '未绑定'}</dd></div>
                  <div><dt className="text-xs text-gray-500">消息端口</dt><dd className="mt-0.5">{service.tcpBound ? '已绑定' : '未绑定'}</dd></div>
                  <div className="sm:col-span-2"><dt className="text-xs text-gray-500">本机地址</dt><dd className="mt-0.5 break-all">{service.localAddresses.join('、') || '等待网络探测'}</dd></div>
                  <div className="sm:col-span-2"><dt className="text-xs text-gray-500">最近发现</dt><dd className="mt-0.5">{service.lastDiscoveryAt ? new Date(service.lastDiscoveryAt).toLocaleString('zh-CN') : '尚未发现其他设备'}</dd></div>
                </dl>
                <div className="mt-5 border-t border-gray-200 pt-4 dark:border-gray-800">
                  <div className="flex items-center gap-1.5">
                    <label htmlFor="lan-discovery-subnet" className="text-xs font-medium text-gray-700 dark:text-gray-200">隧道扫描网段</label>
                    <HelpAssistant
                      title="设置隧道 CIDR"
                      text={[
                        '填写 WireGuard 隧道所在的 IPv4 CIDR，例如 10.13.13.0/24。输入 10.13.13.8/24 也会自动规范为 10.13.13.0/24。',
                        '为避免界面和网络持续产生负担，仅支持 /24 到 /30，单次最多扫描 254 个主机地址；网络地址、广播地址和本机地址会自动跳过。',
                        '“保存并扫描”会保存后立即执行一次；之后每次 PMC 启动会自动扫描一次，“扫描网段”可随时手动扫描已保存配置。这里不会后台循环扫描。',
                        '扫描发现设备后，常规发现循环只对该已知 IP 保持在线状态，不会重复扫描其他地址。',
                        '远端仍无法出现时，请确认隧道路由可达，并在 Windows 防火墙对应的 WireGuard/公用网络配置中允许 PMC 的 UDP 31523 与 TCP 31524。',
                      ]}
                      placement="top-start"
                      width={380}
                    />
                  </div>
                  <div className="mt-2 flex flex-col gap-2 sm:flex-row">
                    <input
                      id="lan-discovery-subnet"
                      value={discoverySubnet}
                      onChange={(event) => setDiscoverySubnet(event.target.value)}
                      placeholder="10.13.13.0/24"
                      spellCheck={false}
                      className="h-9 min-w-0 flex-1 rounded-md border border-gray-300 px-3 font-mono text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-gray-900"
                    />
                    <div className="flex shrink-0 gap-2">
                      <button
                        type="button"
                        disabled={isScanningSubnet}
                        onClick={() => void handleSaveAndScanSubnet()}
                        className="inline-flex h-9 items-center gap-1.5 rounded-md bg-blue-600 px-3 text-xs text-white hover:bg-blue-700 disabled:opacity-50"
                      >
                        <CheckCircle2 className="h-3.5 w-3.5" />{isScanningSubnet ? '扫描中...' : '保存并扫描'}
                      </button>
                      <button
                        type="button"
                        disabled={isScanningSubnet || !localSettings?.discoverySubnet}
                        onClick={() => void handleScanSubnet()}
                        className="inline-flex h-9 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-900"
                      >
                        <RefreshCw className={`h-3.5 w-3.5 ${isScanningSubnet ? 'animate-spin' : ''}`} />扫描网段
                      </button>
                    </div>
                  </div>
                </div>
                {service.lastError || error ? <p className="mt-4 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs leading-5 text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">{service.lastError || error}</p> : null}
              </div>
            </div>
          </div>
        ) : selectedConversation ? (
          <>
            <header className="flex h-16 shrink-0 items-center gap-3 border-b border-gray-200 px-4 dark:border-gray-800">
              <button type="button" onClick={() => setShowMobileConversation(false)} className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 lg:hidden"><ArrowLeft className="h-4 w-4" /></button>
              {selectedConversation.id === 'lobby'
                ? <div className="flex h-10 w-10 items-center justify-center rounded-full bg-blue-600 text-white"><Radio className="h-5 w-5" /></div>
                : <LanAvatar id={selectedContact?.id || selectedConversation.id} name={selectedContact?.displayName || selectedConversation.title} avatarPath={selectedContact?.avatarPath} online={selectedContact?.online} />}
              <div className="min-w-0 flex-1">
                <h2 className="truncate text-sm font-semibold">{selectedConversation.title}</h2>
                <p className="truncate text-xs text-gray-500">{selectedConversation.id === 'lobby' ? `${service.onlineCount} 人在线 · 自动同步并补齐最近 30 条大厅内容` : selectedContact?.online ? `${selectedContact.department || '未分组'} · ${selectedContact.ip}` : formatLastSeen(selectedContact?.lastSeen || 0)}</p>
              </div>
              <button type="button" onClick={() => setConfirmAction('conversation')} title="清空当前会话" className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 hover:text-red-600 dark:hover:bg-gray-800"><History className="h-4 w-4" /></button>
            </header>

            <div ref={messagesViewportRef} className="min-h-0 flex-1 overflow-auto px-4 py-5">
              {selectedTimeline.length === 0 ? (
                <div className="flex h-full min-h-52 flex-col items-center justify-center text-center text-gray-400">
                  <Inbox className="h-10 w-10" />
                  <p className="mt-3 text-sm">暂无消息</p>
                </div>
              ) : (
                <div className="mx-auto max-w-4xl space-y-4">
                  {selectedTimeline.map((item) => item.kind === 'message' ? (
                    <div key={`message:${item.message.id}`} data-lan-message-id={item.message.id}>
                      <MessageBubble
                        message={item.message}
                        profile={profile}
                        senderAvatarPath={contactsById.get(item.message.fromId)?.avatarPath}
                      />
                    </div>
                  ) : (
                    <div key={`transfer:${item.transfer.id}`} data-lan-transfer-id={item.transfer.id}>
                      <LanTransferCard
                        transfer={item.transfer}
                        progress={transferProgress[item.transfer.id]}
                        busy={transferActions.has(item.transfer.id)}
                        cancelling={cancellingTransfers.has(item.transfer.id)}
                        recipientName={item.transfer.direction === 'outgoing'
                          ? contactsById.get(item.transfer.toId)?.displayName
                          : null}
                        avatar={(
                          <LanAvatar
                            id={item.transfer.fromId}
                            name={item.transfer.fromName}
                            avatarPath={contactsById.get(item.transfer.fromId)?.avatarPath}
                            size="sm"
                          />
                        )}
                        onAccept={(transfer) => void handleAcceptTransfer(transfer)}
                        onReject={(transfer) => void handleRejectTransfer(transfer)}
                        onCancel={(transfer) => void handleCancelTransfer(transfer)}
                      />
                    </div>
                  ))}
                  <div aria-hidden="true" />
                </div>
              )}
            </div>

            <footer className="shrink-0 border-t border-gray-200 p-3 dark:border-gray-800">
              <div className="mx-auto flex max-w-4xl items-end gap-2">
                <div ref={attachmentMenuRef} className="relative shrink-0">
                  {showAttachmentMenu ? (
                    <div className="absolute bottom-full left-0 z-30 mb-2 w-40 overflow-hidden rounded-md border border-gray-200 bg-white py-1 shadow-lg dark:border-gray-700 dark:bg-gray-900">
                      <button
                        type="button"
                        onClick={() => {
                          setShowAttachmentMenu(false);
                          void handleChooseFiles();
                        }}
                        className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-800"
                      >
                        <FileUp className="h-4 w-4" />{selectedConversationId === 'lobby' ? '发送图片' : '发送文件'}
                      </button>
                      {selectedConversationId !== 'lobby' ? (
                        <button
                          type="button"
                          disabled={Boolean(selectedContact && selectedContact.protocolVersion < 4)}
                          onClick={() => {
                            setShowAttachmentMenu(false);
                            void handleChooseDirectory();
                          }}
                          className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-gray-700 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-40 dark:text-gray-200 dark:hover:bg-gray-800"
                        >
                          <FolderUp className="h-4 w-4" />发送文件夹
                        </button>
                      ) : null}
                    </div>
                  ) : null}
                  <button
                    type="button"
                    onClick={() => setShowAttachmentMenu((current) => !current)}
                    disabled={!canTransfer || isPreparingFiles}
                    title={selectedConversationId === 'lobby'
                      ? '向大厅发送图片'
                      : selectedContact && selectedContact.protocolVersion < 3
                        ? '对方客户端版本不支持文件传输'
                        : '发送文件或文件夹'}
                    className="flex h-11 w-11 items-center justify-center rounded-md border border-gray-300 text-gray-600 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-40 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
                  >
                    {isPreparingFiles ? <RefreshCw className="h-4 w-4 animate-spin" /> : <Paperclip className="h-4 w-4" />}
                  </button>
                </div>
                <textarea
                  value={input}
                  onChange={(event) => setInput(event.target.value)}
                  onPaste={(event) => void handleClipboardPaste(event)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.shiftKey) {
                      event.preventDefault();
                      void handleSend();
                    }
                  }}
                  rows={2}
                  disabled={!canSend}
                  placeholder={!service.isRunning ? '发现服务已停止' : selectedContact && !selectedContact.online ? '联系人离线，暂时无法发送' : '输入消息或粘贴图片，Enter 发送'}
                  className="min-h-[44px] max-h-32 flex-1 resize-none rounded-md border border-gray-300 bg-white px-3 py-2 text-sm leading-5 outline-none focus:border-blue-500 disabled:bg-gray-100 disabled:text-gray-400 dark:border-gray-700 dark:bg-gray-900 dark:disabled:bg-gray-900"
                />
                <button type="button" onClick={() => void handleSend()} disabled={!canSend || !input.trim() || isSending} title="发送消息" className="flex h-11 w-11 shrink-0 items-center justify-center rounded-md bg-blue-600 text-white hover:bg-blue-700 disabled:bg-gray-300 dark:disabled:bg-gray-700">
                  <Send className="h-4 w-4" />
                </button>
              </div>
            </footer>
          </>
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-gray-400"><UserRound className="mr-2 h-5 w-5" />请选择联系人或会话</div>
        )}
      </main>

      <ConfirmDialog
        isOpen={confirmAction !== null}
        onClose={() => setConfirmAction(null)}
        onConfirm={() => {
          if (confirmAction === 'conversation') {
            void clearConversation(selectedConversationId);
          } else if (confirmAction === 'history') {
            void clearHistory();
          }
        }}
        title={confirmAction === 'conversation' ? '清空当前会话' : '清空全部聊天记录'}
        message={confirmAction === 'conversation' ? '该会话的本地消息记录将被删除，联系人资料不会受影响。' : '大厅和所有私聊的本地记录都会删除，联系人资料不会受影响。'}
        confirmText="清空"
        cancelText="取消"
        type="danger"
      />
    </div>
  );
}
