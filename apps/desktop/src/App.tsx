import { useEffect, useRef, useState } from 'react';
import { Folder, ScanLine, Settings as SettingsIcon } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import { isTauri } from './env';
import { useStore } from './store';
import { TreeView } from './components/TreeView';
import { ChatPanel } from './components/ChatPanel';
import { Studio } from './components/Studio';
import { Settings } from './components/Settings';
import { Splitter } from './components/Splitter';
import { Logo } from './components/Logo';
import { ErrorBoundary } from './components/ErrorBoundary';
import { CleanerDashboardPrototype } from './prototype/CleanerDashboardPrototype';
import { formatBytes } from './format';
import { loadSettings, isConfigured } from './advisorClient';

function isDriveRoot(p: string): boolean {
  // C: / C:\ / C:/  — anything beyond is a subfolder
  return /^[A-Za-z]:[\\/]?$/.test(p);
}

interface ScanStatsEvent {
  mode: string;
  mft_attempted: boolean;
  mft_succeeded: boolean;
  mft_ms: number;
  walk_ms: number;
  build_tree_ms: number;
  scanner_total_ms: number;
  tag_ms: number;             // post-scan walk: detect_compiled + truncation
  cmd_total_ms: number;
  files_seen: number;
  bytes_seen: number;
  dirs_in_acc: number;
}

interface ScanDiag {
  backend: ScanStatsEvent | null;
  ipcMs: number | null;       // null when backend stats event didn't arrive
  scanCallMs: number;         // total api.scan() round-trip
  setRootMs: number;          // setRoot+select sync work
  totalMs: number;            // entire scan() handler
}

const DEFAULT_LEFT = 620;
const DEFAULT_RIGHT = 320;
const MIN_LEFT = 320;
const MIN_RIGHT = 220;
const MIN_CENTER = 360;

export default function App() {
  const prototypeMode = new URLSearchParams(window.location.search).get('prototype');
  if (import.meta.env.DEV && prototypeMode === 'cleaner') {
    return <CleanerDashboardPrototype />;
  }
  return <WorkspaceApp />;
}

