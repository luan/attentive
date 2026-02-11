pub fn run() -> anyhow::Result<()> {
    println!("attentive {}", env!("CARGO_PKG_VERSION"));
    println!("Rust implementation of context routing for AI assistants");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_output() {
        let result = run();
        assert!(result.is_ok());
    }
}
