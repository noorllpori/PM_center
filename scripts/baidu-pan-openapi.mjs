#!/usr/bin/env node

import { createHash, randomUUID, timingSafeEqual } from 'node:crypto';
import { chmod, mkdir, open, readdir, rename, rm, stat, writeFile, readFile } from 'node:fs/promises';
import { createWriteStream } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join, posix, resolve } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

const API_ROOT = 'https://pan.baidu.com/rest/2.0/xpan';
const OAUTH_ROOT = 'https://openapi.baidu.com/oauth/2.0';
const USER_AGENT = 'pan.baidu.com';
const UPLOAD_CHUNK_SIZE = 4 * 1024 * 1024;
const SLICE_MD5_SIZE = 256 * 1024;
const MAX_NORMAL_FILE_SIZE = 4 * 1024 * 1024 * 1024;
const TOKEN_REFRESH_SKEW = 5 * 60 * 1000;
const REQUEST_TIMEOUT_MS = 30 * 1000;
const UPLOAD_TIMEOUT_MS = 10 * 60 * 1000;
const DOWNLOAD_TIMEOUT_MS = 30 * 60 * 1000;

const BOOLEAN_OPTIONS = new Set([
  'force',
  'foldersOnly',
  'help',
  'json',
  'yes',
]);

const ON_DUP_VALUES = new Set(['fail', 'newcopy', 'overwrite', 'skip']);
const COMMANDS = new Set([
  'auth-url',
  'exchange-code',
  'status',
  'list',
  'mkdir',
  'upload',
  'upload-dir',
  'download',
  'copy',
  'move',
  'rename',
  'delete',
]);

class BaiduApiError extends Error {
  constructor(message, { code, requestId, status, payload } = {}) {
    super(message);
    this.name = 'BaiduApiError';
    this.code = code;
    this.requestId = requestId;
    this.status = status;
    this.payload = payload;
  }
}

function parseArgs(args) {
  const positionals = [];
  const options = {};

  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];

    if (argument === '--') {
      positionals.push(...args.slice(index + 1));
      break;
    }

    if (argument === '-h') {
      options.help = true;
      continue;
    }

    if (!argument.startsWith('--')) {
      positionals.push(argument);
      continue;
    }

    const rawOption = argument.slice(2);
    const separator = rawOption.indexOf('=');
    const rawName = separator >= 0 ? rawOption.slice(0, separator) : rawOption;
    const inlineValue = separator >= 0 ? rawOption.slice(separator + 1) : undefined;
    const name = rawName.replace(/-([a-z])/g, (_match, character) => character.toUpperCase());

    if (!name) {
      throw new Error(`无效参数：${argument}`);
    }

    if (BOOLEAN_OPTIONS.has(name)) {
      options[name] = inlineValue === undefined ? true : parseBoolean(inlineValue, name);
      continue;
    }

    const value = inlineValue ?? args[index + 1];
    if (value === undefined || value.startsWith('--')) {
      throw new Error(`参数 --${rawName} 需要一个值`);
    }

    options[name] = value;
    if (inlineValue === undefined) {
      index += 1;
    }
  }

  return { options, positionals };
}

function parseBoolean(value, optionName) {
  if (value === 'true' || value === '1') return true;
  if (value === 'false' || value === '0') return false;
  throw new Error(`参数 --${optionName} 的值必须是 true/false`);
}

function requirePositionals(positionals, count, usage) {
  if (positionals.length < count) {
    throw new Error(`参数不足。用法：${usage}`);
  }
}

function parsePositiveInteger(value, name, { max } = {}) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0 || (max !== undefined && parsed > max)) {
    const suffix = max === undefined ? '' : `，范围为 1-${max}`;
    throw new Error(`参数 --${name} 必须是正整数${suffix}`);
  }
  return parsed;
}

function parseDesc(value) {
  if (value === undefined) return 0;
  if (value === '0' || value === 0 || value === false) return 0;
  if (value === '1' || value === 1 || value === true) return 1;
  throw new Error('参数 --desc 只能是 0 或 1');
}

function envValue(...names) {
  for (const name of names) {
    const value = process.env[name]?.trim();
    if (value) return value;
  }
  return '';
}

function defaultTokenPath() {
  const configBase = process.platform === 'win32'
    ? envValue('LOCALAPPDATA') || join(homedir(), 'AppData', 'Local')
    : envValue('XDG_CONFIG_HOME') || join(homedir(), '.config');

  return join(configBase, 'PM_center', 'baidu-pan', 'token.json');
}

function normalizeRemoteRoot(rawRoot) {
  const normalized = posix.normalize(String(rawRoot).replaceAll('\\', '/'));
  const root = normalized.replace(/\/$/, '');
  if (!root.startsWith('/apps/') || root === '/apps' || root.length <= '/apps/'.length) {
    throw new Error('BAIDU_REMOTE_ROOT 必须位于 /apps/<应用名> 下');
  }
  return root;
}

