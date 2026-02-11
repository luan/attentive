const MAX_INPUT_CHARS: usize = 10000;

pub fn build_compression_prompt(tool_name: &str, output: &str) -> String {
    let truncated = if output.len() > MAX_INPUT_CHARS {
        &output[..MAX_INPUT_CHARS]
    } else {
        output
    };
    format!(
        "Analyze this {} tool output. Return JSON with: \
         {{\"summary\": \"<2-3 sentence summary>\", \"key_facts\": [\"fact1\", ...]}}\n\n{}",
        tool_name, truncated
    )
}

pub fn fallback_compress(tool_name: &str, output: &str) -> CompressResult {
    let summary = if output.len() > 500 {
        format!("[{}] {}...", tool_name, &output[..497])
    } else {
        format!("[{}] {}", tool_name, output)
    };
    let raw_tokens = output.len() / 4;
    let compressed_tokens = summary.len() / 4;
    CompressResult {
        summary,
        key_facts: Vec::new(),
        raw_tokens,
        compressed_tokens,
    }
}

#[derive(Debug, Clone)]
pub struct CompressResult {
    pub summary: String,
    pub key_facts: Vec<String>,
    pub raw_tokens: usize,
    pub compressed_tokens: usize,
}

pub async fn compress_via_api(
    tool_name: &str,
    output: &str,
    api_key: &str,
) -> Result<CompressResult, Box<dyn std::error::Error>> {
    const COMPRESSION_MODEL: &str = "claude-3-haiku-20240307";

    let client = reqwest::Client::new();
    let prompt = build_compression_prompt(tool_name, output);

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": COMPRESSION_MODEL,
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await?;

    let body: serde_json::Value = response.json().await?;
    let text = body["content"][0]["text"].as_str().unwrap_or("");

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
        let summary = parsed["summary"].as_str().unwrap_or(text).to_string();
        let key_facts: Vec<String> = parsed["key_facts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let raw_tokens = output.len() / 4;
        let compressed_tokens = summary.len() / 4;
        Ok(CompressResult {
            summary,
            key_facts,
            raw_tokens,
            compressed_tokens,
        })
    } else {
        Ok(fallback_compress(tool_name, output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_compress() {
        let result = fallback_compress("Read", "fn main() { println!(\"hello\"); }");
        assert!(!result.summary.is_empty());
        assert!(result.compressed_tokens < 500);
    }

    #[test]
    fn test_compression_prompt_format() {
        let prompt = build_compression_prompt("Edit", "some code output");
        assert!(prompt.contains("Edit"));
        assert!(prompt.contains("some code output"));
    }
}
