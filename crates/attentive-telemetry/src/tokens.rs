//! Token estimation utilities

/// Estimate BPE token count from text
///
/// Falls back to heuristic estimation based on content type detection:
/// - Code-heavy content: ~2.5 chars/token
/// - Natural language: ~4.0 chars/token
/// - Markdown: ~3.0 chars/token
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let total_chars = text.len();
    let total_lines = text.lines().count().max(1);

    // Count code indicators
    let code_chars = text
        .chars()
        .filter(|&c| "{}[]();=<>|&!@#$%^*~`\\".contains(c))
        .count();

    // Count markdown indicators
    let md_chars = text.chars().filter(|&c| "#-*_>".contains(c)).count();

    // Count indented lines (code indicator)
    let indent_lines = text
        .lines()
        .filter(|line| line.starts_with("    ") || line.starts_with('\t'))
        .count();
    let indent_ratio = indent_lines as f64 / total_lines as f64;

    // Estimate content fractions
    let code_fraction =
        ((code_chars as f64 / total_chars as f64) * 10.0 + indent_ratio * 0.5).min(1.0);
    let md_fraction = ((md_chars as f64 / total_chars as f64) * 8.0).min(1.0 - code_fraction);
    let prose_fraction = 1.0 - code_fraction - md_fraction;

    // Weighted average chars-per-token
    let chars_per_token = code_fraction * 2.5 + md_fraction * 3.0 + prose_fraction * 4.0;

    (total_chars as f64 / chars_per_token).max(1.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_code() {
        let code = "fn main() {\n    println!(\"Hello\");\n}";
        let tokens = estimate_tokens(code);
        // Code should be ~2.5 chars/token, so 38 chars / 2.5 ~= 15 tokens
        assert!((12..=20).contains(&tokens), "Got {}", tokens);
    }

    #[test]
    fn test_estimate_tokens_prose() {
        let prose = "This is a simple sentence with natural language that should be counted at about four characters per token.";
        let tokens = estimate_tokens(prose);
        // Prose should be ~4.0 chars/token, so 106 chars / 4.0 ~= 26 tokens
        assert!((20..=32).contains(&tokens), "Got {}", tokens);
    }
}