function getConfig({ requireAppName = false, requireAppKey = false, requireSecretKey = false, requireRedirect = false } = {}) {
  const appKey = envValue('BAIDU_APP_KEY', 'BAIDU_APPKEY');
  const secretKey = envValue('BAIDU_SECRET_KEY', 'BAIDU_SECRETKEY');
  const redirectUri = envValue('BAIDU_REDIRECT_URI');
  const appName = envValue('BAIDU_APP_NAME');

  if (requireAppKey && !appKey) {
    throw new Error('缺少 BAIDU_APP_KEY。请从百度网盘开放平台控制台填写应用 AppKey。');
  }
  if (requireSecretKey && !secretKey) {
    throw new Error('缺少 BAIDU_SECRET_KEY。请从百度网盘开放平台控制台填写应用 SecretKey。');
  }
  if (requireRedirect && !redirectUri) {
    throw new Error('缺少 BAIDU_REDIRECT_URI。它必须与开放平台控制台登记的回调地址完全一致。');
  }
  if (requireAppName && !appName) {
    throw new Error('缺少 BAIDU_APP_NAME。百度开放 API 只允许访问应用自己的 /apps/<应用名> 目录。');
  }
  if (appName && (appName.includes('/') || appName.includes('\\') || appName === '.' || appName === '..')) {
    throw new Error('BAIDU_APP_NAME 不能包含路径分隔符');
  }

  const remoteRoot = normalizeRemoteRoot(
    envValue('BAIDU_REMOTE_ROOT') || (appName ? `/apps/${appName}` : '/apps/placeholder'),
  );

  return {
    appKey,
    appName,
    redirectUri,
    remoteRoot,
    secretKey,
    tokenPath: resolve(envValue('BAIDU_TOKEN_FILE') || defaultTokenPath()),
  };
}

function resolveRemotePath(value, config) {
  const raw = String(value ?? '').trim().replaceAll('\\', '/');
  if (!raw || raw === '.' || raw === '/') return config.remoteRoot;

  const candidate = raw.startsWith('/') ? posix.normalize(raw) : posix.join(config.remoteRoot, raw);
  if (candidate !== config.remoteRoot && !candidate.startsWith(`${config.remoteRoot}/`)) {
    throw new Error(`远端路径必须位于 ${config.remoteRoot} 下：${value}`);
  }
  return candidate;
}

function resolveDestinationPath(value, source, config) {
  const raw = String(value).trim();
  const destination = resolveRemotePath(raw, config);
  return /[\\/]$/.test(raw) ? posix.join(destination, posix.basename(source)) : destination;
}

function getNameFromRemotePath(remotePath) {
  return posix.basename(remotePath);
}

function getErrorCode(payload) {
  const candidates = [
    payload?.errno,
    payload?.error_code,
    payload?.data?.errno,
    payload?.data?.error_code,
  ];
  for (const candidate of candidates) {
    if (candidate !== undefined && candidate !== null && Number(candidate) !== 0) {
      const parsed = Number(candidate);
      return Number.isNaN(parsed) ? candidate : parsed;
    }
  }
  return undefined;
}

function getRequestId(payload) {
  return payload?.request_id ?? payload?.data?.request_id;
}

function getErrorText(payload) {
  return payload?.show_msg
    ?? payload?.errmsg
    ?? payload?.error_msg
    ?? payload?.error_description
    ?? payload?.data?.show_msg
    ?? payload?.data?.errmsg
    ?? payload?.data?.error_msg
    ?? '';
}

function throwIfApiError(payload, status) {
  const code = getErrorCode(payload);
  if (code === undefined && !payload?.error) return;

  const oauthCode = payload?.error;
  const displayCode = code ?? oauthCode;
  const text = getErrorText(payload) || payload?.error || '未知错误';
  throw new BaiduApiError(
    `百度 API 请求失败${displayCode !== undefined ? `（错误码 ${displayCode}）` : ''}：${text}`,
    { code: code ?? oauthCode, requestId: getRequestId(payload), status, payload },
  );
}

