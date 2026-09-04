#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const DEFAULT_WSL_DISTRO = 'Ubuntu-22.04';

const COMMAND_ALIASES = new Map([
  ['auth', 'login'],
  ['status', 'whoami'],
  ['list', 'ls'],
  ['copy', 'cp'],
  ['move', 'mv'],
  ['delete', 'rm'],
]);

const SUPPORTED_COMMANDS = new Set([
  'login',
  'auth',
  'logout',
  'status',
  'whoami',
  'list',
  'ls',
  'search',
  'upload',
  'download',
  'transfer',
  'share',
  'mkdir',
  'copy',
  'cp',
  'move',
  'mv',
  'rename',
  'delete',
  'rm',
  'version',
  'vip',
]);

function envValue(name) {
  const value = process.env[name]?.trim();
  return value || '';
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\\"'\\\"'")}'`;
}

function toWslPath(value) {
  const normalized = String(value).replaceAll('\\', '/');
  const drivePath = normalized.match(/^([A-Za-z]):(?:\/(.*))?$/);
  if (drivePath) {
    const suffix = drivePath[2] ? `/${drivePath[2]}` : '';
    return `/mnt/${drivePath[1].toLowerCase()}${suffix}`;
  }
  if (normalized.startsWith('/')) return normalized;
  throw new Error(`无法转换为 WSL 路径：${value}`);
}

function getSkillDirectory() {
  const configured = envValue('BAIDU_SKILL_DIR');
  const windowsOrWslPath = configured || join(homedir(), '.codex', 'skills', 'baidu-drive');
  return toWslPath(windowsOrWslPath);
}

function getRuntime() {
  if (process.platform === 'win32') {
    return {
      launcher: 'wsl.exe',
      prefix: ['-d', envValue('BAIDU_WSL_DISTRO') || DEFAULT_WSL_DISTRO, '--'],
    };
  }

  return { launcher: 'bash', prefix: [] };
}

function shellPreamble() {
  return `cd ${shellQuote(toWslPath(process.cwd()))} && export PATH="$HOME/.local/bin:$PATH"`;
}

function runShell(command) {
  const runtime = getRuntime();
  const result = spawnSync(
    runtime.launcher,
    [...runtime.prefix, 'bash', '-lc', command],
    { stdio: 'inherit', windowsHide: false },
  );

  if (result.error) {
    throw new Error(`无法启动 ${runtime.launcher}：${result.error.message}`);
  }

  if (result.status !== 0) {
    process.exitCode = typeof result.status === 'number' ? result.status : 1;
  }
}

function runLogin() {
  const loginScript = `${getSkillDirectory()}/scripts/login.sh`;
  runShell(`${shellPreamble()}; exec bash ${shellQuote(loginScript)}`);
}

function translateLocalArguments(command, args) {
  if (process.platform !== 'win32') return args;

  const translated = [...args];
  if (command === 'upload' && translated[0] && !translated[0].startsWith('-')) {
    translated[0] = toWslPath(translated[0]);
  }
  if (command === 'download' && translated[1] && !translated[1].startsWith('-')) {
    translated[1] = toWslPath(translated[1]);
  }
  return translated;
}

function normalizeArguments(rawCommand, args) {
  const command = COMMAND_ALIASES.get(rawCommand) || rawCommand;
  let normalized = translateLocalArguments(rawCommand, args);

  if (rawCommand === 'delete' && normalized.includes('--yes')) {
    normalized = normalized.map((value) => (value === '--yes' ? '--force' : value));
  }
  if (rawCommand === 'list' && normalized.includes('--folders-only')) {
    normalized = normalized.map((value) => (value === '--folders-only' ? '--folder' : value));
  }

  return { command, args: normalized };
}

function runBdpan(rawCommand, args) {
  const { command, args: normalizedArgs } = normalizeArguments(rawCommand, args);
  const commandLine = [
    shellPreamble(),
    `exec bdpan ${[command, ...normalizedArgs].map(shellQuote).join(' ')}`,
  ].join('; ');
  runShell(commandLine);
}

function printHelp() {
  console.log(`用法：npm run baidu-pan -- <命令> [参数] [选项]

授权：
  login                                  调用 baidu-drive skill 登录脚本
  auth                                   login 的别名
  logout                                 注销当前百度网盘授权

查询：
  status                                 查看认证状态
  list [远端目录]                        列出文件（远端路径位于 /apps/bdpan）
  search <关键词>                        搜索文件

文件操作：
  upload <本地路径> <远端路径>            上传文件或目录
  download <远端路径> <本地路径>          下载文件或目录
  transfer <分享链接>                    转存分享文件
  share <远端路径>                       创建分享链接
  mkdir <远端目录>                       创建文件夹
  copy <源路径> <目标目录>                复制文件或目录
  move <源路径> <目标目录>                移动文件或目录
  rename <路径> <新名称>                  重命名文件或目录
  delete <远端路径>...                    删除文件或目录

其他：
  version                                查看 bdpan 版本
  vip                                    获取会员入口
  --help, -h                             显示帮助

登录说明：
  login 会在 WSL 中执行已安装 skill 的 scripts/login.sh。
  首次登录时打开它输出的授权链接，完成授权后输入浏览器显示的 32 位授权码。
  不需要 AppKey，不使用项目内的 token.json；授权配置由 WSL 中的 bdpan 管理。

环境变量：
  BAIDU_WSL_DISTRO                       WSL 发行版，默认 Ubuntu-22.04
  BAIDU_SKILL_DIR                        skill 路径，默认 ~/.codex/skills/baidu-drive

示例：
  npm run baidu-pan -- login
  npm run baidu-pan -- status
  npm run baidu-pan -- list --json
  npm run baidu-pan -- upload "E:\\文件\\报告.pdf" 报告.pdf
  npm run baidu-pan -- download 报告.pdf "E:\\下载\\报告.pdf"
`);
}

function main() {
  const [rawCommand, ...args] = process.argv.slice(2);

  if (!rawCommand || rawCommand === 'help' || rawCommand === '--help' || rawCommand === '-h') {
    printHelp();
    return;
  }
  if (args.includes('--help') || args.includes('-h')) {
    printHelp();
    return;
  }
  if (rawCommand === 'auth-url' || rawCommand === 'exchange-code') {
    throw new Error('当前 CLI 已切换为 baidu-drive skill 授权流程，请执行：npm run baidu-pan -- login');
  }
  if (!SUPPORTED_COMMANDS.has(rawCommand)) {
    throw new Error(`未知命令：${rawCommand}。执行 help 查看用法。`);
  }
  if (rawCommand === 'login' || rawCommand === 'auth') {
    if (args.length > 0) {
      throw new Error('login 不需要额外参数；执行 npm run baidu-pan -- help 查看用法');
    }
    runLogin();
    return;
  }

  runBdpan(rawCommand, args);
}

try {
  main();
} catch (error) {
  console.error(`[baidu-pan] ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}
