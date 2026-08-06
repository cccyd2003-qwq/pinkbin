import { useEffect, useState } from 'react';
import { X, CheckCircle2, Info, Eye, EyeOff, Settings2 } from 'lucide-react';
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
  anthropic: 'Anthropic',
  gemini: 'Gemini',
  ollama: 'Ollama（本地，免 Key）',
};

// 手动开关里的选项要短，五个塞在一行；「已识别」标签用完整版说明。
const PROVIDER_LABEL_SHORT: Record<Provider, string> = {
  openai: 'OpenAI 兼容',
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
  const [testing, setTesting] = useState(false);
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

  const validate = (): string | null => {
    if (!baseUrl.trim()) return '请填 Base URL';
    if (!model.trim()) return '请填 Model 名';
    if (needsKey && !apiKey.trim()) return '请填 API Key';
    return null;
  };

  const save = async () => {
    setErr(null); setMsg(null);
    const validationError = validate();
    if (validationError) { setErr(validationError); return; }
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

  const test = async () => {
    setErr(null); setMsg(null);
    const validationError = validate();
    if (validationError) { setErr(validationError); return; }
    setTesting(true);
    try {
      await api.testAdvisor(provider, model, needsKey ? apiKey : undefined, baseUrl);
      setMsg('连接成功 · 当前配置可用');
    } catch (e) {
      setErr(`连接失败：${String(e)}`);
    } finally {
      setTesting(false);
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
          <span>填你服务商给你的 Base URL、API Key 和模型名。OpenAI、DeepSeek、Kimi、各种中转都直接填就能用；本地 Ollama 不用 Key。</span>
        </p>

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
          <div className="seg seg-5">
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
                onClick={() => setProviderOverride(p)}
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
          {saved && <button className="ghost" onClick={wipe} disabled={testing}>清除</button>}
          <button className="secondary" onClick={test} disabled={testing} title="测试当前配置的连通性">
            {testing ? '测试中…' : '测试'}
          </button>
          <button className="primary" onClick={save} disabled={testing}>保存</button>
          <button className="ghost" onClick={onClose}>关闭</button>
        </div>

        <p className="muted small" style={{ marginTop: 4 }}>
          Pinkbin 只把目录元数据发给 AI（路径、大小、文件数、扩展名分布、抽样路径），<strong>不会</strong>读取或上传文件内容。
        </p>
      </div>
    </div>
  );
}