async function fetchJson(url, { body, headers = {}, method = 'GET', params, timeoutMs = REQUEST_TIMEOUT_MS } = {}) {
  const target = new URL(url);
  if (params) {
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined && value !== null) {
        target.searchParams.set(key, String(value));
      }
    }
  }

  let response;
  try {
    response = await fetch(target, {
      body,
      headers,
      method,
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (error) {
    throw new Error(`请求百度 API 失败：${error instanceof Error ? error.message : String(error)}`);
  }

  const text = await response.text();
  let payload;
  try {
    payload = text ? JSON.parse(text) : {};
  } catch {
    throw new Error(`百度 API 返回了无法解析的响应（HTTP ${response.status}）`);
  }

  if (!response.ok) {
    throwIfApiError(payload, response.status);
    throw new BaiduApiError(`百度 API 请求失败（HTTP ${response.status}）`, {
      requestId: getRequestId(payload),
      status: response.status,
      payload,
    });
  }

  throwIfApiError(payload, response.status);
  return payload;
}

function toFormBody(values) {
  const body = new URLSearchParams();
  for (const [key, value] of Object.entries(values)) {
    if (value !== undefined && value !== null) body.set(key, String(value));
  }
  return body;
}

function authStatePath(config) {
  return `${config.tokenPath}.auth-state`;
}

async function saveAuthState(config, state) {
  await mkdir(dirname(config.tokenPath), { recursive: true });
  await writeFile(authStatePath(config), `${state}\n`, { encoding: 'utf8', mode: 0o600 });
  try {
    await chmod(authStatePath(config), 0o600);
  } catch {
    // chmod is best-effort on Windows; the state is still kept outside the repository.
  }
}

async function verifyAuthState(config, state) {
  let expected;
  try {
    expected = (await readFile(authStatePath(config), 'utf8')).trim();
  } catch (error) {
    if (error?.code === 'ENOENT') {
      throw new Error('没有找到本次授权的 state，请重新执行 auth-url 后再授权');
    }
    throw new Error(`读取 OAuth state 失败：${error instanceof Error ? error.message : String(error)}`);
  }

  if (!state) {
    throw new Error('exchange-code 需要同时传入回调地址中的 state：exchange-code <code> <state>');
  }

  const expectedBuffer = Buffer.from(expected, 'utf8');
  const actualBuffer = Buffer.from(state, 'utf8');
  if (
    expectedBuffer.length !== actualBuffer.length
    || !timingSafeEqual(expectedBuffer, actualBuffer)
  ) {
    throw new Error('OAuth state 校验失败，请确认 code 和 state 来自同一次授权');
  }
}

async function clearAuthState(config) {
  await rm(authStatePath(config), { force: true });
}

async function saveToken(config, oauthPayload, previousToken = {}) {
  if (!oauthPayload?.access_token) {
    throw new Error('百度 OAuth 响应中没有 access_token，授权没有完成');
  }

  const expiresIn = Number(oauthPayload.expires_in);
  const token = {
    access_token: oauthPayload.access_token,
    expires_at: Number.isFinite(expiresIn) && expiresIn > 0
      ? Date.now() + expiresIn * 1000
      : previousToken.expires_at,
    refresh_token: oauthPayload.refresh_token || previousToken.refresh_token,
    scope: oauthPayload.scope || previousToken.scope || '',
    updated_at: new Date().toISOString(),
  };

  await mkdir(dirname(config.tokenPath), { recursive: true });
  const temporaryPath = `${config.tokenPath}.${randomUUID()}.tmp`;
  try {
    await writeFile(temporaryPath, `${JSON.stringify(token, null, 2)}\n`, { encoding: 'utf8', mode: 0o600 });
    try {
      await chmod(temporaryPath, 0o600);
    } catch {
      // chmod is best-effort on Windows; the token is still kept outside the repository.
    }

    try {
      await rename(temporaryPath, config.tokenPath);
    } catch {
      await rm(config.tokenPath, { force: true });
      await rename(temporaryPath, config.tokenPath);
    }
  } finally {
    await rm(temporaryPath, { force: true });
  }

  return token;
}

async function readToken(config) {
  const environmentToken = envValue('BAIDU_ACCESS_TOKEN');
  if (environmentToken) {
    return {
      access_token: environmentToken,
      expires_at: Number.POSITIVE_INFINITY,
      refresh_token: '',
      scope: 'environment',
    };
  }

  let raw;
  try {
    raw = await readFile(config.tokenPath, 'utf8');
  } catch (error) {
    if (error?.code === 'ENOENT') {
      throw new Error(`没有找到百度授权令牌：${config.tokenPath}\n请先执行 auth-url，再执行 exchange-code <授权码>`);
    }
    throw new Error(`读取百度授权令牌失败：${error instanceof Error ? error.message : String(error)}`);
  }

  let token;
  try {
    token = JSON.parse(raw);
  } catch {
    throw new Error(`百度授权令牌文件不是有效 JSON：${config.tokenPath}`);
  }

  if (!token?.access_token) {
    throw new Error(`百度授权令牌文件缺少 access_token：${config.tokenPath}`);
  }

  if (!token.expires_at && token.expires_in) {
    token.expires_at = Date.now() + Number(token.expires_in) * 1000;
  }
  return token;
}

class BaiduPanClient {
  constructor(config) {
    this.config = config;
    this.token = null;
  }

  async refreshToken() {
    const current = this.token ?? await readToken(this.config);
    if (!current.refresh_token) {
      throw new Error('当前令牌没有 refresh_token，请重新执行 OAuth 授权');
    }

    let response;
    try {
      response = await fetchJson(`${OAUTH_ROOT}/token`, {
        params: {
          client_id: this.config.appKey,
          client_secret: this.config.secretKey,
          grant_type: 'refresh_token',
          refresh_token: current.refresh_token,
        },
        headers: { 'User-Agent': USER_AGENT },
      });
    } catch (error) {
      throw new Error(`刷新百度 Access Token 失败：${error instanceof Error ? error.message : String(error)}\n请不要重复使用旧 refresh_token；失败后需要重新授权。`);
    }

    this.token = await saveToken(this.config, response, current);
    return this.token;
  }

  async getToken({ forceRefresh = false } = {}) {
    if (!this.token) this.token = await readToken(this.config);

    const expiresAt = Number(this.token.expires_at);
    const shouldRefresh = forceRefresh
      || (Number.isFinite(expiresAt) && expiresAt <= Date.now() + TOKEN_REFRESH_SKEW);

    if (shouldRefresh && this.token.refresh_token) {
      return this.refreshToken();
    }
    if (Number.isFinite(expiresAt) && expiresAt <= Date.now()) {
      throw new Error('百度 Access Token 已过期，且没有可用的 refresh_token，请重新授权');
    }
    return this.token;
  }

  async accessToken(options) {
    const token = await this.getToken(options);
    return token.access_token;
  }

  async request(url, { auth = true, body, headers = {}, method = 'GET', params = {}, timeoutMs = REQUEST_TIMEOUT_MS } = {}) {
    let accessToken = auth ? await this.accessToken() : '';

    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        return await fetchJson(url, {
          body,
          headers: { 'User-Agent': USER_AGENT, ...headers },
          method,
          params: auth ? { ...params, access_token: accessToken } : params,
          timeoutMs,
        });
      } catch (error) {
        if (
          attempt === 0
          && auth
          && error instanceof BaiduApiError
          && Number(error.code) === 31045
          && this.config.appKey
          && this.config.secretKey
          && this.token?.refresh_token
        ) {
          accessToken = await this.accessToken({ forceRefresh: true });
          continue;
        }
        throw error;
      }
    }

    throw new Error('百度 API 请求失败');
  }

  async api(method, resource, options = {}) {
    return this.request(`${API_ROOT}/${resource}`, { method, ...options });
  }

  async getUserInfo() {
    return this.api('GET', 'nas', {
      params: { method: 'uinfo', vip_version: 'v2' },
    });
  }

  async getQuota() {
    return this.request('https://pan.baidu.com/api/quota', {
      params: { checkexpire: 1, checkfree: 1 },
    });
  }

  async list(remoteDir, { foldersOnly = false, limit = 1000, order = 'name', desc = 0 } = {}) {
    const items = [];
    let start = 0;

    while (true) {
      const response = await this.api('GET', 'file', {
        params: {
          desc,
          dir: remoteDir,
          folder: foldersOnly ? 1 : 0,
          limit,
          method: 'list',
          order,
          start,
          web: 0,
        },
      });
      const page = Array.isArray(response?.list)
        ? response.list
        : Array.isArray(response?.data?.list)
          ? response.data.list
          : [];
      items.push(...page);
      if (page.length < limit) break;
      start += page.length;
    }

    return items;
  }

  async getMeta(remotePath, { dlink = false } = {}) {
    try {
      const response = await this.api('GET', 'file', {
        params: { dlink: dlink ? 1 : 0, method: 'meta', path: remotePath },
      });
      const list = response?.list
        ?? response?.info
        ?? response?.data?.list
        ?? response?.data?.info;
      if (Array.isArray(list)) return list[0] ?? null;
      if (response?.path) return response;
      if (response?.data?.path) return response.data;
      return null;
    } catch (error) {
      if (error instanceof BaiduApiError && Number(error.code) === -9) return null;
      throw error;
    }
  }

  async createFolder(remotePath) {
    const existing = await this.getMeta(remotePath);
    if (existing) {
      if (Number(existing.isdir) === 1) return { created: false, existing };
      throw new Error(`远端路径已被文件占用：${remotePath}`);
    }

    try {
      const response = await this.api('POST', 'file', {
        body: toFormBody({
          block_list: '[]',
          isdir: 1,
          path: remotePath,
          rtype: 1,
          size: 0,
        }),
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        params: { method: 'create' },
      });
      return { created: true, response };
    } catch (error) {
      if (error instanceof BaiduApiError && Number(error.code) === -8) {
        return { created: false, existing: await this.getMeta(remotePath) };
      }
      throw error;
    }
  }

  async fileManager(opera, fileList, { ondup = 'fail' } = {}) {
    const values = {
      async: 1,
      filelist: JSON.stringify(fileList),
    };
    if (opera !== 'delete') values.ondup = ondup;

    return this.api('POST', 'file', {
      body: toFormBody(values),
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      params: { method: 'filemanager', opera },
    });
  }

  async locateUploadServer(remotePath, uploadId) {
    const response = await this.request('https://d.pcs.baidu.com/rest/2.0/pcs/file', {
      params: {
        appid: 250528,
        method: 'locateupload',
        path: remotePath,
        upload_version: '2.0',
        uploadid: uploadId,
      },
    });
    const servers = Array.isArray(response?.servers)
      ? response.servers.map((item) => item?.server).filter(Boolean)
      : [];
    const server = servers.find((item) => item.startsWith('https://'));
    if (!server) {
      throw new Error('百度没有返回可用的 HTTPS 上传域名');
    }
    return server.replace(/\/$/, '');
  }

  async uploadChunk(server, remotePath, uploadId, partSequence, chunk) {
    const token = await this.accessToken();
    const target = new URL(`${server}/rest/2.0/pcs/superfile2`);
    target.searchParams.set('access_token', token);
    target.searchParams.set('method', 'upload');
    target.searchParams.set('partseq', String(partSequence));
    target.searchParams.set('path', remotePath);
    target.searchParams.set('type', 'tmpfile');
    target.searchParams.set('uploadid', uploadId);

    const form = new FormData();
    form.append('file', new Blob([chunk], { type: 'application/octet-stream' }), 'part');

    let response;
    try {
      response = await fetch(target, {
        body: form,
        headers: { 'User-Agent': USER_AGENT },
        method: 'POST',
        signal: AbortSignal.timeout(UPLOAD_TIMEOUT_MS),
      });
    } catch (error) {
      throw new Error(`上传第 ${partSequence} 个分片失败：${error instanceof Error ? error.message : String(error)}`);
    }

    const text = await response.text();
    let payload;
    try {
      payload = text ? JSON.parse(text) : {};
    } catch {
      throw new Error(`上传第 ${partSequence} 个分片时返回了无法解析的响应（HTTP ${response.status}）`);
    }
    if (!response.ok) {
      throwIfApiError(payload, response.status);
      throw new BaiduApiError(`上传第 ${partSequence} 个分片失败（HTTP ${response.status}）`, {
        status: response.status,
        payload,
      });
    }
    throwIfApiError(payload, response.status);
    return payload;
  }

  async downloadDlink(dlink, start = 0) {
    const token = await this.accessToken();
    const separator = dlink.includes('?') ? '&' : '?';
    const target = `${dlink}${separator}access_token=${encodeURIComponent(token)}`;
    let response;
    try {
      response = await fetch(target, {
        headers: {
          ...(start > 0 ? { Range: `bytes=${start}-` } : {}),
          'User-Agent': USER_AGENT,
        },
        redirect: 'follow',
        signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS),
      });
    } catch (error) {
      throw new Error(`下载百度文件失败：${error instanceof Error ? error.message : String(error)}`);
    }
    return response;
  }
}

