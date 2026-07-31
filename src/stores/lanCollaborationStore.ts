import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { load } from '@tauri-apps/plugin-store';
import { create } from 'zustand';

export interface LanProfile {
  id: string;
  displayName: string;
  department: string;
  avatarHash: string | null;
  avatarPath: string | null;
  profileRevision: number;
}

export interface LanLocalSettings {
  receiveDirectory: string;
  autoReceiveImages: boolean;
}

export interface LanContact {
  id: string;
  displayName: string;
  department: string;
  avatarHash: string | null;
  avatarPath: string | null;
  ip: string;
  online: boolean;
  firstSeen: number;
  lastSeen: number;
  protocolVersion: number;
  profileRevision: number;
}

export interface LanMessage {
  id: string;
  conversationId: string;
  fromId: string;
  fromName: string;
  toId: string | null;
  content: string;
  timestamp: number;
  direction: 'incoming' | 'outgoing' | string;
  deliveryStatus: 'delivered' | 'partial' | 'failed' | string;
  deliverySummary: string | null;
}

export interface LanTransfer {
  id: string;
  lobbyItemId: string | null;
  conversationId: string;
  kind: string;
  fromId: string;
  fromName: string;
  providerId: string;
  providerName: string;
  toId: string;
  displayName: string;
  itemCount: number;
  totalBytes: number;
  mimeType: string | null;
  contentHash: string;
  payloadFormat: string;
  manifest: unknown;
  status: 'waiting' | 'pending' | 'transferring' | 'completed' | 'rejected' | 'failed' | string;
  direction: 'incoming' | 'outgoing' | string;
  sourcePath: string | null;
  receivedPath: string | null;
  error: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface LanTransferProgress {
  transferId: string;
  status: string;
  direction: string;
  transferredBytes: number;
  totalBytes: number;
  bytesPerSecond: number;
}

export interface LanTransferFailure {
  path: string;
  error: string;
}

export interface LanFileOfferResult {
  transfers: LanTransfer[];
  failures: LanTransferFailure[];
}

export interface LanConversation {
  id: string;
  kind: 'lobby' | 'direct' | string;
  contactId: string | null;
  title: string;
  unreadCount: number;
  lastMessageAt: number | null;
}

export interface LanServiceStatus {
  isRunning: boolean;
  udpBound: boolean;
  tcpBound: boolean;
  localAddresses: string[];
  onlineCount: number;
  lastDiscoveryAt: number | null;
  lastError: string | null;
}

export interface LanSnapshot {
  profile: LanProfile;
  localSettings: LanLocalSettings;
  contacts: LanContact[];
  messages: LanMessage[];
  transfers: LanTransfer[];
  conversations: LanConversation[];
  unreadCount: number;
  service: LanServiceStatus;
}

export interface LanDeliveryFailure {
  userId: string;
  userName: string;
  error: string;
}

export interface LanDeliveryResult {
  targetCount: number;
  deliveredCount: number;
  failures: LanDeliveryFailure[];
}

export interface LanConversationNavigationRequest {
  requestId: number;
  conversationId: string;
  messageId: string | null;
  transferId: string | null;
}

interface LanCollaborationState {
  profile: LanProfile | null;
  localSettings: LanLocalSettings | null;
  contacts: LanContact[];
  messages: LanMessage[];
  transfers: LanTransfer[];
  transferProgress: Record<string, LanTransferProgress>;
  conversations: LanConversation[];
  unreadCount: number;
  service: LanServiceStatus;
  navigationRequest: LanConversationNavigationRequest | null;
  isLoading: boolean;
  isInitialized: boolean;
  error: string | null;
  initialize: () => Promise<void>;
  refresh: () => Promise<void>;
  updateProfile: (displayName: string, department: string) => Promise<void>;
  updateReceiveDirectory: (receiveDirectory: string) => Promise<void>;
  setAvatar: (imagePath: string | null) => Promise<void>;
  startDiscovery: () => Promise<void>;
  stopDiscovery: () => Promise<void>;
  sendMessage: (conversationId: string, content: string) => Promise<LanDeliveryResult>;
  offerFiles: (
    contactIds: string | string[],
    paths: string[],
    conversationId?: 'lobby' | null,
  ) => Promise<LanFileOfferResult>;
  respondTransfer: (transferId: string, action: 'accept' | 'reject', destinationPath?: string) => Promise<LanTransfer>;
  createTransferStagingPath: (fileName: string) => Promise<string>;
  discardTransferStagingFile: (path: string) => Promise<void>;
  markConversationRead: (conversationId: string) => Promise<void>;
  clearConversation: (conversationId: string) => Promise<void>;
  clearHistory: () => Promise<void>;
  removeContact: (contactId: string) => Promise<void>;
  requestConversationNavigation: (
    conversationId: string,
    messageId?: string | null,
    transferId?: string | null,
  ) => void;
  clearConversationNavigation: () => void;
}

const EMPTY_SERVICE: LanServiceStatus = {
  isRunning: false,
  udpBound: false,
  tcpBound: false,
  localAddresses: [],
  onlineCount: 0,
  lastDiscoveryAt: null,
  lastError: null,
};

const LEGACY_STORE_FILE = 'p2p-settings.json';
const LEGACY_USER_ID_KEY = 'p2p-user-id';
const LEGACY_USER_NAME_KEY = 'p2p-user-name';

let initializationPromise: Promise<void> | null = null;
let listenersPromise: Promise<void> | null = null;
let refreshPromise: Promise<void> | null = null;
let refreshTimer: number | null = null;
let navigationRequestId = 0;
const unlisteners: UnlistenFn[] = [];

function applySnapshot(snapshot: LanSnapshot) {
  useLanCollaborationStore.setState({
    profile: snapshot.profile,
    localSettings: snapshot.localSettings,
    contacts: snapshot.contacts,
    messages: snapshot.messages,
    transfers: snapshot.transfers,
    conversations: snapshot.conversations,
    unreadCount: snapshot.unreadCount,
    service: snapshot.service,
    isInitialized: true,
    isLoading: false,
    error: null,
  });
}

async function readLegacyProfile() {
  try {
    const store = await load(LEGACY_STORE_FILE);
    return {
      userId: await store.get<string>(LEGACY_USER_ID_KEY) || null,
      userName: await store.get<string>(LEGACY_USER_NAME_KEY) || null,
    };
  } catch (error) {
    console.warn('Failed to read legacy P2P settings:', error);
    return null;
  }
}

function scheduleRefresh(delay = 30) {
  if (refreshTimer !== null) {
    window.clearTimeout(refreshTimer);
  }
  refreshTimer = window.setTimeout(() => {
    refreshTimer = null;
    void useLanCollaborationStore.getState().refresh();
  }, delay);
}

async function setupListeners() {
  if (listenersPromise) {
    return listenersPromise;
  }
  listenersPromise = (async () => {
    const eventNames = [
      'pm-center:lan-message',
      'pm-center:lan-contacts-changed',
      'pm-center:lan-profile-changed',
      'pm-center:lan-settings-changed',
      'pm-center:lan-read-state-changed',
      'pm-center:lan-service-status',
      'pm-center:lan-transfer-changed',
    ];
    for (const eventName of eventNames) {
      unlisteners.push(await listen(eventName, () => scheduleRefresh()));
    }
    unlisteners.push(await listen<LanTransferProgress>('pm-center:lan-transfer-progress', (event) => {
      const progress = event.payload;
      if (!progress?.transferId) return;
      useLanCollaborationStore.setState((state) => ({
        transferProgress: {
          ...state.transferProgress,
          [progress.transferId]: progress,
        },
      }));
    }));
  })();
  return listenersPromise;
}

export const useLanCollaborationStore = create<LanCollaborationState>((set, get) => ({
  profile: null,
  localSettings: null,
  contacts: [],
  messages: [],
  transfers: [],
  transferProgress: {},
  conversations: [],
  unreadCount: 0,
  service: EMPTY_SERVICE,
  navigationRequest: null,
  isLoading: false,
  isInitialized: false,
  error: null,

  initialize: async () => {
    if (initializationPromise) {
      return initializationPromise;
    }
    initializationPromise = (async () => {
      set({ isLoading: true, error: null });
      try {
        await setupListeners();
        const legacyProfile = await readLegacyProfile();
        const snapshot = await invoke<LanSnapshot>('initialize_lan_collaboration', {
          legacyProfile,
        });
        applySnapshot(snapshot);
      } catch (error) {
        const message = String(error);
        set({ isLoading: false, error: message });
        throw error;
      }
    })();
    try {
      await initializationPromise;
    } catch (error) {
      initializationPromise = null;
      throw error;
    }
  },

  refresh: async () => {
    if (!get().isInitialized && !initializationPromise) {
      return get().initialize();
    }
    if (refreshPromise) {
      return refreshPromise;
    }
    refreshPromise = (async () => {
      try {
        const snapshot = await invoke<LanSnapshot>('get_lan_collaboration_snapshot');
        applySnapshot(snapshot);
      } catch (error) {
        set({ error: String(error) });
      } finally {
        refreshPromise = null;
      }
    })();
    return refreshPromise;
  },

  updateProfile: async (displayName, department) => {
    const profile = await invoke<LanProfile>('update_lan_profile', {
      request: { displayName, department },
    });
    set({ profile });
    await get().refresh();
  },

  updateReceiveDirectory: async (receiveDirectory) => {
    const localSettings = await invoke<LanLocalSettings>('update_lan_receive_directory', {
      request: { receiveDirectory },
    });
    set({ localSettings });
    await get().refresh();
  },

  setAvatar: async (imagePath) => {
    const profile = await invoke<LanProfile>('set_lan_avatar', { imagePath });
    set({ profile });
    await get().refresh();
  },

  startDiscovery: async () => {
    await invoke('start_lan_discovery');
    await get().refresh();
  },

  stopDiscovery: async () => {
    await invoke('stop_lan_discovery');
    await get().refresh();
  },

  sendMessage: async (conversationId, content) => {
    const toId = conversationId.startsWith('direct:')
      ? conversationId.slice('direct:'.length)
      : null;
    const result = await invoke<LanDeliveryResult>('send_lan_message', {
      request: { toId, content },
    });
    await get().refresh();
    return result;
  },

  offerFiles: async (contactIds, paths, conversationId = null) => {
    const recipients = Array.isArray(contactIds) ? contactIds : [contactIds];
    const result = await invoke<LanFileOfferResult>('offer_lan_files', {
      request: {
        toId: recipients.length === 1 ? recipients[0] : null,
        toIds: recipients.length > 1 ? recipients : [],
        paths,
        conversationId,
      },
    });
    await get().refresh();
    return result;
  },

  respondTransfer: async (transferId, action, destinationPath) => {
    const transfer = await invoke<LanTransfer>('respond_lan_transfer', {
      request: { transferId, action, destinationPath: destinationPath || null },
    });
    await get().refresh();
    return transfer;
  },

  createTransferStagingPath: (fileName) => invoke<string>('create_lan_transfer_staging_path', { fileName }),

  discardTransferStagingFile: (path) => invoke('discard_lan_transfer_staging_file', { path }),

  markConversationRead: async (conversationId) => {
    await invoke('mark_lan_conversation_read', { conversationId });
    await get().refresh();
  },

  clearConversation: async (conversationId) => {
    await invoke('clear_lan_conversation', { conversationId });
    await get().refresh();
  },

  clearHistory: async () => {
    await invoke('clear_lan_history');
    await get().refresh();
  },

  removeContact: async (contactId) => {
    await invoke('remove_lan_contact', { contactId });
    await get().refresh();
  },

  requestConversationNavigation: (conversationId, messageId = null, transferId = null) => {
    navigationRequestId += 1;
    set({
      navigationRequest: {
        requestId: navigationRequestId,
        conversationId,
        messageId,
        transferId,
      },
    });
  },

  clearConversationNavigation: () => set({ navigationRequest: null }),
}));

export function disposeLanCollaborationListeners() {
  unlisteners.splice(0).forEach((unlisten) => unlisten());
  listenersPromise = null;
}
