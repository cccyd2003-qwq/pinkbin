import type { Node, Scaffold, AdvisorRequest, AdvisorResponse, UndoEntry, Plan, SteamInventory, WorkshopItem } from './types';
import { callAdvisor, isConfigured, loadSettings } from './advisorClient';

const GB = 1024 ** 3;

function leaf(name: string, path: string, size: number, files: number, scaffold_id: string | null = null): Node {
  return {
    name,
    path,
    is_dir: true,
    size,
    file_count: files,
    children: [],
    scaffold_id,
    top_extensions: [],
  };
}

const MOCK_TREE: Node = {
  name: 'C:',
  path: 'C:\\',
  is_dir: true,
  size: 187.1 * GB,
  file_count: 805_990,
  scaffold_id: null,
  top_extensions: [
    { ext: '.dll', bytes: 48.6 * GB, count: 56_729 },
    { ext: '(none)', bytes: 37.5 * GB, count: 165_172 },
    { ext: '.exe', bytes: 16.6 * GB, count: 6_644 },
    { ext: '.db', bytes: 6.4 * GB, count: 1_473 },
  ],
  children: [
    {
      name: 'Users', path: 'C:\\Users', is_dir: true, size: 90 * GB, file_count: 480_730,
      scaffold_id: null, top_extensions: [],
      children: [
        {
          name: '90740', path: 'C:\\Users\\90740', is_dir: true, size: 90.3 * GB, file_count: 478_120,
          scaffold_id: null, top_extensions: [],
          children: [
            {
              name: 'AppData', path: 'C:\\Users\\90740\\AppData', is_dir: true, size: 71.1 * GB, file_count: 410_220,
              scaffold_id: null, top_extensions: [],
              children: [
                {
                  name: 'Local', path: 'C:\\Users\\90740\\AppData\\Local', is_dir: true, size: 51 * GB, file_count: 280_110,
                  scaffold_id: null, top_extensions: [],
                  children: [
                    leaf('Microsoft', 'C:\\Users\\90740\\AppData\\Local\\Microsoft', 14.8 * GB, 88_400, null),
                    {
                      name: 'Edge', path: 'C:\\Users\\90740\\AppData\\Local\\Microsoft\\Edge', is_dir: true,
                      size: 12.8 * GB, file_count: 41_320, scaffold_id: 'edge',
                      top_extensions: [{ ext: '(none)', bytes: 6 * GB, count: 12_000 }],
                      children: [
                        leaf('User Data', 'C:\\Users\\90740\\AppData\\Local\\Microsoft\\Edge\\User Data', 12.5 * GB, 40_900, 'edge'),
                      ],
                    },
                    {
                      name: 'Google', path: 'C:\\Users\\90740\\AppData\\Local\\Google', is_dir: true,
                      size: 6.4 * GB, file_count: 22_100, scaffold_id: 'chrome',
                      top_extensions: [], children: [
                        leaf('Chrome', 'C:\\Users\\90740\\AppData\\Local\\Google\\Chrome', 6.4 * GB, 22_100, 'chrome'),
                      ],
                    },
                    leaf('npm-cache', 'C:\\Users\\90740\\AppData\\Local\\npm-cache', 1.8 * GB, 8_400, 'npm'),
                    leaf('pnpm', 'C:\\Users\\90740\\AppData\\Local\\pnpm', 4.2 * GB, 15_200, 'pnpm'),
                    leaf('pip', 'C:\\Users\\90740\\AppData\\Local\\pip', 2.1 * GB, 5_300, 'pip'),
                    leaf('JetBrains', 'C:\\Users\\90740\\AppData\\Local\\JetBrains', 3.6 * GB, 88_900, 'jetbrains'),
                    leaf('Docker', 'C:\\Users\\90740\\AppData\\Local\\Docker', 5.2 * GB, 1_200, 'docker'),
                  ],
                },
                {
                  name: 'Roaming', path: 'C:\\Users\\90740\\AppData\\Roaming', is_dir: true, size: 17.7 * GB, file_count: 95_400,
                  scaffold_id: null, top_extensions: [], children: [
                    leaf('Tencent', 'C:\\Users\\90740\\AppData\\Roaming\\Tencent', 2.3 * GB, 4_100, null),
                  ],
                },
                leaf('LocalLow', 'C:\\Users\\90740\\AppData\\LocalLow', 2.1 * GB, 12_900, null),
              ],
            },
            {
              name: 'Documents', path: 'C:\\Users\\90740\\Documents', is_dir: true, size: 12.1 * GB, file_count: 18_400,
              scaffold_id: null, top_extensions: [],
              children: [
                {
                  name: 'WeChat Files', path: 'C:\\Users\\90740\\Documents\\WeChat Files', is_dir: true,
                  size: 11.4 * GB, file_count: 17_220, scaffold_id: 'wechat-pc',
                  top_extensions: [
                    { ext: '.dat', bytes: 6.8 * GB, count: 12_400 },
                    { ext: '.jpg', bytes: 2.1 * GB, count: 3_100 },
                    { ext: '.mp4', bytes: 1.6 * GB, count: 420 },
                  ],
                  children: [
                    leaf('wxid_gmsp9xjx12', 'C:\\Users\\90740\\Documents\\WeChat Files\\wxid_gmsp9xjx12', 10.7 * GB, 16_800, 'wechat-pc'),
                  ],
                },
              ],
            },
            leaf('.cargo', 'C:\\Users\\90740\\.cargo', 0.8 * GB, 12_400, 'cargo'),
            leaf('.cache/huggingface', 'C:\\Users\\90740\\.cache\\huggingface', 4.6 * GB, 320, 'huggingface'),
          ],
        },
      ],
    },
    {
      name: 'Windows', path: 'C:\\Windows', is_dir: true, size: 36.2 * GB, file_count: 169_900,
      scaffold_id: null, top_extensions: [],
      children: [
        leaf('System32', 'C:\\Windows\\System32', 8.2 * GB, 32_400),
        leaf('WinSxS', 'C:\\Windows\\WinSxS', 7.7 * GB, 48_200),
      ],
    },
    {
      name: 'Program Files', path: 'C:\\Program Files', is_dir: true, size: 24.5 * GB, file_count: 94_900,
      scaffold_id: null, top_extensions: [], children: [
        leaf('WindowsApps', 'C:\\Program Files\\WindowsApps', 9.5 * GB, 31_200),
        leaf('Steam', 'C:\\Program Files\\Steam', 4.8 * GB, 21_000, 'steam'),
      ],
    },
    leaf('Program Files (x86)', 'C:\\Program Files (x86)', 17.4 * GB, 22_000),
    leaf('ProgramData', 'C:\\ProgramData', 9.7 * GB, 31_160),
    leaf('Recovery', 'C:\\Recovery', 3.7 * GB, 48),
    leaf('Eastmoney', 'C:\\Eastmoney', 1.1 * GB, 4_280),
    leaf('System Volume Information', 'C:\\System Volume Information', 0.96 * GB, 27),
    leaf('Microsoft VS Code', 'C:\\Microsoft VS Code', 0.43 * GB, 2_410),
  ],
};

