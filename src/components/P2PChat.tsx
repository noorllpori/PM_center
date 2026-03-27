import { useState, useEffect, useRef } from 'react';
import { useP2PStore } from '../stores/p2pStore';
import { 
  MessageCircle, Users, Send, Settings, X, Radio, 
  User, MoreVertical, Trash2, Edit2 
} from 'lucide-react';

interface P2PChatProps {
  isOpen: boolean;
  onClose: () => void;
}

export function P2PChat({ isOpen, onClose }: P2PChatProps) {
  const {
    userId,
    userName,
    onlineUsers,
    messages,
    unreadCount,
    isDiscoveryEnabled,
    loadSettings,
    setUserName,
    startDiscovery,
    stopDiscovery,
    sendMessage,
    markAllAsRead,
    clearMessages,
  } = useP2PStore();

  const [inputMessage, setInputMessage] = useState('');
  const [selectedUserId, setSelectedUserId] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [newUserName, setNewUserName] = useState('');
  const [activeTab, setActiveTab] = useState<'chat' | 'users'>('chat');
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // 初始化
  useEffect(() => {
    loadSettings();
  }, []);

  // 自动滚动到底部
  useEffect(() => {
    if (isOpen && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, isOpen]);

  // 标记已读
  useEffect(() => {
    if (isOpen) {
      markAllAsRead();
    }
  }, [isOpen]);

  // 开始/停止发现
  const toggleDiscovery = async () => {
    if (isDiscoveryEnabled) {
      await stopDiscovery();
    } else {
      await startDiscovery();
    }
  };

  // 发送消息
  const handleSendMessage = async () => {
    if (!inputMessage.trim()) return;
    
    await sendMessage(selectedUserId, inputMessage.trim());
    setInputMessage('');
  };

  // 更新用户名
  const handleUpdateUserName = async () => {
    if (newUserName.trim()) {
      await setUserName(newUserName.trim());
      setShowSettings(false);
    }
  };

  // 过滤消息
  const filteredMessages = selectedUserId
    ? messages.filter(m => 
        (m.fromId === selectedUserId && m.toId === userId) ||
        (m.fromId === userId && m.toId === selectedUserId)
      )
    : messages.filter(m => m.toId === null); // 广播消息

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white dark:bg-gray-900 rounded-xl shadow-xl w-[900px] max-w-[95vw] h-[600px] max-h-[90vh] flex flex-col">
        {/* 头部 */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-3">
            <div className="relative">
              <MessageCircle className="w-6 h-6 text-blue-500" />
              {unreadCount > 0 && (
                <span className="absolute -top-1 -right-1 w-4 h-4 bg-red-500 text-white text-xs rounded-full flex items-center justify-center">
                  {unreadCount > 9 ? '9+' : unreadCount}
                </span>
              )}
            </div>
            <div>
              <h3 className="font-semibold text-gray-900 dark:text-gray-100">局域网消息</h3>
              <p className="text-xs text-gray-500">
                {isDiscoveryEnabled ? (
                  <span className="flex items-center gap-1">
                    <Radio className="w-3 h-3 text-green-500 animate-pulse" />
                    发现服务运行中 ({onlineUsers.length} 人在线)
                  </span>
                ) : (
                  '发现服务已停止'
                )}
              </p>
            </div>
          </div>
          
          <div className="flex items-center gap-2">
            <button
              onClick={toggleDiscovery}
              className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
                isDiscoveryEnabled
                  ? 'bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400'
                  : 'bg-gray-100 dark:bg-gray-800 text-gray-700 dark:text-gray-400'
              }`}
            >
              {isDiscoveryEnabled ? '停止发现' : '开始发现'}
            </button>
            <button
              onClick={() => setShowSettings(true)}
              className="p-2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
            >
              <Settings className="w-4 h-4" />
            </button>
            <button
              onClick={onClose}
              className="p-2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* 标签切换 */}
        <div className="flex border-b border-gray-200 dark:border-gray-700">
          <button
            onClick={() => setActiveTab('chat')}
            className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${
              activeTab === 'chat'
                ? 'text-blue-600 border-b-2 border-blue-600'
                : 'text-gray-600 hover:text-gray-900 dark:hover:text-gray-100'
            }`}
          >
            消息
          </button>
          <button
            onClick={() => setActiveTab('users')}
            className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${
              activeTab === 'users'
                ? 'text-blue-600 border-b-2 border-blue-600'
                : 'text-gray-600 hover:text-gray-900 dark:hover:text-gray-100'
            }`}
          >
            在线用户 ({onlineUsers.length})
          </button>
        </div>

        {activeTab === 'chat' ? (
          <>
            {/* 聊天模式选择 */}
            <div className="flex items-center gap-2 px-4 py-2 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
              <button
                onClick={() => setSelectedUserId(null)}
                className={`px-3 py-1.5 text-sm rounded-full transition-colors ${
                  selectedUserId === null
                    ? 'bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-400'
                    : 'bg-white dark:bg-gray-700 text-gray-700 dark:text-gray-300'
                }`}
              >
                💬 广播
              </button>
              {onlineUsers.map(user => (
                <button
                  key={user.id}
                  onClick={() => setSelectedUserId(user.id)}
                  className={`px-3 py-1.5 text-sm rounded-full transition-colors flex items-center gap-1.5 ${
                    selectedUserId === user.id
                      ? 'bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-400'
                      : 'bg-white dark:bg-gray-700 text-gray-700 dark:text-gray-300'
                  }`}
                >
                  <span className="w-2 h-2 bg-green-500 rounded-full" />
                  {user.name}
                </button>
              ))}
            </div>

            {/* 消息列表 */}
            <div className="flex-1 overflow-auto p-4 space-y-3">
              {filteredMessages.length === 0 ? (
                <div className="text-center py-12 text-gray-400">
                  <MessageCircle className="w-12 h-12 mx-auto mb-3 opacity-50" />
                  <p className="text-sm">
                    {selectedUserId ? '暂无消息，开始聊天吧' : '暂无广播消息'}
                  </p>
                </div>
              ) : (
                filteredMessages.map((msg, idx) => {
                  const isMe = msg.fromId === userId;
                  const showTime = idx === 0 || 
                    msg.timestamp - filteredMessages[idx - 1].timestamp > 60000;
                  
                  return (
                    <div key={msg.id}>
                      {showTime && (
                        <div className="text-center text-xs text-gray-400 my-2">
                          {new Date(msg.timestamp).toLocaleTimeString('zh-CN', { 
                            hour: '2-digit', 
                            minute: '2-digit' 
                          })}
                        </div>
                      )}
                      <div className={`flex ${isMe ? 'justify-end' : 'justify-start'}`}>
                        <div className={`max-w-[70%] ${isMe ? 'items-end' : 'items-start'}`}>
                          {!isMe && (
                            <span className="text-xs text-gray-500 ml-1 mb-0.5">
                              {msg.fromName}
                            </span>
                          )}
                          <div className={`px-3 py-2 rounded-lg text-sm ${
                            isMe
                              ? 'bg-blue-600 text-white'
                              : 'bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100'
                          }`}>
                            {msg.content}
                          </div>
                        </div>
                      </div>
                    </div>
                  );
                })
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* 输入框 */}
            <div className="p-3 border-t border-gray-200 dark:border-gray-700">
              <div className="flex gap-2">
                <input
                  type="text"
                  value={inputMessage}
                  onChange={(e) => setInputMessage(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && handleSendMessage()}
                  placeholder={selectedUserId ? '发送私信...' : '发送广播消息...'}
                  className="flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg
                             bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 text-sm"
                />
                <button
                  onClick={handleSendMessage}
                  disabled={!inputMessage.trim()}
                  className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300
                             text-white rounded-lg transition-colors"
                >
                  <Send className="w-4 h-4" />
                </button>
              </div>
            </div>
          </>
        ) : (
          /* 用户列表 */
          <div className="flex-1 overflow-auto p-4">
            {onlineUsers.length === 0 ? (
              <div className="text-center py-12 text-gray-400">
                <Users className="w-12 h-12 mx-auto mb-3 opacity-50" />
                <p className="text-sm">暂无在线用户</p>
                <p className="text-xs mt-1">确保发现服务已启动</p>
              </div>
            ) : (
              <div className="space-y-2">
                {onlineUsers.map(user => (
                  <div
                    key={user.id}
                    className="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-800 rounded-lg"
                  >
                    <div className="w-10 h-10 rounded-full bg-blue-100 dark:bg-blue-900/30 
                                    flex items-center justify-center">
                      <User className="w-5 h-5 text-blue-500" />
                    </div>
                    <div className="flex-1">
                      <p className="font-medium text-gray-900 dark:text-gray-100">
                        {user.name}
                      </p>
                      <p className="text-xs text-gray-500">{user.ip}</p>
                    </div>
                    <div className="flex items-center gap-1">
                      <span className="w-2 h-2 bg-green-500 rounded-full" />
                      <span className="text-xs text-gray-500">在线</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* 设置对话框 */}
        {showSettings && (
          <div className="absolute inset-0 bg-black/50 flex items-center justify-center z-10">
            <div className="bg-white dark:bg-gray-900 rounded-xl shadow-xl w-[360px] p-5">
              <h4 className="font-semibold text-gray-900 dark:text-gray-100 mb-4">
                用户设置
              </h4>
              
              <div className="space-y-4">
                <div>
                  <label className="block text-sm text-gray-700 dark:text-gray-300 mb-1">
                    用户 ID
                  </label>
                  <code className="block p-2 bg-gray-100 dark:bg-gray-800 rounded text-xs text-gray-600 break-all">
                    {userId}
                  </code>
                </div>
                
                <div>
                  <label className="block text-sm text-gray-700 dark:text-gray-300 mb-1">
                    显示名称
                  </label>
                  <input
                    type="text"
                    defaultValue={userName}
                    onChange={(e) => setNewUserName(e.target.value)}
                    placeholder="输入显示名称"
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg
                               bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 text-sm"
                  />
                </div>
              </div>

              <div className="flex justify-end gap-2 mt-5">
                <button
                  onClick={() => setShowSettings(false)}
                  className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
                >
                  取消
                </button>
                <button
                  onClick={handleUpdateUserName}
                  className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg"
                >
                  保存
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
