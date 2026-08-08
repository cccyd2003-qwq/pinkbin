import { useEffect, useMemo, useRef, useState } from 'react';
import {
  Archive,
  ArrowRight,
  AppWindow,
  Bot,
  Boxes,
  Check,
  CheckCircle2,
  ChevronRight,
  CircleHelp,
  Clock3,
  Cpu,
  FileSearch,
  Film,
  FolderOpen,
  Gamepad2,
  Globe,
  HardDrive,
  History,
  LayoutDashboard,
  LockKeyhole,
  MessageCircle,
  PackageOpen,
  PanelLeft,
  RefreshCw,
  Search,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  TriangleAlert,
  X,
  Zap,
  type LucideIcon,
} from 'lucide-react';
import { api } from '../api';
import { buildCleanerReadModel, type CleanerEvidenceSource, type CleanerIconKey, type CleanerReadItem } from '../cleanerReadModel';
import { isTauri } from '../env';
import type { Plan, UndoEntry } from '../types';
import './CleanerDashboardPrototype.css';

// PROTOTYPE: A composed cleaner workspace. A is the home surface, B is space
// analysis, and C is the advanced cleanup/review workbench.

type Surface = 'home' | 'space' | 'packs' | 'history' | 'settings';
type Risk = 'low' | 'medium' | 'high';
type CleanupStatus = '等待中' | '执行中' | '已完成' | '已跳过' | '失败';
type ExecutionState = 'idle' | 'running' | 'success' | 'partial-failure' | 'failure';
type ExecutionRunStatus = Exclude<ExecutionState, 'idle'>;

type ExecutionRun = {
  id: string;
  startedAt: string;
  finishedAt?: string;
  status: ExecutionRunStatus;
  plan: Plan;
  itemTitles: string[];
  entries: UndoEntry[];
  failedPaths: string[];
  error?: string;
};

type CleanupItem = {
  id: string;
  title: string;
  subtitle: string;
  group: string;
  groupLabel: string;
  risk: Risk;
  label: string;
  size: number;
  files: string;
  icon: LucideIcon;
  tone: string;
  description: string;
  consequence: string;
  paths: string[];
  evidence: string[];
  audited: boolean;
  planEligible: boolean;
  source: CleanerEvidenceSource;
  defaultSelected: boolean;
  status: CleanupStatus;
};

const GB = 1024 ** 3;
const DEFAULT_SCAN_ROOT = 'C:\\';
const DEMO_TOTAL_BYTES = 187.1 * GB;
const CLEANER_SCAN_ROOT_STORAGE_KEY = 'pinkbin.cleaner.scan-root';

function normalizeScanRoot(value: string): string {
  const normalized = value.trim().replace(/\//g, '\\');
  if (!normalized) return DEFAULT_SCAN_ROOT;
  if (/^[A-Za-z]:\\*$/.test(normalized)) return `${normalized.slice(0, 2)}\\`;
  return normalized.replace(/\\+$/, '');
}

function isWindowsScanRoot(value: string): boolean {
  return /^(?:[A-Za-z]:\\|\\\\)/.test(value);
}

function loadCleanerScanRoot(): string {
  if (typeof window === 'undefined') return DEFAULT_SCAN_ROOT;
  try {
    return normalizeScanRoot(window.localStorage.getItem(CLEANER_SCAN_ROOT_STORAGE_KEY) ?? DEFAULT_SCAN_ROOT);
  } catch {
    return DEFAULT_SCAN_ROOT;
  }
}

function persistCleanerScanRoot(path: string): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(CLEANER_SCAN_ROOT_STORAGE_KEY, path);
  } catch {
    // Local persistence is optional in the prototype shell.
  }
}

const CLEANUP_ITEMS: CleanupItem[] = [
  {
    id: 'browser-cache',
    title: '浏览器缓存',
    subtitle: 'Chrome · Edge · 3 个位置',
    group: 'system',
    groupLabel: '系统与应用',
    risk: 'low',
    label: '可直接清理',
    size: 18.4 * GB,
    files: '82,401 个文件',
    icon: Globe,
    tone: 'pink',
    description: 'HTTP、GPU 和代码缓存，浏览器会自动重建。',
    consequence: '可能需要重新加载部分网页资源，但不会退出账号。',
    paths: ['C:\\Users\\90740\\AppData\\Local\\Google\\Chrome\\User Data\\Cache', 'C:\\Users\\90740\\AppData\\Local\\Microsoft\\Edge\\User Data\\Cache'],
    evidence: ['命中 Chrome / Edge Cache 目录', '内容可由浏览器自动重建'],
    audited: true,
    planEligible: true,
    source: 'scaffold',
    defaultSelected: true,
    status: '等待中',
  },
  {
    id: 'dev-cache',
    title: '开发环境缓存',
    subtitle: 'npm · pnpm · pip · Cargo',
    group: 'dev',
    groupLabel: '开发环境',
    risk: 'low',
    label: '可直接清理',
    size: 9.6 * GB,
    files: '41,206 个文件',
    icon: Cpu,
    tone: 'violet',
    description: '包管理器下载缓存，下一次安装时可以重新获取。',
    consequence: '下次构建或安装可能重新下载依赖。',
    paths: ['C:\\Users\\90740\\AppData\\Local\\npm-cache', 'C:\\Users\\90740\\AppData\\Local\\pnpm\\store', 'C:\\Users\\90740\\AppData\\Local\\pip\\cache'],
    evidence: ['命中 npm / pnpm / pip / Cargo 下载缓存', '内容可在下一次安装时重新获取'],
    audited: true,
    planEligible: true,
    source: 'scaffold',
    defaultSelected: true,
    status: '等待中',
  },
  {
    id: 'wechat-media',
    title: '微信媒体缓存',
    subtitle: '图片 · 视频 · 接收文件',
    group: 'app',
    groupLabel: '应用数据',
    risk: 'medium',
    label: '需要确认',
    size: 7.8 * GB,
    files: '15,820 个文件',
    icon: MessageCircle,
    tone: 'mint',
    description: '只匹配 FileStorage 中超过 30 天的媒体缓存。',
    consequence: '旧聊天里的部分媒体可能需要重新下载；聊天数据库保留。',
    paths: ['C:\\Users\\90740\\Documents\\WeChat Files\\wxid_gmsp9xjx12\\FileStorage\\Image', 'C:\\Users\\90740\\Documents\\WeChat Files\\wxid_gmsp9xjx12\\FileStorage\\Video'],
    evidence: ['命中 FileStorage/Image、Video', '只处理超过 30 天的媒体缓存'],
    audited: true,
    planEligible: true,
    source: 'scaffold',
    defaultSelected: false,
    status: '等待中',
  },
  {
    id: 'steam-shader',
    title: 'Steam Shader 缓存',
    subtitle: 'Steam · 4 个游戏',
    group: 'game',
    groupLabel: '游戏',
    risk: 'medium',
    label: '需要确认',
    size: 4.3 * GB,
    files: '6,482 个文件',
    icon: Gamepad2,
    tone: 'orange',
    description: '游戏启动时生成的着色器缓存，可以重新生成。',
    consequence: '下次启动游戏可能需要重新编译着色器。',
    paths: ['D:\\SteamLibrary\\steamapps\\shadercache\\730', 'D:\\SteamLibrary\\steamapps\\shadercache\\570'],
    evidence: ['命中 Steam shadercache 目录', '由 Steam 启动时重新生成'],
    audited: true,
    planEligible: true,
    source: 'scaffold',
    defaultSelected: false,
    status: '已跳过',
  },
  {
    id: 'docker-buildx',
    title: 'Docker Buildx 缓存',
    subtitle: 'Docker Desktop · 需要检查',
    group: 'dev',
    groupLabel: '开发环境',
    risk: 'high',
    label: '仅建议查看',
    size: 5.2 * GB,
    files: '1,204 个对象',
    icon: Boxes,
    tone: 'blue',
    description: '构建层缓存可能被未来的镜像构建复用。',
    consequence: '建议优先使用 Docker 自己的 prune 命令，不直接删除 VHDX。',
    paths: ['C:\\Users\\90740\\AppData\\Local\\Docker\\buildx'],
    evidence: ['扫描命中 Docker Buildx 路径', '尚未接入安全清理脚本'],
    audited: false,
    planEligible: false,
    source: 'scanned',
    defaultSelected: false,
    status: '已跳过',
  },
  {
    id: 'user-media',
    title: '视频素材',
    subtitle: 'Documents · 仅建议查看',
    group: 'media',
    groupLabel: '用户内容',
    risk: 'high',
    label: '仅建议查看',
    size: 12.1 * GB,
    files: '2,840 个文件',
    icon: Film,
    tone: 'sun',
    description: '个人录屏和素材，不属于可重建内容。',
    consequence: '删除后无法由 Pinkbin 恢复，只能从你的备份找回。',
    paths: ['C:\\Users\\90740\\Documents\\素材', 'D:\\Recordings\\2026'],
    evidence: ['命中 Documents / Recordings 用户目录', '属于用户内容，不可重建'],
    audited: false,
    planEligible: false,
    source: 'scanned',
    defaultSelected: false,
    status: '已跳过',
  },
];

