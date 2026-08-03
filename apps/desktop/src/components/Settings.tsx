import { useEffect, useState } from 'react';
import { X, CheckCircle2, Info, Eye, EyeOff, Settings2, Cloud } from 'lucide-react';
import { api } from '../api';
import { isTauri } from '../env';
import {
  loadSettings,
  saveSettings,
  clearSettings,
  detectProvider,
  type Provider,
} from '../advisorClient';

type Props = { onClose: () => void };

const PROVIDER_LABEL: Record<Provider, string> = {
  openai: 'OpenAI 兼容',
  atlas: 'Atlas Cloud',
  anthropic: 'Anthropic',
  gemini: 'Gemini',
  ollama: 'Ollama（本地，免 Key）',
};

// 手动开关里的选项要短，六个塞在一行；「已识别」标签用完整版说明。
const PROVIDER_LABEL_SHORT: Record<Provider, string> = {
  openai: 'OpenAI 兼容',
  atlas: 'Atlas Cloud',
  anthropic: 'Anthropic',
  gemini: 'Gemini',
  ollama: 'Ollama',
};

export function Settings({ onClose }: Props) {
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [model, setModel] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  // undefined = 自动识别；手动指定时存具体 Provider。默认收起，不常驻展示。
  const [providerOverride, setProviderOverride] = useState<Provider | undefined>(undefined);
  const [showManual, setShowManual] = useState(false);

  useEffect(() => {
    const existing = loadSettings();
    if (existing) {
      setModel(existing.model);
      setApiKey(existing.apiKey);
      setBaseUrl(existing.baseUrl);
      setProviderOverride(existing.providerOverride);
      setSaved(true);
    }
  }, []);

  const provider = providerOverride ?? detectProvider(baseUrl);
  const needsKey = provider !== 'ollama';

  const selectProvider = (nextProvider: Provider) => {
    setProviderOverride(nextProvider);
    if (nextProvider === 'atlas') {
      setBaseUrl('https://api.atlascloud.ai/v1');
      setModel('deepseek-ai/deepseek-v4-pro');
    }
  };

  const save = async () => {
    setErr(null); setMsg(null);
    if (!baseUrl.trim()) { setErr('请填 Base URL'); return; }
    if (!model.trim())   { setErr('请填 Model 名'); return; }
    if (needsKey && !apiKey.trim()) { setErr('请填 API Key'); return; }
    try {
      saveSettings({ provider, model, apiKey, baseUrl, providerOverride });
      if (isTauri) {
        await api.setAdvisor(provider, model, needsKey ? apiKey : undefined, baseUrl);
      }
      setMsg('已保存 · key 只存在你本机 localStorage');
      setSaved(true);
    } catch (e) {
      setErr(String(e));
    }
  };

  const wipe = () => {
    clearSettings();
    setApiKey('');
    setBaseUrl('');
    setModel('');
    setProviderOverride(undefined);
    setShowManual(false);
    setSaved(false);
    setMsg('已清除本地保存的配置');
  };

  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>AI 顾问设置 {saved && <CheckCircle2 size={16} style={{ verticalAlign: 'middle', marginLeft: 6, color: 'var(--pink-deep)' }} />}</div>
          <button className="ghost icon" onClick={onClose}><X size={16} /></button>
        </div>

        <p className="hint">
          <Info size={12} />
          <span>填你服务商给你的 Base URL、API Key 和模型名。OpenAI、Atlas Cloud、DeepSeek、Kimi、各种中转都直接填就能用；本地 Ollama 不用 Key。</span>
        </p>

        <div className="provider-presets">
          <button type="button" className="ghost" onClick={() => selectProvider('atlas')}>
            <Cloud size={13} />
            Atlas Cloud
          </button>
        </div>

        <label className="field">
          <span>Base URL</span>
          <input
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://api.openai.com/v1"
          />
        </label>

        {baseUrl.trim() && (
          <div className="provider-detect">
            <span className="badge">
              已识别：{PROVIDER_LABEL[provider]}
              {providerOverride && ' · 手动'}
            </span>
            <button type="button" className="provider-detect-toggle" onClick={() => setShowManual((v) => !v)}>
              <Settings2 size={11} />
              手动指定
            </button>
          </div>
        )}

        {baseUrl.trim() && showManual && (
          <div className="seg seg-6">
            <button
              type="button"
              className={`seg-opt${providerOverride === undefined ? ' active' : ''}`}
              onClick={() => setProviderOverride(undefined)}
            >
              自动
            </button>
            {(Object.keys(PROVIDER_LABEL) as Provider[]).map((p) => (
              <button
                key={p}
                type="button"
                className={`seg-opt${providerOverride === p ? ' active' : ''}`}
                onClick={() => selectProvider(p)}
              >
                {PROVIDER_LABEL_SHORT[p]}
              </button>
            ))}
          </div>
        )}

        {needsKey && (
          <label className="field">
            <span>API Key（只存本机，永不上传）</span>
            <div style={{ display: 'flex', gap: 6 }}>
              <input
                type={showKey ? 'text' : 'password'}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder="sk-..."
                style={{ flex: 1 }}
              />
              <button
                type="button"
                className="ghost icon"
                onClick={() => setShowKey((v) => !v)}
                title={showKey ? '隐藏' : '显示'}
              >
                {showKey ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
            </div>
          </label>
        )}

        <label className="field">
          <span>Model · 模型名</span>
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="gpt-4o-mini · deepseek-chat · claude-haiku-4-5 …"
          />
        </label>

        {msg && <div className="ok">{msg}</div>}
        {err && <div className="error">{err}</div>}

        <div className="modal-actions">
          {saved && <button className="ghost" onClick={wipe}>清除</button>}
          <button className="primary" onClick={save}>保存</button>
          <button className="ghost" onClick={onClose}>关闭</button>
        </div>

        <p className="muted small" style={{ marginTop: 4 }}>
          Pinkbin 只把目录元数据发给 AI（路径、大小、文件数、扩展名分布、抽样路径），<strong>不会</strong>读取或上传文件内容。
        </p>
      </div>
    </div>
  );
}