async function hashFile(filePath) {
  const handle = await open(filePath, 'r');
  const fullHash = createHash('md5');
  const sliceHash = createHash('md5');
  const blockHashes = [];
  const buffer = Buffer.allocUnsafe(UPLOAD_CHUNK_SIZE);
  let position = 0;
  let sliceRemaining = SLICE_MD5_SIZE;

  try {
    while (true) {
      const { bytesRead } = await handle.read(buffer, 0, buffer.length, position);
      if (bytesRead === 0) break;

      const chunk = buffer.subarray(0, bytesRead);
      fullHash.update(chunk);
      blockHashes.push(createHash('md5').update(chunk).digest('hex'));
      if (sliceRemaining > 0) {
        const slice = chunk.subarray(0, Math.min(sliceRemaining, chunk.length));
        sliceHash.update(slice);
        sliceRemaining -= slice.length;
      }
      position += bytesRead;
    }
  } finally {
    await handle.close();
  }

  if (blockHashes.length === 0) {
    const emptyMd5 = createHash('md5').update(Buffer.alloc(0)).digest('hex');
    blockHashes.push(emptyMd5);
  }

  return {
    blockHashes,
    md5: fullHash.digest('hex'),
    size: position,
    sliceMd5: sliceHash.digest('hex'),
  };
}

