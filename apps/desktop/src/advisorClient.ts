// Browser-side AI advisor client — talks to OpenAI / Anthropic / Ollama directly
// from the browser, so the preview mode can give real answers.
//
// Settings persist to localStorage under "pinkbin.advisor".

import type {
  AdvisorRequest,
  AdvisorResponse,
  ChatHistoryItem,
  CleanupCandidateResponse,
  ScanContext,
} from './types';
import { normalizeCleanupResponse } from './scanContext';

export type Provider = 'openai_responses' | 'openai' | 'anthropic' | 'gemini' | 'ollama';

export interface AdvisorSettings {
  provider: Provider;
  model: string;
  apiKey: string;
  baseUrl: string;
}

export interface AdvisorModel {
  id: string;
  label?: string;
}

const STORAGE_KEY = 'pinkbin.advisor';

export function loadSettings(): AdvisorSettings | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as AdvisorSettings;
    if (!parsed.provider || !parsed.model) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function saveSettings(s: AdvisorSettings) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(s));
}

export function clearSettings() {
  localStorage.removeItem(STORAGE_KEY);
}

const SYSTEM_PROMPT = `You are Pinkbin's local file advisor. Given a folder's metadata, decide what it is and whether it can be cleaned. Reply in strict JSON ONLY, matching this schema exactly:

{
  "what": "string",
  "category": "browser_cache|app_cache|package_cache|build_artifact|game_data|user_content|system|model_weights|unknown",
  "safe_to_delete": true|false,
  "risk": "low|medium|high",
  "action": "keep|recycle|delete|custom",
  "reasoning": "short string, one sentence",
  "needs_inspection": true|false,
  "suggested_scaffold": "string or null"
}

Rules:
- Be conservative. If uncertain, set needs_inspection=true and action="keep".
- "user_content" (Documents/Pictures/Music/Source code) is never safe_to_delete.
- "model_weights" (HuggingFace, Ollama models) is medium risk: deletable but expensive to redownload.
- Do not include any prose outside the JSON object.`;

function stripCodeFence(s: string): string {
  let t = s.trim();
  if (t.startsWith('```json')) t = t.slice(7);
  else if (t.startsWith('```')) t = t.slice(3);
  if (t.endsWith('```')) t = t.slice(0, -3);
  return t.trim();
}

// Anthropic 响应里 content 是 block 数组，extended-thinking 模型（如 DeepSeek
// 的 anthropic 兼容端点）会先返一个 {type:"thinking",...} 再返 {type:"text",...}，
// 不能假设 content[0] 是 text。stop_reason="max_tokens" 时还可能根本没 text
// block（thinking 把额度吃光），给明确错误而不是静默返回空串。
function extractAnthropicText(data: unknown): string {
  const d = data as { content?: Array<{ type?: string; text?: string }>; stop_reason?: string };
  const blocks = d?.content ?? [];
  const text = blocks
    .filter((b) => b?.type === 'text')
    .map((b) => b?.text ?? '')
    .join('')
    .trim();
  if (!text) {
    const stop = d?.stop_reason ?? 'unknown';
    if (stop === 'max_tokens') {
      throw new Error('AI 在 thinking 阶段被截断（max_tokens 太小，思考把额度吃光了）。把 max_tokens 调大重试。');
    }
    throw new Error(`Anthropic: 没拿到 text block（stop_reason=${stop}）`);
  }
  return text;
}