const NAV_ITEMS: { id: Surface; label: string; icon: LucideIcon }[] = [
  { id: 'home', label: '首页', icon: LayoutDashboard },
  { id: 'space', label: '空间分析', icon: HardDrive },
  { id: 'packs', label: '深度清理', icon: PackageOpen },
  { id: 'history', label: '历史记录', icon: History },
  { id: 'settings', label: '设置', icon: Settings2 },
];

const GROUPS: { id: string; label: string; icon: LucideIcon }[] = [
  { id: 'system', label: '系统与应用', icon: AppWindow },
  { id: 'dev', label: '开发环境', icon: Cpu },
  { id: 'app', label: '应用数据', icon: MessageCircle },
  { id: 'game', label: '游戏', icon: Gamepad2 },
  { id: 'media', label: '用户内容', icon: Film },
];

const ICON_BY_KEY: Record<CleanerIconKey, LucideIcon> = {
  browser: Globe,
  dev: Cpu,
  chat: MessageCircle,
  game: Gamepad2,
  container: Boxes,
  media: Film,
  system: HardDrive,
  unknown: FolderOpen,
};

function cleanupItemsFromReadModel(readItems: CleanerReadItem[]): CleanupItem[] {
  return readItems.map(({ iconKey, state, ...item }) => ({
    ...item,
    icon: ICON_BY_KEY[iconKey],
    status: state === 'view-only' ? '已跳过' : '等待中',
  }));
}