export const SCAFFOLDS: Scaffold[] = [
  {
    id: 'wechat-pc', name: 'WeChat (PC)', risk: 'low',
    disclaimer: '只动 FileStorage 媒体缓存。聊天记录保留，但已删的图片视频在旧聊天里会显示缺失。',
    detect: ['**/WeChat Files/*/FileStorage'],
    match: { name_contains: ['WeChat Files'], must_have_child: ['FileStorage'] },
    scopes: [
      { id: 'image-cache', label: '图片缓存（FileStorage/Image）', glob: '**/Image/**', mode: 'recycle', prompt: { kind: 'days', default: 30, label: '删除多少天前的图片' } },
      { id: 'video-cache', label: '视频缓存（FileStorage/Video）', glob: '**/Video/**', mode: 'recycle', prompt: { kind: 'days', default: 7, label: '删除多少天前的视频' } },
      { id: 'file-cache',  label: '接收的文件（FileStorage/File）', glob: '**/File/**',  mode: 'recycle', prompt: { kind: 'days', default: 30, label: '删除多少天前的文件' } },
      ],
  },
  {
    id: 'netease-cloud-music', name: '网易云音乐（PC）', risk: 'low',
    disclaimer: '只清播放/Web 缓存、临时文件和日志；下载歌曲、账号状态、播放列表和 Library 保留。',
    detect: ['%LOCALAPPDATA%/NetEase/CloudMusic', '%APPDATA%/NetEase/CloudMusic', '**/NetEase/CloudMusic'],
    match: {},
    scopes: [
      { id: 'playback-cache', label: '播放缓存（Cache/Cache）', glob: '**/NetEase/CloudMusic/Cache/Cache/**', mode: 'recycle', category: 'cache', prompt: { kind: 'days', default: 30, label: '保留最近多少天' } },
      { id: 'static-assets', label: '静态资源缓存（Statics）', glob: '**/NetEase/CloudMusic/Statics/**', mode: 'recycle', category: 'cache', prompt: { kind: 'none' } },
      { id: 'web-cache', label: '网页内核缓存', glob: '**/NetEase/CloudMusic/webapp*/{Cache,Code Cache,GPUCache}/**', mode: 'recycle', category: 'cache', prompt: { kind: 'none' } },
      { id: 'temp-files', label: '临时文件', glob: '**/NetEase/CloudMusic/Temp/**', mode: 'recycle', category: 'cache', prompt: { kind: 'none' } },
      { id: 'app-logs', label: '运行日志', glob: '**/NetEase/CloudMusic/Log/**', mode: 'recycle', category: 'cache', prompt: { kind: 'none' } },
      { id: 'main-log', label: '主程序日志（cloudmusic.elog）', glob: '**/NetEase/CloudMusic/cloudmusic.elog', mode: 'recycle', category: 'cache', prompt: { kind: 'none' } },
      { id: 'update-cache', label: '更新下载残留', glob: '**/NetEase/CloudMusic/update/**', mode: 'recycle', category: 'cache', prompt: { kind: 'none' } },
    ],
  },
  { id: 'qq-pc', name: '腾讯 QQ', risk: 'low', disclaimer: '只清接收的图片/视频/文件缓存。聊天数据库保留。', detect: ['**/Tencent Files'], match: {}, scopes: [{ id: 'image-c2c', label: '私聊图片', glob: '**/Image/C2C/**', mode: 'recycle', prompt: { kind: 'days', default: 30 } }] },
  { id: 'dingtalk', name: '钉钉 DingTalk', risk: 'low', disclaimer: '只清缓存图片/视频。', detect: ['**/DingTalk'], match: {}, scopes: [{ id: 'image-cache', label: '图片缓存', glob: '**/ImageFiles/**', mode: 'recycle' }] },
  { id: 'feishu', name: '飞书 / Lark', risk: 'low', disclaimer: '只清媒体与缓存。', detect: ['**/Feishu', '**/Lark'], match: {}, scopes: [{ id: 'media', label: '媒体缓存', glob: '**/storage/{image,video}/**', mode: 'recycle' }] },
  { id: 'slack', name: 'Slack', risk: 'low', disclaimer: 'Electron 缓存。', detect: ['**/Slack'], match: {}, scopes: [{ id: 'http-cache', label: 'Cache', glob: '**/Cache/**', mode: 'recycle' }] },
  { id: 'discord', name: 'Discord', risk: 'low', disclaimer: 'Electron 缓存，可能要重登。', detect: ['**/discord'], match: {}, scopes: [{ id: 'http-cache', label: 'Cache', glob: '**/Cache/**', mode: 'recycle' }] },
  { id: 'telegram', name: 'Telegram Desktop', risk: 'low', disclaimer: '只清媒体缓存。', detect: ['**/Telegram Desktop'], match: {}, scopes: [{ id: 'media', label: '媒体缓存', glob: '**/cache/**', mode: 'recycle', prompt: { kind: 'days', default: 30 } }] },
  { id: 'spotify', name: 'Spotify', risk: 'low', disclaimer: '本地音乐缓存，离线模式失效需重下。', detect: ['**/Spotify'], match: {}, scopes: [{ id: 'data', label: 'Storage / Data', glob: '**/{Storage,Data}/**', mode: 'recycle' }] },
  { id: 'teams', name: 'Microsoft Teams', risk: 'low', disclaimer: '清浏览器内核缓存，可能要重登。', detect: ['**/Microsoft/Teams'], match: {}, scopes: [{ id: 'http-cache', label: 'Cache', glob: '**/Cache/**', mode: 'recycle' }] },
  { id: 'vscode', name: 'VS Code', risk: 'low', disclaimer: '清缓存日志，配置/扩展保留。', detect: ['**/Code/User'], match: {}, scopes: [{ id: 'cached-data', label: 'CachedData', glob: '**/CachedData/**', mode: 'recycle' }, { id: 'logs', label: 'Logs', glob: '**/logs/**', mode: 'recycle' }] },
  { id: 'cursor', name: 'Cursor', risk: 'low', disclaimer: 'VS Code fork。', detect: ['**/Cursor/User'], match: {}, scopes: [{ id: 'cached-data', label: 'CachedData', glob: '**/CachedData/**', mode: 'recycle' }] },
  { id: 'obs', name: 'OBS Studio', risk: 'medium', disclaimer: '清浏览器源缓存与日志。', detect: ['**/obs-studio'], match: {}, scopes: [{ id: 'browser', label: 'Browser cache', glob: '**/obs-browser/cache/**', mode: 'recycle' }] },
  { id: 'battlenet', name: 'Battle.net', risk: 'medium', disclaimer: '清缓存可解决登录/启动问题。', detect: ['**/Battle.net', '**/Blizzard Entertainment'], match: {}, scopes: [{ id: 'client-cache', label: 'Cache', glob: '**/Cache/**', mode: 'recycle' }] },
  { id: 'epicgames', name: 'Epic Games', risk: 'low', disclaimer: '清启动器 webcache。', detect: ['**/EpicGamesLauncher/Saved'], match: {}, scopes: [{ id: 'webcache', label: 'webcache', glob: '**/webcache*/**', mode: 'recycle' }] },
  { id: 'brave', name: 'Brave', risk: 'low', disclaimer: '清浏览器缓存。', detect: ['**/Brave-Browser/User Data'], match: {}, scopes: [{ id: 'http', label: 'Cache', glob: '**/Cache/**', mode: 'recycle' }] },
  { id: 'go-mod', name: 'Go module cache', risk: 'low', disclaimer: '下次 build 重下。', detect: ['**/go/pkg/mod'], match: {}, scopes: [{ id: 'mod', label: 'pkg/mod', glob: '**/pkg/mod/**', mode: 'recycle' }] },
  { id: 'gradle', name: 'Gradle cache', risk: 'low', disclaimer: '下次 build 重下。', detect: ['**/.gradle/caches'], match: {}, scopes: [{ id: 'modules', label: 'modules-2', glob: '**/modules-*/**', mode: 'recycle' }] },
  { id: 'maven', name: 'Maven local', risk: 'low', disclaimer: '下次 build 重下。', detect: ['**/.m2/repository'], match: {}, scopes: [{ id: 'all', label: '所有 jar', glob: '**/repository/**', mode: 'recycle' }] },
  { id: 'nuget', name: 'NuGet packages', risk: 'low', disclaimer: '下次 restore 重下。', detect: ['**/.nuget/packages'], match: {}, scopes: [{ id: 'global', label: '全局 packages', glob: '**/packages/**', mode: 'recycle' }] },
  { id: 'conda', name: 'Conda packages', risk: 'medium', disclaimer: '等同 conda clean --all。', detect: ['**/anaconda3/pkgs', '**/miniconda3/pkgs'], match: {}, scopes: [{ id: 'tarballs', label: '下载 tarball', glob: '**/cache/**', mode: 'recycle' }] },
  { id: 'ollama', name: 'Ollama 模型库', risk: 'high', disclaimer: '已下载的模型权重，删了重下很费流量。', detect: ['**/.ollama/models'], match: {}, scopes: [{ id: 'blobs', label: 'blobs', glob: '**/blobs/**', mode: 'recycle' }] },
  { id: 'windows-temp', name: 'Windows 临时文件', risk: 'low', disclaimer: '%TEMP% 临时。', detect: ['**/AppData/Local/Temp'], match: {}, scopes: [{ id: 'old', label: '30 天前', glob: '**/Temp/**', mode: 'recycle', prompt: { kind: 'days', default: 30 } }] },
  { id: 'crash-dumps', name: '崩溃转储', risk: 'low', disclaimer: '.dmp 文件，普通用户可全清。', detect: ['**/CrashDumps'], match: {}, scopes: [{ id: 'all', label: '全部', glob: '**/*', mode: 'recycle' }] },
  { id: 'windows-old', name: 'Windows.old', risk: 'high', disclaimer: '升级残留，10 天后系统会自动清。', detect: ['**/Windows.old'], match: {}, scopes: [{ id: 'all', label: '整个目录', glob: '**/*', mode: 'delete' }] },
  { id: 'recycle-bin', name: '回收站', risk: 'medium', disclaimer: '清空后不可恢复（除非数据恢复软件）。', detect: ['**/$Recycle.Bin'], match: {}, scopes: [{ id: 'empty', label: '清空', glob: '**/*', mode: 'delete' }] },
  { id: 'node-modules', name: 'node_modules（项目级）', risk: 'high', disclaimer: '只清不再开发的项目。', detect: ['**/node_modules'], match: {}, scopes: [{ id: 'stale', label: '180 天没改的', glob: '**/*', mode: 'recycle', prompt: { kind: 'days', default: 180 } }] },
  {
    id: 'edge', name: 'Microsoft Edge', risk: 'low',
    disclaimer: '只清浏览器缓存，书签、密码、历史记录保留。可能要重新登录某些网站。',
    detect: ['**/Microsoft/Edge/User Data'], match: {},
    scopes: [
      { id: 'http-cache', label: 'HTTP 缓存', glob: '**/Cache/**', mode: 'recycle' },
      { id: 'code-cache', label: 'JS 字节码缓存', glob: '**/Code Cache/**', mode: 'recycle' },
      { id: 'gpu-cache',  label: 'GPU 着色器缓存', glob: '**/GPUCache/**', mode: 'recycle' },
    ],
  },
  {
    id: 'chrome', name: 'Google Chrome', risk: 'low',
    disclaimer: '只清浏览器缓存。',
    detect: ['**/Google/Chrome/User Data'], match: {},
    scopes: [
      { id: 'http-cache', label: 'HTTP 缓存', glob: '**/Cache/**', mode: 'recycle' },
      { id: 'code-cache', label: 'JS 字节码缓存', glob: '**/Code Cache/**', mode: 'recycle' },
    ],
  },
  {
    id: 'npm', name: 'npm cache', risk: 'low',
    disclaimer: 'npm 下次安装会重新下载。',
    detect: ['**/npm-cache'], match: {},
    scopes: [{ id: 'all', label: '所有缓存包', glob: '**/*', mode: 'recycle' }],
  },
  {
    id: 'pnpm', name: 'pnpm store', risk: 'low',
    disclaimer: 'pnpm 下次安装会重新建仓。',
    detect: ['**/pnpm/store'], match: {},
    scopes: [{ id: 'store', label: 'CAS 存储', glob: '**/store/**', mode: 'recycle' }],
  },
  {
    id: 'pip', name: 'pip cache', risk: 'low',
    disclaimer: '等同于 pip cache purge。',
    detect: ['**/pip/cache'], match: {},
    scopes: [
      { id: 'http', label: 'HTTP 缓存', glob: '**/http/**', mode: 'recycle' },
      { id: 'wheels', label: '已构建的 wheel', glob: '**/wheels/**', mode: 'recycle' },
    ],
  },
  {
    id: 'jetbrains', name: 'JetBrains IDEs', risk: 'low',
    disclaimer: '清 caches/logs/system，配置保留。下次启动会重建索引。',
    detect: ['**/JetBrains'], match: {},
    scopes: [
      { id: 'caches', label: 'Caches', glob: '**/caches/**', mode: 'recycle' },
      { id: 'logs',   label: 'Logs',   glob: '**/log/**',    mode: 'recycle' },
    ],
  },
  {
    id: 'docker', name: 'Docker / Docker Desktop', risk: 'high',
    disclaimer: '建议先用 docker system prune，不要直接删 vhdx。',
    detect: ['**/Docker'], match: {},
    scopes: [{ id: 'buildx', label: 'buildx 缓存', glob: '**/buildx/**', mode: 'recycle' }],
  },
  {
    id: 'cargo', name: 'Cargo cache', risk: 'medium',
    disclaimer: '不动 ~/.cargo/bin。Cargo 下次构建会重下载。',
    detect: ['**/.cargo'], match: {},
    scopes: [
      { id: 'registry-cache', label: 'registry/cache', glob: '**/registry/cache/**', mode: 'recycle' },
      { id: 'registry-src',   label: 'registry/src',   glob: '**/registry/src/**',   mode: 'recycle' },
    ],
  },
  {
    id: 'huggingface', name: 'Hugging Face Hub cache', risk: 'medium',
    disclaimer: '是已下载的模型权重，删了重下可能很费流量。',
    detect: ['**/huggingface'], match: {},
    scopes: [{ id: 'hub', label: '模型/数据集缓存', glob: '**/hub/**', mode: 'recycle' }],
  },
  {
    id: 'steam', name: 'Steam', risk: 'medium',
    disclaimer: '只清下载/着色器/创意工坊临时文件，游戏和存档不动。',
    detect: ['**/Steam'], match: {},
    scopes: [
      { id: 'shadercache', label: 'Shader cache', glob: '**/shadercache/**', mode: 'recycle' },
      { id: 'downloading', label: '下载中临时', glob: '**/downloading/**', mode: 'recycle' },
    ],
  },
];

