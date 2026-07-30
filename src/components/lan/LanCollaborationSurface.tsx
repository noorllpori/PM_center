import { useEffect, useMemo, useRef, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import {
  ArrowLeft,
  Building2,
  CircleUserRound,
  ContactRound,
  Eraser,
  FolderOpen,
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
import { LanTransferCard } from './LanTransferCard';
import { clipboardImageFiles, prepareBrowserFiles, type PreparedTransferFile } from './lanTransferFiles';
import {
  useLanCollaborationStore,
  type LanContact,
  type LanMessage,
  type LanProfile,
  type LanTransfer,
} from '../../stores/lanCollaborationStore';

type LanViewMode = 'messages' | 'contacts' | 'profile';

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

type LanTimelineItem =
  | { kind: 'message'; timestamp: number; message: LanMessage }
  | { kind: 'transfer'; timestamp: number; transfer: LanTransfer };

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
  return (
    <div className={`flex gap-2.5 ${mine ? 'justify-end' : 'justify-start'}`}>
      {!mine ? <LanAvatar id={message.fromId} name={message.fromName} avatarPath={senderAvatarPath} size="sm" /> : null}
      <div className={`max-w-[min(72%,680px)] ${mine ? 'items-end' : 'items-start'}`}>
        {!mine ? <p className="mb-1 px-1 text-[11px] text-gray-500">{message.fromName}</p> : null}
        <div className={`whitespace-pre-wrap break-words rounded-md px-3 py-2 text-sm leading-6 ${mine ? 'bg-blue-600 text-white' : 'bg-gray-100 text-gray-900 dark:bg-gray-800 dark:text-gray-100'}`}>
          {message.content}
        </div>
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
    isLoading,
    error,
    initialize,
    refresh,
    updateProfile,
    updateReceiveDirectory,
    setAvatar,
    startDiscovery,
    stopDiscovery,
    sendMessage,
    offerFiles,
    respondTransfer,
    createTransferStagingPath,
    discardTransferStagingFile,
    markConversationRead,
    clearConversation,
    clearHistory,
    removeContact,
  } = useLanCollaborationStore();
  const showToast = useUiStore((state) => state.showToast);
  const [mode, setMode] = useState<LanViewMode>('messages');
  const [selectedConversationId, setSelectedConversationId] = useState('lobby');
  const [query, setQuery] = useState('');
  const [input, setInput] = useState('');
  const [isSending, setIsSending] = useState(false);
  const [isPreparingFiles, setIsPreparingFiles] = useState(false);
  const [isFileDragOver, setIsFileDragOver] = useState(false);
  const [transferActions, setTransferActions] = useState<Set<string>>(() => new Set());
  const transferActionsRef = useRef<Set<string>>(new Set());
  const [showMobileConversation, setShowMobileConversation] = useState(false);
  const [profileName, setProfileName] = useState('');
  const [profileDepartment, setProfileDepartment] = useState('');
  const [isProfileDraftDirty, setIsProfileDraftDirty] = useState(false);
  const [isSavingProfile, setIsSavingProfile] = useState(false);
  const [isUpdatingReceiveDirectory, setIsUpdatingReceiveDirectory] = useState(false);
  const [confirmAction, setConfirmAction] = useState<'conversation' | 'history' | null>(null);
  const [scrollRequest, setScrollRequest] = useState(0);
  const messagesViewportRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    void initialize().catch((initializationError) => {
      showToast({ title: '局域网服务初始化失败', message: String(initializationError), tone: 'error' });
    });
  }, [initialize, showToast]);

  useEffect(() => {
    if (!profile || isProfileDraftDirty) return;
    setProfileName(profile.displayName);
    setProfileDepartment(profile.department);
  }, [isProfileDraftDirty, profile]);

  const selectedContactId = selectedConversationId.startsWith('direct:')
    ? selectedConversationId.slice('direct:'.length)
    : null;
  const selectedContact = contacts.find((contact) => contact.id === selectedContactId) || null;
  const selectedConversation = conversations.find((conversation) => conversation.id === selectedConversationId) || conversations[0] || null;
  const selectedMessages = useMemo(
    () => messages.filter((message) => message.conversationId === selectedConversationId),
    [messages, selectedConversationId],
  );
  const selectedTransfers = useMemo(
    () => transfers.filter((transfer) => transfer.conversationId === selectedConversationId),
    [selectedConversationId, transfers],
  );
  const selectedTimeline = useMemo<LanTimelineItem[]>(() => [
    ...selectedMessages.map((message) => ({ kind: 'message' as const, timestamp: message.timestamp, message })),
    ...selectedTransfers.map((transfer) => ({ kind: 'transfer' as const, timestamp: transfer.createdAt, transfer })),
  ].sort((left, right) => left.timestamp - right.timestamp), [selectedMessages, selectedTransfers]);
  const contactsById = useMemo(
    () => new Map(contacts.map((contact) => [contact.id, contact] as const)),
    [contacts],
  );

  useEffect(() => {
    if (!isActive || !selectedConversation || selectedConversation.unreadCount === 0) return;
    void markConversationRead(selectedConversation.id);
  }, [isActive, markConversationRead, selectedConversation, selectedTimeline.length]);

  useEffect(() => {
    if (!isActive) return;
    const viewport = messagesViewportRef.current;
    if (!viewport) return;
    const scrollToBottom = () => {
      viewport.scrollTop = viewport.scrollHeight;
    };
    scrollToBottom();
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      scrollToBottom();
      secondFrame = window.requestAnimationFrame(scrollToBottom);
    });
    const content = viewport.firstElementChild;
    const resizeObserver = content instanceof HTMLElement
      ? new ResizeObserver(scrollToBottom)
      : null;
    if (content instanceof HTMLElement) resizeObserver?.observe(content);
    const settleTimer = window.setTimeout(() => resizeObserver?.disconnect(), 800);
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame) window.cancelAnimationFrame(secondFrame);
      window.clearTimeout(settleTimer);
      resizeObserver?.disconnect();
    };
  }, [isActive, scrollRequest, selectedConversationId, selectedTimeline.length, showMobileConversation]);

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
    setSelectedConversationId(conversationId);
    setMode('messages');
    setShowMobileConversation(true);
    setScrollRequest((current) => current + 1);
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
          title: result.deliveredCount > 0 ? '消息部分送达' : '消息发送失败',
          message: `已送达 ${result.deliveredCount}/${result.targetCount}；${result.failures.map((failure) => failure.userName).join('、')} 未收到`,
          tone: result.deliveredCount > 0 ? 'warning' : 'error',
        });
      }
    } catch (sendError) {
      showToast({ title: '消息发送失败', message: String(sendError), tone: 'error' });
    } finally {
      setIsSending(false);
    }
  };

  const requireTransferContact = () => {
    if (!selectedContactId || !selectedContact) {
      showToast({ title: '请选择联系人', message: '文件传输需要在联系人私聊中发起。', tone: 'warning' });
      return null;
    }
    if (!selectedContact.online) {
      showToast({ title: '联系人离线', message: '对方在线后才能接收文件传输请求。', tone: 'warning' });
      return null;
    }
    if (selectedContact.protocolVersion < 3) {
      showToast({ title: '客户端版本不兼容', message: '对方需要升级后才能使用确认式文件传输。', tone: 'warning' });
      return null;
    }
    return selectedContact;
  };

  const offerPreparedFiles = async (prepared: PreparedTransferFile[]) => {
    const contact = requireTransferContact();
    if (!contact || prepared.length === 0 || isPreparingFiles) return;
    setIsPreparingFiles(true);
    try {
      const result = await offerFiles(contact.id, prepared.map((item) => item.path));
      const failedPaths = new Set(result.failures.map((failure) => failure.path));
      await Promise.all(prepared
        .filter((item) => item.staged && failedPaths.has(item.path))
        .map((item) => discardTransferStagingFile(item.path).catch(() => {})));
      if (result.transfers.length > 0) {
        showToast({
          title: '文件请求已发送',
          message: `${result.transfers.length} 个文件正在等待 ${contact.displayName} 接收。`,
          tone: 'success',
        });
      }
      if (result.failures.length > 0) {
        showToast({
          title: result.transfers.length > 0 ? '部分文件未发送' : '文件发送失败',
          message: result.failures.slice(0, 3).map((failure) => `${failure.path.split(/[\\/]/).pop()}：${failure.error}`).join('；'),
          tone: result.transfers.length > 0 ? 'warning' : 'error',
        });
      }
    } catch (offerError) {
      await Promise.all(prepared
        .filter((item) => item.staged)
        .map((item) => discardTransferStagingFile(item.path).catch(() => {})));
      showToast({ title: '文件发送失败', message: String(offerError), tone: 'error' });
    } finally {
      setIsPreparingFiles(false);
    }
  };

  const handleChooseFiles = async () => {
    if (!requireTransferContact()) return;
    const selected = await open({
      title: '选择要发送的文件',
      multiple: true,
      directory: false,
    });
    const paths = typeof selected === 'string' ? [selected] : selected;
    if (!paths?.length) return;
    await offerPreparedFiles(paths.map((path) => ({
      path,
      staged: false,
      name: path.split(/[\\/]/).pop() || path,
    })));
  };

  const handleFileDrop = async (event: React.DragEvent<HTMLElement>) => {
    event.preventDefault();
    event.stopPropagation();
    setIsFileDragOver(false);
    if (!requireTransferContact()) return;
    const internalPaths = getInternalDragPaths(event.dataTransfer);
    if (internalPaths.length > 0) {
      await offerPreparedFiles(internalPaths.map((path) => ({
        path,
        staged: false,
        name: path.split(/[\\/]/).pop() || path,
      })));
      return;
    }
    const containsDirectory = Array.from(event.dataTransfer.items || []).some((item) => {
      const entry = (item as DataTransferItem & {
        webkitGetAsEntry?: () => FileSystemEntry | null;
      }).webkitGetAsEntry?.();
      return Boolean(entry?.isDirectory);
    });
    if (containsDirectory) {
      showToast({ title: '暂不支持发送目录', message: '目录和项目同步会复用当前传输引擎，在后续入口中提供。', tone: 'warning' });
      return;
    }
    const files = Array.from(event.dataTransfer.files || []);
    if (files.length === 0) {
      showToast({ title: '没有可发送的文件', message: '当前只支持文件，目录同步功能将使用同一传输引擎后续接入。', tone: 'warning' });
      return;
    }
    try {
      const prepared = await prepareBrowserFiles(
        files,
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
    if (!requireTransferContact()) return;
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
    try {
      const completed = await respondTransfer(transfer.id, 'accept');
      showToast({
        title: '文件接收完成',
        message: completed.receivedPath || transfer.displayName,
        tone: 'success',
      });
    } catch (acceptError) {
      showToast({ title: '文件接收失败', message: String(acceptError), tone: 'error' });
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
      && (selectedConversationId === 'lobby' ? service.onlineCount > 0 : selectedContact?.online),
  );
  const canTransfer = Boolean(
    profile
      && service.isRunning
      && selectedContact?.online
      && selectedContact.protocolVersion >= 3,
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
              placeholder={mode === 'contacts' ? '搜索联系人...' : '搜索会话...'}
              className="h-9 w-full rounded-md border border-gray-200 bg-gray-50 pl-8 pr-8 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-100"
            />
            {query ? <button type="button" onClick={() => setQuery('')} className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400"><X className="h-4 w-4" /></button> : null}
          </div>
          <button type="button" onClick={() => void refresh()} title="刷新联系人" className="flex h-9 w-9 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800">
            <RefreshCw className="h-4 w-4" />
          </button>
        </div>
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
          <NavigationButton active={mode === 'messages'} title="最近消息" badge={unreadCount} onClick={() => { setMode('messages'); setShowMobileConversation(false); }}><MessageCircle className="h-5 w-5" /></NavigationButton>
          <NavigationButton active={mode === 'contacts'} title="联系人" onClick={() => { setMode('contacts'); setShowMobileConversation(false); }}><ContactRound className="h-5 w-5" /></NavigationButton>
        </div>
        <div className="mt-auto flex flex-col items-center gap-2">
          <span title={service.isRunning ? '发现服务运行中' : '发现服务已停止'} className={`h-2.5 w-2.5 rounded-full ${service.isRunning ? 'bg-emerald-500' : 'bg-gray-400'}`} />
          <NavigationButton active={mode === 'profile'} title="个人资料与设置" onClick={() => { setMode('profile'); setShowMobileConversation(true); }}><Settings2 className="h-5 w-5" /></NavigationButton>
        </div>
      </aside>

      {middlePanel}

      <main
        className={`${showMobileConversation ? 'flex' : 'hidden lg:flex'} relative min-w-0 flex-1 flex-col bg-white dark:bg-gray-950`}
        onDragEnter={(event) => {
          if (mode === 'profile') return;
          const isFileDrag = hasInternalDragData(event.dataTransfer)
            || Array.from(event.dataTransfer.types || []).includes('Files');
          if (!isFileDrag) return;
          event.preventDefault();
          setIsFileDragOver(true);
        }}
        onDragOver={(event) => {
          if (mode === 'profile') return;
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
        onDrop={(event) => void handleFileDrop(event)}
      >
        {isFileDragOver ? (
          <div className="pointer-events-none absolute inset-3 z-50 flex items-center justify-center rounded-md border-2 border-dashed border-blue-400 bg-blue-50/95 text-blue-700 shadow-sm dark:border-blue-600 dark:bg-blue-950/90 dark:text-blue-200">
            <div className="text-center">
              <Paperclip className="mx-auto h-7 w-7" />
              <p className="mt-2 text-sm font-medium">拖到此处发送文件</p>
              <p className="mt-1 text-xs opacity-75">普通文件由对方确认，图片会自动接收</p>
            </div>
          </div>
        ) : null}
        {mode === 'profile' ? (
          <div className="min-h-0 flex-1 overflow-auto">
            <div className="border-b border-gray-200 px-5 py-4 dark:border-gray-800">
              <div className="flex items-center gap-2">
                <CircleUserRound className="h-5 w-5 text-blue-600" />
                <h2 className="text-base font-semibold">个人资料与局域网状态</h2>
                <HelpAssistant
                  title="局域网协同说明"
                  text={[
                    '消息和压缩头像仅在当前局域网内传输，不经过云端，但内容未加密，请只在可信网络中使用。',
                    '普通文件在联系人确认后保存到默认接收目录；图片会自动接收。所有内容都使用临时文件和 BLAKE3 完整性校验。',
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
                    <p className="mt-1 text-xs text-gray-500">按发送者名称建立子目录；同名文件自动编号，不覆盖已有内容。</p>
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
                    <h3 className="text-sm font-semibold">网络诊断</h3>
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
                <p className="truncate text-xs text-gray-500">{selectedConversation.id === 'lobby' ? `${service.onlineCount} 人在线 · 大厅消息发送给所有在线联系人` : selectedContact?.online ? `${selectedContact.department || '未分组'} · ${selectedContact.ip}` : formatLastSeen(selectedContact?.lastSeen || 0)}</p>
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
                    <MessageBubble
                      key={`message:${item.message.id}`}
                      message={item.message}
                      profile={profile}
                      senderAvatarPath={contactsById.get(item.message.fromId)?.avatarPath}
                    />
                  ) : (
                    <LanTransferCard
                      key={`transfer:${item.transfer.id}`}
                      transfer={item.transfer}
                      progress={transferProgress[item.transfer.id]}
                      busy={transferActions.has(item.transfer.id)}
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
                    />
                  ))}
                  <div aria-hidden="true" />
                </div>
              )}
            </div>

            <footer className="shrink-0 border-t border-gray-200 p-3 dark:border-gray-800">
              <div className="mx-auto flex max-w-4xl items-end gap-2">
                <button
                  type="button"
                  onClick={() => void handleChooseFiles()}
                  disabled={!canTransfer || isPreparingFiles}
                  title={selectedConversationId === 'lobby'
                    ? '请先进入联系人私聊'
                    : selectedContact && selectedContact.protocolVersion < 3
                      ? '对方客户端版本不支持文件传输'
                      : '发送文件'}
                  className="flex h-11 w-11 shrink-0 items-center justify-center rounded-md border border-gray-300 text-gray-600 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-40 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
                >
                  {isPreparingFiles ? <RefreshCw className="h-4 w-4 animate-spin" /> : <Paperclip className="h-4 w-4" />}
                </button>
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
