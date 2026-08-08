import { buildLocalCandidates, buildScanContext } from './scanContext';
import type { CleanupCandidate, Node, Risk, Scaffold } from './types';

export type CleanerIconKey = 'browser' | 'dev' | 'chat' | 'game' | 'container' | 'media' | 'system' | 'unknown';
export type CleanerItemState = 'ready' | 'review' | 'view-only';
export type CleanerEvidenceSource = 'scanned' | 'scaffold';

export interface CleanerReadItem {
  id: string;
  title: string;
  subtitle: string;
  group: string;
  groupLabel: string;
  risk: Risk;
  label: string;
  size: number;
  files: string;
  iconKey: CleanerIconKey;
  tone: string;
  description: string;
  consequence: string;
  paths: string[];
  defaultSelected: boolean;
  state: CleanerItemState;
  evidence: string[];
  audited: boolean;
  planEligible: boolean;
  source: CleanerEvidenceSource;
}

export interface CleanerReadModel {
  rootPath: string;
  totalBytes: number;
  totalFiles: number;
  items: CleanerReadItem[];
}

type CleanerMeta = {
  title?: string;
  iconKey: CleanerIconKey;
  tone: string;
  group: string;
  groupLabel: string;
};

const SCAFFOLD_META: Record<string, CleanerMeta> = {
  edge: { title: '浏览器缓存', iconKey: 'browser', tone: 'pink', group: 'system', groupLabel: '系统与应用' },
  chrome: { title: '浏览器缓存', iconKey: 'browser', tone: 'pink', group: 'system', groupLabel: '系统与应用' },
  brave: { title: '浏览器缓存', iconKey: 'browser', tone: 'pink', group: 'system', groupLabel: '系统与应用' },
  firefox: { title: '浏览器缓存', iconKey: 'browser', tone: 'pink', group: 'system', groupLabel: '系统与应用' },
  npm: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  pnpm: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  pip: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  cargo: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  conda: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  'go-mod': { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  gradle: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  maven: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  nuget: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  jetbrains: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  vscode: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  cursor: { title: '开发环境缓存', iconKey: 'dev', tone: 'violet', group: 'dev', groupLabel: '开发环境' },
  'wechat-pc': { title: '微信媒体缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  'qq-pc': { title: '聊天媒体缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  dingtalk: { title: '聊天媒体缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  feishu: { title: '聊天媒体缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  slack: { title: '应用缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  discord: { title: '应用缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  telegram: { title: '聊天媒体缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  teams: { title: '应用缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  spotify: { title: '应用缓存', iconKey: 'chat', tone: 'mint', group: 'app', groupLabel: '应用数据' },
  steam: { title: 'Steam 缓存', iconKey: 'game', tone: 'orange', group: 'game', groupLabel: '游戏' },
  epicgames: { title: '游戏缓存', iconKey: 'game', tone: 'orange', group: 'game', groupLabel: '游戏' },
  battlenet: { title: '游戏缓存', iconKey: 'game', tone: 'orange', group: 'game', groupLabel: '游戏' },
  docker: { title: 'Docker 数据', iconKey: 'container', tone: 'blue', group: 'dev', groupLabel: '开发环境' },
  ollama: { title: 'AI 模型缓存', iconKey: 'container', tone: 'blue', group: 'dev', groupLabel: '开发环境' },
  huggingface: { title: 'AI 模型缓存', iconKey: 'container', tone: 'blue', group: 'dev', groupLabel: '开发环境' },
  'windows-temp': { title: 'Windows 临时文件', iconKey: 'system', tone: 'pink', group: 'system', groupLabel: '系统与应用' },
  'crash-dumps': { title: '崩溃转储', iconKey: 'system', tone: 'pink', group: 'system', groupLabel: '系统与应用' },
};

const USER_CONTENT_PATTERN = /\/users\/[^/]+\/(desktop|documents|downloads|pictures|music|videos)(\/|$)/i;
const USER_PROFILE_PATTERN = /\/users\/[^/]+\/?$/i;
const USERS_ROOT_PATTERN = /\/users\/?$/i;

function pathKey(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+/g, '/').replace(/\/$/, '').toLowerCase();
}

function compactPath(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/').filter(Boolean);
  return parts.length > 3 ? `…/${parts.slice(-3).join('/')}` : path;
}

function filesLabel(fileCount: number): string {
  return fileCount > 0 ? `${fileCount.toLocaleString()} 个文件` : '文件数量待确认';
}

function riskLabel(risk: Risk): string {
  return risk === 'low' ? '可直接清理' : risk === 'medium' ? '需要确认' : '仅建议查看';
}

function metadataFor(candidate: CleanupCandidate): CleanerMeta {
  if (candidate.scaffold_id && SCAFFOLD_META[candidate.scaffold_id]) return SCAFFOLD_META[candidate.scaffold_id];
  const normalized = pathKey(candidate.path);
  if (USER_CONTENT_PATTERN.test(normalized)) {
    return { title: '用户内容', iconKey: 'media', tone: 'sun', group: 'media', groupLabel: '用户内容' };
  }
  if (normalized.includes('/windows/') || normalized.includes('/program files') || normalized.includes('/programdata/')) {
    return { title: candidate.name, iconKey: 'system', tone: 'blue', group: 'system', groupLabel: '系统与应用' };
  }
  return { iconKey: 'unknown', tone: 'blue', group: 'other', groupLabel: '其他' };
}

function effectiveRisk(candidate: CleanupCandidate): Risk {
  const normalized = pathKey(candidate.path);
  if (!candidate.audited_scaffold && (USER_CONTENT_PATTERN.test(normalized) || USER_PROFILE_PATTERN.test(normalized) || USERS_ROOT_PATTERN.test(normalized))) return 'high';
  if (candidate.status === 'inspect' && candidate.risk === 'low') return 'medium';
  return candidate.risk;
}

function stateFor(candidate: CleanupCandidate, risk: Risk): CleanerItemState {
  if (risk === 'high' || candidate.status === 'keep') return 'view-only';
  if (candidate.status === 'preview' && risk === 'low') return 'ready';
  return 'review';
}

function consequenceFor(candidate: CleanupCandidate, risk: Risk): string {
  if (risk === 'high') return '可能包含个人或系统状态；Pinkbin 不会把它加入默认清理计划。';
  if (risk === 'low') return '应用会在需要时重建，可能导致下次启动或安装时重新生成。';
  return '可能影响应用状态或触发重新下载，执行前需要人工确认。';
}

function toReadItem(candidate: CleanupCandidate): CleanerReadItem {
  const risk = effectiveRisk(candidate);
  const state = stateFor(candidate, risk);
  const meta = metadataFor(candidate);
  const planEligible = risk !== 'high' && (candidate.audited_scaffold || state === 'ready');
  return {
    id: `scan:${pathKey(candidate.path)}`,
    title: meta.title ?? candidate.name,
    subtitle: `${candidate.name} · ${compactPath(candidate.path)}`,
    group: meta.group,
    groupLabel: meta.groupLabel,
    risk,
    label: riskLabel(risk),
    size: candidate.size_bytes,
    files: filesLabel(candidate.file_count),
    iconKey: meta.iconKey,
    tone: meta.tone,
    description: candidate.reason,
    consequence: consequenceFor(candidate, risk),
    paths: [candidate.path],
    defaultSelected: state === 'ready',
    state,
    evidence: candidate.evidence,
    audited: candidate.audited_scaffold,
    planEligible,
    source: candidate.source,
  };
}

export function buildCleanerReadModel(root: Node, scaffolds: Scaffold[], scanId = Date.now()): CleanerReadModel {
  const context = buildScanContext(root, scaffolds, scanId);
  const candidates = [
    ...buildLocalCandidates(context, 'list-cleanable'),
    ...buildLocalCandidates(context, 'protected'),
  ];
  const unique = new Map<string, CleanupCandidate>();
  for (const candidate of candidates) unique.set(pathKey(candidate.path), candidate);

  return {
    rootPath: root.path,
    totalBytes: root.size,
    totalFiles: root.file_count,
    items: Array.from(unique.values())
      .sort((a, b) => b.size_bytes - a.size_bytes)
      .slice(0, 24)
      .map(toReadItem),
  };
}