function formatSize(bytes: number): string {
  if (bytes >= GB) return `${(bytes / GB).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

function pathKey(path: string): string {
  return path.replace(/[\\/]+/g, '/').replace(/\/$/, '').toLowerCase();
}

function executionStatusClass(status: CleanupStatus): string {
  if (status === '执行中') return 'running';
  if (status === '已完成') return 'completed';
  if (status === '已跳过') return 'skipped';
  if (status === '失败') return 'failed';
  return 'waiting';
}

function executionRunLabel(status: ExecutionRunStatus): string {
  if (status === 'running') return '执行中';
  if (status === 'success') return '已完成';
  if (status === 'partial-failure') return '部分失败';
  return '失败';
}

function executionRunClass(status: ExecutionRunStatus): string {
  if (status === 'running') return 'running';
  if (status === 'success') return 'completed';
  return 'failed';
}

function formatRunTime(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return '时间未知';
  return date.toLocaleString('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

function buildCleanupPlan(items: CleanupItem[]): Plan {
  return {
    action: 'quarantine',
    paths: [...new Set(items.flatMap((item) => item.paths))],
    reason: `Pinkbin 深度清理：${items.map((item) => item.title).join('、')}`,
  };
}

function mockUndoEntries(plan: Plan): UndoEntry[] {
  return plan.paths.map((source) => ({
    timestamp: new Date().toISOString(),
    action: plan.action,
    source,
    destination: `${source} → 隔离区`,
    reason: `原型执行：${plan.reason}`,
  }));
}

async function executePrototypePlan(plan: Plan): Promise<UndoEntry[]> {
  // The prototype never mutates user files. Browser mode uses the existing mock
  // executor; a Tauri shell gets the same local result until real execution is
  // explicitly authorized and wired into the product flow.
  if (isTauri) {
    await new Promise<void>((resolve) => window.setTimeout(resolve, 650));
    return mockUndoEntries(plan);
  }
  return api.execute(plan, false);
}

function riskClass(risk: Risk): string {
  return `cleaner-risk cleaner-risk-${risk}`;
}

function RiskLabel({ risk, text }: { risk: Risk; text?: string }) {
  const label = text ?? (risk === 'low' ? '可直接清理' : risk === 'medium' ? '需要确认' : '仅建议查看');
  return <span className={riskClass(risk)}>{label}</span>;
}

function visibleRiskLabel(item: Pick<CleanupItem, 'risk' | 'label' | 'planEligible'>): string {
  if (!item.planEligible && item.risk !== 'high') return '先检查';
  return item.label;
}

function planAccessLabel(item: Pick<CleanupItem, 'risk' | 'planEligible'>): string {
  if (item.planEligible) return '已满足清理计划入口条件';
  if (item.risk === 'high') return '风险较高，只能查看';
  return '尚未接入审核清理规则，只能查看';
}

function IconTile({ icon: Icon, tone }: { icon: LucideIcon; tone: string }) {
  return <span className={`cleaner-icon-tile cleaner-tone-${tone}`}><Icon size={18} strokeWidth={2.2} /></span>;
}

function SizeValue({ bytes }: { bytes: number }) {
  return <span className="cleaner-size mono-num">{formatSize(bytes)}</span>;
}

function PrototypeNav({ active, onNavigate }: { active: Surface; onNavigate: (surface: Surface) => void }) {
  return (
    <nav className="cleaner-nav" aria-label="主导航">
      {NAV_ITEMS.map(({ id, label, icon: Icon }) => (
        <button
          type="button"
          key={id}
          className={active === id ? 'is-active' : ''}
          onClick={() => onNavigate(id)}
        >
          <Icon size={16} />
          <span>{label}</span>
        </button>
      ))}
    </nav>
  );
}

function CleanerSidebar({ activeSurface, onNavigate, scanRoot, totalBytes }: { activeSurface: Surface; onNavigate: (surface: Surface) => void; scanRoot: string; totalBytes: number }) {
  return (
    <aside className="cleaner-sidebar">
      <div className="cleaner-brand"><span className="cleaner-brand-mark"><Zap size={17} fill="currentColor" /></span><strong>pinkbin</strong><small>DISK CARE</small></div>
      <div className="cleaner-sidebar-caption">你的电脑</div>
      <div className="cleaner-drive-chip"><HardDrive size={16} /><span><strong>{scanRoot}</strong><small>{formatSize(totalBytes)} 已用</small></span><span className="cleaner-drive-dot" /></div>
      <PrototypeNav active={activeSurface} onNavigate={onNavigate} />
      <div className="cleaner-sidebar-foot"><ShieldCheck size={14} /><span>本地优先<br /><small>不读取文件内容</small></span></div>
    </aside>
  );
}

function ScanStatus({ scanning, onScan, summary }: { scanning: boolean; onScan: () => void; summary: string }) {
  return (
    <div className="cleaner-scan-status">
      <span className={scanning ? 'cleaner-live-dot is-scanning' : 'cleaner-live-dot'} />
      <span>{scanning ? '正在检查已知安全目录…' : summary}</span>
      <button type="button" onClick={() => onScan()} disabled={scanning}><RefreshCw size={13} /> 重新扫描</button>
    </div>
  );
}

function getInitialSurface(): Surface {
  const params = new URLSearchParams(window.location.search);
  const requested = params.get('surface');
  if (requested === 'home' || requested === 'space' || requested === 'packs' || requested === 'history' || requested === 'settings') {
    return requested;
  }

  // Keep the comparison URLs useful while the prototype is being reviewed.
  const legacyVariant = params.get('variant');
  if (legacyVariant === 'B') return 'space';
  if (legacyVariant === 'C') return 'packs';
  return 'home';
}

function displayScanRoot(path: string): string {
  const drive = path.match(/^([A-Za-z]):[\\/]?$/);
  return drive ? `Windows (${drive[1].toUpperCase()}:)` : path;
}

function SelectionButton({ item, selected, onToggle, disabled = false }: { item: CleanupItem; selected: boolean; onToggle: () => void; disabled?: boolean }) {
  const isDisabled = disabled || item.risk === 'high' || !item.planEligible;
  const ariaLabel = !item.planEligible ? `暂不可加入清理计划 ${item.title}` : `${selected ? '取消选择' : '选择'} ${item.title}`;
  return (
    <button
      type="button"
      className={`cleaner-select-button ${selected ? 'is-selected' : ''} ${isDisabled ? 'is-disabled' : ''}`}
      onClick={isDisabled ? undefined : onToggle}
      disabled={isDisabled}
      aria-pressed={selected}
      aria-label={ariaLabel}
    >
      {selected ? <Check size={13} strokeWidth={3} /> : <span />}
    </button>
  );
}

function CompactItemRow({ item, selected, onToggle, onInspect }: { item: CleanupItem; selected: boolean; onToggle: () => void; onInspect: () => void }) {
  return (
    <div className={`cleaner-item-row ${selected ? 'is-selected' : ''}`}>
      <SelectionButton item={item} selected={selected} onToggle={onToggle} />
      <IconTile icon={item.icon} tone={item.tone} />
      <button type="button" className="cleaner-item-copy" onClick={onInspect}>
        <strong>{item.title}</strong>
        <span>{item.subtitle}</span>
      </button>
      <RiskLabel risk={item.risk} text={visibleRiskLabel(item)} />
      <SizeValue bytes={item.size} />
      <button type="button" className="cleaner-icon-button" onClick={onInspect} aria-label={`查看 ${item.title}`}><ChevronRight size={16} /></button>
    </div>
  );
}

function ReviewSheet({
  items,
  selectedIds,
  executionState,
  executionItemIds,
  executionEntries,
  executionError,
  onToggle,
  onClose,
  onConfirm,
}: {
  items: CleanupItem[];
  selectedIds: Set<string>;
  executionState: ExecutionState;
  executionItemIds: string[];
  executionEntries: UndoEntry[];
  executionError: string | null;
  onToggle: (id: string) => void;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const [safetyConfirmed, setSafetyConfirmed] = useState(false);
  const reviewIds = executionItemIds.length ? new Set(executionItemIds) : selectedIds;
  const selected = items.filter((item) => reviewIds.has(item.id));
  const total = selected.reduce((sum, item) => sum + item.size, 0);
  const planPathCount = new Set(selected.flatMap((item) => item.paths)).size;
  const completedCount = selected.filter((item) => item.status === '已完成').length;
  const failedCount = selected.filter((item) => item.status === '失败').length;
  const isRunning = executionState === 'running';
  const hasResult = executionState !== 'idle';
  const stateContent: Record<Exclude<ExecutionState, 'idle'>, { title: string; copy: string; icon: LucideIcon }> = {
    running: { title: '正在隔离清理项', copy: `正在处理 ${selected.length} 个范围，完成后会逐项更新状态。`, icon: RefreshCw },
    success: { title: '清理计划已完成', copy: `${formatSize(total)} 已进入隔离区；${executionEntries.length} 个路径已完成，恢复入口后续接入。`, icon: CheckCircle2 },
    'partial-failure': { title: '部分项目未完成', copy: `${completedCount} 个已完成，${failedCount} 个失败项仍保留在计划中。`, icon: TriangleAlert },
    failure: { title: '执行未完成', copy: executionError ? `这次没有完成任何范围：${executionError}` : '这次没有完成任何范围，失败项仍保留在计划中。', icon: TriangleAlert },
  };
  const state = hasResult ? stateContent[executionState] : null;
  const StateIcon = state?.icon;
  const actionLabel = executionState === 'partial-failure' || executionState === 'failure' ? '重试失败项' : `隔离 ${selected.length ? formatSize(total) : ''}`;
  const toggleReviewItem = (id: string) => {
    setSafetyConfirmed(false);
    onToggle(id);
  };
  return (
    <div className="cleaner-sheet-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="cleaner-review-sheet" role="dialog" aria-modal="true" aria-label="复核清理计划" aria-busy={isRunning} onMouseDown={(event) => event.stopPropagation()}>
        <div className="cleaner-sheet-head">
          <div><span className="cleaner-kicker">{hasResult ? '清理执行' : '最后一步 · 人工审核'}</span><h2>{hasResult ? state?.title : '确认这次清理'}</h2></div>
          <button type="button" className="cleaner-icon-button" onClick={onClose} aria-label="关闭复核"><X size={18} /></button>
        </div>
        <div className="cleaner-review-total"><span>{hasResult ? '本次计划' : '预计可释放'}</span><strong>{formatSize(total)}</strong><small>{hasResult ? `${completedCount} 个完成 · ${failedCount} 个失败 · ${planPathCount} 个路径` : `${selected.length} 个范围 · ${planPathCount} 个路径 · 执行方式：隔离区`}</small></div>
        {executionState === 'idle' && <label className="cleaner-review-confirm"><input type="checkbox" checked={safetyConfirmed} onChange={(event) => setSafetyConfirmed(event.target.checked)} /><span><strong>我确认只处理以上可重建范围</strong><small>本次只进入隔离区，不会永久删除；路径和清理影响已逐项查看。</small></span></label>}
        {state && StateIcon && <div className={`cleaner-execution-state cleaner-execution-${executionState}`} role="status"><StateIcon size={17} className={isRunning ? 'cleaner-spin' : undefined} /><span><strong>{state.title}</strong><small>{state.copy}</small></span>{executionState !== 'running' && <b>{completedCount}/{selected.length}</b>}</div>}
        <div className="cleaner-review-list">
          {selected.length === 0 ? <div className="cleaner-empty-review">没有选择任何项目。回到结果页勾选低风险内容，或只把这次当作查看报告。</div> : selected.map((item) => (
            <div className={`cleaner-review-row ${hasResult ? `cleaner-review-row-${executionStatusClass(item.status)}` : ''}`} key={item.id}>
              {hasResult ? <span className={`cleaner-review-status cleaner-review-status-${executionStatusClass(item.status)}`}>{item.status}</span> : <SelectionButton item={item} selected onToggle={() => toggleReviewItem(item.id)} disabled={isRunning} />}
              <div className="cleaner-review-row-copy"><strong>{item.title}</strong><span>{item.paths[0]}</span><small>{item.consequence}</small></div>
              <SizeValue bytes={item.size} />
            </div>
          ))}
        </div>
        <div className="cleaner-sheet-foot">
          <span className="cleaner-safe-note"><ShieldCheck size={15} /><span><strong>执行方式：隔离区</strong><small>不读取文件内容 · 永久删除未启用 · 恢复入口后续接入</small></span></span>
          <div className="cleaner-sheet-actions">
            <button type="button" className="cleaner-button cleaner-button-quiet" onClick={onClose}>{isRunning ? '隐藏进度' : hasResult ? '返回工作台' : '继续查看'}</button>
            {executionState === 'idle' && <button type="button" className="cleaner-button cleaner-button-primary" disabled={!selected.length || !safetyConfirmed} onClick={onConfirm}><Archive size={15} /> {actionLabel}</button>}
            {(executionState === 'partial-failure' || executionState === 'failure') && <button type="button" className="cleaner-button cleaner-button-primary" disabled={!selectedIds.size || isRunning} onClick={onConfirm}><RefreshCw size={15} /> {actionLabel}</button>}
          </div>
        </div>
      </section>
    </div>
  );
}

function SurfaceNote({ surface }: { surface: Surface }) {
  const data: Record<Surface, { eyebrow: string; title: string; copy: string; icon: LucideIcon }> = {
    home: { eyebrow: '快速清理', title: '先从能安全释放的空间开始', copy: '选择首页查看本次扫描摘要。', icon: LayoutDashboard },
    space: { eyebrow: '空间分析', title: '看懂这些空间从哪里来', copy: '完整扫描后的语义空间图和目录下钻会放在这里。', icon: HardDrive },
    packs: { eyebrow: '深度清理', title: '把复杂应用拆成可审查的范围', copy: '稳定范围默认可用，实验范围需要主动开启。', icon: PackageOpen },
    history: { eyebrow: '历史记录', title: '先记录每次清理结果', copy: '本轮先保留执行记录与隔离状态，恢复入口后续接入。', icon: History },
    settings: { eyebrow: '设置', title: '把控制权留在你手里', copy: '管理扫描根目录、排除项、AI 和深度清理规则更新。', icon: Settings2 },
  };
  const value = data[surface];
  const Icon = value.icon;
  return <div className="cleaner-surface-note"><Icon size={22} /><span className="cleaner-kicker">{value.eyebrow}</span><h2>{value.title}</h2><p>{value.copy}</p></div>;
}

type CleanerSettingsProps = {
  scanRootPath: string;
  scanSummary: string;
  scanning: boolean;
  onScan: (rootPath?: string) => void;
  onSaveScanRoot: (path: string) => void;
};

function CleanerSettings({ scanRootPath, scanSummary, scanning, onScan, onSaveScanRoot }: CleanerSettingsProps) {
  const [draftRoot, setDraftRoot] = useState(scanRootPath);
  const normalizedRoot = normalizeScanRoot(draftRoot);
  const validRoot = isWindowsScanRoot(normalizedRoot);
  const hasChanges = normalizedRoot !== scanRootPath;

  useEffect(() => {
    setDraftRoot(scanRootPath);
  }, [scanRootPath]);

  const saveAndScan = () => {
    if (!validRoot) return;
    onSaveScanRoot(normalizedRoot);
    onScan(normalizedRoot);
  };

  return (
    <section className="cleaner-settings-page">
      <div className="cleaner-settings-heading">
        <div>
          <span className="cleaner-kicker">设置 · 扫描范围</span>
          <h1>决定 Pinkbin 看哪里</h1>
          <p>只扫描目录元数据，不读取文件内容；清理前仍会经过审核和隔离确认。</p>
        </div>
        <Settings2 size={23} />
      </div>

      <section className="cleaner-settings-section">
        <div className="cleaner-settings-section-head"><div><span className="cleaner-kicker">扫描根目录</span><h2>从哪个 Windows 位置开始</h2></div><HardDrive size={17} /></div>
        <label className="cleaner-settings-field">
          <span>路径</span>
          <input value={draftRoot} onChange={(event) => setDraftRoot(event.target.value)} placeholder="C:\\ 或 D:\\Projects" spellCheck={false} />
        </label>
        <p className="cleaner-settings-help">支持盘符根目录或其下的文件夹，例如 <code>C:\\</code>、<code>D:\\Projects</code>。</p>
        {!validRoot && draftRoot.trim() && <p className="cleaner-settings-error">请输入 Windows 路径，例如 C:\\ 或 D:\\Projects。</p>}
        <div className="cleaner-settings-actions">
          <button type="button" className="cleaner-button cleaner-button-quiet" onClick={() => onSaveScanRoot(normalizedRoot)} disabled={!validRoot || !hasChanges}>保存范围</button>
          <button type="button" className="cleaner-button cleaner-button-primary" onClick={saveAndScan} disabled={!validRoot || scanning}><RefreshCw size={14} />{scanning ? '扫描中…' : '保存并重新扫描'}</button>
        </div>
        <div className="cleaner-settings-last-scan"><Clock3 size={14} /><span><strong>最近一次扫描</strong><small>{scanSummary}</small></span></div>
      </section>

      <section className="cleaner-settings-section">
        <div className="cleaner-settings-section-head"><div><span className="cleaner-kicker">安全边界</span><h2>当前原型的执行规则</h2></div><ShieldCheck size={17} /></div>
        <div className="cleaner-settings-rule"><ShieldCheck size={15} /><span><strong>执行方式</strong><small>只生成隔离计划，不永久删除文件。</small></span><b>隔离区</b></div>
        <div className="cleaner-settings-rule"><FileSearch size={15} /><span><strong>扫描内容</strong><small>使用路径、大小、文件数和扩展名分布判断范围。</small></span><b>仅元数据</b></div>
        <div className="cleaner-settings-rule is-muted"><LockKeyhole size={15} /><span><strong>恢复与永久删除</strong><small>恢复、回滚和永久删除入口暂未启用。</small></span><b>未启用</b></div>
      </section>
    </section>
  );
}

function HistoryLog({ runs }: { runs: ExecutionRun[] }) {
  return (
    <section className="cleaner-history-log">
      <span className="cleaner-kicker">历史记录 · 只读</span>
      <h1>最近的清理计划</h1>
      <p>这里只记录计划和执行结果；恢复入口后续接入。</p>
      <div className="cleaner-history-list">
        {!runs.length ? <div className="cleaner-history-empty"><History size={19} /><strong>还没有执行记录</strong><span>完成一次隔离后，计划会出现在这里。</span></div> : runs.map((run) => (
          <article className={`cleaner-history-item cleaner-history-item-${executionRunClass(run.status)}`} key={run.id}>
            <div className="cleaner-history-item-head"><span className={`cleaner-history-status cleaner-history-status-${executionRunClass(run.status)}`}>{executionRunLabel(run.status)}</span><strong>隔离区</strong><small>{formatRunTime(run.startedAt)} · {run.itemTitles.length} 个范围 · {run.plan.paths.length} 个路径</small></div>
            <div className="cleaner-history-item-names">{run.itemTitles.join('、')}</div>
            <div className="cleaner-history-item-meta"><span>{run.entries.length} 个路径已返回结果</span>{run.failedPaths.length > 0 && <span className="cleaner-history-failed">{run.failedPaths.length} 个路径失败</span>}{run.error && <span className="cleaner-history-failed">{run.error}</span>}</div>
          </article>
        ))}
      </div>
    </section>
  );
}

function VariantA({ activeSurface, onNavigate, items, selectedIds, onToggle, onInspect, onReview, scanning, onScan, scanRoot, totalBytes, scanSummary, cleaned, cleanedBytes, cleanedCount, executionLog, scanRootPath, onSaveScanRoot }: VariantProps) {
  const low = items.filter((item) => item.risk === 'low');
  const medium = items.filter((item) => item.risk === 'medium');
  const high = items.filter((item) => item.risk === 'high');
  const selectedBytes = items.filter((item) => selectedIds.has(item.id)).reduce((sum, item) => sum + item.size, 0);
  const defaultSelectedBytes = items.filter((item) => item.defaultSelected).reduce((sum, item) => sum + item.size, 0);
  const completedBytes = cleanedBytes ?? defaultSelectedBytes;
  const completedCount = cleanedCount ?? items.filter((item) => item.defaultSelected).length;
  return (
    <div className="cleaner-variant cleaner-variant-a">
      <CleanerSidebar activeSurface={activeSurface} onNavigate={onNavigate} scanRoot={scanRoot} totalBytes={totalBytes} />
      <main className="cleaner-a-main">
        {activeSurface === 'home' && <ScanStatus scanning={scanning} onScan={onScan} summary={scanSummary} />}
        {activeSurface !== 'home' ? activeSurface === 'history' ? <HistoryLog runs={executionLog ?? []} /> : activeSurface === 'settings' ? <CleanerSettings scanRootPath={scanRootPath} scanSummary={scanSummary} scanning={scanning} onScan={onScan} onSaveScanRoot={onSaveScanRoot} /> : <SurfaceNote surface={activeSurface} /> : (
          <>
            <div className="cleaner-a-heading"><div><span className="cleaner-kicker">今天 · 快速清理</span><h1>你的空间，<em>看懂了。</em></h1><p>发现 {items.length} 个范围，其中 {low.length} 个可以马上释放。</p></div><button type="button" className="cleaner-icon-button cleaner-heading-settings" title="调整扫描范围" onClick={() => onNavigate('settings')}><SlidersHorizontal size={17} /></button></div>
            {cleaned && <div className="cleaner-success-banner"><CheckCircle2 size={18} /><span><strong>已完成隔离 {completedCount} 个范围</strong><small>{formatSize(completedBytes)} 已进入隔离区 · 恢复入口后续接入</small></span><button type="button" className="cleaner-link-button" onClick={() => onNavigate('packs')}>查看清理计划 <ArrowRight size={13} /></button></div>}
            <section className="cleaner-a-hero">
              <div><span className="cleaner-hero-label"><ShieldCheck size={15} /> {cleaned ? '本次已隔离' : '可安全释放'}</span><strong className="cleaner-hero-number">{formatSize(cleaned ? completedBytes : selectedBytes)}</strong><p>{cleaned ? `${completedCount} 个范围已进入隔离区` : <>来自 {selectedIds.size} 个低风险可重建范围<br /><span>预计完成后实际数字可能略有变化</span></>}<br />{cleaned && <span>恢复入口后续接入</span>}</p><div className="cleaner-hero-actions"><button type="button" className="cleaner-button cleaner-button-primary" onClick={onReview} disabled={!selectedIds.size || cleaned}><Archive size={15} /> {cleaned ? '本次已完成' : '查看并清理'}</button><button type="button" className="cleaner-button cleaner-button-quiet" onClick={() => onScan()}><RefreshCw size={14} /> 完整扫描</button></div></div>
              <div className="cleaner-hero-orbit"><div className="cleaner-hero-orbit-ring"><span>{new Set(low.map((item) => item.group)).size}</span><small>可清理<br />类别</small></div><div className="cleaner-orbit-label cleaner-orbit-top">缓存</div><div className="cleaner-orbit-label cleaner-orbit-right">开发</div><div className="cleaner-orbit-label cleaner-orbit-bottom">残留</div></div>
            </section>
            <div className="cleaner-section-heading"><div><h2>建议先处理</h2><span>这些内容可重建，不会碰你的个人文件</span></div><button type="button" className="cleaner-text-button" onClick={onReview}>查看清单 <ArrowRight size={13} /></button></div>
            <div className="cleaner-a-list">{low.map((item) => <CompactItemRow key={item.id} item={item} selected={selectedIds.has(item.id)} onToggle={() => onToggle(item.id)} onInspect={() => onInspect(item)} />)}</div>
            <div className="cleaner-section-heading cleaner-section-heading-spaced"><div><h2>需要你判断</h2><span>可能影响应用状态，默认不选择</span></div><span className="cleaner-count-note">{medium.length} 个范围</span></div>
            <div className="cleaner-a-list cleaner-a-list-muted">{medium.map((item) => <CompactItemRow key={item.id} item={item} selected={selectedIds.has(item.id)} onToggle={() => onToggle(item.id)} onInspect={() => onInspect(item)} />)}</div>
            {high.length > 0 && <div className="cleaner-a-view-only"><LockKeyhole size={15} /><span><strong>{high.length} 个内容仅建议查看</strong><small>包括系统、用户内容和高风险应用数据 · 不会进入清理计划</small></span><button type="button" className="cleaner-link-button" onClick={() => onInspect(high[0])}>查看原因 <ChevronRight size={13} /></button></div>}
          </>
        )}
      </main>
    </div>
  );
}

function VariantB({ activeSurface, onNavigate, items, selectedIds, onToggle, onInspect, onReview, scanning, onScan, scanRoot, totalBytes, scanSummary }: VariantProps) {
  const [focusId, setFocusId] = useState(items[0]?.id ?? '');
  const focused = items.find((item) => item.id === focusId) ?? items[0];
  const selectedBytes = items.filter((item) => selectedIds.has(item.id)).reduce((sum, item) => sum + item.size, 0);
  const mapLayout: { size: 'large' | 'medium' | 'small' | 'tiny'; col: string; row: string }[] = [
    { size: 'large', col: '1 / 5', row: '1 / 4' },
    { size: 'medium', col: '5 / 8', row: '1 / 3' },
    { size: 'medium', col: '8 / 11', row: '1 / 3' },
    { size: 'small', col: '5 / 8', row: '3 / 5' },
    { size: 'small', col: '8 / 11', row: '3 / 5' },
    { size: 'tiny', col: '1 / 4', row: '4 / 5' },
  ];
  const blocks = items.slice(0, mapLayout.length).map((item, index) => ({
    ...mapLayout[index],
    id: item.id,
    label: item.title,
    sub: formatSize(item.size),
  }));
  return (
    <div className="cleaner-variant cleaner-variant-b">
      <CleanerSidebar activeSurface={activeSurface} onNavigate={onNavigate} scanRoot={scanRoot} totalBytes={totalBytes} />
      <div className="cleaner-b-workspace">
        <header className="cleaner-b-topbar"><div className="cleaner-topbar-actions"><button type="button" className="cleaner-plain-button"><CircleHelp size={16} /></button><button type="button" className="cleaner-plain-button"><Settings2 size={16} /></button><button type="button" className="cleaner-button cleaner-button-primary" onClick={onReview} disabled={!selectedIds.size}><Archive size={14} /> 清理 {formatSize(selectedBytes)}</button></div></header>
        <main className="cleaner-b-main">
        <div className="cleaner-b-heading"><div><span className="cleaner-kicker">空间分析 · {scanRoot}</span><h1>{formatSize(totalBytes)} <span>已用</span></h1><p>拖动和点击空间块，查看它们的来源与清理建议。</p></div><div className="cleaner-b-scan"><div className="cleaner-b-gauge"><span>{items.length ? Math.round((items.reduce((sum, item) => sum + item.size, 0) / Math.max(totalBytes, 1)) * 100) : 0}%</span></div><span><strong>已扫描</strong><small>{items.length} 个位置 · {scanning ? '扫描中…' : scanSummary}</small></span><button type="button" className="cleaner-plain-button" onClick={() => onScan()}><RefreshCw size={15} /></button></div></div>
        {activeSurface !== 'space' && activeSurface !== 'home' ? <SurfaceNote surface={activeSurface} /> : (
          <>
            <div className="cleaner-b-map-row">{focused ? <><section className="cleaner-space-map"><div className="cleaner-map-label"><span>按占用空间</span><span>点击查看详情</span></div><div className="cleaner-map-grid">{blocks.map((block) => { const item = items.find((entry) => entry.id === block.id)!; return <button type="button" key={block.id} className={`cleaner-map-block cleaner-map-${block.size} cleaner-tone-bg-${item.tone} ${focusId === block.id ? 'is-focus' : ''}`} style={{ gridColumn: block.col, gridRow: block.row }} onClick={() => { setFocusId(block.id); onInspect(item); }}><strong>{block.label}</strong><span>{block.sub}</span><small>{item.planEligible ? selectedIds.has(item.id) ? '已选择' : '可加入计划' : item.risk === 'high' ? '仅查看' : '先检查'}</small></button>; })}</div><div className="cleaner-map-legend"><span><i className="legend-dot legend-safe" />可重建</span><span><i className="legend-dot legend-review" />需要确认</span><span><i className="legend-dot legend-view" />用户内容 / 高风险</span></div></section><aside className="cleaner-b-inspector"><span className="cleaner-kicker">当前选择</span><div className="cleaner-inspector-title"><IconTile icon={focused.icon} tone={focused.tone} /><div><h2>{focused.title}</h2><span>{focused.subtitle}</span></div></div><div className="cleaner-inspector-size"><strong>{formatSize(focused.size)}</strong><span>{focused.files}</span></div><RiskLabel risk={focused.risk} text={visibleRiskLabel(focused)} /><p>{focused.description}</p><div className="cleaner-inspector-impact"><TriangleAlert size={14} /><span>{focused.consequence}</span></div><div className="cleaner-inspector-path"><FolderOpen size={14} /><span>{focused.paths[0]}</span></div><div className="cleaner-inspector-actions">{focused.planEligible ? <button type="button" className={`cleaner-button ${selectedIds.has(focused.id) ? 'cleaner-button-quiet' : 'cleaner-button-primary'}`} onClick={() => onToggle(focused.id)}>{selectedIds.has(focused.id) ? '取消选择' : '加入清理计划'} <ArrowRight size={14} /></button> : <span className="cleaner-inspector-locked"><LockKeyhole size={14} /><span><strong>{focused.risk === 'high' ? '仅建议查看' : '先检查'}</strong><small>{planAccessLabel(focused)}</small></span></span>}<button type="button" className="cleaner-text-button" onClick={() => onInspect(focused)}>查看所有路径</button></div></aside></> : <div className="cleaner-surface-note"><HardDrive size={22} /><span className="cleaner-kicker">空间分析</span><h2>还没有可展示的扫描位置</h2><p>重新扫描后，这里会按占用空间显示目录来源。</p></div>}</div>
            <div className="cleaner-b-bottom"><div><span className="cleaner-kicker">当前清理计划</span><strong>{formatSize(selectedBytes)}</strong><span>{selectedIds.size} 个可重建范围已选择</span></div><button type="button" className="cleaner-button cleaner-button-primary" onClick={onReview} disabled={!selectedIds.size}>进入复核 <ArrowRight size={14} /></button></div>
          </>
        )}
        </main>
      </div>
    </div>
  );
}

function VariantC({ activeSurface, onNavigate, items, selectedIds, onToggle, onInspect, onReview, scanning, onScan, scanRoot, totalBytes }: VariantProps) {
  const [filter, setFilter] = useState<'all' | Risk>('all');
  const [category, setCategory] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const normalizedQuery = query.trim().toLowerCase();
  const visible = items.filter((item) => {
    const matchesRisk = filter === 'all' || item.risk === filter;
    const matchesCategory = !category || item.group === category;
    const searchable = [item.title, item.subtitle, item.groupLabel, ...item.paths].join(' ').toLowerCase();
    return matchesRisk && matchesCategory && (!normalizedQuery || searchable.includes(normalizedQuery));
  });
  const activeFilterLabels = [
    filter === 'all' ? null : filter === 'low' ? '低风险' : filter === 'medium' ? '需确认' : '仅查看',
    category ? GROUPS.find((group) => group.id === category)?.label : null,
    normalizedQuery ? `搜索“${query.trim()}”` : null,
  ].filter(Boolean).join(' · ') || '全部';
  const clearFilters = () => {
    setFilter('all');
    setCategory(null);
    setQuery('');
  };
  const hasActiveFilters = filter !== 'all' || category !== null || Boolean(normalizedQuery);
  const selectedBytes = items.filter((item) => selectedIds.has(item.id)).reduce((sum, item) => sum + item.size, 0);
  return (
    <div className="cleaner-variant cleaner-variant-c">
      <CleanerSidebar activeSurface={activeSurface} onNavigate={onNavigate} scanRoot={scanRoot} totalBytes={totalBytes} />
      <div className="cleaner-c-workspace">
        <header className="cleaner-c-header"><div className="cleaner-c-title"><PanelLeft size={17} /><div><span className="cleaner-kicker">深度清理 · 审核工作台</span><h1>清理计划</h1></div></div><div className="cleaner-c-header-meta"><span><Clock3 size={14} />扫描于刚刚</span><button type="button" className="cleaner-button cleaner-button-quiet" onClick={() => onScan()}><RefreshCw size={14} /> {scanning ? '扫描中…' : '重新扫描'}</button></div></header>
        {activeSurface !== 'packs' && activeSurface !== 'home' ? <SurfaceNote surface={activeSurface} /> : (
          <main className="cleaner-c-main">
           <aside className="cleaner-c-filter"><div className="cleaner-c-filter-top"><span className="cleaner-kicker">筛选</span><SlidersHorizontal size={15} /></div><div className="cleaner-filter-search"><Search size={14} /><input aria-label="搜索应用或路径" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索应用或路径" /></div>{hasActiveFilters && <div className="cleaner-filter-clear-row"><button type="button" className="cleaner-filter-clear" onClick={clearFilters}>清除筛选</button></div>}<div className="cleaner-c-filter-label">风险</div>{(['all', 'low', 'medium', 'high'] as const).map((value) => <button type="button" key={value} className={filter === value ? 'is-active' : ''} onClick={() => setFilter(value)} aria-pressed={filter === value}><span className={`filter-marker filter-marker-${value}`} />{value === 'all' ? '全部范围' : value === 'low' ? '可直接清理' : value === 'medium' ? '需要确认' : '仅建议查看'}<small>{value === 'all' ? items.length : items.filter((item) => item.risk === value).length}</small></button>)}<div className="cleaner-c-filter-label">类别</div>{GROUPS.map(({ id, label, icon: Icon }) => <button type="button" key={id} className={`cleaner-filter-category ${category === id ? 'is-active' : ''}`} onClick={() => setCategory((current) => current === id ? null : id)} aria-pressed={category === id}><Icon size={14} />{label}<small>{items.filter((item) => item.group === id).length}</small></button>)}<div className="cleaner-c-filter-foot"><LockKeyhole size={13} />系统保护路径已排除</div></aside>
           <section className="cleaner-c-table-section"><div className="cleaner-c-table-head"><div><span className="cleaner-kicker">{visible.length} 个范围 · {activeFilterLabels}</span><h2>逐项审核选择</h2></div><span className="cleaner-report-state"><FileSearch size={14} />扫描证据已载入</span></div><div className="cleaner-c-table"><div className="cleaner-table-header"><span>选择</span><span>范围</span><span>风险</span><span>大小</span><span>状态</span><span /></div>{visible.map((item) => <div className={`cleaner-table-row ${selectedIds.has(item.id) ? 'is-selected' : ''}`} key={item.id}><SelectionButton item={item} selected={selectedIds.has(item.id)} onToggle={() => onToggle(item.id)} /><button type="button" className="cleaner-table-item" onClick={() => onInspect(item)}><IconTile icon={item.icon} tone={item.tone} /><span><strong>{item.title}</strong><small>{item.subtitle}</small></span></button><RiskLabel risk={item.risk} text={visibleRiskLabel(item)} /><SizeValue bytes={item.size} /><span className={`cleaner-task-state cleaner-task-${executionStatusClass(item.status)}`}>{item.status}</span><button type="button" className="cleaner-icon-button" onClick={() => onInspect(item)}><ChevronRight size={15} /></button></div>)}{!visible.length && <div className="cleaner-filter-empty"><Search size={18} /><strong>没有匹配的范围</strong><span>换个关键词或清除当前筛选，查看全部扫描结果。</span><button type="button" className="cleaner-text-button" onClick={clearFilters}>清除筛选</button></div>}</div></section>
          <aside className="cleaner-c-plan"><div className="cleaner-c-plan-head"><div><span className="cleaner-kicker">待执行</span><h2>清理计划</h2></div><Archive size={19} /></div><div className="cleaner-c-plan-number">{formatSize(selectedBytes)}<small>预计可释放</small></div><div className="cleaner-c-plan-bar"><span style={{ width: `${Math.min(100, (selectedBytes / (40 * GB)) * 100)}%` }} /></div><div className="cleaner-c-plan-list">{items.filter((item) => selectedIds.has(item.id)).map((item) => <div key={item.id}><span className={`plan-status-dot cleaner-plan-dot-${executionStatusClass(item.status)}`} /><span>{item.title}</span><SizeValue bytes={item.size} /></div>)}{!selectedIds.size && <div className="cleaner-plan-empty">从左侧选择低风险范围，计划会显示在这里。</div>}</div><div className="cleaner-c-plan-note"><ShieldCheck size={14} /><span>执行方式：隔离区<br /><small>不读取文件内容 · 永久删除未启用</small></span></div><button type="button" className="cleaner-button cleaner-button-primary cleaner-button-wide" onClick={onReview} disabled={!selectedIds.size}><Archive size={15} /> 复核并隔离</button></aside>
          </main>
        )}
      </div>
    </div>
  );
}

type VariantProps = {
  activeSurface: Surface;
  onNavigate: (surface: Surface) => void;
  items: CleanupItem[];
  selectedIds: Set<string>;
  onToggle: (id: string) => void;
  onInspect: (item: CleanupItem) => void;
  onReview: () => void;
  scanning: boolean;
  onScan: (rootPath?: string) => void;
  onSaveScanRoot: (path: string) => void;
  scanRootPath: string;
  scanRoot: string;
  totalBytes: number;
  scanSummary: string;
  cleaned?: boolean;
  cleanedBytes?: number;
  cleanedCount?: number;
  executionLog?: ExecutionRun[];
};

function DetailPopover({ item, onClose, onAsk }: { item: CleanupItem; onClose: () => void; onAsk: () => void }) {
  return (
    <div className="cleaner-detail-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="cleaner-detail-popover" role="dialog" aria-modal="true" aria-label={`${item.title}详情`} onMouseDown={(event) => event.stopPropagation()}>
        <div className="cleaner-detail-head"><div><IconTile icon={item.icon} tone={item.tone} /><span><span className="cleaner-kicker">{item.groupLabel}</span><h2>{item.title}</h2></span></div><button type="button" className="cleaner-icon-button" onClick={onClose}><X size={17} /></button></div>
        <div className="cleaner-detail-meta"><RiskLabel risk={item.risk} text={visibleRiskLabel(item)} /><SizeValue bytes={item.size} /><span>{item.files}</span></div>
        <p className="cleaner-detail-description">{item.description}</p>
        <div className="cleaner-detail-impact"><TriangleAlert size={15} /><span><strong>清理后</strong>{item.consequence}</span></div>
        <div className="cleaner-detail-paths"><div className="cleaner-detail-path-label"><FolderOpen size={13} />命中的位置</div>{item.paths.map((path) => <code key={path}>{path}</code>)}</div>
        <div className={`cleaner-detail-evidence ${item.planEligible ? 'is-eligible' : 'is-blocked'}`}><div className="cleaner-detail-path-label"><FileSearch size={13} />审核证据</div>{item.evidence.map((entry) => <span key={entry}>{entry}</span>)}<strong>{planAccessLabel(item)}</strong></div>
        <div className="cleaner-detail-actions"><button type="button" className="cleaner-button cleaner-button-quiet" onClick={onAsk}><Bot size={15} />询问 AI 为什么</button><button type="button" className="cleaner-button cleaner-button-quiet" onClick={() => { navigator.clipboard?.writeText(item.paths[0]).catch(() => {}); }}>复制路径</button></div>
      </section>
    </div>
  );
}

export function CleanerDashboardPrototype() {
  const [activeSurface, setActiveSurface] = useState<Surface>(getInitialSurface);
  const [items, setItems] = useState<CleanupItem[]>(CLEANUP_ITEMS);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set(CLEANUP_ITEMS.filter((item) => item.defaultSelected).map((item) => item.id)));
  const [inspected, setInspected] = useState<CleanupItem | null>(null);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [cleaned, setCleaned] = useState(false);
  const [executionState, setExecutionState] = useState<ExecutionState>('idle');
  const [executionItemIds, setExecutionItemIds] = useState<string[]>([]);
  const [executionEntries, setExecutionEntries] = useState<UndoEntry[]>([]);
  const [executionLog, setExecutionLog] = useState<ExecutionRun[]>([]);
  const [executionError, setExecutionError] = useState<string | null>(null);
  const [cleanedBytes, setCleanedBytes] = useState(0);
  const [cleanedCount, setCleanedCount] = useState(0);
  const [toast, setToast] = useState<string | null>(null);
  const [scanRootPath, setScanRootPath] = useState(loadCleanerScanRoot);
  const [scanRoot, setScanRoot] = useState(() => displayScanRoot(loadCleanerScanRoot()));
  const [totalBytes, setTotalBytes] = useState(DEMO_TOTAL_BYTES);
  const [scanSummary, setScanSummary] = useState('演示数据 · 点击重新扫描读取本地扫描结果');
  const timerRef = useRef<number | null>(null);

  useEffect(() => () => { if (timerRef.current !== null) window.clearTimeout(timerRef.current); }, []);

  const selectedItems = useMemo(() => items.filter((item) => selectedIds.has(item.id)), [items, selectedIds]);

  const resetExecution = () => {
    setExecutionState('idle');
    setExecutionItemIds([]);
    setExecutionEntries([]);
    setExecutionError(null);
    setCleaned(false);
    setCleanedBytes(0);
    setCleanedCount(0);
  };

  const updateExecutionRun = (runId: string, patch: Partial<ExecutionRun>) => {
    setExecutionLog((current) => current.map((run) => run.id === runId ? { ...run, ...patch } : run));
  };

  const toggle = (id: string) => {
    const target = items.find((item) => item.id === id);
    if (!target || target.risk === 'high' || !target.planEligible || executionState === 'running') return;
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
    resetExecution();
  };

  const inspect = (item: CleanupItem) => setInspected(item);
  const askAi = () => {
    setInspected(null);
    setToast('AI 解释会基于脱敏元数据，不会读取文件内容。');
    window.setTimeout(() => setToast(null), 3200);
  };
  const saveScanRoot = (path: string) => {
    const normalizedRoot = normalizeScanRoot(path);
    if (!isWindowsScanRoot(normalizedRoot)) {
      setToast('扫描范围必须是 Windows 路径，例如 C:\\ 或 D:\\Projects。');
      window.setTimeout(() => setToast(null), 2800);
      return;
    }
    setScanRootPath(normalizedRoot);
    setScanRoot(displayScanRoot(normalizedRoot));
    persistCleanerScanRoot(normalizedRoot);
    setToast(`扫描范围已保存：${displayScanRoot(normalizedRoot)}`);
    window.setTimeout(() => setToast(null), 2400);
  };

  const startScan = async (requestedRoot?: string) => {
    if (scanning) return;
    if (executionState === 'running') {
      setToast('清理执行中，请先等待完成。');
      window.setTimeout(() => setToast(null), 2400);
      return;
    }
    const scanPath = normalizeScanRoot(requestedRoot ?? scanRootPath);
    if (!isWindowsScanRoot(scanPath)) {
      setToast('扫描范围必须是 Windows 路径，例如 C:\\ 或 D:\\Projects。');
      window.setTimeout(() => setToast(null), 2800);
      return;
    }
    setScanning(true);
    resetExecution();
    setInspected(null);
    setToast('正在检查已知安全目录…');
    try {
      const [node, scaffolds] = await Promise.all([api.scan(scanPath), api.listScaffolds()]);
      const model = buildCleanerReadModel(node, scaffolds);
      const nextItems = cleanupItemsFromReadModel(model.items);
      setItems(nextItems);
      setSelectedIds(new Set(nextItems.filter((item) => item.defaultSelected).map((item) => item.id)));
      setScanRootPath(scanPath);
      setScanRoot(displayScanRoot(scanPath));
      persistCleanerScanRoot(scanPath);
      setTotalBytes(model.totalBytes);
      setScanSummary(`上次扫描：刚刚 · ${nextItems.length} 个位置已确认`);
      const availableBytes = nextItems.filter((item) => item.defaultSelected).reduce((sum, item) => sum + item.size, 0);
      setToast(`扫描完成：发现 ${nextItems.length} 个范围，预计可释放 ${formatSize(availableBytes)}。`);
      window.setTimeout(() => setToast(null), 3200);
    } catch (error) {
      setToast(`扫描失败：${String(error)}`);
      window.setTimeout(() => setToast(null), 4200);
    } finally {
      setScanning(false);
    }
  };
  const confirmClean = async () => {
    if (executionState === 'running' || !selectedItems.length) return;

    const targetIds = selectedItems.map((item) => item.id);
    const plan = buildCleanupPlan(selectedItems);
    const targetIdSet = new Set(targetIds);
    const runId = `run-${Date.now()}`;
    const startedAt = new Date().toISOString();
    const run: ExecutionRun = { id: runId, startedAt, status: 'running', plan, itemTitles: selectedItems.map((item) => item.title), entries: [], failedPaths: [] };
    setExecutionLog((current) => [run, ...current].slice(0, 8));
    setExecutionItemIds(targetIds);
    setExecutionEntries([]);
    setExecutionError(null);
    setExecutionState('running');
    setCleaned(false);
    setCleanedBytes(0);
    setCleanedCount(0);
    setItems((current) => current.map((item) => (targetIdSet.has(item.id) ? { ...item, status: '执行中' } : item)));
    setToast(`正在生成隔离计划：${plan.paths.length} 个路径…`);

    try {
      const entries = await executePrototypePlan(plan);
      const completedPaths = new Set(entries.map((entry) => pathKey(entry.source)));
      const completedItems = selectedItems.filter((item) => item.paths.length > 0 && item.paths.every((path) => completedPaths.has(pathKey(path))));
      const failedItems = selectedItems.filter((item) => !completedItems.some((completed) => completed.id === item.id));
      const completedIdSet = new Set(completedItems.map((item) => item.id));
      const failedIdSet = new Set(failedItems.map((item) => item.id));

      setExecutionEntries(entries);
      setItems((current) => current.map((item) => {
        if (completedIdSet.has(item.id)) return { ...item, status: '已完成' };
        if (failedIdSet.has(item.id)) return { ...item, status: '失败' };
        return item;
      }));

      if (!failedItems.length) {
        const releasedBytes = completedItems.reduce((sum, item) => sum + item.size, 0);
        setExecutionState('success');
        updateExecutionRun(runId, { status: 'success', finishedAt: new Date().toISOString(), entries, failedPaths: [] });
        setSelectedIds((current) => {
          const next = new Set(current);
          completedIdSet.forEach((id) => next.delete(id));
          return next;
        });
        setCleaned(true);
        setCleanedBytes(releasedBytes);
        setCleanedCount(completedItems.length);
        setToast(`已完成隔离 ${completedItems.length} 个范围 · 恢复入口后续接入`);
      } else {
        setExecutionState(completedItems.length ? 'partial-failure' : 'failure');
        updateExecutionRun(runId, { status: completedItems.length ? 'partial-failure' : 'failure', finishedAt: new Date().toISOString(), entries, failedPaths: plan.paths.filter((path) => !completedPaths.has(pathKey(path))) });
        setSelectedIds(new Set(failedItems.map((item) => item.id)));
        setToast(`已完成 ${completedItems.length} 个范围，${failedItems.length} 个失败项仍保留在计划中。`);
      }
      window.setTimeout(() => setToast(null), 3600);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setExecutionEntries([]);
      setExecutionState('failure');
      setExecutionError(message);
      updateExecutionRun(runId, { status: 'failure', finishedAt: new Date().toISOString(), failedPaths: plan.paths, error: message });
      setItems((current) => current.map((item) => (targetIdSet.has(item.id) ? { ...item, status: '失败' } : item)));
      setSelectedIds(new Set(targetIds));
      setToast(`清理执行失败：${message}`);
      window.setTimeout(() => setToast(null), 4200);
    }
  };
  const navigate = (surface: Surface) => {
    setActiveSurface(surface);
    const url = new URL(window.location.href);
    url.searchParams.set('prototype', 'cleaner');
    url.searchParams.set('surface', surface);
    url.searchParams.delete('variant');
    window.history.replaceState({}, '', url);
  };

  return (
    <div className="cleaner-prototype-shell">
      {activeSurface === 'space' && <VariantB activeSurface={activeSurface} onNavigate={navigate} items={items} selectedIds={selectedIds} onToggle={toggle} onInspect={inspect} onReview={() => setReviewOpen(true)} scanning={scanning} onScan={startScan} onSaveScanRoot={saveScanRoot} scanRootPath={scanRootPath} scanRoot={scanRoot} totalBytes={totalBytes} scanSummary={scanSummary} />}
      {activeSurface === 'packs' && <VariantC activeSurface={activeSurface} onNavigate={navigate} items={items} selectedIds={selectedIds} onToggle={toggle} onInspect={inspect} onReview={() => setReviewOpen(true)} scanning={scanning} onScan={startScan} onSaveScanRoot={saveScanRoot} scanRootPath={scanRootPath} scanRoot={scanRoot} totalBytes={totalBytes} scanSummary={scanSummary} />}
      {activeSurface !== 'space' && activeSurface !== 'packs' && <VariantA activeSurface={activeSurface} onNavigate={navigate} items={items} selectedIds={selectedIds} onToggle={toggle} onInspect={inspect} onReview={() => setReviewOpen(true)} scanning={scanning} onScan={startScan} onSaveScanRoot={saveScanRoot} scanRootPath={scanRootPath} scanRoot={scanRoot} totalBytes={totalBytes} scanSummary={scanSummary} cleaned={cleaned} cleanedBytes={cleanedBytes} cleanedCount={cleanedCount} executionLog={executionLog} />}
      {inspected && <DetailPopover item={inspected} onClose={() => setInspected(null)} onAsk={askAi} />}
      {reviewOpen && <ReviewSheet items={items} selectedIds={selectedIds} executionState={executionState} executionItemIds={executionItemIds} executionEntries={executionEntries} executionError={executionError} onToggle={toggle} onClose={() => setReviewOpen(false)} onConfirm={confirmClean} />}
      {toast && <div className="cleaner-prototype-toast"><Sparkles size={14} />{toast}</div>}
    </div>
  );
}
