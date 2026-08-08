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
import { buildCleanerReadModel, type CleanerIconKey, type CleanerReadItem } from '../cleanerReadModel';
import './CleanerDashboardPrototype.css';

// PROTOTYPE: A composed cleaner workspace. A is the home surface, B is space
// analysis, and C is the advanced cleanup/review workbench.

type Surface = 'home' | 'space' | 'packs' | 'history' | 'settings';
type Risk = 'low' | 'medium' | 'high';

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
  defaultSelected: boolean;
  status?: '等待中' | '执行中' | '已完成' | '已跳过' | '失败';
};

const GB = 1024 ** 3;
const DEFAULT_SCAN_ROOT = 'C:\\';
const DEMO_TOTAL_BYTES = 187.1 * GB;

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
    defaultSelected: false,
    status: '失败',
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
    defaultSelected: false,
    status: '已跳过',
  },
];

const NAV_ITEMS: { id: Surface; label: string; icon: LucideIcon }[] = [
  { id: 'home', label: '首页', icon: LayoutDashboard },
  { id: 'space', label: '空间分析', icon: HardDrive },
  { id: 'packs', label: '深度清理', icon: PackageOpen },
  { id: 'history', label: '历史 / 恢复', icon: History },
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

function riskClass(risk: Risk): string {
  return `cleaner-risk cleaner-risk-${risk}`;
}

function RiskLabel({ risk, text }: { risk: Risk; text?: string }) {
  const label = text ?? (risk === 'low' ? '可直接清理' : risk === 'medium' ? '需要确认' : '仅建议查看');
  return <span className={riskClass(risk)}>{label}</span>;
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
      <button type="button" onClick={onScan} disabled={scanning}><RefreshCw size={13} /> 重新扫描</button>
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

function SelectionButton({ item, selected, onToggle }: { item: CleanupItem; selected: boolean; onToggle: () => void }) {
  const disabled = item.risk === 'high';
  return (
    <button
      type="button"
      className={`cleaner-select-button ${selected ? 'is-selected' : ''} ${disabled ? 'is-disabled' : ''}`}
      onClick={disabled ? undefined : onToggle}
      disabled={disabled}
      aria-pressed={selected}
      aria-label={`${selected ? '取消选择' : '选择'} ${item.title}`}
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
      <RiskLabel risk={item.risk} text={item.label} />
      <SizeValue bytes={item.size} />
      <button type="button" className="cleaner-icon-button" onClick={onInspect} aria-label={`查看 ${item.title}`}><ChevronRight size={16} /></button>
    </div>
  );
}

function ReviewSheet({ items, selectedIds, onToggle, onClose, onConfirm }: { items: CleanupItem[]; selectedIds: Set<string>; onToggle: (id: string) => void; onClose: () => void; onConfirm: () => void }) {
  const selected = items.filter((item) => selectedIds.has(item.id));
  const total = selected.reduce((sum, item) => sum + item.size, 0);
  return (
    <div className="cleaner-sheet-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="cleaner-review-sheet" role="dialog" aria-modal="true" aria-label="复核清理计划" onMouseDown={(event) => event.stopPropagation()}>
        <div className="cleaner-sheet-head">
          <div><span className="cleaner-kicker">最后一步 · 人工审核</span><h2>确认这次清理</h2></div>
          <button type="button" className="cleaner-icon-button" onClick={onClose} aria-label="关闭复核"><X size={18} /></button>
        </div>
        <div className="cleaner-review-total"><span>预计可释放</span><strong>{formatSize(total)}</strong><small>{selected.length} 个可重建范围 · 默认进入隔离区</small></div>
        <div className="cleaner-review-list">
          {selected.length === 0 ? <div className="cleaner-empty-review">没有选择任何项目。回到结果页勾选低风险内容，或只把这次当作查看报告。</div> : selected.map((item) => (
            <div className="cleaner-review-row" key={item.id}>
              <SelectionButton item={item} selected onToggle={() => onToggle(item.id)} />
              <div className="cleaner-review-row-copy"><strong>{item.title}</strong><span>{item.paths[0]}</span><small>{item.consequence}</small></div>
              <SizeValue bytes={item.size} />
            </div>
          ))}
        </div>
        <div className="cleaner-sheet-foot">
          <span className="cleaner-safe-note"><ShieldCheck size={15} /> 不读取文件内容 · 7 天内可恢复</span>
          <div className="cleaner-sheet-actions"><button type="button" className="cleaner-button cleaner-button-quiet" onClick={onClose}>继续查看</button><button type="button" className="cleaner-button cleaner-button-primary" disabled={!selected.length} onClick={onConfirm}><Archive size={15} /> 隔离 {selected.length ? formatSize(total) : ''}</button></div>
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
    history: { eyebrow: '历史 / 恢复', title: '每次清理都有记录和退路', copy: '本地报告、隔离区和恢复入口会在这里汇合。', icon: History },
    settings: { eyebrow: '设置', title: '把控制权留在你手里', copy: '管理扫描根目录、排除项、AI 和深度清理规则更新。', icon: Settings2 },
  };
  const value = data[surface];
  const Icon = value.icon;
  return <div className="cleaner-surface-note"><Icon size={22} /><span className="cleaner-kicker">{value.eyebrow}</span><h2>{value.title}</h2><p>{value.copy}</p></div>;
}

function VariantA({ activeSurface, onNavigate, items, selectedIds, onToggle, onInspect, onReview, scanning, onScan, scanRoot, totalBytes, scanSummary, cleaned }: VariantProps) {
  const low = items.filter((item) => item.risk === 'low');
  const medium = items.filter((item) => item.risk === 'medium');
  const high = items.filter((item) => item.risk === 'high');
  const selectedBytes = items.filter((item) => selectedIds.has(item.id)).reduce((sum, item) => sum + item.size, 0);
  const defaultSelectedBytes = items.filter((item) => item.defaultSelected).reduce((sum, item) => sum + item.size, 0);
  return (
    <div className="cleaner-variant cleaner-variant-a">
      <CleanerSidebar activeSurface={activeSurface} onNavigate={onNavigate} scanRoot={scanRoot} totalBytes={totalBytes} />
      <main className="cleaner-a-main">
        <ScanStatus scanning={scanning} onScan={onScan} summary={scanSummary} />
        {activeSurface !== 'home' ? <SurfaceNote surface={activeSurface} /> : (
          <>
            <div className="cleaner-a-heading"><div><span className="cleaner-kicker">今天 · 快速清理</span><h1>你的空间，<em>看懂了。</em></h1><p>发现 {items.length} 个范围，其中 {low.length} 个可以马上释放。</p></div><button type="button" className="cleaner-icon-button cleaner-heading-settings" title="调整扫描范围"><SlidersHorizontal size={17} /></button></div>
            {cleaned && <div className="cleaner-success-banner"><CheckCircle2 size={18} /><span><strong>已模拟隔离 2 个范围</strong><small>{formatSize(defaultSelectedBytes)} 已进入隔离区 · 7 天内可恢复</small></span><button type="button" className="cleaner-link-button">去历史 / 恢复 <ArrowRight size={13} /></button></div>}
            <section className="cleaner-a-hero">
              <div><span className="cleaner-hero-label"><ShieldCheck size={15} /> {cleaned ? '本次已隔离' : '可安全释放'}</span><strong className="cleaner-hero-number">{formatSize(cleaned ? defaultSelectedBytes : selectedBytes)}</strong><p>{cleaned ? `${items.filter((item) => item.defaultSelected).length} 个范围已进入隔离区` : <>来自 {selectedIds.size} 个低风险可重建范围<br /><span>预计完成后实际数字可能略有变化</span></>}<br />{cleaned && <span>仍可在 7 天内恢复</span>}</p><div className="cleaner-hero-actions"><button type="button" className="cleaner-button cleaner-button-primary" onClick={onReview} disabled={!selectedIds.size || cleaned}><Archive size={15} /> {cleaned ? '本次已完成' : '查看并清理'}</button><button type="button" className="cleaner-button cleaner-button-quiet" onClick={onScan}><RefreshCw size={14} /> 完整扫描</button></div></div>
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
        <div className="cleaner-b-heading"><div><span className="cleaner-kicker">空间分析 · {scanRoot}</span><h1>{formatSize(totalBytes)} <span>已用</span></h1><p>拖动和点击空间块，查看它们的来源与清理建议。</p></div><div className="cleaner-b-scan"><div className="cleaner-b-gauge"><span>{items.length ? Math.round((items.reduce((sum, item) => sum + item.size, 0) / Math.max(totalBytes, 1)) * 100) : 0}%</span></div><span><strong>已扫描</strong><small>{items.length} 个位置 · {scanning ? '扫描中…' : scanSummary}</small></span><button type="button" className="cleaner-plain-button" onClick={onScan}><RefreshCw size={15} /></button></div></div>
        {activeSurface !== 'space' && activeSurface !== 'home' ? <SurfaceNote surface={activeSurface} /> : (
          <>
            <div className="cleaner-b-map-row">{focused ? <><section className="cleaner-space-map"><div className="cleaner-map-label"><span>按占用空间</span><span>点击查看详情</span></div><div className="cleaner-map-grid">{blocks.map((block) => { const item = items.find((entry) => entry.id === block.id)!; return <button type="button" key={block.id} className={`cleaner-map-block cleaner-map-${block.size} cleaner-tone-bg-${item.tone} ${focusId === block.id ? 'is-focus' : ''}`} style={{ gridColumn: block.col, gridRow: block.row }} onClick={() => { setFocusId(block.id); onInspect(item); }}><strong>{block.label}</strong><span>{block.sub}</span><small>{item.risk === 'high' ? '仅查看' : selectedIds.has(item.id) ? '已选择' : '可选择'}</small></button>; })}</div><div className="cleaner-map-legend"><span><i className="legend-dot legend-safe" />可重建</span><span><i className="legend-dot legend-review" />需要确认</span><span><i className="legend-dot legend-view" />用户内容 / 高风险</span></div></section><aside className="cleaner-b-inspector"><span className="cleaner-kicker">当前选择</span><div className="cleaner-inspector-title"><IconTile icon={focused.icon} tone={focused.tone} /><div><h2>{focused.title}</h2><span>{focused.subtitle}</span></div></div><div className="cleaner-inspector-size"><strong>{formatSize(focused.size)}</strong><span>{focused.files}</span></div><RiskLabel risk={focused.risk} text={focused.label} /><p>{focused.description}</p><div className="cleaner-inspector-impact"><TriangleAlert size={14} /><span>{focused.consequence}</span></div><div className="cleaner-inspector-path"><FolderOpen size={14} /><span>{focused.paths[0]}</span></div><div className="cleaner-inspector-actions">{focused.risk !== 'high' && <button type="button" className={`cleaner-button ${selectedIds.has(focused.id) ? 'cleaner-button-quiet' : 'cleaner-button-primary'}`} onClick={() => onToggle(focused.id)}>{selectedIds.has(focused.id) ? '取消选择' : '加入清理计划'} <ArrowRight size={14} /></button>}<button type="button" className="cleaner-text-button" onClick={() => onInspect(focused)}>查看所有路径</button></div></aside></> : <div className="cleaner-surface-note"><HardDrive size={22} /><span className="cleaner-kicker">空间分析</span><h2>还没有可展示的扫描位置</h2><p>重新扫描后，这里会按占用空间显示目录来源。</p></div>}</div>
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
  const visible = filter === 'all' ? items : items.filter((item) => item.risk === filter);
  const selectedBytes = items.filter((item) => selectedIds.has(item.id)).reduce((sum, item) => sum + item.size, 0);
  return (
    <div className="cleaner-variant cleaner-variant-c">
      <CleanerSidebar activeSurface={activeSurface} onNavigate={onNavigate} scanRoot={scanRoot} totalBytes={totalBytes} />
      <div className="cleaner-c-workspace">
        <header className="cleaner-c-header"><div className="cleaner-c-title"><PanelLeft size={17} /><div><span className="cleaner-kicker">深度清理 · 审核工作台</span><h1>清理计划</h1></div></div><div className="cleaner-c-header-meta"><span><Clock3 size={14} />扫描于刚刚</span><button type="button" className="cleaner-button cleaner-button-quiet" onClick={onScan}><RefreshCw size={14} /> {scanning ? '扫描中…' : '重新扫描'}</button></div></header>
        {activeSurface !== 'packs' && activeSurface !== 'home' ? <SurfaceNote surface={activeSurface} /> : (
          <main className="cleaner-c-main">
          <aside className="cleaner-c-filter"><div className="cleaner-c-filter-top"><span className="cleaner-kicker">筛选</span><SlidersHorizontal size={15} /></div><div className="cleaner-filter-search"><Search size={14} /><input placeholder="搜索应用或路径" /></div><div className="cleaner-c-filter-label">风险</div>{(['all', 'low', 'medium', 'high'] as const).map((value) => <button type="button" key={value} className={filter === value ? 'is-active' : ''} onClick={() => setFilter(value)}><span className={`filter-marker filter-marker-${value}`} />{value === 'all' ? '全部范围' : value === 'low' ? '可直接清理' : value === 'medium' ? '需要确认' : '仅建议查看'}<small>{value === 'all' ? items.length : items.filter((item) => item.risk === value).length}</small></button>)}<div className="cleaner-c-filter-label">类别</div>{GROUPS.map(({ id, label, icon: Icon }) => <button type="button" key={id} className="cleaner-filter-category"><Icon size={14} />{label}<small>{items.filter((item) => item.group === id).length}</small></button>)}<div className="cleaner-c-filter-foot"><LockKeyhole size={13} />系统保护路径已排除</div></aside>
          <section className="cleaner-c-table-section"><div className="cleaner-c-table-head"><div><span className="cleaner-kicker">{visible.length} 个范围 · {filter === 'all' ? '全部' : filter === 'low' ? '低风险' : filter === 'medium' ? '需确认' : '仅查看'}</span><h2>逐项审核选择</h2></div><button type="button" className="cleaner-text-button"><FileSearch size={14} />打开完整报告</button></div><div className="cleaner-c-table"><div className="cleaner-table-header"><span>选择</span><span>范围</span><span>风险</span><span>大小</span><span>状态</span><span /></div>{visible.map((item) => <div className={`cleaner-table-row ${selectedIds.has(item.id) ? 'is-selected' : ''}`} key={item.id}><SelectionButton item={item} selected={selectedIds.has(item.id)} onToggle={() => onToggle(item.id)} /><button type="button" className="cleaner-table-item" onClick={() => onInspect(item)}><IconTile icon={item.icon} tone={item.tone} /><span><strong>{item.title}</strong><small>{item.subtitle}</small></span></button><RiskLabel risk={item.risk} text={item.label} /><SizeValue bytes={item.size} /><span className={`cleaner-task-state cleaner-task-${(item.status ?? '等待中').replace('中', '').replace('已', '')}`}>{item.status ?? '等待中'}</span><button type="button" className="cleaner-icon-button" onClick={() => onInspect(item)}><ChevronRight size={15} /></button></div>)}</div></section>
          <aside className="cleaner-c-plan"><div className="cleaner-c-plan-head"><div><span className="cleaner-kicker">待执行</span><h2>清理计划</h2></div><Archive size={19} /></div><div className="cleaner-c-plan-number">{formatSize(selectedBytes)}<small>预计可释放</small></div><div className="cleaner-c-plan-bar"><span style={{ width: `${Math.min(100, (selectedBytes / (40 * GB)) * 100)}%` }} /></div><div className="cleaner-c-plan-list">{items.filter((item) => selectedIds.has(item.id)).map((item) => <div key={item.id}><span className="plan-status-dot" /><span>{item.title}</span><SizeValue bytes={item.size} /></div>)}{!selectedIds.size && <div className="cleaner-plan-empty">从左侧选择低风险范围，计划会显示在这里。</div>}</div><div className="cleaner-c-plan-note"><ShieldCheck size={14} /><span>默认进入隔离区<br /><small>7 天内可恢复 · 不读取文件内容</small></span></div><button type="button" className="cleaner-button cleaner-button-primary cleaner-button-wide" onClick={onReview} disabled={!selectedIds.size}><Archive size={15} /> 复核并隔离</button></aside>
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
  onScan: () => void;
  scanRoot: string;
  totalBytes: number;
  scanSummary: string;
  cleaned?: boolean;
};

function DetailPopover({ item, onClose, onAsk }: { item: CleanupItem; onClose: () => void; onAsk: () => void }) {
  return (
    <div className="cleaner-detail-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="cleaner-detail-popover" role="dialog" aria-modal="true" aria-label={`${item.title}详情`} onMouseDown={(event) => event.stopPropagation()}>
        <div className="cleaner-detail-head"><div><IconTile icon={item.icon} tone={item.tone} /><span><span className="cleaner-kicker">{item.groupLabel}</span><h2>{item.title}</h2></span></div><button type="button" className="cleaner-icon-button" onClick={onClose}><X size={17} /></button></div>
        <div className="cleaner-detail-meta"><RiskLabel risk={item.risk} text={item.label} /><SizeValue bytes={item.size} /><span>{item.files}</span></div>
        <p className="cleaner-detail-description">{item.description}</p>
        <div className="cleaner-detail-impact"><TriangleAlert size={15} /><span><strong>清理后</strong>{item.consequence}</span></div>
        <div className="cleaner-detail-paths"><div className="cleaner-detail-path-label"><FolderOpen size={13} />命中的位置</div>{item.paths.map((path) => <code key={path}>{path}</code>)}</div>
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
  const [toast, setToast] = useState<string | null>(null);
  const [scanRoot, setScanRoot] = useState('Windows (C:)');
  const [totalBytes, setTotalBytes] = useState(DEMO_TOTAL_BYTES);
  const [scanSummary, setScanSummary] = useState('演示数据 · 点击重新扫描读取本地扫描结果');
  const timerRef = useRef<number | null>(null);

  useEffect(() => () => { if (timerRef.current !== null) window.clearTimeout(timerRef.current); }, []);

  const selectedItems = useMemo(() => items.filter((item) => selectedIds.has(item.id)), [items, selectedIds]);

  const toggle = (id: string) => {
    const target = items.find((item) => item.id === id);
    if (!target || target.risk === 'high') return;
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
    setCleaned(false);
  };

  const inspect = (item: CleanupItem) => setInspected(item);
  const askAi = () => {
    setInspected(null);
    setToast('AI 解释会基于脱敏元数据，不会读取文件内容。');
    window.setTimeout(() => setToast(null), 3200);
  };
  const startScan = async () => {
    if (scanning) return;
    setScanning(true);
    setCleaned(false);
    setInspected(null);
    setToast('正在检查已知安全目录…');
    try {
      const [node, scaffolds] = await Promise.all([api.scan(DEFAULT_SCAN_ROOT), api.listScaffolds()]);
      const model = buildCleanerReadModel(node, scaffolds);
      const nextItems = cleanupItemsFromReadModel(model.items);
      setItems(nextItems);
      setSelectedIds(new Set(nextItems.filter((item) => item.defaultSelected).map((item) => item.id)));
      setScanRoot(displayScanRoot(model.rootPath));
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
  const confirmClean = () => {
    setReviewOpen(false);
    setCleaned(true);
    setSelectedIds(new Set());
    setToast(`已模拟隔离 ${selectedItems.length} 个范围 · 7 天内可恢复`);
    window.setTimeout(() => setToast(null), 3400);
  };
  const navigate = (surface: Surface) => {
    setActiveSurface(surface);
    const url = new URL(window.location.href);
    url.searchParams.set('prototype', 'cleaner');
    url.searchParams.set('surface', surface);
    url.searchParams.delete('variant');
    window.history.replaceState({}, '', url);
    if (surface !== 'home' && surface !== 'space' && surface !== 'packs') setToast(`${NAV_ITEMS.find((item) => item.id === surface)?.label} 页面将在后续接入。`);
    window.setTimeout(() => setToast(null), 2400);
  };

  return (
    <div className="cleaner-prototype-shell">
      {activeSurface === 'space' && <VariantB activeSurface={activeSurface} onNavigate={navigate} items={items} selectedIds={selectedIds} onToggle={toggle} onInspect={inspect} onReview={() => setReviewOpen(true)} scanning={scanning} onScan={startScan} scanRoot={scanRoot} totalBytes={totalBytes} scanSummary={scanSummary} />}
      {activeSurface === 'packs' && <VariantC activeSurface={activeSurface} onNavigate={navigate} items={items} selectedIds={selectedIds} onToggle={toggle} onInspect={inspect} onReview={() => setReviewOpen(true)} scanning={scanning} onScan={startScan} scanRoot={scanRoot} totalBytes={totalBytes} scanSummary={scanSummary} />}
      {activeSurface !== 'space' && activeSurface !== 'packs' && <VariantA activeSurface={activeSurface} onNavigate={navigate} items={items} selectedIds={selectedIds} onToggle={toggle} onInspect={inspect} onReview={() => setReviewOpen(true)} scanning={scanning} onScan={startScan} scanRoot={scanRoot} totalBytes={totalBytes} scanSummary={scanSummary} cleaned={cleaned} />}
      {inspected && <DetailPopover item={inspected} onClose={() => setInspected(null)} onAsk={askAi} />}
      {reviewOpen && <ReviewSheet items={items} selectedIds={selectedIds} onToggle={toggle} onClose={() => setReviewOpen(false)} onConfirm={confirmClean} />}
      {toast && <div className="cleaner-prototype-toast"><Sparkles size={14} />{toast}</div>}
    </div>
  );
}
