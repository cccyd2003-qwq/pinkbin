import { useEffect, useState } from 'react';
import { X, CheckCircle2, Info, Eye, EyeOff } from 'lucide-react';
import { api } from '../api';
import {
  loadSettings,
  saveSettings,
  clearSettings,
  type Provider,
} from '../advisorClient';

type Props = { onClose: () => void };

const PROVIDER_LABEL: Record<Provider, string> = {
  openai_responses: 'OpenAI Responses',
  openai: 'OpenAI 兼容',
  anthropic: 'Anthropic',
  gemini: 'Gemini',
  ollama: 'Ollama（本地，免 Key）',
};

export function Settings({ onClose }: Props) {
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [model, setModel] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [detectedProvider, setDetectedProvider] = useState<Provider | null>(null);

  useEffect(() => {
    const existing = loadSettings();
    if (existing) {
      setModel(existing.model);
      setApiKey(existing.apiKey);
      setBaseUrl(existing.baseUrl);
      setDetectedProvider(existing.provider);
      setSaved(true);
    }
  }, []);

  const validate = (): string | null => {
    const trimmedBaseUrl = baseUrl.trim();
    if (!trimmedBaseUrl) return '请填 Base URL';
    try {
      const url = new URL(trimmedBaseUrl);
      if (url.protocol !== 'http:' && url.protocol !== 'https:') return 'Base URL 必须使用 http 或 https';
    } catch {
      return '请输入合法的 Base URL';
    }
    if (!model.trim()) return '请填 Model 名';
    return null;
  };

  const save = async () => {
    setErr(null); setMsg(null);
    const validationError = validate();
    if (validationError) { setErr(validationError); return; }
    try {
      setSaving(true);
      const cleanBaseUrl = baseUrl.trim().replace(/\/$/, '');
      const cleanModel = model.trim();
      const cleanApiKey = apiKey.trim();
      const provider = await api.detectAndSetAdvisor(cleanModel, cleanApiKey || undefined, cleanBaseUrl);
      saveSettings({ provider, model: cleanModel, apiKey: cleanApiKey, baseUrl: cleanBaseUrl });
      setBaseUrl(cleanBaseUrl);
      setModel(cleanModel);
      setApiKey(cleanApiKey);
      setDetectedProvider(provider);
      setMsg(`已保存 · 已识别 ${PROVIDER_LABEL[provider]} · Key 只存在你本机`);
      setSaved(true);
    } catch (e) {
      setErr(`探测失败：${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const wipe = () => {
    clearSettings();
    setApiKey('');
    setBaseUrl('');
    setModel('');
    setDetectedProvider(null);
    setSaved(false);
    setMsg('已清除本地保存的配置');
  };

  return (
    <div className="modal-bg" onClick={saving ? undefined : onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>AI 顾问设置 {saved && <CheckCircle2 size={16} style={{ verticalAlign: 'middle', marginLeft: 6, color: 'var(--pink-deep)' }} />}</div>
          <button className="ghost icon" onClick={onClose} disabled={saving}><X size={16} /></button>
        </div>

        <p className="hint">
          <Info size={12} />
          <span>填你服务商给你的 Base URL、API Key 和模型名。保存时会自动探测可用协议；本地 Ollama 可不填 Key。</span>
        </p>

        <label className="field">
          <span>Base URL</span>
          <input
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://api.openai.com/v1"
            disabled={saving}
          />
        </label>

        {detectedProvider && (
          <div className="provider-detect">
            <span className="badge">
              已识别：{PROVIDER_LABEL[detectedProvider]}
            </span>
          </div>
        )}

        <label className="field">
          <span>API Key（只存本机，永不上传；Ollama 可留空）</span>
          <div style={{ display: 'flex', gap: 6 }}>
            <input
              type={showKey ? 'text' : 'password'}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              style={{ flex: 1 }}
              disabled={saving}
            />
            <button
              type="button"
              className="ghost icon"
              onClick={() => setShowKey((v) => !v)}
              title={showKey ? '隐藏' : '显示'}
              disabled={saving}
            >
              {showKey ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          </div>
        </label>

        <label className="field">
          <span>Model · 模型名</span>
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="gpt-4o-mini · deepseek-chat · claude-haiku-4-5 …"
            disabled={saving}
          />
        </label>

        {msg && <div className="ok">{msg}</div>}
        {err && <div className="error">{err}</div>}

        <div className="modal-actions">
          {saved && <button className="ghost" onClick={wipe} disabled={saving}>清除</button>}
          <button className="primary" onClick={save} disabled={saving}>{saving ? '正在探测协议…' : '保存'}</button>
          <button className="ghost" onClick={onClose} disabled={saving}>关闭</button>
        </div>

        <p className="muted small" style={{ marginTop: 4 }}>
          Pinkbin 只把目录元数据发给 AI（路径、大小、文件数、扩展名分布、抽样路径），<strong>不会</strong>读取或上传文件内容。
        </p>
      </div>
    </div>
  );
}