export async function callAdvisor(
  settings: AdvisorSettings,
  req: AdvisorRequest,
): Promise<AdvisorResponse> {
  const userPrompt = JSON.stringify(req, null, 2);
  let raw = '';

  if (settings.provider === 'openai_responses') {
    const url = (settings.baseUrl || 'https://api.openai.com/v1').replace(/\/$/, '');
    const r = await fetch(`${url}/responses`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${settings.apiKey}`,
      },
      body: JSON.stringify({
        model: settings.model,
        instructions: SYSTEM_PROMPT,
        input: userPrompt,
        store: false,
      }),
    });
    if (!r.ok) throw new Error(`Responses ${r.status}: ${await r.text()}`);
    const data = await r.json();
    raw = data?.output_text ?? '';
  } else if (settings.provider === 'openai') {
    const url = (settings.baseUrl || 'https://api.openai.com/v1').replace(/\/$/, '');
    const r = await fetch(`${url}/chat/completions`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${settings.apiKey}`,
      },
      body: JSON.stringify({
        model: settings.model,
        response_format: { type: 'json_object' },
        messages: [
          { role: 'system', content: SYSTEM_PROMPT },
          { role: 'user', content: userPrompt },
        ],
      }),
    });
    if (!r.ok) throw new Error(`OpenAI ${r.status}: ${await r.text()}`);
    const data = await r.json();
    raw = data?.choices?.[0]?.message?.content ?? '';
  } else if (settings.provider === 'anthropic') {
    const url = (settings.baseUrl || 'https://api.anthropic.com').replace(/\/$/, '');
    const r = await fetch(`${url}/v1/messages`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'x-api-key': settings.apiKey,
        'anthropic-version': '2023-06-01',
        'anthropic-dangerous-direct-browser-access': 'true',
      },
      body: JSON.stringify({
        model: settings.model,
        max_tokens: 2048,
        system: SYSTEM_PROMPT,
        messages: [{ role: 'user', content: userPrompt }],
      }),
    });
    if (!r.ok) throw new Error(`Anthropic ${r.status}: ${await r.text()}`);
    const data = await r.json();
    raw = extractAnthropicText(data);
  } else if (settings.provider === 'gemini') {
    const url = (settings.baseUrl || 'https://generativelanguage.googleapis.com').replace(/\/$/, '');
    const r = await fetch(
      `${url}/v1beta/models/${encodeURIComponent(settings.model)}:generateContent?key=${encodeURIComponent(settings.apiKey)}`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          systemInstruction: { parts: [{ text: SYSTEM_PROMPT }] },
          contents: [{ role: 'user', parts: [{ text: userPrompt }] }],
          generationConfig: { responseMimeType: 'application/json', temperature: 0.2 },
        }),
      },
    );
    if (!r.ok) throw new Error(`Gemini ${r.status}: ${await r.text()}`);
    const data = await r.json();
    raw = data?.candidates?.[0]?.content?.parts?.[0]?.text ?? '';
  } else if (settings.provider === 'ollama') {
    const url = (settings.baseUrl || 'http://localhost:11434').replace(/\/$/, '');
    const r = await fetch(`${url}/api/chat`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: settings.model,
        format: 'json',
        stream: false,
        messages: [
          { role: 'system', content: SYSTEM_PROMPT },
          { role: 'user', content: userPrompt },
        ],
      }),
    });
    if (!r.ok) throw new Error(`Ollama ${r.status}: ${await r.text()}`);
    const data = await r.json();
    raw = data?.message?.content ?? '';
  }

  if (!raw) throw new Error('Empty response from advisor');
  return JSON.parse(stripCodeFence(raw)) as AdvisorResponse;
}

const PROBE_ORDER: Provider[] = ['openai_responses', 'openai', 'anthropic', 'gemini', 'ollama'];