export async function scan(_path: string): Promise<Node> {
  await wait(800);
  return JSON.parse(JSON.stringify(MOCK_TREE));
}

export async function detectScaffold(path: string): Promise<string | null> {
  const p = path.toLowerCase();
  for (const s of SCAFFOLDS) {
    for (const d of s.detect) {
      const frag = d.replace(/\*\*?\//g, '').replace(/\\/g, '/').toLowerCase();
      if (p.replace(/\\/g, '/').includes(frag)) return s.id;
    }
  }
  return null;
}

export async function inspect(path: string, n: number): Promise<string[]> {
  await wait(150);
  const lower = path.toLowerCase();
  if (lower.includes('edge')) {
    return [
      `${path}\\Default\\Cache\\Cache_Data\\f_001234`,
      `${path}\\Default\\Cache\\Cache_Data\\f_005a2b`,
      `${path}\\Default\\Code Cache\\js\\index-DHc9.bin`,
      `${path}\\Default\\GPUCache\\data_3`,
      `${path}\\Default\\Service Worker\\CacheStorage\\...`,
    ].slice(0, n);
  }
  if (lower.includes('huggingface')) {
    return [
      `${path}\\hub\\models--meta-llama--Llama-3.1-8B-Instruct\\blobs\\...`,
      `${path}\\hub\\models--openai--clip-vit-base-patch32\\snapshots\\...`,
    ].slice(0, n);
  }
  return Array.from({ length: Math.min(n, 12) }, (_, i) => `${path}\\sample_${i + 1}.bin`);
}

export async function advise(req: AdvisorRequest): Promise<AdvisorResponse> {
  // Try the real API first if the user has configured one in Settings.
  const settings = loadSettings();
  if (isConfigured(settings)) {
    try {
      return await callAdvisor(settings, req);
    } catch (e) {
      console.warn('[pinkbin] real advisor failed, falling back to canned response:', e);
      // fall through to canned mock
    }
  }
  return cannedAdvice(req);
}

async function cannedAdvice(req: AdvisorRequest): Promise<AdvisorResponse> {
  await wait(900);
  const p = req.path.toLowerCase();
  if (p.includes('windows\\winsxs')) {
    return { what: 'Windows 组件存储（WinSxS）', category: 'system', safe_to_delete: false, risk: 'high', action: 'keep', reasoning: '系统更新需要的硬链接库；不要直接删，用 dism 清理。', needs_inspection: false };
  }
  if (p.includes('windows')) {
    return { what: 'Windows 系统目录', category: 'system', safe_to_delete: false, risk: 'high', action: 'keep', reasoning: '系统核心，绝对不要删。', needs_inspection: false };
  }
  if (p.includes('program files')) {
    return { what: '已安装应用程序', category: 'app_cache', safe_to_delete: false, risk: 'high', action: 'keep', reasoning: '应该用控制面板卸载，不要直接删整个文件夹。', needs_inspection: false };
  }
  if (p.includes('recovery')) {
    return { what: 'Windows 恢复分区数据', category: 'system', safe_to_delete: false, risk: 'high', action: 'keep', reasoning: '出问题时用来重置系统的，留着。', needs_inspection: false };
  }
  if (p.includes('eastmoney')) {
    return { what: '东方财富 Choice 数据', category: 'app_cache', safe_to_delete: true, risk: 'medium', action: 'recycle', reasoning: '看起来是金融客户端的本地数据库；如果你不再用 Choice 终端可清，否则保留。', needs_inspection: true };
  }
  if (p.includes('locallow')) {
    return { what: '低权限 App 数据', category: 'app_cache', safe_to_delete: true, risk: 'low', action: 'recycle', reasoning: '通常是 Edge/IE 沙盒数据，可清。', needs_inspection: false };
  }
  return { what: '（演示数据）未配置 AI Key 时使用预设回答', category: 'unknown', safe_to_delete: false, risk: 'medium', action: 'keep', reasoning: '在右上角设置里填 OpenAI / Anthropic / Ollama 的 key 后，就能拿到真正的 AI 判断。', needs_inspection: true };
}

export async function execute(plan: Plan, _dryRun: boolean): Promise<UndoEntry[]> {
  await wait(400);
  return plan.paths.map((src) => ({
    timestamp: new Date().toISOString(),
    action: plan.action,
    source: src,
    destination: plan.action === 'quarantine' ? `${src} → quarantine` : null,
    reason: plan.reason,
  }));
}

function wait(ms: number) { return new Promise<void>((r) => setTimeout(r, ms)); }

export async function scopeSizes(
  _scaffoldId: string,
  _rootPath: string,
): Promise<{ scope_id: string; bytes: number; file_count: number; total_bytes: number; total_files: number }[]> {
  await wait(50);
  return [];
}

const NOW = Math.floor(Date.now() / 1000);
const DAY = 86400;

export const STEAM_INVENTORY: SteamInventory = {
  steam_root: 'C:/Program Files (x86)/Steam',
  candidates_checked: [
    'C:/Program Files (x86)/Steam',
    'C:/Program Files/Steam',
  ],
  libraries: [
    {
      root: 'C:/Program Files (x86)/Steam',
      total_size_bytes: 80 * GB,
      games: [
        {
          appid: 730,
          name_en: 'Counter-Strike 2',
          name_cn: null,
          install_dir_name: 'Counter-Strike Global Offensive',
          install_path: 'C:/Program Files (x86)/Steam/steamapps/common/Counter-Strike Global Offensive',
          appmanifest_path: 'C:/Program Files (x86)/Steam/steamapps/appmanifest_730.acf',
          size_bytes: 35 * GB,
          last_played_ts: NOW - DAY,
          library_root: 'C:/Program Files (x86)/Steam',
          state_flags: 4,
          is_fully_installed: true,
          is_ghost: false,
          default_recommended: false,
          recommendation_reason: null,
          workshop_item_count: 7,
        },
        {
          appid: 440,
          name_en: 'Team Fortress 2',
          name_cn: null,
          install_dir_name: 'Team Fortress 2',
          install_path: 'C:/Program Files (x86)/Steam/steamapps/common/Team Fortress 2',
          appmanifest_path: 'C:/Program Files (x86)/Steam/steamapps/appmanifest_440.acf',
          size_bytes: 45 * GB,
          last_played_ts: NOW - 270 * DAY,
          library_root: 'C:/Program Files (x86)/Steam',
          state_flags: 4,
          is_fully_installed: true,
          is_ghost: false,
          default_recommended: true,
          recommendation_reason: '45GB · 9 个月未启动',
          workshop_item_count: 0,
        },
      ],
    },
    {
      root: 'D:/SteamLibrary',
      total_size_bytes: 192 * GB,
      games: [
        {
          appid: 1091500,
          name_en: 'Cyberpunk 2077',
          name_cn: null,
          install_dir_name: 'Cyberpunk 2077',
          install_path: 'D:/SteamLibrary/steamapps/common/Cyberpunk 2077',
          appmanifest_path: 'D:/SteamLibrary/steamapps/appmanifest_1091500.acf',
          size_bytes: 72 * GB,
          last_played_ts: null,
          library_root: 'D:/SteamLibrary',
          state_flags: 4,
          is_fully_installed: true,
          is_ghost: false,
          default_recommended: true,
          recommendation_reason: '72GB · 从未启动',
          workshop_item_count: 0,
        },
        {
          appid: 1174180,
          name_en: 'Red Dead Redemption 2',
          name_cn: null,
          install_dir_name: 'Red Dead Redemption 2',
          install_path: 'D:/SteamLibrary/steamapps/common/Red Dead Redemption 2',
          appmanifest_path: 'D:/SteamLibrary/steamapps/appmanifest_1174180.acf',
          size_bytes: 119 * GB,
          last_played_ts: NOW - 540 * DAY,
          library_root: 'D:/SteamLibrary',
          state_flags: 4,
          is_fully_installed: true,
          is_ghost: false,
          default_recommended: true,
          recommendation_reason: '119GB · 1 年未启动',
          workshop_item_count: 0,
        },
        {
          appid: 999,
          name_en: 'Forgotten Game',
          name_cn: null,
          install_dir_name: 'Forgotten Game',
          install_path: 'D:/SteamLibrary/steamapps/common/Forgotten Game',
          appmanifest_path: 'D:/SteamLibrary/steamapps/appmanifest_999.acf',
          size_bytes: 1 * GB,
          last_played_ts: null,
          library_root: 'D:/SteamLibrary',
          state_flags: 4,
          is_fully_installed: false,
          is_ghost: true,
          default_recommended: true,
          recommendation_reason: 'ACF 存在但安装目录缺失',
          workshop_item_count: 0,
        },
      ],
    },
  ],
};

export function steamWorkshopItems(appid: number): WorkshopItem[] {
  if (appid !== 730) return [];
  return [
    {
      id: 2185699891,
      size_bytes: 145 * 1024 * 1024,
      last_modified_ts: NOW - 5 * DAY,
      path: `C:/Program Files (x86)/Steam/steamapps/workshop/content/${appid}/2185699891`,
    },
    {
      id: 3010055,
      size_bytes: 320 * 1024 * 1024,
      last_modified_ts: NOW - 90 * DAY,
      path: `C:/Program Files (x86)/Steam/steamapps/workshop/content/${appid}/3010055`,
    },
    {
      id: 3010099,
      size_bytes: 78 * 1024 * 1024,
      last_modified_ts: NOW - 400 * DAY,
      path: `C:/Program Files (x86)/Steam/steamapps/workshop/content/${appid}/3010099`,
    },
  ];
}

export function workshopTitles(ids: number[]): Record<number, string> {
  const knownTitles: Record<number, string> = {
    2185699891: 'CSGOHUB Skills Training Map by csstats.gg',
    3010055: 'Aim Practice Pack',
    // 3010099 intentionally omitted — simulates a deleted/private item
    // that Steam's API skips, so the UI shows the ID-only fallback path.
  };
  const out: Record<number, string> = {};
  for (const id of ids) {
    if (knownTitles[id]) out[id] = knownTitles[id];
  }
  return out;
}
