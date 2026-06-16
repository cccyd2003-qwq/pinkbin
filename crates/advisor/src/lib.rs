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
    let user_prompt = serde_json::to_string_pretty(req)?;
    let raw = complete_text(provider, SYSTEM, &user_prompt, ResponseMode::Json).await?;

    let parsed: AdvisorResponse =
        serde_json::from_str(&raw).or_else(|_| serde_json::from_str(strip_codefence(&raw)))?;
    Ok(parsed)
}

pub async fn chat(provider: &Provider, system: &str, user: &str) -> anyhow::Result<String> {
    let text = complete_text(provider, system, user, ResponseMode::Plain)
        .await?
        .trim()
        .to_string();
    if text.is_empty() {
        anyhow::bail!("empty response from advisor chat");
    }
    Ok(text)
}

#[derive(Clone, Copy)]
enum ResponseMode {
    Json,
    Plain,
}

async fn complete_text(
    provider: &Provider,
    system: &str,
    user_prompt: &str,
    mode: ResponseMode,
) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let raw = match provider {
        Provider::OpenAI {
            api_key,
            model,
            base_url,
        } => {
            let mut body = serde_json::json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user",   "content": user_prompt }
                ]
            });
            if matches!(mode, ResponseMode::Json) {
                body["response_format"] = serde_json::json!({ "type": "json_object" });
            }
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
            extract_openai_text(&v)?.to_string()
        }
        Provider::Anthropic {
            api_key,
            model,
            base_url,
        } => {
            let body = serde_json::json!({
                "model": model,
                "max_tokens": if matches!(mode, ResponseMode::Json) { 2048 } else { 4096 },
                "system": system,
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
            extract_anthropic_text(&v)?
        }
        Provider::Gemini {
            api_key,
            model,
            base_url,
        } => {
            let generation_config = match mode {
                ResponseMode::Json => serde_json::json!({
                    "responseMimeType": "application/json",
                    "temperature": 0.2
                }),
                ResponseMode::Plain => serde_json::json!({ "temperature": 0.4 }),
            };
            let body = serde_json::json!({
                "systemInstruction": { "parts": [{ "text": system }] },
                "contents": [{ "role": "user", "parts": [{ "text": user_prompt }] }],
                "generationConfig": generation_config
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
            extract_gemini_text(&v)?.to_string()
        }
        Provider::Ollama { base_url, model } => {
            let mut body = serde_json::json!({
                "model": model,
                "stream": false,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user",   "content": user_prompt }
                ]
            });
            if matches!(mode, ResponseMode::Json) {
                body["format"] = serde_json::json!("json");
            }
            let r = client
                .post(format!("{}/api/chat", base_url.trim_end_matches('/')))
                .json(&body)
                .send()
                .await?
                .error_for_status()?;
            let v: serde_json::Value = r.json().await?;
            extract_ollama_text(&v)?.to_string()
        }
    };
    Ok(raw)
}

fn extract_openai_text(v: &serde_json::Value) -> anyhow::Result<&str> {
    v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("openai: missing message.content"))
}

fn extract_anthropic_text(v: &serde_json::Value) -> anyhow::Result<String> {
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
    Ok(text)
}

fn extract_gemini_text(v: &serde_json::Value) -> anyhow::Result<&str> {
    v["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("gemini: missing candidates[0].content.parts[0].text"))
}

fn extract_ollama_text(v: &serde_json::Value) -> anyhow::Result<&str> {
    v["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("ollama: missing message.content"))
}

fn strip_codefence(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_json_code_fence() {
        assert_eq!(
            strip_codefence("```json\n{\"ok\":true}\n```"),
            "{\"ok\":true}"
        );
        assert_eq!(strip_codefence("```\nhello\n```"), "hello");
    }

    #[test]
    fn extracts_text_from_provider_shapes() {
        let openai = serde_json::json!({
            "choices": [{ "message": { "content": "openai text" } }]
        });
        assert_eq!(extract_openai_text(&openai).unwrap(), "openai text");

        let anthropic = serde_json::json!({
            "content": [
                { "type": "thinking", "thinking": "hidden" },
                { "type": "text", "text": "hello " },
                { "type": "text", "text": "world" }
            ]
        });
        assert_eq!(extract_anthropic_text(&anthropic).unwrap(), "hello world");

        let gemini = serde_json::json!({
            "candidates": [{ "content": { "parts": [{ "text": "gemini text" }] } }]
        });
        assert_eq!(extract_gemini_text(&gemini).unwrap(), "gemini text");

        let ollama = serde_json::json!({ "message": { "content": "ollama text" } });
        assert_eq!(extract_ollama_text(&ollama).unwrap(), "ollama text");
    }

    #[test]
    fn anthropic_empty_text_reports_stop_reason() {
        let value = serde_json::json!({
            "stop_reason": "max_tokens",
            "content": [{ "type": "thinking", "thinking": "..." }]
        });
        let err = extract_anthropic_text(&value).unwrap_err().to_string();
        assert!(err.contains("stop_reason=max_tokens"));
    }
}