async function readChunk(filePath, position, length) {
  const handle = await open(filePath, 'r');
  const buffer = Buffer.allocUnsafe(length);
  try {
    const { bytesRead } = await handle.read(buffer, 0, length, position);
    return buffer.subarray(0, bytesRead);
  } finally {
    await handle.close();
  }
}

function retryable(error) {
  if (!(error instanceof BaiduApiError)) return true;
  if (error.status >= 500 || error.status === 429) return true;
  return false;
}

async function withRetries(action, attempts = 3) {
  let lastError;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await action();
    } catch (error) {
      lastError = error;
      if (attempt === attempts || !retryable(error)) throw error;
      await new Promise((resolveDelay) => setTimeout(resolveDelay, attempt * 1000));
    }
  }
  throw lastError;
}

function parseUploadBlockList(value, blockCount) {
  let raw = value;
  if (typeof raw === 'string') {
    try {
      raw = JSON.parse(raw);
    } catch {
      raw = raw.split(',').map((item) => item.trim()).filter(Boolean);
    }
  }

  if (!Array.isArray(raw)) return Array.from({ length: blockCount }, (_item, index) => index);

  const blockList = raw.map(Number);
  if (blockList.some((item) => !Number.isSafeInteger(item) || item < 0 || item >= blockCount)) {
    throw new Error('预上传响应中的 block_list 不是有效的分片序号列表');
  }

  // 百度接口用 [] 表示小文件仍需上传第 0 个分片。
  return blockList.length > 0 ? blockList : [0];
}

async function ensureRemoteDirectory(client, remoteDir) {
  const root = client.config.remoteRoot;
  if (remoteDir !== root && !remoteDir.startsWith(`${root}/`)) {
    throw new Error(`远端目录必须位于 ${root} 下：${remoteDir}`);
  }

  const relative = remoteDir.slice(root.length).replace(/^\//, '');
  let current = root;
  await client.createFolder(current);
  for (const segment of relative.split('/').filter(Boolean)) {
    current = posix.join(current, segment);
    await client.createFolder(current);
  }
}

async function uploadFile(
  client,
  localPath,
  remotePath,
  { ensureParent = true, json = false, ondup = 'fail' } = {},
) {
  const fileInfo = await stat(localPath);
  if (!fileInfo.isFile()) throw new Error(`本地路径不是文件：${localPath}`);
  if (fileInfo.size > MAX_NORMAL_FILE_SIZE) {
    throw new Error('当前脚本按普通用户限制上传，单文件不能超过 4GB');
  }

  if (ondup === 'skip') {
    const existing = await client.getMeta(remotePath);
    if (existing) {
      if (Number(existing.isdir) === 1) throw new Error(`远端路径是目录，不能作为文件上传目标：${remotePath}`);
      return { localPath, remotePath, size: fileInfo.size, skipped: true };
    }
  }

  if (ensureParent) await ensureRemoteDirectory(client, posix.dirname(remotePath));

  if (!json) console.error(`[baidu-pan] 正在计算 ${localPath} 的 MD5...`);
  const hashes = await hashFile(localPath);
  const rtype = { fail: 0, newcopy: 1, overwrite: 3, skip: 0 }[ondup];
  const precreate = await client.api('POST', 'file', {
    body: toFormBody({
      autoinit: 1,
      block_list: JSON.stringify(hashes.blockHashes),
      'content-md5': hashes.md5,
      isdir: 0,
      path: remotePath,
      rtype,
      size: hashes.size,
      'slice-md5': hashes.sliceMd5,
    }),
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    params: { method: 'precreate' },
  });

  const uploadId = precreate?.uploadid;
  if (!uploadId) {
    const fsId = precreate?.fs_id ?? precreate?.data?.fs_id;
    if (fsId !== undefined && fsId !== null) {
      return {
        fsId,
        localPath,
        md5: precreate?.md5 ?? hashes.md5,
        remotePath,
        response: precreate,
        size: hashes.size,
        skipped: true,
      };
    }
    throw new Error('预上传响应中没有 uploadid，无法继续上传');
  }

  const needBlocks = parseUploadBlockList(precreate.block_list, hashes.blockHashes.length);
  const server = await client.locateUploadServer(remotePath, uploadId);

  for (const partSequence of needBlocks) {
    const position = partSequence * UPLOAD_CHUNK_SIZE;
    const length = Math.min(UPLOAD_CHUNK_SIZE, Math.max(0, hashes.size - position));
    const chunk = await readChunk(localPath, position, length || UPLOAD_CHUNK_SIZE);
    if (!json) {
      console.error(`[baidu-pan] 上传分片 ${partSequence + 1}/${hashes.blockHashes.length}...`);
    }
    await withRetries(() => client.uploadChunk(server, remotePath, uploadId, partSequence, chunk));
  }

  const created = await client.api('POST', 'file', {
    body: toFormBody({
      block_list: JSON.stringify(hashes.blockHashes),
      isdir: 0,
      local_ctime: Math.floor(fileInfo.birthtimeMs / 1000),
      local_mtime: Math.floor(fileInfo.mtimeMs / 1000),
      path: remotePath,
      rtype,
      size: hashes.size,
      uploadid: uploadId,
    }),
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    params: { method: 'create' },
  });

  return {
    fsId: created?.fs_id,
    localPath,
    md5: created?.md5 || hashes.md5,
    remotePath,
    response: created,
    size: hashes.size,
    skipped: false,
  };
}

async function collectLocalTree(rootPath) {
  const directories = [];
  const files = [];

  async function visit(currentPath, relativePath) {
    const entries = await readdir(currentPath, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.isSymbolicLink()) continue;
      const childPath = join(currentPath, entry.name);
      const childRelativePath = relativePath ? join(relativePath, entry.name) : entry.name;
      if (entry.isDirectory()) {
        directories.push({ localPath: childPath, relativePath: childRelativePath });
        await visit(childPath, childRelativePath);
      } else if (entry.isFile()) {
        files.push({ localPath: childPath, relativePath: childRelativePath });
      }
    }
  }

  await visit(rootPath, '');
  directories.sort((left, right) => left.relativePath.localeCompare(right.relativePath));
  files.sort((left, right) => left.relativePath.localeCompare(right.relativePath));
  return { directories, files };
}

