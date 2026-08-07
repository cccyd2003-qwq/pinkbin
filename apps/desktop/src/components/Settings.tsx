import { useEffect, useState } from 'react';
import { CheckCircle2, Eye, EyeOff, Info, List as ListIcon, X } from 'lucide-react';
import { api } from '../api';
import {
  clearSettings,
  loadSettings,
  saveSettings,
  type AdvisorModel,
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

function endpointSignature(baseUrl: string, apiKey: string): string {
  return `${baseUrl}\u0000${apiKey}`;
}

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  return String(error);
}

export function Settings({ onClose }: Props) {
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [model, setModel] = useState('');
  const [models, setModels] = useState<AdvisorModel[]>([]);
  const [showKey, setShowKey] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [fetchingModels, setFetchingModels] = useState(false);
  const [saving, setSaving] = useState(false);
  const [detectedProvider, setDetectedProvider] = useState<Provider | null>(null);
  const [probedEndpoint, setProbedEndpoint] = useState<string | null>(null);
  const busy = fetchingModels || saving;

  useEffect(() => {
    const existing = loadSettings();
    if (existing) {
      const existingBaseUrl = existing.baseUrl ?? '';
      const existingApiKey = existing.apiKey ?? '';
      setModel(existing.model);
      setApiKey(existingApiKey);
      setBaseUrl(existingBaseUrl);
      setDetectedProvider(existing.provider);
      setProbedEndpoint(endpointSignature(existingBaseUrl, existingApiKey));
      setSaved(true);
    }
  }, []);

  const invalidateEndpointProbe = () => {
    setDetectedProvider(null);
    setProbedEndpoint(null);
    setModels([]);
    setSaved(false);
    setMsg(null);
    setErr(null);
  };

  const markModelDirty = () => {
    setSaved(false);
    setMsg(null);
    setErr(null);
  };

  const validate = (requireModel: boolean): string | null => {
    const trimmedBaseUrl = baseUrl.trim();
    if (!trimmedBaseUrl) return '请填 Base URL';
    try {
      const url = new URL(trimmedBaseUrl);
      if (url.protocol !== 'http:' && url.protocol !== 'https:') return 'Base URL 必须使用 http 或 https';
    } catch {
      return '请输入合法的 Base URL';
    }
    if (requireModel && !model.trim()) return '请填 Model 名';
    return null;
  };

  const fetchModels = async () => {
    setErr(null);
    setMsg(null);
    const validationError = validate(false);
    if (validationError) {
      setErr(validationError);
      return;
    }

    const cleanBaseUrl = baseUrl.trim().replace(/\/$/, '');
    const cleanApiKey = apiKey.trim();
    const typedModel = model.trim();

    try {
      setFetchingModels(true);
      setModels([]);
      const fetchedModels = await api.listAdvisorModels(cleanBaseUrl, cleanApiKey || undefined);
      if (fetchedModels.length === 0) {
        throw new Error('服务商返回的模型列表为空');
      }
      setModels(fetchedModels);

      // Probe with a known model from the service when the manually entered
      // value is not part of the returned list. The custom value remains in
      // the input and can still be saved after the provider is identified.
      const probeModel = fetchedModels.some((item) => item.id === typedModel)
        ? typedModel
        : fetchedModels[0].id;
      const provider = await api.detectAndSetAdvisor(
        probeModel,
        cleanApiKey || undefined,
        cleanBaseUrl,
      );

      setBaseUrl(cleanBaseUrl);
      setApiKey(cleanApiKey);
      setModel(typedModel || fetchedModels[0].id);
      setDetectedProvider(provider);
      setProbedEndpoint(endpointSignature(cleanBaseUrl, cleanApiKey));
      setSaved(false);
      setMsg(`已获取 ${fetchedModels.length} 个模型 · 已识别 ${PROVIDER_LABEL[provider]}`);
    } catch (error) {
      setDetectedProvider(null);
      setProbedEndpoint(null);
      setErr(`获取模型列表失败：${errorText(error)}`);
    } finally {
      setFetchingModels(false);
    }
  };

  const save = async () => {
    setErr(null);
    setMsg(null);
    const validationError = validate(true);
    if (validationError) {
      setErr(validationError);
      return;
    }

    const cleanBaseUrl = baseUrl.trim().replace(/\/$/, '');
    const cleanModel = model.trim();
    const cleanApiKey = apiKey.trim();
    if (!detectedProvider) {
      setErr('请先点击“获取模型列表”完成协议探测');
      return;
    }
    if (probedEndpoint !== endpointSignature(cleanBaseUrl, cleanApiKey)) {
      setErr('Base URL 或 API Key 已修改，请重新获取模型列表');
      return;
    }

    try {
      setSaving(true);
      await api.configureAdvisor(
        detectedProvider,
        cleanModel,
        cleanApiKey || undefined,
        cleanBaseUrl,
      );
      saveSettings({
        provider: detectedProvider,
        model: cleanModel,
        apiKey: cleanApiKey,
        baseUrl: cleanBaseUrl,
      });
      setSaved(true);
      onClose();
    } catch (error) {
      setErr(`保存失败：${errorText(error)}`);
    } finally {
      setSaving(false);
    }
  };

  const wipe = () => {
    clearSettings();
    setApiKey('');
    setBaseUrl('');
    setModel('');
    setModels([]);
    setDetectedProvider(null);
    setProbedEndpoint(null);
    setSaved(false);
    setMsg('已清除本地保存的配置');
    setErr(null);
  };

  return (
    <div className="modal-bg" onClick={busy ? undefined : onClose}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="modal-head">
          <div>AI 顾问设置 {saved && <CheckCircle2 size={16} style={{ verticalAlign: 'middle', marginLeft: 6, color: 'var(--pink-deep)' }} />}</div>
          <button className="ghost icon" onClick={onClose} disabled={busy}><X size={16} /></button>
        </div>

        <p className="hint">
          <Info size={12} />
          <span>点击“获取模型列表”会读取服务商支持的模型并探测可用协议；保存只写入本机配置并关闭窗口。Ollama 可不填 Key。</span>
        </p>

        <label className="field">
          <span>Base URL</span>
          <input
            value={baseUrl}
            onChange={(event) => {
              setBaseUrl(event.target.value);
              invalidateEndpointProbe();
            }}
            placeholder="https://api.openai.com/v1"
            disabled={busy}
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
              onChange={(event) => {
                setApiKey(event.target.value);
                invalidateEndpointProbe();
              }}
              placeholder="sk-..."
              style={{ flex: 1 }}
              disabled={busy}
            />
            <button
              type="button"
              className="ghost icon"
              onClick={() => setShowKey((value) => !value)}
              title={showKey ? '隐藏' : '显示'}
              disabled={busy}
            >
              {showKey ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          </div>
        </label>

        <label className="field">
          <span>Model · 模型名</span>
          <div className="model-row">
            <input
              list={models.length > 0 ? 'advisor-model-list' : undefined}
              value={model}
              onChange={(event) => {
                setModel(event.target.value);
                markModelDirty();
              }}
              placeholder="gpt-4o-mini · deepseek-chat · claude-haiku-4-5 …"
              disabled={busy}
            />
            <button type="button" className="ghost model-fetch" onClick={fetchModels} disabled={busy}>
              <ListIcon size={14} />
              {fetchingModels ? '获取中…' : '获取模型列表'}
            </button>
          </div>
          {models.length > 0 && (
            <>
              <datalist id="advisor-model-list">
                {models.map((item) => <option key={item.id} value={item.id} label={item.label} />)}
              </datalist>
              <span className="model-list-meta">已加载 {models.length} 个模型，可直接选择或手动输入</span>
            </>
          )}
        </label>

        {msg && <div className="ok">{msg}</div>}
        {err && <div className="error">{err}</div>}

        <div className="modal-actions">
          {saved && <button className="ghost" onClick={wipe} disabled={busy}>清除</button>}
          <button className="primary" onClick={save} disabled={busy}>{saving ? '正在保存…' : '保存'}</button>
          <button className="ghost" onClick={onClose} disabled={busy}>关闭</button>
        </div>

        <p className="muted small" style={{ marginTop: 4 }}>
          Pinkbin 只把目录元数据发给 AI（路径、大小、文件数、扩展名分布、抽样路径），<strong>不会</strong>读取或上传文件内容。
        </p>
      </div>
    </div>
  );
}
