import type {
  CleanupCandidate,
  CleanupCandidateResponse,
  CleanupCandidateStatus,
  Node,
  QuickAction,
  Risk,
  ScanContext,
  ScanEntry,
  ScanScaffoldMatch,
  Scaffold,
} from './types';

const MAX_TOP_ENTRIES = 40;
const MAX_SCAFFOLD_MATCHES = 12;
const AUDITED_SCAFFOLD_IDS = new Set(['wechat-pc', 'conda']);

function pathKey(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

function entryFromNode(node: Node, depth: number): ScanEntry {
  return {
    path: node.path,
    name: node.name || node.path,
    size_bytes: node.size,
    file_count: node.file_count,
    depth,
    kind: node.is_dir ? 'dir' : 'file',
    scaffold_id: node.scaffold_id ?? null,
    top_extensions: (node.top_extensions ?? []).slice(0, 8),
  };
}

function collectVisibleEntries(root: Node): ScanEntry[] {
  const all: ScanEntry[] = [];
  const visit = (node: Node, depth: number) => {
    if (depth > 0) all.push(entryFromNode(node, depth));
    if (depth >= 2) return;
    for (const child of node.children ?? []) visit(child, depth + 1);
  };
  visit(root, 0);

  const unique = new Map<string, ScanEntry>();
  for (const entry of all) unique.set(pathKey(entry.path), entry);
  return Array.from(unique.values())
    .sort((a, b) => b.size_bytes - a.size_bytes)
    .slice(0, MAX_TOP_ENTRIES);
}

function collectScaffoldMatches(root: Node, scaffold: Scaffold): ScanEntry[] {
  const matches: ScanEntry[] = [];
  const visit = (node: Node, depth: number) => {
    if (node.scaffold_id === scaffold.id) {
      matches.push(entryFromNode(node, depth));
      return;
    }
    for (const child of node.children ?? []) visit(child, depth + 1);
  };
  visit(root, 0);
  return matches.sort((a, b) => b.size_bytes - a.size_bytes).slice(0, MAX_SCAFFOLD_MATCHES);
}

export function buildScanContext(root: Node, scaffolds: Scaffold[], scanId: number): ScanContext {
  const topEntries = collectVisibleEntries(root);
  const knownScaffolds: ScanScaffoldMatch[] = scaffolds
    .map((scaffold) => {
      const matches = collectScaffoldMatches(root, scaffold);
      return {
        id: scaffold.id,
        name: scaffold.name,
        risk: scaffold.risk,
        total_size_bytes: matches.reduce((sum, match) => sum + match.size_bytes, 0),
        total_files: matches.reduce((sum, match) => sum + match.file_count, 0),
        cleanup_supported: AUDITED_SCAFFOLD_IDS.has(scaffold.id),
        matches,
      };
    })
    .filter((match) => match.matches.length > 0)
    .sort((a, b) => b.total_size_bytes - a.total_size_bytes);

  return {
    scan_id: scanId,
    root_path: root.path,
    total_size_bytes: root.size,
    total_files: root.file_count,
    largest_directory: topEntries.find((entry) => entry.kind === 'dir') ?? null,
    top_entries: topEntries,
    known_scaffolds: knownScaffolds,
    coverage: {
      root_totals_exact: true,
      child_entries: 'visible_ranked_sample',
      max_depth: 2,
      note: '根目录大小和文件数来自完整扫描；子目录列表是已返回树中的按占用排序样本，不代表完整目录清单。',
    },
  };
}

export function buildQuickActions(context: ScanContext): QuickAction[] {
  const base: QuickAction[] = [
    {
      id: 'list-cleanable',
      label: '列出可清理目录',
      prompt: '列出本次扫描中可以清理或回收的目录，按占用从大到小排列。只使用扫描证据里的完整路径；没有已审核规则的目录标为“需要检查”，不要直接说可以删除。',
      description: '基于本次扫描路径生成候选清单',
    },
    {
      id: 'low-risk',
      label: '只看低风险项目',
      prompt: '只显示低风险、可以先生成回收站预览的项目。不要把用户文档、系统目录或未接入清理脚本的目录算作可清理。',
      description: '只展示有安全依据的项目',
    },
    {
      id: 'largest',
      label: '解释最大目录',
      prompt: '按占用从大到小解释本次扫描里最大的目录：它是什么、为什么占空间、是否有安全的清理方式。',
      description: '先看最值得关注的空间大户',
    },
    {
      id: 'protected',
      label: '列出不要动的目录',
      prompt: '列出本次扫描中明确不应该直接删除的系统目录和用户资料目录，并说明原因。',
      description: '先把系统和个人数据边界讲清楚',
    },
  ];

  const audited = context.known_scaffolds.filter((scaffold) => scaffold.cleanup_supported);
  const unsupported = context.known_scaffolds.filter((scaffold) => !scaffold.cleanup_supported);
  const appAction: QuickAction = audited.length === 1
    ? {
        id: `scaffold:${audited[0].id}`,
        label: `预览 ${audited[0].name} 清理空间`,
        prompt: `检查已识别的 ${audited[0].name}，只说明已审核清理 scope 的占用和风险，先生成预览，不执行清理。`,
        description: `已检测到 ${audited[0].matches.length} 个位置`,
      }
    : audited.length > 1
      ? {
          id: `scaffolds:${audited.map((scaffold) => scaffold.id).join(',')}`,
          label: `预览 ${audited.map((scaffold) => scaffold.name).join('、')} 清理空间`,
          prompt: `分别检查已识别的 ${audited.map((scaffold) => scaffold.name).join('、')}，只说明已审核清理 scope 的占用和风险，先生成预览，不执行清理。`,
          description: `已检测到 ${audited.reduce((count, scaffold) => count + scaffold.matches.length, 0)} 个位置`,
        }
      : unsupported.length > 0
        ? {
            id: 'app-check',
            label: `发现 ${unsupported[0].name}（未接入脚本）`,
            prompt: '列出扫描中识别到但尚未接入安全清理脚本的应用目录，只给出发现证据和人工检查建议，不要把它们伪装成可执行清理项。',
            description: '只解释发现，不提供删除或清理按钮',
          }
        : {
            id: 'app-check',
            label: '检查已识别应用',
            prompt: '检查扫描里能明确识别的应用目录，说明哪些已有已审核清理脚本，哪些只能手动检查；不要凭空添加 Docker、Visual Studio 等未检测到的应用。',
            description: '只基于扫描里出现的应用名称',
          };

  return [base[0], base[1], base[2], appAction, base[3]];
}

function isProtectedPath(path: string): boolean {
  const normalized = pathKey(path);
  return [
    /\/windows(?:\/|$)/,
    /\/program files(?: \(x86\))?(?:\/|$)/,
    /\/programdata(?:\/|$)/,
    /\/recovery(?:\/|$)/,
    /\/boot(?:\/|$)/,
    /\/\$recycle\.bin(?:\/|$)/,
    /\/system volume information(?:\/|$)/,
    /\/users\/[^/]+\/(?:desktop|documents|downloads|pictures|music|videos)(?:\/|$)/,
  ].some((pattern) => pattern.test(normalized));
}

function protectedReason(path: string): string {
  const normalized = pathKey(path);
  if (/\/users\/[^/]+\/(?:desktop|documents|downloads|pictures|music|videos)(?:\/|$)/.test(normalized)) {
    return '用户资料目录，不能仅凭占用大小判断是否可删除。';
  }
  return '系统目录或系统保留目录，不提供直接删除建议。';
}

function looksLikeCacheOrBuild(path: string): boolean {
  return /(?:cache|cached|temp|tmp|logs?|crash|node_modules|\.nuget|docker|packages?)/i.test(path);
}

function localCandidateFor(entry: ScanEntry, context: ScanContext): CleanupCandidate {
  const detectedScaffold = context.known_scaffolds.find((scaffold) =>
    scaffold.matches.some((match) => pathKey(match.path) === pathKey(entry.path)),
  );
  const matchedScaffold = detectedScaffold?.cleanup_supported ? detectedScaffold : undefined;

  const baseEvidence = [
    `scan_path:${entry.path}`,
    `size_bytes:${entry.size_bytes}`,
    `file_count:${entry.file_count}`,
    `depth:${entry.depth}`,
  ];

  // An audited scaffold is more specific than the generic Documents/Pictures
  // guard: WeChat media under Documents is safe to preview through its
  // reviewed scopes, while unrelated user data remains protected below.
  if (matchedScaffold) {
    const keep = matchedScaffold.risk === 'high';
    return {
      path: entry.path,
      name: entry.name,
      kind: entry.kind,
      size_bytes: entry.size_bytes,
      file_count: entry.file_count,
      risk: matchedScaffold.risk,
      status: keep ? 'keep' : 'preview',
      method: keep ? 'keep' : 'scaffold',
      suggested_handling: keep ? 'keep' : 'studio_scope_preview',
      evidence: [...baseEvidence, `audited_scaffold:${matchedScaffold.id}`],
      audited_scaffold: true,
      reason: keep
        ? `${matchedScaffold.name} 风险较高，只能先保留并人工确认。`
        : `${matchedScaffold.name} 已有已审核清理脚本，可在 Studio 中查看 scope 预览。`,
      scaffold_id: matchedScaffold.id,
      source: 'scaffold',
    };
  }

  if (isProtectedPath(entry.path)) {
    return {
      path: entry.path,
      name: entry.name,
      kind: entry.kind,
      size_bytes: entry.size_bytes,
      file_count: entry.file_count,
      risk: 'high',
      status: 'keep',
      method: 'keep',
      suggested_handling: 'keep',
      evidence: baseEvidence,
      audited_scaffold: false,
      reason: protectedReason(entry.path),
      scaffold_id: entry.scaffold_id ?? null,
      source: 'scanned',
    };
  }

  if (detectedScaffold && !detectedScaffold.cleanup_supported) {
    return {
      path: entry.path,
      name: entry.name,
      kind: entry.kind,
      size_bytes: entry.size_bytes,
      file_count: entry.file_count,
      risk: detectedScaffold.risk,
      status: 'inspect',
      method: 'manual',
      suggested_handling: 'inspect_in_explorer',
      evidence: [...baseEvidence, `detected_scaffold:${detectedScaffold.id}`, 'cleanup_script:not_connected'],
      audited_scaffold: false,
      reason: `${detectedScaffold.name} 已发现，但当前未接入安全清理脚本，只能人工检查。`,
      scaffold_id: detectedScaffold.id,
      source: 'scanned',
    };
  }

  return {
    path: entry.path,
    name: entry.name,
    kind: entry.kind,
    size_bytes: entry.size_bytes,
    file_count: entry.file_count,
    risk: 'medium',
    status: 'inspect',
    method: 'manual',
    suggested_handling: 'inspect_in_explorer',
    evidence: baseEvidence,
    audited_scaffold: false,
    reason: looksLikeCacheOrBuild(entry.path)
      ? '目录名像缓存或开发产物，但当前没有已审核清理脚本，先检查再处理。'
      : '仅凭目录元数据不能确认是否可删除，先查看内容和所属应用。',
    scaffold_id: entry.scaffold_id ?? null,
    source: 'scanned',
  };
}

function evidenceEntries(context: ScanContext): ScanEntry[] {
  const all = [...context.top_entries, ...context.known_scaffolds.flatMap((scaffold) => scaffold.matches)];
  const unique = new Map<string, ScanEntry>();
  for (const entry of all) {
    if (entry.kind === 'dir') unique.set(pathKey(entry.path), entry);
  }
  return Array.from(unique.values()).sort((a, b) => b.size_bytes - a.size_bytes);
}

export function buildLocalCandidates(context: ScanContext, intent: string): CleanupCandidate[] {
  const all = evidenceEntries(context).map((entry) => localCandidateFor(entry, context));
  if (intent === 'protected') return all.filter((candidate) => candidate.status === 'keep');
  if (intent === 'low-risk') return all.filter((candidate) => candidate.status === 'preview' && candidate.risk === 'low');
  if (intent.startsWith('scaffold:')) {
    const id = intent.slice('scaffold:'.length);
    return all.filter((candidate) => candidate.scaffold_id === id);
  }
  if (intent.startsWith('scaffolds:')) {
    const ids = new Set(intent.slice('scaffolds:'.length).split(',').filter(Boolean));
    return all.filter((candidate) => candidate.audited_scaffold && candidate.scaffold_id && ids.has(candidate.scaffold_id));
  }
  if (intent === 'app-check') return all.filter((candidate) => candidate.scaffold_id !== null && candidate.scaffold_id !== undefined);
  if (intent === 'list-cleanable') return all.filter((candidate) => candidate.status !== 'keep');
  return all.filter((candidate) => candidate.status !== 'keep').slice(0, 12);
}

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' ? value as Record<string, unknown> : null;
}