async function uploadDirectory(client, localRoot, remoteRoot, options) {
  const tree = await collectLocalTree(localRoot);
  const results = [];

  await ensureRemoteDirectory(client, remoteRoot);
  for (const directory of tree.directories) {
    const remotePath = posix.join(remoteRoot, directory.relativePath.replaceAll('\\', '/'));
    await client.createFolder(remotePath);
  }

  for (const file of tree.files) {
    const remotePath = posix.join(remoteRoot, file.relativePath.replaceAll('\\', '/'));
    results.push(await uploadFile(client, file.localPath, remotePath, { ...options, ensureParent: false }));
  }

  return {
    files: results,
    localRoot,
    remoteRoot,
    skipped: results.filter((item) => item.skipped).length,
    uploaded: results.filter((item) => !item.skipped).length,
  };
}

async function fileExists(filePath) {
  try {
    await stat(filePath);
    return true;
  } catch (error) {
    if (error?.code === 'ENOENT') return false;
    throw error;
  }
}

async function getDownloadResponse(client, remotePath, initialMeta, offset) {
  let meta = initialMeta;

  for (let attempt = 0; attempt < 2; attempt += 1) {
    const response = await client.downloadDlink(meta.dlink, offset);
    const contentType = response.headers.get('content-type') || '';
    if (response.ok && !contentType.toLowerCase().includes('json')) {
      return { meta, response };
    }

    const payload = await response.json().catch(() => null);
    if (attempt === 0 && Number(getErrorCode(payload)) === 31360) {
      const refreshedMeta = await client.getMeta(remotePath, { dlink: true });
      if (!refreshedMeta?.dlink) throw new Error('百度下载直链已过期，重新获取仍失败');
      meta = refreshedMeta;
      continue;
    }

    if (payload) throwIfApiError(payload, response.status);
    if (!response.ok) {
      throw new Error(`下载百度文件失败（HTTP ${response.status}）`);
    }
    throw new Error('百度下载响应不是文件内容');
  }

  throw new Error('百度下载直链已过期，重新获取仍失败');
}

async function downloadFile(client, remotePath, localPath, { force = false, json = false } = {}) {
  let meta = await client.getMeta(remotePath, { dlink: true });
  if (!meta) throw new Error(`远端文件不存在：${remotePath}`);
  if (Number(meta.isdir) === 1) throw new Error(`远端路径是目录，不能下载为文件：${remotePath}`);
  if (!meta.dlink) throw new Error('百度没有返回下载直链，请确认应用拥有下载权限');

  const finalPath = resolve(localPath);
  const partPath = `${finalPath}.part`;
  await mkdir(dirname(finalPath), { recursive: true });

  if (await fileExists(finalPath)) {
    if (!force) {
      const existing = await stat(finalPath);
      if (Number(meta.size) === existing.size) {
        return { localPath: finalPath, remotePath, size: existing.size, skipped: true };
      }
      throw new Error(`本地文件已存在：${finalPath}。如需覆盖请加 --force`);
    }
    await rm(finalPath, { force: true });
  }

  let offset = 0;
  if (await fileExists(partPath)) offset = (await stat(partPath)).size;
  if (Number.isFinite(Number(meta.size)) && offset > Number(meta.size)) {
    await rm(partPath, { force: true });
    offset = 0;
  }
  if (Number(meta.size) > 0 && offset === Number(meta.size)) {
    await rename(partPath, finalPath);
    return { localPath: finalPath, remotePath, size: offset, skipped: false, resumed: true };
  }

  const download = await getDownloadResponse(client, remotePath, meta, offset);
  meta = download.meta;
  const { response } = download;

  if (offset > 0 && response.status !== 206) {
    offset = 0;
    await rm(partPath, { force: true });
  }
  if (!response.body) throw new Error('百度下载响应没有文件内容');
  if (!json) console.error(`[baidu-pan] 正在下载 ${remotePath}...`);
  await pipeline(
    Readable.fromWeb(response.body),
    createWriteStream(partPath, { flags: offset > 0 ? 'a' : 'w' }),
  );

  const downloadedSize = (await stat(partPath)).size;
  if (Number.isFinite(Number(meta.size)) && downloadedSize !== Number(meta.size)) {
    throw new Error(`下载文件大小校验失败，已保留临时文件：${partPath}`);
  }
  await rename(partPath, finalPath);
  return { localPath: finalPath, remotePath, resumed: offset > 0, size: downloadedSize, skipped: false };
}