function WorkspaceApp() {
  const root = useStore((s) => s.root);
  const setRoot = useStore((s) => s.setRoot);
  const setScaffolds = useStore((s) => s.setScaffolds);
  const scaffolds = useStore((s) => s.scaffolds);
  const selectedPath = useStore((s) => s.selectedPath);
  const select = useStore((s) => s.selectPath);

  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<{ files: number; bytes: number; path: string } | null>(null);
  const [scanTotalBytes, setScanTotalBytes] = useState<number | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [pickedPath, setPickedPath] = useState<string>('');
  const [showSettings, setShowSettings] = useState(false);
  const [advisorTag, setAdvisorTag] = useState<{ provider: string } | null>(null);
  const [diag, setDiag] = useState<ScanDiag | null>(null);
  // Holds the latest scan-stats event so we can merge it into ScanDiag once
  // api.scan() returns. Tauri emits the event right before the command resolves.
  const lastBackendStats = useRef(null as ScanStatsEvent | null);

  const refreshAdvisorTag = () => {
    const s = loadSettings();
    setAdvisorTag(isConfigured(s) ? { provider: s.provider } : null);
  };
  useEffect(() => { refreshAdvisorTag(); }, []);
  const [leftWidth, setLeftWidth] = useState<number>(() => {
    const v = Number(localStorage.getItem('pinkbin.leftWidth'));
    return Number.isFinite(v) && v > MIN_LEFT ? v : DEFAULT_LEFT;
  });
  const [rightWidth, setRightWidth] = useState<number>(() => {
    const v = Number(localStorage.getItem('pinkbin.rightWidth'));
    return Number.isFinite(v) && v > MIN_RIGHT ? v : DEFAULT_RIGHT;
  });

  useEffect(() => { localStorage.setItem('pinkbin.leftWidth', String(leftWidth)); }, [leftWidth]);
  useEffect(() => { localStorage.setItem('pinkbin.rightWidth', String(rightWidth)); }, [rightWidth]);

  const dragLeft = (dx: number) => {
    setLeftWidth((w) => {
      const winW = window.innerWidth;
      const maxLeft = Math.max(MIN_LEFT, winW - rightWidth - MIN_CENTER);
      return Math.max(MIN_LEFT, Math.min(maxLeft, w + dx));
    });
  };
  const dragRight = (dx: number) => {
    setRightWidth((w) => {
      const winW = window.innerWidth;
      const maxRight = Math.max(MIN_RIGHT, winW - leftWidth - MIN_CENTER);
      return Math.max(MIN_RIGHT, Math.min(maxRight, w - dx));
    });
  };

  useEffect(() => { api.listScaffolds().then(setScaffolds).catch(() => {}); }, [setScaffolds]);

  useEffect(() => {
    if (!isTauri) return;
    const unlisten = listen<{ files_seen: number; bytes_seen: number; current_path: string }>(
      'scan-progress',
      (e) => setScanProgress({ files: e.payload.files_seen, bytes: e.payload.bytes_seen, path: e.payload.current_path }),
    );
    return () => { unlisten.then((u) => u()); };
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    const unlisten = listen<ScanStatsEvent>('scan-stats', (e) => {
      lastBackendStats.current = e.payload;
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

  const pickDirectory = async () => {
    if (!isTauri) {
      const p = window.prompt('浏览器预览模式：输入一个路径（任意值都可以）', 'C:\\');
      if (p) setPickedPath(p);
      return;
    }
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === 'string') setPickedPath(picked);
  };

  const scan = async () => {
    if (!pickedPath) return;
    setErr(null); setScanning(true); setScanProgress(null); setScanTotalBytes(null); setDiag(null);
    lastBackendStats.current = null;
    if (isTauri) {
      if (isDriveRoot(pickedPath)) {
        // Drive root: ask the OS for used bytes — instant, exact.
        api.volumeInfo(pickedPath)
          .then((info) => { if (info) setScanTotalBytes(info.used_bytes); })
          .catch(() => {});
      } else {
        // Subfolder: run a fast size-only walk in parallel with the real scan.
        // The real scan is doing the same work either way; this just gives the
        // progress bar an exact denominator a bit ahead of completion.
        api.estimateSize(pickedPath)
          .then((bytes) => { if (bytes > 0) setScanTotalBytes(bytes); })
          .catch(() => {});
      }
    }
    const tTotal0 = performance.now();
    try {
      const tScan0 = performance.now();
      const node = await api.scan(pickedPath);
      const tScan1 = performance.now();

      const tSet0 = performance.now();
      setRoot(node);
      select(node.path);
      const tSet1 = performance.now();

      const totalMs = tSet1 - tTotal0;
      // Cast through the ref accessor: TS narrows `.current` to the last
      // assignment it sees in this flow (the `= null` reset earlier), missing
      // the listener's assignment from another effect.
      const backend = (lastBackendStats.current as unknown) as ScanStatsEvent | null;
      const ipcMs: number | null =
        backend !== null ? Math.max(0, (tScan1 - tScan0) - backend.cmd_total_ms) : null;
      const next: ScanDiag = {
        backend,
        ipcMs,
        scanCallMs: tScan1 - tScan0,
        setRootMs: tSet1 - tSet0,
        totalMs,
      };
      setDiag(next);
      // eslint-disable-next-line no-console
      console.log('[pinkbin.diag]', {
        backend,
        scanCallMs: next.scanCallMs.toFixed(1),
        ipcMs: ipcMs?.toFixed(1) ?? null,
        setRootMs: next.setRootMs.toFixed(1),
        totalMs: totalMs.toFixed(1),
      });
    } catch (e) {
      setErr(String(e));
    } finally {
      setScanning(false);
    }
  };

  return (
    <div className="app">
      <header>
        <span className="brand"><Logo size={22} /> Pinkbin</span>
        <button className="ghost" onClick={pickDirectory}>
          <Folder size={14} /> {pickedPath || '选择磁盘或文件夹'}
        </button>
        <button className="primary" onClick={scan} disabled={!pickedPath || scanning}>
          <ScanLine size={14} /> {scanning ? '扫描中…' : '扫描'}
        </button>
        <div className="grow" />
        <span className="muted small">
          {root ? `${formatBytes(root.size)} · ${root.file_count.toLocaleString()} 文件` : '未扫描'}
        </span>
        <button
          className={'ghost icon settings-btn' + (advisorTag ? ' bound' : '')}
          onClick={() => setShowSettings(true)}
          title={advisorTag ? `已绑定 ${advisorTag.provider} · 点开管理` : 'AI 还没配置 · 点开设置'}
        >
          <SettingsIcon size={16} />
          {advisorTag && <span className="settings-dot" />}
        </button>
        {advisorTag && (
          <span className="provider-pill" title="当前 AI 提供商">
            {advisorTag.provider}
          </span>
        )}
      </header>

      {scanning && (
        <div className="scan-bar">
          <div
            className={'scan-bar-fill' + (scanTotalBytes && scanProgress ? ' determinate' : ' indeterminate')}
            style={
              scanTotalBytes && scanProgress
                ? { width: `${Math.min(99, (scanProgress.bytes / scanTotalBytes) * 100)}%` }
                : undefined
            }
          />
          <div className="scan-bar-label">
            {scanProgress
              ? `${scanProgress.files.toLocaleString()} 个文件 · ${formatBytes(scanTotalBytes ? Math.min(scanProgress.bytes, scanTotalBytes) : scanProgress.bytes)}${scanTotalBytes ? ` / ${formatBytes(scanTotalBytes)}` : ''}`
              : '准备扫描…'}
          </div>
        </div>
      )}
      {diag && !scanning && <DiagnosticsBar diag={diag} />}
      {err && <div className="banner error">{err}</div>}

      <main style={{ gridTemplateColumns: `${leftWidth}px 4px 1fr 4px ${rightWidth}px` }}>
        <aside className="left">
          {root ? <TreeView root={root} selectedPath={selectedPath} onSelect={select} /> : <EmptyLeft />}
        </aside>

        <Splitter onDrag={dragLeft} onDoubleClick={() => setLeftWidth(DEFAULT_LEFT)} />

        <section className="center">
          <ChatPanel />
        </section>

        <Splitter onDrag={dragRight} onDoubleClick={() => setRightWidth(DEFAULT_RIGHT)} />

        <aside className="right">
          <ErrorBoundary fallbackLabel="Studio 面板渲染失败">
            <Studio />
          </ErrorBoundary>
        </aside>
      </main>

      <footer>
        <span>Pinkbin v0.1.1 · {scaffolds.length} 个脚本</span>
        <span>{root?.path ?? '还没扫描'}</span>
      </footer>

      {showSettings && <Settings onClose={() => { setShowSettings(false); refreshAdvisorTag(); }} />}
    </div>
  );
}

function EmptyLeft() {
  return (
    <div className="empty">
      <div className="empty-title">还没扫描</div>
      <div className="empty-sub">在顶栏选一个文件夹，然后点「扫描」。<br />扫完之后，左侧会列出每个文件夹和文件。</div>
    </div>
  );
}

function fmtMs(ms: number | null | undefined): string {
  if (ms == null) return '—';
  if (ms < 1000) return `${ms.toFixed(0)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function DiagnosticsBar({ diag }: { diag: ScanDiag }) {
  const b = diag.backend;
  const parts: string[] = [];
  if (b) {
    parts.push(`mode=${b.mode}`);
    if (b.mft_attempted) parts.push(`mft=${b.mft_succeeded ? 'ok' : 'fail'}/${fmtMs(b.mft_ms)}`);
    if (b.mode === 'walkdir') {
      parts.push(`walk=${fmtMs(b.walk_ms)}`);
      parts.push(`build_tree=${fmtMs(b.build_tree_ms)}`);
      parts.push(`dirs=${b.dirs_in_acc.toLocaleString()}`);
    }
    parts.push(`scanner=${fmtMs(b.scanner_total_ms)}`);
    parts.push(`tag=${fmtMs(b.tag_ms)}`);
  }
  parts.push(`ipc=${fmtMs(diag.ipcMs)}`);
  parts.push(`setRoot=${fmtMs(diag.setRootMs)}`);
  parts.push(`total=${fmtMs(diag.totalMs)}`);
  return (
    <div className="diag-bar" title="扫描各阶段耗时 — localStorage 开关：pinkbin.hideStudio">
      <span className="diag-label">诊断</span>
      <span className="diag-stats">{parts.join(' · ')}</span>
    </div>
  );
}