/** Probe the real advisor request and return the first protocol that works. */
export async function detectAdvisorProvider(
  model: string,
  apiKey: string,
  baseUrl: string,
): Promise<Provider> {
  const candidates: Provider[] = apiKey.trim() ? PROBE_ORDER : ['ollama'];
  const request: AdvisorRequest = {
    path: 'Pinkbin connectivity test',
    size_bytes: 0,
    file_count: 0,
    top_extensions: [],
    sample_paths: [],
    neighbors: [],
    scaffold_hint: null,
  };
  const failures: string[] = [];

  for (const provider of candidates) {
    try {
      await callAdvisor({ provider, model, apiKey, baseUrl }, request);
      return provider;
    } catch {
      // Do not surface upstream response bodies here: they can be noisy and may
      // contain provider-specific diagnostic details. The protocol name is
      // enough to explain the aggregate failure without risking secret leakage.
      failures.push(provider);
    }
  }

  throw new Error(`未探测到可用 AI 协议（已尝试：${failures.join('、')}）`);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function modelOption(idValue: unknown, labelValue: unknown): AdvisorModel | null {
  if (typeof idValue !== 'string') return null;
  const id = idValue.trim();
  if (!id) return null;
  const label = typeof labelValue === 'string' ? labelValue.trim() : '';
  return label && label !== id ? { id, label } : { id };
}

function dedupeModels(models: AdvisorModel[]): AdvisorModel[] {
  const seen = new Set<string>();
  return models.filter((model) => {
    if (seen.has(model.id)) return false;
    seen.add(model.id);
    return true;
  });
}

function parseOpenAIModels(value: unknown): AdvisorModel[] {
  const data = asRecord(value)?.data;
  if (!Array.isArray(data)) throw new Error('invalid OpenAI model response');
  return dedupeModels(data
    .map((entry) => {
      const record = asRecord(entry);
      return modelOption(record?.id, record?.display_name);
    })
    .filter((model): model is AdvisorModel => Boolean(model)));
}

function parseAnthropicModels(value: unknown): AdvisorModel[] {
  const data = asRecord(value)?.data;
  if (!Array.isArray(data)) throw new Error('invalid Anthropic model response');
  return dedupeModels(data
    .map((entry) => {
      const record = asRecord(entry);
      return modelOption(record?.id, record?.display_name);
    })
    .filter((model): model is AdvisorModel => Boolean(model)));
}

function parseGeminiModels(value: unknown): AdvisorModel[] {
  const data = asRecord(value)?.models;
  if (!Array.isArray(data)) throw new Error('invalid Gemini model response');
  return dedupeModels(data
    .map((entry) => {
      const record = asRecord(entry);
      const methods = record?.supportedGenerationMethods;
      if (!Array.isArray(methods) || !methods.includes('generateContent')) return null;
      const option = modelOption(record?.name, record?.displayName);
      if (!option) return null;
      return option.id.startsWith('models/') ? { ...option, id: option.id.slice(7) } : option;
    })
    .filter((model): model is AdvisorModel => Boolean(model)));
}

function parseOllamaModels(value: unknown): AdvisorModel[] {
  const data = asRecord(value)?.models;
  if (!Array.isArray(data)) throw new Error('invalid Ollama model response');
  return dedupeModels(data
    .map((entry) => {
      const record = asRecord(entry);
      return modelOption(record?.name ?? record?.model, undefined);
    })
    .filter((model): model is AdvisorModel => Boolean(model)));
}

async function tryModelList(
  name: string,
  url: string,
  init: RequestInit,
  parse: (value: unknown) => AdvisorModel[],
  failures: string[],
): Promise<AdvisorModel[] | null> {
  try {
    const response = await fetch(url, init);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return parse(await response.json() as unknown);
  } catch {
    failures.push(name);
    return null;
  }
}

/** Browser-preview counterpart to the desktop model-list command. */
export async function listAdvisorModels(apiKey: string, baseUrl: string): Promise<AdvisorModel[]> {
  const base = baseUrl.trim().replace(/\/$/, '');
  if (!base) throw new Error('Base URL 不能为空');

  const key = apiKey.trim();
  const failures: string[] = [];
  if (key) {
    const openai = await tryModelList(
      'OpenAI /models',
      `${base}/models`,
      { headers: { Authorization: `Bearer ${key}` } },
      parseOpenAIModels,
      failures,
    );
    if (openai) return openai;

    const anthropic = await tryModelList(
      'Anthropic /v1/models',
      `${base}/v1/models`,
      {
        headers: {
          'x-api-key': key,
          'anthropic-version': '2023-06-01',
          'anthropic-dangerous-direct-browser-access': 'true',
        },
      },
      parseAnthropicModels,
      failures,
    );
    if (anthropic) return anthropic;

    const gemini = await tryModelList(
      'Gemini /v1beta/models',
      `${base}/v1beta/models?key=${encodeURIComponent(key)}`,
      {},
      parseGeminiModels,
      failures,
    );
    if (gemini) return gemini;
  }

  const ollama = await tryModelList(
    'Ollama /api/tags',
    `${base}/api/tags`,
    {},
    parseOllamaModels,
    failures,
  );
  if (ollama) return ollama;

  throw new Error(`未获取到可用模型列表（已尝试：${failures.join('、')}）`);
}

export function isConfigured(s: AdvisorSettings | null): s is AdvisorSettings {
  if (!s) return false;
  if (s.provider === 'ollama') return Boolean(s.model);
  return Boolean(s.apiKey && s.model);
}

const CHAT_SYSTEM = `You are Pinkbin's AI advisor — a friendly assistant that helps users figure out what their disk folders are and whether to delete them. Use the metadata you are given (the user's question references a folder by its path, size, samples). Be concise (2-4 sentences), in the user's language. If you suggest deleting, say what to delete (the whole folder vs a sub-scope) and via what mechanism (回收站 / 手动整理 / 卸载应用). Never recommend rm -rf on system paths.`;

const OVERVIEW_SYSTEM = `You are Pinkbin's AI advisor. The user just finished scanning their disk. You receive a JSON summary of the largest folders. Write a friendly Chinese overview (~180-220 字) covering, in order, with empty lines between sections:

【整体】 一句话概括磁盘的整体结构（操作系统 / 用户数据 / 应用 各占多少）。

【这里都有什么】 点名 4-6 个最大的目录，每个一行：名字、大小、大致是什么 / 哪个软件的。要具体到软件名（例：WeChat Files = 微信聊天记录、node_modules = npm 包、HuggingFace = 模型权重）。

【可以删的】 直接列出 2-4 项可以删 / 可以清理的东西，每条说清楚 ① 路径或名字 ② 删了会怎样 ③ 怎么删（回收 / 卸载 / 跑脚本）。如果某个东西看起来可以删但有风险，就不要列在这里。

【不要动】 简短提一下扫描里看到的不该动的东西（系统目录 / 用户文档），一行带过。

口语化中文，不要 markdown bullet（用纯文本换行就行），不要客套话。`;

const CLEANUP_LIST_SYSTEM = `You are Pinkbin's conservative cleanup triage assistant. Return strict JSON only:
{
  "summary": "short Chinese summary",
  "candidates": [
    {
      "path": "one exact path copied from scan_context.top_entries or scan_context.known_scaffolds.matches",
      "risk": "low|medium|high",
      "status": "preview|inspect|keep",
      "reason": "short Chinese reason"
    }
  ]
}

Rules:
- Only use exact paths present in the supplied scan context. Never invent a path or software.
- A detected scaffold may be status=preview only when cleanup_supported=true; it is only a Studio preview, never direct deletion.
- A detected scaffold with cleanup_supported=false is evidence of discovery only and must stay status=inspect or keep.
- System directories and user content are status=keep.
- Unknown cache/build-looking directories are status=inspect unless an audited scaffold proves the cleanup scope.
- If evidence is not enough, return an empty candidates list and explain why in summary.
- Do not output PowerShell, rm, shell commands, or instructions to directly delete a directory.`;

export interface ChatImage {
  /** Full data URL (e.g. `data:image/png;base64,...`). */
  dataUrl: string;
  /** Mime type — `image/png`, `image/jpeg`, etc. Used by Anthropic / Gemini
   *  which need it as a separate field. */
  mimeType: string;
}

function dataUrlBase64(dataUrl: string): string {
  const i = dataUrl.indexOf(',');
  return i >= 0 ? dataUrl.slice(i + 1) : dataUrl;
}

export async function overviewChat(summary: object): Promise<string> {
  const evidenceRules = `
Evidence rules for this response:
- The root path, root size, and root file count in scan_context are authoritative.
- top_entries is a ranked visible sample. Never describe the first child as the whole scan.
- largest_directory is the largest visible child directory and includes its complete path; do not call it the whole scan root.
- Only mention an application or cleanup capability when it appears in top_entries or known_scaffolds.
- known_scaffolds.cleanup_supported=false means the app was detected but has no safe cleanup script in this version.
- Never ask the user to provide the directory list; the scan context is already available.`;
  return runChatRaw(`${OVERVIEW_SYSTEM}${evidenceRules}`, JSON.stringify(summary, null, 2));
}

export async function freeChat(
  request: FreeChatRequest,
): Promise<string> {
  const blocks = [
    request.scanContext ? `本次扫描上下文（必须优先使用）：\n${JSON.stringify(request.scanContext, null, 2)}` : '',
    request.history && request.history.length > 0
      ? `最近对话（仅用于理解上下文，不要把其中的猜测当成扫描事实）：\n${JSON.stringify(request.history, null, 2)}`
      : '',
    request.context ?? '',
    `用户的问题：${request.userMessage}`,
  ].filter(Boolean);
  const chatRules = `
Additional rules:
- When scan_context exists, answer from it and do not ask for a directory list.
- Never invent paths, applications, or cleanup support.
- AI is advisory only; never output PowerShell, rm, shell commands, or direct deletion instructions.`;
  return runChatRaw(`${CHAT_SYSTEM}${chatRules}`, blocks.join('\n\n'), request.images);
}

export interface FreeChatRequest {
  context?: string;
  userMessage: string;
  scanContext?: ScanContext | null;
  history?: ChatHistoryItem[];
  images?: ChatImage[];
}

export interface CleanupChatRequest {
  context: ScanContext;
  intent: string;
  history?: ChatHistoryItem[];
}

export async function cleanupChat(request: CleanupChatRequest): Promise<CleanupCandidateResponse> {
  const raw = await runChatRaw(
    CLEANUP_LIST_SYSTEM,
    JSON.stringify({
      scan_context: request.context,
      recent_history: request.history ?? [],
      task: request.intent,
    }, null, 2),
  );
  const parsed = JSON.parse(stripCodeFence(raw)) as unknown;
  return normalizeCleanupResponse(parsed, request.context);
}

async function runChatRaw(system: string, user: string, images?: ChatImage[]): Promise<string> {
  const settings = loadSettings();
  if (!isConfigured(settings)) {
    throw new Error('AI 未配置 — 在右上角的设置里填一个 API key');
  }
  const fullUser = user;
  const imgs = images ?? [];

  if (settings.provider === 'openai_responses') {
    const url = (settings.baseUrl || 'https://api.openai.com/v1').replace(/\/$/, '');
    const userContent: unknown = imgs.length === 0
      ? fullUser
      : [
          { type: 'input_text', text: fullUser },
          ...imgs.map((img) => ({ type: 'input_image', image_url: img.dataUrl })),
        ];
    const r = await fetch(`${url}/responses`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${settings.apiKey}` },
      body: JSON.stringify({
        model: settings.model,
        instructions: system,
        input: [{ role: 'user', content: userContent }],
        store: false,
      }),
    });
    if (!r.ok) throw new Error(`Responses ${r.status}: ${await r.text()}`);
    const data = await r.json();
    return data?.output_text?.trim() ?? '';
  }
  if (settings.provider === 'openai') {
    const url = (settings.baseUrl || 'https://api.openai.com/v1').replace(/\/$/, '');
    const userContent: unknown = imgs.length === 0
      ? fullUser
      : [
          { type: 'text', text: fullUser },
          ...imgs.map((img) => ({ type: 'image_url', image_url: { url: img.dataUrl } })),
        ];
    const r = await fetch(`${url}/chat/completions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${settings.apiKey}` },
      body: JSON.stringify({
        model: settings.model,
        messages: [
          { role: 'system', content: system },
          { role: 'user', content: userContent },
        ],
      }),
    });
    if (!r.ok) throw new Error(`OpenAI ${r.status}: ${await r.text()}`);
    const data = await r.json();
    return data?.choices?.[0]?.message?.content?.trim() ?? '';
  }
  if (settings.provider === 'anthropic') {
    const url = (settings.baseUrl || 'https://api.anthropic.com').replace(/\/$/, '');
    const userContent: unknown = imgs.length === 0
      ? fullUser
      : [
          ...imgs.map((img) => ({
            type: 'image',
            source: { type: 'base64', media_type: img.mimeType, data: dataUrlBase64(img.dataUrl) },
          })),
          { type: 'text', text: fullUser },
        ];
    const r = await fetch(`${url}/v1/messages`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'x-api-key': settings.apiKey,
        'anthropic-version': '2023-06-01',
        'anthropic-dangerous-direct-browser-access': 'true',
      },
      body: JSON.stringify({
        model: settings.model,
        max_tokens: 4096,
        system,
        messages: [{ role: 'user', content: userContent }],
      }),
    });
    if (!r.ok) throw new Error(`Anthropic ${r.status}: ${await r.text()}`);
    const data = await r.json();
    return extractAnthropicText(data);
  }
  if (settings.provider === 'gemini') {
    const url = (settings.baseUrl || 'https://generativelanguage.googleapis.com').replace(/\/$/, '');
    const parts: unknown[] = [{ text: fullUser }];
    for (const img of imgs) {
      parts.push({ inline_data: { mime_type: img.mimeType, data: dataUrlBase64(img.dataUrl) } });
    }
    const r = await fetch(
      `${url}/v1beta/models/${encodeURIComponent(settings.model)}:generateContent?key=${encodeURIComponent(settings.apiKey)}`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          systemInstruction: { parts: [{ text: system }] },
          contents: [{ role: 'user', parts }],
          generationConfig: { temperature: 0.4 },
        }),
      },
    );
    if (!r.ok) throw new Error(`Gemini ${r.status}: ${await r.text()}`);
    const data = await r.json();
    return data?.candidates?.[0]?.content?.parts?.[0]?.text?.trim() ?? '';
  }
  // ollama — uses `images` field (array of base64) on the message.
  const url = (settings.baseUrl || 'http://localhost:11434').replace(/\/$/, '');
  const userMsg: Record<string, unknown> = { role: 'user', content: fullUser };
  if (imgs.length > 0) userMsg.images = imgs.map((i) => dataUrlBase64(i.dataUrl));
  const r = await fetch(`${url}/api/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: settings.model,
      stream: false,
      messages: [
        { role: 'system', content: system },
        userMsg,
      ],
    }),
  });
  if (!r.ok) throw new Error(`Ollama ${r.status}: ${await r.text()}`);
  const data = await r.json();
  return data?.message?.content?.trim() ?? '';
}