function formatBytes(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) return '-';
  if (number < 1024) return `${number} B`;
  if (number < 1024 ** 2) return `${(number / 1024).toFixed(1)} KB`;
  if (number < 1024 ** 3) return `${(number / 1024 ** 2).toFixed(1)} MB`;
  return `${(number / 1024 ** 3).toFixed(2)} GB`;
}

function formatTime(unixSeconds) {
  const number = Number(unixSeconds);
  if (!Number.isFinite(number) || number <= 0) return '-';
  return new Date(number * 1000).toLocaleString('zh-CN', { hour12: false });
}

function printJson(value) {
  console.log(JSON.stringify(value, null, 2));
}

function printOperation(response, json) {
  if (json) {
    printJson(response);
    return;
  }
  console.log('百度网盘操作请求已提交');
  if (response?.taskid) console.log(`异步任务：${response.taskid}`);
  if (response?.request_id) console.log(`请求 ID：${response.request_id}`);
}

function printHelp() {
  console.log(`用法：npm run baidu-pan -- <命令> [参数] [选项]

一次性授权：
  auth-url                              输出百度 OAuth 授权地址
  exchange-code <code> <state>          用授权码换取并保存令牌

查询：
  status                                查询账号、会员和容量
  list [远端目录]                       列出应用目录下的文件

文件操作：
  mkdir <远端目录>                      创建目录
  upload <本地文件> [远端文件]          上传单个文件
  upload-dir <本地目录> [远端目录]      递归备份目录内容
  download <远端文件> <本地文件>        下载文件，自动使用 .part 续传
  copy <源路径> <目标路径>              复制文件，目标以 / 结尾时保留原文件名
  move <源路径> <目标路径>              移动文件，目标以 / 结尾时保留原文件名
  rename <源路径> <新名称>              重命名文件或目录
  delete <远端路径>... --yes             删除文件或目录

环境变量：
  BAIDU_APP_KEY                         开放平台 AppKey
  BAIDU_SECRET_KEY                      开放平台 SecretKey
  BAIDU_REDIRECT_URI                    控制台登记的 OAuth 回调地址
  BAIDU_APP_NAME                        开放平台应用产品名
  BAIDU_REMOTE_ROOT                     可选，默认 /apps/<应用名>
  BAIDU_TOKEN_FILE                      可选，默认保存到用户配置目录
  BAIDU_ACCESS_TOKEN                    可选，仅使用当前令牌，不自动刷新

常用选项：
  --help, -h                            显示帮助
  --json                                输出 JSON
  --ondup fail|newcopy|overwrite|skip   上传/复制/移动冲突策略，默认 fail
  --force                               下载时覆盖本地文件
  --folders-only                        list 只显示目录
  --limit <1-1000>                      list 每页数量
  --order name|time|size                list 排序字段
  --desc 0|1                            list 是否降序
`);
}