function riskOf(value: unknown, fallback: Risk): Risk {
  return value === 'low' || value === 'medium' || value === 'high' ? value : fallback;
}

function statusOf(value: unknown): CleanupCandidateStatus | null {
  if (value === 'preview' || value === 'inspect' || value === 'keep') return value;
  if (value === 'recycle' || value === 'scaffold') return 'preview';
  if (value === 'manual' || value === 'custom') return 'inspect';
  return null;
}

/** Keep only AI candidates whose paths are present in the scan evidence. */
export function normalizeCleanupResponse(value: unknown, context: ScanContext): CleanupCandidateResponse {
  const root = record(value);
  const local = buildLocalCandidates(context, 'list-cleanable');
  const localByPath = new Map(local.map((candidate) => [pathKey(candidate.path), candidate]));
  const evidence = new Map(evidenceEntries(context).map((entry) => [pathKey(entry.path), entry]));
  const rawCandidates = root && Array.isArray(root.candidates) ? root.candidates : [];
  const candidates: CleanupCandidate[] = [];

  for (const raw of rawCandidates) {
    const item = record(raw);
    const path = typeof item?.path === 'string' ? item.path.trim() : '';
    const key = pathKey(path);
    const entry = evidence.get(key);
    if (!entry) continue;
    const base = localByPath.get(key) ?? localCandidateFor(entry, context);
    const aiStatus = statusOf(item?.status ?? item?.recommendation ?? item?.action);
    // The model may downgrade a candidate, but it cannot upgrade an unknown
    // path into a Studio-previewable item. Only an audited scaffold can grant
    // the preview status.
    const status = base.status === 'keep'
      ? 'keep'
      : base.status === 'preview'
        ? (aiStatus === 'keep' ? 'keep' : aiStatus === 'inspect' ? 'inspect' : 'preview')
        : (aiStatus === 'keep' ? 'keep' : 'inspect');
    candidates.push({
      ...base,
      risk: base.risk === 'high' ? 'high' : riskOf(item?.risk, base.risk),
      status,
      method: status === 'keep' ? 'keep' : base.method,
      reason: typeof item?.reason === 'string' && item.reason.trim() ? item.reason.trim() : base.reason,
    });
  }

  const unique = new Map<string, CleanupCandidate>();
  for (const candidate of candidates) unique.set(pathKey(candidate.path), candidate);
  const fallbackSummary = 'AI 暂时没有返回新的路径判断，下面只显示本次扫描中有证据的候选。';
  return {
    summary: typeof root?.summary === 'string' && root.summary.trim() ? root.summary.trim() : fallbackSummary,
    candidates: Array.from(unique.values()).slice(0, 16),
  };
}
