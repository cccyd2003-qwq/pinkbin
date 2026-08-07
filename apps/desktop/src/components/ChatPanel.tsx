import { useEffect, useRef, useState } from 'react';
import {
  Copy,
  ExternalLink,
  File,
  Folder,
  FolderOpen,
  ImagePlus,
  ListChecks,
  Lock,
  MessageSquare,
  RotateCcw,
  Send,
  ShieldAlert,
  ShieldCheck,
  ShieldX,
  Sparkles,
  X,
} from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { api } from '../api';
import { isTauri } from '../env';
import { useStore, type ChatTurn } from '../store';
import { formatBytes } from '../format';
import { cleanupChat, freeChat, overviewChat } from '../advisorClient';
import type {
  AdvisorResponse,
  ChatHistoryItem,
  CleanupCandidate,
  Node,
  QuickAction,
  ScanContext,
  Scaffold,
} from '../types';
import {
  buildLocalCandidates,
  buildQuickActions,
  buildScanContext,
} from '../scanContext';

function uid() {
  return Math.random().toString(36).slice(2);
}

function findNodeByPath(root: Node | null, path: string): Node | null {
  if (!root) return null;
  if (root.path === path) return root;
  for (const c of root.children ?? []) {
    const f = findNodeByPath(c, path);
    if (f) return f;
  }
  return null;
}

function historyFromTurns(turns: ChatTurn[]): ChatHistoryItem[] {
  return turns
    .filter((turn): turn is ChatTurn & { role: 'user' | 'assistant' } =>
      !turn.pending && (turn.role === 'user' || turn.role === 'assistant'))
    .slice(-6)
    .map((turn) => ({ role: turn.role, text: turn.text }));
}

function buildOverviewSummary(context: ScanContext) {
  return {
    scan_context: context,
    root: context.root_path,
    total_size_human: formatBytes(context.total_size_bytes),
    total_files: context.total_files,
    largest_directory: context.largest_directory
      ? {
          path: context.largest_directory.path,
          name: context.largest_directory.name,
          size_human: formatBytes(context.largest_directory.size_bytes),
          size_bytes: context.largest_directory.size_bytes,
          file_count: context.largest_directory.file_count,
        }
      : null,
    top_entries: context.top_entries.map((entry) => ({
      path: entry.path,
      name: entry.name,
      size_human: formatBytes(entry.size_bytes),
      size_bytes: entry.size_bytes,
      depth: entry.depth,
      kind: entry.kind,
      file_count: entry.file_count,
      scaffold_id: entry.scaffold_id ?? null,
    })),
    known_scaffolds: context.known_scaffolds.map((scaffold) => ({
      id: scaffold.id,
      name: scaffold.name,
      risk: scaffold.risk,
      total_size_human: formatBytes(scaffold.total_size_bytes),
      total_files: scaffold.total_files,
      cleanup_supported: scaffold.cleanup_supported,
      matches: scaffold.matches.map((match) => ({
        path: match.path,
        size_human: formatBytes(match.size_bytes),
        file_count: match.file_count,
      })),
    })),
  };
}

