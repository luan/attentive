pub fn fallback_compress(content: &str, max_sentences: usize) -> String {
    let sentences: Vec<&str> = content
        .split(['.', '!', '?'])
        .filter(|s| !s.trim().is_empty())
        .take(max_sentences)
        .collect();

    sentences.join(". ")
        + if sentences.len() < max_sentences {
            ""
        } else {
            "..."
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_compress() {
        let text = "First sentence. Second sentence. Third sentence.";
        let compressed = fallback_compress(text, 2);
        assert!(compressed.contains("First sentence"));
        assert!(compressed.contains("Second sentence"));
        assert!(!compressed.contains("Third"));
    }
}
