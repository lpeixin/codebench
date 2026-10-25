//! Ollama transport: `POST /api/chat` with `format: "json"`.
//!
//! Two attempts are made when the model returns unparsable JSON; the second one
//! appends a strictness reminder. After that the error carries the raw output
//! (truncated) so the user can see what the model actually said instead of
//! guessing.

use std::time::Duration;

use serde_json::{json, Value};

use super::{build_user_message, parse_payload, truncate_for_display, AiProvider, AiRequest, AiDraft, SYSTEM_PROMPT};
use crate::error::{AppError, AppResult};

pub struct OllamaProvider {
    endpoint: String,
    model: String,
    /// Short timeout for reachability checks.
    probe: ureq::Agent,
    /// Long timeout: a 7B model generating 6 cases can take a while on CPU.
    generator: ureq::Agent,
}

impl OllamaProvider {
    pub fn new(endpoint: &str, model: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model: model.to_string(),
            probe: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(3))
                .build(),
            generator: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(180))
                .build(),
        }
    }

    /// GET /api/tags. Returns the model names Ollama has locally.
    pub fn ping(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/api/tags", self.endpoint);
        let resp = self
            .probe
            .get(&url)
            .call()
            .map_err(|e| describe_ureq_error(e, &url))?;
        let value: Value = resp
            .into_json()
            .map_err(|e| format!("invalid /api/tags response: {e}"))?;
        let models = value
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        m.get("name")
                            .or_else(|| m.get("model"))
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(models)
    }

    fn chat(&self, user_message: &str) -> Result<String, String> {
        let url = format!("{}/api/chat", self.endpoint);
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_message}
            ],
            "stream": false,
            "format": "json",
            "options": {"temperature": 0.2, "num_ctx": 8192}
        });

        let resp = self
            .generator
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| describe_ureq_error(e, &url))?;

        let value: Value = resp
            .into_json()
            .map_err(|e| format!("invalid /api/chat response: {e}"))?;

        if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
            return Err(format!("Ollama error: {err}"));
        }
        let content = value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            // Older/simpler deployments may answer on `response`.
            .or_else(|| value.get("response").and_then(|c| c.as_str()))
            .ok_or_else(|| {
                format!(
                    "Ollama response had no message.content: {}",
                    truncate_for_display(&value.to_string(), 400)
                )
            })?;
        Ok(content.to_string())
    }
}

impl AiProvider for OllamaProvider {
    fn label(&self) -> String {
        format!("ollama:{}", self.model)
    }

    fn generate(&self, req: &AiRequest) -> AppResult<AiDraft> {
        let base = build_user_message(req);
        let mut last_error = String::new();
        let mut last_raw = String::new();

        for attempt in 0..2 {
            let user = if attempt == 0 {
                base.clone()
            } else {
                format!(
                    "{base}\n\nYour previous reply was not valid JSON in the required schema. \
                     Reply with ONE JSON object only: no prose, no markdown fences, no trailing text."
                )
            };
            match self.chat(&user) {
                Ok(raw) => {
                    last_raw = raw.clone();
                    match parse_payload(&raw) {
                        Ok(draft) => return Ok(draft),
                        Err(e) => last_error = e,
                    }
                }
                Err(e) => {
                    // A transport failure will not improve on retry.
                    return Err(AppError::Msg(format!(
                        "Ollama request failed: {e}\n\
                         Check that `ollama serve` is running and that the model `{}` is pulled.",
                        self.model
                    )));
                }
            }
        }

        Err(AppError::Msg(format!(
            "The model did not return usable JSON after 2 attempts: {last_error}\n\n\
             Raw output (truncated):\n{}",
            truncate_for_display(&last_raw, 2000)
        )))
    }
}

fn describe_ureq_error(e: ureq::Error, url: &str) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            format!(
                "HTTP {code} from {url}: {}",
                truncate_for_display(body.trim(), 300)
            )
        }
        ureq::Error::Transport(t) => format!("cannot reach {url}: {t}"),
    }
}
