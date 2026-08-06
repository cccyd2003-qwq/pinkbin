//! AI advisor: provider-agnostic, JSON-mode structured output.
//!
//! Sends only directory metadata and sample paths — never file contents.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtShare {
    pub ext: String,
    pub share: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorRequest {
    pub path: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub top_extensions: Vec<ExtShare>,
    pub sample_paths: Vec<String>,
    pub neighbors: Vec<String>,
    pub scaffold_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdvisorResponse {
    pub what: String,
    pub category: String,
    pub safe_to_delete: bool,
    pub risk: String,
    pub action: String,
    pub reasoning: String,
    #[serde(default)]
    pub needs_inspection: bool,
    #[serde(default)]
    pub suggested_scaffold: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Provider {
    OpenAI {
        api_key: String,
        model: String,
        base_url: String,
    },
    Anthropic {
        api_key: String,
        model: String,
        base_url: String,
    },
    Ollama {
        base_url: String,
        model: String,
    },
    Gemini {
        api_key: String,
        model: String,
        base_url: String,
    },
}

const SYSTEM: &str = r#"You are Pinkbin's local file advisor. Given a folder's metadata, decide what it is and whether it can be cleaned. Reply in strict JSON ONLY, matching this schema exactly:

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
- Do not include any prose outside the JSON object."#;

pub async fn advise(provider: &Provider, req: &AdvisorRequest) -> anyhow::Result<AdvisorResponse> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let user_prompt = serde_json::to_string_pretty(req)?;

    let raw = match provider {
        Provider::OpenAI {
            api_key,
            model,
            base_url,
        } => {
            let body = serde_json::json!({
                "model": model,
                "response_format": { "type": "json_object" },
                "messages": [
                    { "role": "system", "content": SYSTEM },
                    { "role": "user",   "content": user_prompt }
                ]
            });
            let r = client
                .post(format!(
                    "{}/chat/completions",
                    base_url.trim_end_matches('/')
                ))
                .bearer_auth(api_key)
                .json(&body)
                .send()
                .await?
                .error_for_status()?;
            let v: serde_json::Value = r.json().await?;
            v["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("openai: missing message.content"))?
                .to_string()
        }
        Provider::Anthropic {
            api_key,
            model,
            base_url,
        } => {
            let body = serde_json::json!({
                "model": model,
                "max_tokens": 2048,
                "system": SYSTEM,
                "messages": [{ "role": "user", "content": user_prompt }]
            });
            let r = client
                .post(format!("{}/v1/messages", base_url.trim_end_matches('/')))
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await?
                .error_for_status()?;
            let v: serde_json::Value = r.json().await?;
            // extended-thinking 模型先返 {type:"thinking",...} 再返 {type:"text",...},
            // 不能假设 content[0] 是 text；遍历 content 数组拼所有 text block。
            let text = v["content"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter(|b| b["type"] == "text")
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            if text.trim().is_empty() {
                let stop = v["stop_reason"].as_str().unwrap_or("unknown");
                if stop == "max_tokens" {
                    anyhow::bail!(
                        "anthropic: 没拿到 text block (stop_reason=max_tokens) — 模型在 thinking 阶段被截断, 把 max_tokens 调大重试"
                    );
                }
                anyhow::bail!("anthropic: 没拿到 text block (stop_reason={stop})");
            }
            text
        }
        Provider::Gemini {
            api_key,
            model,
            base_url,
        } => {
            let body = serde_json::json!({
                "systemInstruction": { "parts": [{ "text": SYSTEM }] },
                "contents": [{ "role": "user", "parts": [{ "text": user_prompt }] }],
                "generationConfig": {
                    "responseMimeType": "application/json",
                    "temperature": 0.2
                }
            });
            let url = format!(
                "{}/v1beta/models/{}:generateContent?key={}",
                base_url.trim_end_matches('/'),
                model,
                api_key
            );
            let r = client
                .post(url)
                .json(&body)
                .send()
                .await?
                .error_for_status()?;
            let v: serde_json::Value = r.json().await?;
            v["candidates"][0]["content"]["parts"][0]["text"]
                .as_str()
                .ok_or_else(|| {
                    anyhow::anyhow!("gemini: missing candidates[0].content.parts[0].text")
                })?
                .to_string()
        }
        Provider::Ollama { base_url, model } => {
            let body = serde_json::json!({
                "model": model,
                "format": "json",
                "stream": false,
                "messages": [
                    { "role": "system", "content": SYSTEM },
                    { "role": "user",   "content": user_prompt }
                ]
            });
            let r = client
                .post(format!("{}/api/chat", base_url.trim_end_matches('/')))
                .json(&body)
                .send()
                .await?
                .error_for_status()?;
            let v: serde_json::Value = r.json().await?;
            v["message"]["content"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("ollama: missing message.content"))?
                .to_string()
        }
    };

    let parsed: AdvisorResponse =
        serde_json::from_str(&raw).or_else(|_| serde_json::from_str(strip_codefence(&raw)))?;
    Ok(parsed)
}

/// Sends the same smallest useful request as the real advisor flow, so the
/// settings dialog can verify both the endpoint credentials and model.
pub async fn test_connection(provider: &Provider) -> anyhow::Result<()> {
    let req = AdvisorRequest {
        path: "Pinkbin connectivity test".to_string(),
        size_bytes: 0,
        file_count: 0,
        top_extensions: Vec::new(),
        sample_paths: Vec::new(),
        neighbors: Vec::new(),
        scaffold_hint: None,
    };
    advise(provider, &req).await.map(|_| ())
}

fn strip_codefence(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}