async function run(command, args) {
  const { options, positionals } = parseArgs(args);
  if (!command || command === 'help' || options.help) {
    printHelp();
    return;
  }

  if (!COMMANDS.has(command)) {
    throw new Error(`未知命令：${command}。执行 help 查看用法。`);
  }

  if (command === 'auth-url') {
    const config = getConfig({ requireAppKey: true, requireRedirect: true });
    const state = randomUUID();
    await saveAuthState(config, state);
    const url = new URL(`${OAUTH_ROOT}/authorize`);
    url.searchParams.set('client_id', config.appKey);
    url.searchParams.set('redirect_uri', config.redirectUri);
    url.searchParams.set('response_type', 'code');
    url.searchParams.set('scope', 'basic,netdisk');
    url.searchParams.set('state', state);
    console.log(url.href);
    console.error('请在浏览器中完成授权，再把回调地址中的 code 和 state 传给 exchange-code。');
    return;
  }

  if (command === 'exchange-code') {
    requirePositionals(positionals, 2, 'exchange-code <code> <state>');
    const config = getConfig({ requireAppKey: true, requireRedirect: true, requireSecretKey: true });
    await verifyAuthState(config, positionals[1]);
    const response = await fetchJson(`${OAUTH_ROOT}/token`, {
      headers: { 'User-Agent': USER_AGENT },
      params: {
        client_id: config.appKey,
        client_secret: config.secretKey,
        code: positionals[0],
        grant_type: 'authorization_code',
        redirect_uri: config.redirectUri,
      },
    });
    const token = await saveToken(config, response);
    await clearAuthState(config);
    console.log(`授权成功，令牌已保存到：${config.tokenPath}`);
    if (Number.isFinite(Number(token.expires_at))) {
      console.log(`Access Token 到期时间：${new Date(token.expires_at).toLocaleString('zh-CN', { hour12: false })}`);
    }
    return;
  }

  if (command === 'status') {
    const config = getConfig();
    const client = new BaiduPanClient(config);
    const token = await client.getToken();
    const [user, quota] = await Promise.all([client.getUserInfo(), client.getQuota()]);
    const result = {
      account: user?.baidu_name || user?.netdisk_name || '',
      netdiskName: user?.netdisk_name || '',
      quota: {
        expireSoon: quota?.expire ?? false,
        free: quota?.free,
        total: quota?.total,
        used: quota?.used,
      },
      scope: token.scope || '',
      tokenExpiresAt: Number.isFinite(Number(token.expires_at)) ? new Date(token.expires_at).toISOString() : null,
      userId: user?.uk,
      vipType: user?.vip_type,
    };
    if (options.json) printJson(result);
    else {
      console.log(`百度账号：${result.account || '-'}`);
      console.log(`网盘账号：${result.netdiskName || '-'}`);
      console.log(`用户 ID：${result.userId ?? '-'}`);
      console.log(`会员类型：${result.vipType ?? '-'}`);
      console.log(`容量：${formatBytes(result.quota.used)} / ${formatBytes(result.quota.total)}（剩余 ${formatBytes(result.quota.total - result.quota.used)}）`);
      console.log(`令牌到期：${result.tokenExpiresAt ? new Date(result.tokenExpiresAt).toLocaleString('zh-CN', { hour12: false }) : '环境变量令牌'}`);
    }
    return;
  }

  const config = getConfig({ requireAppName: true });
  const client = new BaiduPanClient(config);

  if (command === 'list') {
    const remoteDir = resolveRemotePath(positionals[0] || '/', config);
    const limit = options.limit === undefined ? 1000 : parsePositiveInteger(options.limit, 'limit', { max: 1000 });
    const order = options.order || 'name';
    const items = await client.list(remoteDir, {
      desc: parseDesc(options.desc),
      foldersOnly: Boolean(options.foldersOnly),
      limit,
      order,
    });
    if (options.json) printJson(items);
    else {
      for (const item of items) {
        const marker = Number(item.isdir) === 1 ? '[D]' : '[F]';
        console.log(`${marker} ${item.server_filename || item.path}  ${Number(item.isdir) === 1 ? '' : formatBytes(item.size)}  ${formatTime(item.server_mtime)}`.trimEnd());
      }
      console.log(`共 ${items.length} 项`);
    }
    return;
  }

  if (command === 'mkdir') {
    requirePositionals(positionals, 1, 'mkdir <远端目录>');
    const remotePath = resolveRemotePath(positionals[0], config);
    const result = await client.createFolder(remotePath);
    if (options.json) printJson({ path: remotePath, ...result });
    else console.log(`${result.created ? '已创建' : '已存在'}：${remotePath}`);
    return;
  }

  if (command === 'upload') {
    requirePositionals(positionals, 1, 'upload <本地文件> [远端文件]');
    const localPath = resolve(positionals[0]);
    const remotePath = resolveDestinationPath(positionals[1] || basename(localPath), basename(localPath), config);
    const ondup = options.ondup || 'fail';
    if (!ON_DUP_VALUES.has(ondup)) throw new Error(`--ondup 只能是 ${[...ON_DUP_VALUES].join('|')}`);
    const result = await uploadFile(client, localPath, remotePath, { json: Boolean(options.json), ondup });
    if (options.json) printJson(result);
    else console.log(`${result.skipped ? '已跳过' : '上传完成'}：${remotePath}（${formatBytes(result.size)}）`);
    return;
  }

  if (command === 'upload-dir') {
    requirePositionals(positionals, 1, 'upload-dir <本地目录> [远端目录]');
    const localRoot = resolve(positionals[0]);
    const localInfo = await stat(localRoot);
    if (!localInfo.isDirectory()) throw new Error(`本地路径不是目录：${localRoot}`);
    const defaultRemote = basename(localRoot.replace(/[\\/]$/, ''));
    const remoteRoot = resolveRemotePath(positionals[1] || defaultRemote, config);
    const ondup = options.ondup || 'skip';
    if (!ON_DUP_VALUES.has(ondup)) throw new Error(`--ondup 只能是 ${[...ON_DUP_VALUES].join('|')}`);
    const result = await uploadDirectory(client, localRoot, remoteRoot, { json: Boolean(options.json), ondup });
    if (options.json) printJson(result);
    else console.log(`目录备份完成：上传 ${result.uploaded} 个，跳过 ${result.skipped} 个`);
    return;
  }

  if (command === 'download') {
    requirePositionals(positionals, 2, 'download <远端文件> <本地文件>');
    const remotePath = resolveRemotePath(positionals[0], config);
    const result = await downloadFile(client, remotePath, positionals[1], {
      force: Boolean(options.force),
      json: Boolean(options.json),
    });
    if (options.json) printJson(result);
    else console.log(`${result.skipped ? '本地已有同大小文件，已跳过' : '下载完成'}：${result.localPath}`);
    return;
  }

  if (command === 'copy' || command === 'move') {
    requirePositionals(positionals, 2, `${command} <源路径> <目标路径>`);
    const source = resolveRemotePath(positionals[0], config);
    const destination = resolveDestinationPath(positionals[1], source, config);
    const ondup = options.ondup || 'fail';
    if (!ON_DUP_VALUES.has(ondup)) throw new Error(`--ondup 只能是 ${[...ON_DUP_VALUES].join('|')}`);
    const response = await client.fileManager(command, [{
      dest: posix.dirname(destination),
      newname: getNameFromRemotePath(destination),
      path: source,
    }], { ondup });
    printOperation(response, Boolean(options.json));
    return;
  }

  if (command === 'rename') {
    requirePositionals(positionals, 2, 'rename <源路径> <新名称>');
    const source = resolveRemotePath(positionals[0], config);
    const nameOrPath = positionals[1].replaceAll('\\', '/');
    const destination = nameOrPath.includes('/')
      ? resolveRemotePath(nameOrPath, config)
      : posix.join(posix.dirname(source), nameOrPath);
    const response = await client.fileManager('rename', [{ path: source, newname: destination }]);
    printOperation(response, Boolean(options.json));
    return;
  }

  if (command === 'delete') {
    requirePositionals(positionals, 1, 'delete <远端路径>... --yes');
    if (!options.yes) throw new Error('删除云端文件需要显式加 --yes');
    const paths = positionals.map((value) => resolveRemotePath(value, config));
    const response = await client.fileManager('delete', paths);
    printOperation(response, Boolean(options.json));
    return;
  }

  throw new Error(`未知命令：${command}。执行 help 查看用法。`);
}

try {
  const [command, ...args] = process.argv.slice(2);
  if (command === '--help' || command === '-h') {
    printHelp();
  } else {
    await run(command, args);
  }
} catch (error) {
  console.error(`[baidu-pan] ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}