export function ChatPanel() {
  const root = useStore((s) => s.root);
  const scanId = useStore((s) => s.scanId);
  const chat = useStore((s) => s.chat);
  const pushTurn = useStore((s) => s.pushChatTurn);
  const patchTurn = useStore((s) => s.patchChatTurn);
  const setBusy = useStore((s) => s.setChatBusy);
  const setScanContext = useStore((s) => s.setScanContext);
  const updateScanContext = useStore((s) => s.updateScanContext);
  const resetChat = useStore((s) => s.resetChat);
  const scaffolds = useStore((s) => s.scaffolds);
  const studioRequest = useStore((s) => s.studioRequest);
  const consumeStudio = useStore((s) => s.consumeStudio);
  const requestStudioFocus = useStore((s) => s.requestStudioFocus);

  const [input, setInput] = useState('');
  const [pendingDrops, setPendingDrops] = useState<{ path: string; name: string }[]>([]);
  const [pendingImages, setPendingImages] = useState<{ id: string; name: string; dataUrl: string; mimeType: string }[]>([]);
  const [dropping, setDropping] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const overviewFiredFor = useRef<number | null>(null);

  const context = chat.scanId === scanId
    ? chat.scanContext
    : (root ? buildScanContext(root, scaffolds, scanId) : null);
  const scanStillCurrent = (requestScanId: number | null) => requestScanId === null || (
    useStore.getState().scanId === requestScanId && useStore.getState().chat.scanId === requestScanId
  );

  // A scan id, rather than a path, is the identity of a scan. This means a
  // second scan of the same C:\ drive starts a fresh conversation and overview.
  useEffect(() => {
    if (!root) return;
    if (chat.scanId === scanId && chat.scanContext) {
      // Scaffolds normally load before the user scans. If they arrive later,
      // enrich the same scan context without clearing the conversation.
      if (scaffolds.length > 0 && chat.scanContext.known_scaffolds.length === 0) {
        const enriched = buildScanContext(root, scaffolds, scanId);
        if (enriched.known_scaffolds.length > 0) updateScanContext(enriched);
      }
      return;
    }

    const nextContext = buildScanContext(root, scaffolds, scanId);
    setScanContext(nextContext);
    if (overviewFiredFor.current === scanId) return;
    overviewFiredFor.current = scanId;
    void runOverview(root, nextContext);
    // The effect is intentionally keyed to the completed scan, not chat turns.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, scanId, scaffolds.length]);

  // Handle Studio card clicks — synthesize a prompt about the scaffold.
  useEffect(() => {
    if (!studioRequest) return;
    const sc = scaffolds.find((s) => s.id === studioRequest.scaffoldId);
    consumeStudio();
    if (!sc) return;
    void runStudioPrompt(sc);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [studioRequest?.ts]);

  useEffect(() => {
    scrollerRef.current?.scrollTo({ top: scrollerRef.current.scrollHeight, behavior: 'smooth' });
  }, [chat.turns.length, chat.busy]);

  const findAllScaffoldNodes = (sc: Scaffold): Node[] => {
    if (!root) return [];
    const out: Node[] = [];
    const dfs = (n: Node) => {
      if (n.scaffold_id === sc.id) {
        out.push(n);
        return;
      }
      for (const c of n.children ?? []) dfs(c);
    };
    dfs(root);
    out.sort((a, b) => b.size - a.size);
    return out;
  };

  const runStudioPrompt = async (sc: Scaffold) => {
    const requestScanId = context?.scan_id ?? null;
    const matches = findAllScaffoldNodes(sc);
    const totalSize = matches.reduce((s, m) => s + m.size, 0);
    const totalFiles = matches.reduce((s, m) => s + m.file_count, 0);
    const userText = matches.length === 0
      ? `右侧的【${sc.name}】这次扫描里没扫到。它一般会在哪些路径下？里面通常存什么？`
      : matches.length === 1
        ? `右侧显示扫描里检测到了【${sc.name}】(${formatBytes(totalSize)}, \`${matches[0].path}\`)。这个文件夹里具体都是什么？哪些是可以删的？`
        : `右侧扫描里检测到了【${sc.name}】，分布在 ${matches.length} 个位置，合计 ${formatBytes(totalSize)} / ${totalFiles.toLocaleString()} 文件：\n${matches.map((m) => `- \`${m.path}\` (${formatBytes(m.size)})`).join('\n')}\n\n这些文件夹各自都是什么？哪些是可以删的？`;
    const history = historyFromTurns(chat.turns);
    pushTurn({ id: uid(), role: 'user', text: userText });

    setBusy(true);
    const turnId = uid();
    pushTurn({ id: turnId, role: 'assistant', text: `正在分析 ${sc.name}…`, pending: true, scaffoldId: sc.id });
    try {
      const nonEmpty = matches.filter((m) => m.size > 0 || (m.children?.length ?? 0) > 0);
      const sampledMatches = await Promise.all(
        nonEmpty.map(async (m) => {
          const samples = m.is_dir
            ? await api.inspect(m.path, 8).catch(() => [] as string[])
            : [];
          return {
            path: m.path,
            size: formatBytes(m.size),
            file_count: m.file_count,
            top_extensions: (m.top_extensions ?? []).slice(0, 5),
            top_children: (m.children ?? []).slice(0, 8).map((c) => ({
              name: c.name,
              size: formatBytes(c.size),
              is_dir: c.is_dir,
            })),
            sample_paths: samples,
          };
        }),
      );
      const appContext = {
        app: sc.name,
        scaffold_id: sc.id,
        risk: sc.risk,
        disclaimer: sc.disclaimer,
        declared_paths: sc.detect,
        cleanable_scopes: sc.scopes.map((s) => ({ id: s.id, label: s.label, mode: s.mode, glob: s.glob })),
        scanned_matches: sampledMatches,
        scanned_total: matches.length > 0
          ? { location_count: matches.length, total_size: formatBytes(totalSize), total_files: totalFiles }
          : null,
      };
      const reply = await freeChat({
        context: `用户在 Studio 里点了【${sc.name}】这张卡片。下面是这个清理脚本的元数据和扫描位置，请按位置分别说明里面是什么、哪些可以删、用什么方式删——不要只挑一个位置说：\n${JSON.stringify(appContext, null, 2)}`,
        userMessage: userText,
        scanContext: context,
        history,
      });
      if (!scanStillCurrent(requestScanId)) return;
      patchTurn(turnId, { text: reply, pending: false });
    } catch (e) {
      if (!scanStillCurrent(requestScanId)) return;
      patchTurn(turnId, {
        text: `AI 暂时不可用，但本次扫描上下文仍然保留。你可以先使用下面的推荐动作，或稍后重试。\n\n${String(e)}`,
        pending: false,
      });
    } finally {
      if (scanStillCurrent(requestScanId)) setBusy(false);
    }
  };

  const runOverview = async (r: Node, scanContext: ScanContext) => {
    const requestScanId = scanContext.scan_id;
    setBusy(true);
    const turnId = uid();
    const quickActions = buildQuickActions(scanContext);
    pushTurn({
      id: turnId,
      role: 'assistant',
      text: `已载入 ${r.path} · ${formatBytes(r.size)} · ${r.file_count.toLocaleString()} 个文件。AI 正在生成整体解析…`,
      pending: true,
      quickActions,
    });
    try {
      const reply = await overviewChat(buildOverviewSummary(scanContext));
      if (!scanStillCurrent(requestScanId)) return;
      patchTurn(turnId, { text: reply, pending: false, quickActions });
    } catch (e) {
      if (!scanStillCurrent(requestScanId)) return;
      patchTurn(turnId, {
        text: `AI 总览暂时不可用，但扫描事实已经载入：${scanContext.root_path} 共 ${formatBytes(scanContext.total_size_bytes)}、${scanContext.total_files.toLocaleString()} 个文件。\n\n你不需要再次提供目录，直接选择下面的推荐动作即可。\n\n${String(e)}`,
        pending: false,
        quickActions,
        retryActionId: 'overview',
      });
    } finally {
      if (scanStillCurrent(requestScanId)) setBusy(false);
    }
  };

  const runCleanupIntent = async (action: QuickAction) => {
    if (!context) return;
    const requestScanId = context.scan_id;
    const history = historyFromTurns(chat.turns);
    pushTurn({ id: uid(), role: 'user', text: action.prompt });
    setBusy(true);
    const turnId = uid();
    pushTurn({ id: turnId, role: 'assistant', text: '正在根据本次扫描整理目录清单…', pending: true });

    try {
      const response = await cleanupChat({ context, intent: action.prompt, history });
      if (!scanStillCurrent(requestScanId)) return;
      patchTurn(turnId, {
        text: response.summary,
        pending: false,
        cleanupCandidates: response.candidates,
        candidateSource: 'ai',
        retryActionId: action.id,
      });
    } catch (e) {
      if (!scanStillCurrent(requestScanId)) return;
      const localCandidates = buildLocalCandidates(context, action.id);
      patchTurn(turnId, {
        text: `AI 暂时不可用，下面是基于本机扫描结果的保守清单。未知目录只标为“需要检查”，不会直接建议删除。\n\n${String(e)}`,
        pending: false,
        cleanupCandidates: localCandidates,
        candidateSource: 'local',
        retryActionId: action.id,
      });
    } finally {
      if (scanStillCurrent(requestScanId)) setBusy(false);
    }
  };

  const runQuickAction = (action: QuickAction) => {
    if (action.id.startsWith('scaffold:')) {
      const scaffoldId = action.id.slice('scaffold:'.length);
      requestStudioFocus(scaffoldId);
      const scaffold = scaffolds.find((s) => s.id === scaffoldId);
      if (scaffold) void runStudioPrompt(scaffold);
      return;
    }
    if (action.id.startsWith('scaffolds:')) {
      const firstScaffoldId = action.id.slice('scaffolds:'.length).split(',')[0];
      if (firstScaffoldId) requestStudioFocus(firstScaffoldId);
      void runCleanupIntent(action);
      return;
    }
    void runCleanupIntent(action);
  };

  const askFollowUp = async () => {
    const text = input.trim();
    if (!text && pendingDrops.length === 0 && pendingImages.length === 0) return;
    if (!root && pendingImages.length === 0) return;
    const requestScanId = context?.scan_id ?? null;
    setInput('');
    const drops = pendingDrops.slice();
    const images = pendingImages.slice();
    setPendingDrops([]);
    setPendingImages([]);

    const dropDesc = drops.length > 0 ? `（关于：${drops.map((d) => d.path).join('、')}）` : '';
    const imgDesc = images.length > 0 ? `（带 ${images.length} 张图片）` : '';
    const userText = [text, dropDesc, imgDesc].filter(Boolean).join('\n');
    const history = historyFromTurns(chat.turns);
    pushTurn({ id: uid(), role: 'user', text: userText });
    setBusy(true);
    const turnId = uid();
    pushTurn({ id: turnId, role: 'assistant', text: '正在结合本次扫描结果分析…', pending: true });

    try {
      const targets = drops.length > 0 && root
        ? drops.map((d) => findNodeByPath(root, d.path)).filter(Boolean) as Node[]
        : [];
      const targetContext = targets.map((target) => ({
        path: target.path,
        name: target.name,
        size: formatBytes(target.size),
        is_dir: target.is_dir,
        file_count: target.file_count,
        top_extensions: (target.top_extensions ?? []).slice(0, 6),
        sample_children: (target.children ?? []).slice(0, 8).map((c) => ({ name: c.name, size: formatBytes(c.size), is_dir: c.is_dir })),
      }));
      const targetLine = targetContext.length > 0
        ? `用户额外指定的目标对象：${JSON.stringify(targetContext, null, 2)}`
        : '';
      const reply = await freeChat({
        context: targetLine,
        userMessage: text || (images.length > 0 ? '看看这张图，告诉我是什么、能不能删。' : '这些是什么？能不能删？'),
        scanContext: context,
        history,
        images: images.length > 0 ? images.map((i) => ({ dataUrl: i.dataUrl, mimeType: i.mimeType })) : undefined,
      });
      if (!scanStillCurrent(requestScanId)) return;
      patchTurn(turnId, { text: reply, pending: false });
    } catch (e) {
      if (!scanStillCurrent(requestScanId)) return;
      patchTurn(turnId, {
        text: `AI 调用失败，但扫描上下文没有丢失。请重试或直接选择上方推荐动作。\n\n${String(e)}`,
        pending: false,
      });
    } finally {
      if (scanStillCurrent(requestScanId)) setBusy(false);
    }
  };

  const fileToDataUrl = (file: File) =>
    new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onerror = () => reject(reader.error);
      reader.onload = () => resolve(reader.result as string);
      reader.readAsDataURL(file);
    });

  const addImageFile = async (file: File) => {
    if (!file.type.startsWith('image/')) return;
    if (file.size > 20 * 1024 * 1024) {
      pushTurn({ id: uid(), role: 'system', text: `图片 ${file.name} 太大（>20MB），跳过` });
      return;
    }
    try {
      const dataUrl = await fileToDataUrl(file);
      setPendingImages((prev) => [
        ...prev,
        { id: uid(), name: file.name || 'image', dataUrl, mimeType: file.type || 'image/png' },
      ]);
    } catch (e) {
      pushTurn({ id: uid(), role: 'system', text: `读取图片失败：${String(e)}` });
    }
  };

  const onPaste = async (e: React.ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of items) {
      if (item.kind === 'file') {
        const file = item.getAsFile();
        if (file && file.type.startsWith('image/')) {
          e.preventDefault();
          await addImageFile(file);
        }
      }
    }
  };

  const onDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setDropping(false);
    const files = Array.from(e.dataTransfer.files ?? []).filter((file) => file.type.startsWith('image/'));
    if (files.length > 0) {
      for (const file of files) await addImageFile(file);
      return;
    }
    const path = e.dataTransfer.getData('application/x-pinkbin-path');
    const name = e.dataTransfer.getData('application/x-pinkbin-name') || path.split(/[\\/]/).pop() || path;
    if (!path) return;
    setPendingDrops((prev) => (prev.find((p) => p.path === path) ? prev : [...prev, { path, name }]));
  };

  const openCandidate = (candidate: CleanupCandidate) => {
    api.revealInExplorer(candidate.path).catch(() => {
      pushTurn({ id: uid(), role: 'system', text: `无法打开路径：${candidate.path}` });
    });
  };

  const copyCandidate = (candidate: CleanupCandidate) => {
    navigator.clipboard?.writeText(candidate.path).catch(() => {
      pushTurn({ id: uid(), role: 'system', text: '复制路径失败，请手动选择路径。' });
    });
  };

  const retryAction = (actionId?: string) => {
    if (!actionId) return;
    if (actionId === 'overview' && root && context) {
      void runOverview(root, context);
      return;
    }
    const action = buildQuickActions(context ?? buildScanContext(root!, scaffolds, scanId)).find((item) => item.id === actionId);
    if (action) runQuickAction(action);
  };

  const empty = chat.turns.length === 0;

  return (
    <div
      className={'chat' + (dropping ? ' drop-target' : '')}
      onDragOver={(e) => { e.preventDefault(); setDropping(true); }}
      onDragLeave={() => setDropping(false)}
      onDrop={onDrop}
    >
      <div className="chat-head">
        <Sparkles size={15} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div className="chat-title">
            {root ? (root.name || root.path) : 'Pinkbin AI'}
          </div>
          <div className="chat-sub">
            {root
              ? `${root.path} · ${formatBytes(root.size)} · ${root.file_count.toLocaleString()} 文件`
              : '扫一个磁盘，AI 自动给整体解析'}
          </div>
        </div>
        {root && context && (
          <span className="chat-context-badge" title={context.coverage.note}>
            <ShieldCheck size={12} /> 已载入扫描
          </span>
        )}
        {chat.turns.length > 0 && (
          <button className="ghost icon" onClick={resetChat} title="清空对话，保留本次扫描上下文"><X size={16} /></button>
        )}
      </div>

      <div className="chat-scroll" ref={scrollerRef}>
        {empty && !root && (
          <div className="chat-hero">
            <MessageSquare size={32} />
            <h3>Pinkbin AI</h3>
            <p>选一个磁盘 → 点扫描 → AI 自动给整体解析。<br />扫完之后，可以把左边的任意文件 / 文件夹拖进来问。</p>
            {!isTauri && <p className="muted">浏览器预览模式：扫描数据是模拟的，但 AI 会走真实接口。</p>}
          </div>
        )}
        {empty && root && (
          <div className="chat-hero">
            <Sparkles size={28} />
            <p>扫描结果已载入，AI 正在生成整体解析…</p>
          </div>
        )}
        {empty && root && context && (
          <QuickActionList actions={buildQuickActions(context)} disabled={chat.busy} onRun={runQuickAction} />
        )}
        {chat.turns.map((turn) => (
          <div key={turn.id} className={'chat-turn ' + turn.role + (turn.pending ? ' pending' : '')}>
            {turn.role === 'assistant' && turn.advice && <AdviceCard advice={turn.advice} />}
            <div className="chat-bubble">
              {turn.role === 'assistant'
                ? <ReactMarkdown remarkPlugins={[remarkGfm]}>{turn.text}</ReactMarkdown>
                : turn.text}
            </div>
            {turn.role === 'assistant' && !turn.pending && turn.quickActions && turn.quickActions.length > 0 && (
              <QuickActionList actions={turn.quickActions} disabled={chat.busy} onRun={runQuickAction} />
            )}
            {turn.role === 'assistant' && !turn.pending && turn.cleanupCandidates && (
              <CleanupCandidateList
                candidates={turn.cleanupCandidates}
                source={turn.candidateSource ?? 'local'}
                onOpen={openCandidate}
                onCopy={copyCandidate}
                onStudio={(candidate) => {
                  if (candidate.scaffold_id) requestStudioFocus(candidate.scaffold_id);
                }}
              />
            )}
            {turn.role === 'assistant' && !turn.pending && turn.retryActionId && (
              <button className="ghost chat-retry" onClick={() => retryAction(turn.retryActionId)} disabled={chat.busy}>
                <RotateCcw size={12} /> 重试这个请求
              </button>
            )}
          </div>
        ))}
        {chat.busy && <div className="chat-typing">AI 正在结合扫描结果分析…</div>}
      </div>

      <div className="chat-input-wrap">
        {pendingDrops.length > 0 && (
          <div className="chat-pills">
            {pendingDrops.map((drop) => (
              <span key={drop.path} className="chat-pill" title={drop.path}>
                {drop.path.endsWith(drop.name) && drop.path !== drop.name ? <Folder size={11} /> : <File size={11} />}
                {drop.name}
                <button onClick={() => setPendingDrops((prev) => prev.filter((p) => p.path !== drop.path))}><X size={11} /></button>
              </span>
            ))}
          </div>
        )}
        {pendingImages.length > 0 && (
          <div className="chat-image-pills">
            {pendingImages.map((image) => (
              <span key={image.id} className="chat-image-pill" title={image.name}>
                <img src={image.dataUrl} alt={image.name} />
                <button type="button" onClick={() => setPendingImages((prev) => prev.filter((p) => p.id !== image.id))}><X size={11} /></button>
              </span>
            ))}
          </div>
        )}
        <input
          ref={fileInputRef}
          type="file"
          accept="image/*"
          multiple
          style={{ display: 'none' }}
          onChange={async (e) => {
            const files = Array.from(e.target.files ?? []);
            for (const file of files) await addImageFile(file);
            if (fileInputRef.current) fileInputRef.current.value = '';
          }}
        />
        <div className="chat-input">
          <button
            type="button"
            className="ghost icon chat-attach"
            onClick={() => fileInputRef.current?.click()}
            title="加图片（也可以粘贴或拖进来）"
            disabled={chat.busy}
          >
            <ImagePlus size={15} />
          </button>
          <textarea
            rows={2}
            placeholder={root
              ? '问 AI：列出本次扫描中可清理的目录，或把文件夹 / 图片拖进来…'
              : '先选一个磁盘开始扫描，或粘贴图片直接问'}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onPaste={onPaste}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                void askFollowUp();
              }
            }}
            disabled={!root && pendingImages.length === 0}
          />
          <button
            className="primary"
            onClick={() => void askFollowUp()}
            disabled={
              (!input.trim() && pendingDrops.length === 0 && pendingImages.length === 0) ||
              chat.busy ||
              (!root && pendingImages.length === 0)
            }
          >
            <Send size={14} /> 发送
          </button>
        </div>
      </div>
    </div>
  );
}

function QuickActionList({ actions, disabled, onRun }: { actions: QuickAction[]; disabled: boolean; onRun: (action: QuickAction) => void }) {
  return (
    <div className="chat-quick-actions">
      <div className="chat-quick-head"><ListChecks size={13} /> 下一步可以直接做</div>
      <div className="chat-quick-grid">
        {actions.map((action) => (
          <button key={action.id} className="chat-quick-action" onClick={() => onRun(action)} disabled={disabled} title={action.description}>
            <span>{action.label}</span>
            <ExternalLink size={12} />
          </button>
        ))}
      </div>
    </div>
  );
}

function CleanupCandidateList({
  candidates,
  source,
  onOpen,
  onCopy,
  onStudio,
}: {
  candidates: CleanupCandidate[];
  source: 'ai' | 'local';
  onOpen: (candidate: CleanupCandidate) => void;
  onCopy: (candidate: CleanupCandidate) => void;
  onStudio: (candidate: CleanupCandidate) => void;
}) {
  if (candidates.length === 0) {
    return <div className="cleanup-candidates-empty">本次扫描证据里暂时没有可列出的目录。可以展开左侧大目录后继续提问。</div>;
  }

  return (
    <div className="cleanup-candidates">
      <div className="cleanup-candidates-head">
        <span><ListChecks size={13} /> 目录清单</span>
        <span className="muted small">{source === 'ai' ? 'AI 已按扫描路径核验' : '本机保守兜底'}</span>
      </div>
      {candidates.map((candidate) => {
        const RiskIcon = candidate.risk === 'low' ? ShieldCheck : candidate.risk === 'medium' ? ShieldAlert : ShieldX;
        const statusLabel = candidate.status === 'preview' ? '可进 Studio 预览' : candidate.status === 'inspect' ? '需要检查' : '不要直接删除';
        return (
          <div key={candidate.path} className={'cleanup-candidate cleanup-candidate-' + candidate.status}>
            <div className="cleanup-candidate-top">
              <div className="cleanup-candidate-name"><FolderOpen size={13} /> {candidate.name}</div>
              <div className="cleanup-candidate-size">{formatBytes(candidate.size_bytes)}</div>
            </div>
            <code className="cleanup-candidate-path" title={candidate.path}>{candidate.path}</code>
            <div className="cleanup-candidate-meta">
              <span><RiskIcon size={12} /> {statusLabel}</span>
              <span>{candidate.file_count.toLocaleString()} 个文件</span>
            </div>
            <div className="cleanup-candidate-reason">{candidate.reason}</div>
            <div className="cleanup-candidate-actions">
              {candidate.status === 'preview' && candidate.audited_scaffold && candidate.scaffold_id && (
                <button className="primary" onClick={() => onStudio(candidate)}><ListChecks size={12} /> 在 Studio 预览</button>
              )}
              <button className="ghost" onClick={() => onOpen(candidate)}><FolderOpen size={12} /> 查看目录</button>
              <button className="ghost icon" onClick={() => onCopy(candidate)} title="复制路径"><Copy size={12} /></button>
              {candidate.status === 'keep' && <Lock size={13} className="cleanup-candidate-lock" />}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function AdviceCard({ advice }: { advice: AdvisorResponse }) {
  const Icon = advice.risk === 'low' ? ShieldCheck : advice.risk === 'medium' ? ShieldAlert : ShieldX;
  const color = advice.risk === 'low' ? '#5fcf95' : advice.risk === 'medium' ? '#ffb37a' : '#ff5d7a';
  return (
    <div className="advice-pill" style={{ borderColor: color }}>
      <Icon size={14} style={{ color }} />
      <strong>{advice.category}</strong>
      <span className="badge">{advice.action}</span>
      <span className="muted">风险 {advice.risk}</span>
      {advice.needs_inspection && <span className="badge">需要再看看</span>}
    </div>
  );
}
