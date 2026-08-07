//! AI advisor: provider-agnostic, JSON-mode structured output.
//!
//! Sends only directory metadata and sample paths — never file contents.

use std::collections::HashSet;

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
    OpenAIResponses {
        api_key: String,
        model: String,
        base_url: String,
    },
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelOption {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

fn model_option(
    id: Option<&serde_json::Value>,
    label: Option<&serde_json::Value>,
) -> Option<ModelOption> {
    let id = id?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }

    let label = label
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != id)
        .map(str::to_string);

    Some(ModelOption {
        id: id.to_string(),
        label,
    })
}

fn dedupe_models(models: Vec<ModelOption>) -> Vec<ModelOption> {
    let mut seen = HashSet::new();
    models
        .into_iter()
        .filter(|model| seen.insert(model.id.clone()))
        .collect()
}

fn parse_openai_models(value: &serde_json::Value) -> anyhow::Result<Vec<ModelOption>> {
    let data = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing data array"))?;

    Ok(dedupe_models(
        data.iter()
            .filter_map(|entry| model_option(entry.get("id"), entry.get("display_name")))
            .collect(),
    ))
}

fn parse_anthropic_models(value: &serde_json::Value) -> anyhow::Result<Vec<ModelOption>> {
    let data = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing data array"))?;

    Ok(dedupe_models(
        data.iter()
            .filter_map(|entry| model_option(entry.get("id"), entry.get("display_name")))
            .collect(),
    ))
}

fn parse_gemini_models(value: &serde_json::Value) -> anyhow::Result<Vec<ModelOption>> {
    let data = value
        .get("models")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing models array"))?;

    Ok(dedupe_models(
        data.iter()
            .filter_map(|entry| {
                let supports_generate_content = entry
                    .get("supportedGenerationMethods")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|methods| {
                        methods
                            .iter()
                            .any(|method| method.as_str() == Some("generateContent"))
                    });
                if !supports_generate_content {
                    return None;
                }

                let mut option = model_option(entry.get("name"), entry.get("displayName"))?;
                if let Some(id) = option.id.strip_prefix("models/") {
                    option.id = id.to_string();
                }
                Some(option)
            })
            .collect(),
    ))
}

fn parse_ollama_models(value: &serde_json::Value) -> anyhow::Result<Vec<ModelOption>> {
    let data = value
        .get("models")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing models array"))?;

    Ok(dedupe_models(
        data.iter()
            .filter_map(|entry| {
                model_option(entry.get("name").or_else(|| entry.get("model")), None)
            })
            .collect(),
    ))
}

async fn fetch_json(request: reqwest::RequestBuilder) -> anyhow::Result<serde_json::Value> {
    Ok(request.send().await?.error_for_status()?.json().await?)
}

/// Fetches model IDs from the configured endpoint without exposing provider
/// response bodies (which may contain credentials or upstream diagnostics).
pub async fn list_models(
    api_key: Option<&str>,
    base_url: &str,
) -> anyhow::Result<Vec<ModelOption>> {
    let base_url = base_url.trim().trim_end_matches('/');
    if base_url.is_empty() {
        anyhow::bail!("base URL is empty");
    }

    let api_key = api_key.map(str::trim).filter(|key| !key.is_empty());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let mut failures = Vec::new();

    if let Some(api_key) = api_key {
        let result = fetch_json(
            client
                .get(format!("{base_url}/models"))
                .bearer_auth(api_key),
        )
        .await
        .and_then(|value| parse_openai_models(&value));
        match result {
            Ok(models) => return Ok(models),
            Err(_) => failures.push("OpenAI /models"),
        }

        let result = fetch_json(
            client
                .get(format!("{base_url}/v1/models"))
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01"),
        )
        .await
        .and_then(|value| parse_anthropic_models(&value));
        match result {
            Ok(models) => return Ok(models),
            Err(_) => failures.push("Anthropic /v1/models"),
        }

        let result = fetch_json(
            client
                .get(format!("{base_url}/v1beta/models"))
                .query(&[("key", api_key)]),
        )
        .await
        .and_then(|value| parse_gemini_models(&value));
        match result {
            Ok(models) => return Ok(models),
            Err(_) => failures.push("Gemini /v1beta/models"),
        }
    }

    let result = fetch_json(client.get(format!("{base_url}/api/tags")))
        .await
        .and_then(|value| parse_ollama_models(&value));
    match result {
        Ok(models) => Ok(models),
        Err(_) => {
            failures.push("Ollama /api/tags");
            anyhow::bail!("未获取到可用模型列表（已尝试：{}）", failures.join("、"));
        }
    }
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
        Provider::OpenAIResponses {
            api_key,
            model,
            base_url,
        } => {
            let body = serde_json::json!({
                "model": model,
                "instructions": SYSTEM,
                "input": user_prompt,
                "store": false,
            });
            let r = client
                .post(format!("{}/responses", base_url.trim_end_matches('/')))
                .bearer_auth(api_key)
                .json(&body)
                .send()
                .await?
                .error_for_status()?;
            let v: serde_json::Value = r.json().await?;
            v["output_text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("responses: missing output_text"))?
                .to_string()
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_deduplicates_openai_models() {
        let value = serde_json::json!({
            "data": [
                { "id": "gpt-4o", "display_name": "GPT-4o" },
                { "id": "gpt-4o", "display_name": "GPT-4o duplicate" },
                { "id": "  ", "display_name": "empty" }
            ]
        });

        let models = parse_openai_models(&value).expect("valid OpenAI response");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-4o");
        assert_eq!(models[0].label.as_deref(), Some("GPT-4o"));
    }

    #[test]
    fn parses_anthropic_display_names() {
        let value = serde_json::json!({
            "data": [{ "id": "claude-sonnet", "display_name": "Claude Sonnet" }]
        });

        let models = parse_anthropic_models(&value).expect("valid Anthropic response");
        assert_eq!(models[0].id, "claude-sonnet");
        assert_eq!(models[0].label.as_deref(), Some("Claude Sonnet"));
    }

    #[test]
    fn filters_gemini_models_without_generate_content() {
        let value = serde_json::json!({
            "models": [
                {
                    "name": "models/gemini-2.5-flash",
                    "displayName": "Gemini 2.5 Flash",
                    "supportedGenerationMethods": ["generateContent"]
                },
                {
                    "name": "models/text-embedding-005",
                    "supportedGenerationMethods": ["embedContent"]
                }
            ]
        });

        let models = parse_gemini_models(&value).expect("valid Gemini response");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gemini-2.5-flash");
    }

    #[test]
    fn parses_ollama_model_names() {
        let value = serde_json::json!({
            "models": [
                { "name": "llama3.2:latest" },
                { "model": "qwen2.5:7b" }
            ]
        });

        let models = parse_ollama_models(&value).expect("valid Ollama response");
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["llama3.2:latest", "qwen2.5:7b"]
        );
    }
}
