//! Symbol extraction from source files

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Kind of symbol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Class,
    Method,
    Import,
}

/// A code symbol (function, class, method)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    pub line: usize,
}

/// Symbols extracted from a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbols {
    pub path: String,
    pub language: String,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<String>,
    pub token_estimate: usize,
}

impl FileSymbols {
    pub fn new(path: String, language: String) -> Self {
        Self {
            path,
            language,
            symbols: Vec::new(),
            imports: Vec::new(),
            token_estimate: 0,
        }
    }
}

static PYTHON_FUNC_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_CLASS_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_IMPORT_RE: OnceLock<Regex> = OnceLock::new();

static JS_FUNC_RE: OnceLock<Regex> = OnceLock::new();
static JS_CLASS_RE: OnceLock<Regex> = OnceLock::new();
static JS_IMPORT_RE: OnceLock<Regex> = OnceLock::new();

static RUST_FN_RE: OnceLock<Regex> = OnceLock::new();
static RUST_STRUCT_RE: OnceLock<Regex> = OnceLock::new();
static RUST_USE_RE: OnceLock<Regex> = OnceLock::new();

static GO_FUNC_RE: OnceLock<Regex> = OnceLock::new();
static GO_TYPE_RE: OnceLock<Regex> = OnceLock::new();
static GO_IMPORT_RE: OnceLock<Regex> = OnceLock::new();

static JAVA_CLASS_RE: OnceLock<Regex> = OnceLock::new();
static JAVA_METHOD_RE: OnceLock<Regex> = OnceLock::new();

static C_FUNC_RE: OnceLock<Regex> = OnceLock::new();
static C_INCLUDE_RE: OnceLock<Regex> = OnceLock::new();

/// Extract symbols from Python source using regex
pub fn extract_python_symbols(content: &str, path: &str) -> FileSymbols {
    let func_re = PYTHON_FUNC_RE.get_or_init(|| Regex::new(r"^\s*def\s+(\w+)\s*\(").unwrap());
    let class_re = PYTHON_CLASS_RE.get_or_init(|| Regex::new(r"^\s*class\s+(\w+)").unwrap());
    let import_re = PYTHON_IMPORT_RE
        .get_or_init(|| Regex::new(r"^\s*(?:from\s+(\S+)\s+)?import\s+(.+)").unwrap());

    let mut file_symbols = FileSymbols::new(path.to_string(), "python".to_string());

    for (line_num, line) in content.lines().enumerate() {
        if let Some(cap) = func_re.captures(line) {
            file_symbols.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Function,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = class_re.captures(line) {
            file_symbols.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Class,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = import_re.captures(line) {
            let import_path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            file_symbols.imports.push(import_path.to_string());
        }
    }

    file_symbols.token_estimate = estimate_tokens(&file_symbols);
    file_symbols
}

fn estimate_tokens(file_symbols: &FileSymbols) -> usize {
    // ~5 tokens overhead + ~10 tokens per symbol
    5 + file_symbols.symbols.len() * 10
}

/// Extract symbols from JavaScript/TypeScript source
pub fn extract_js_symbols(content: &str, path: &str) -> FileSymbols {
    let func_re = JS_FUNC_RE
        .get_or_init(|| Regex::new(r"^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)").unwrap());
    let class_re =
        JS_CLASS_RE.get_or_init(|| Regex::new(r"^\s*(?:export\s+)?class\s+(\w+)").unwrap());
    let import_re =
        JS_IMPORT_RE.get_or_init(|| Regex::new(r#"^\s*import\s+.*from\s+['"]([^'"]+)"#).unwrap());

    let mut fs = FileSymbols::new(path.to_string(), "javascript".to_string());
    for (line_num, line) in content.lines().enumerate() {
        if let Some(cap) = func_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Function,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = class_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Class,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = import_re.captures(line) {
            fs.imports.push(cap[1].to_string());
        }
    }
    fs.token_estimate = estimate_tokens(&fs);
    fs
}

/// Extract symbols from Rust source
pub fn extract_rust_symbols(content: &str, path: &str) -> FileSymbols {
    let fn_re =
        RUST_FN_RE.get_or_init(|| Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").unwrap());
    let struct_re = RUST_STRUCT_RE
        .get_or_init(|| Regex::new(r"^\s*(?:pub\s+)?(?:struct|enum|trait)\s+(\w+)").unwrap());
    let use_re = RUST_USE_RE.get_or_init(|| Regex::new(r"^\s*use\s+(\S+)").unwrap());

    let mut fs = FileSymbols::new(path.to_string(), "rust".to_string());
    for (line_num, line) in content.lines().enumerate() {
        if let Some(cap) = fn_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Function,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = struct_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Class,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = use_re.captures(line) {
            fs.imports.push(cap[1].to_string());
        }
    }
    fs.token_estimate = estimate_tokens(&fs);
    fs
}

/// Extract symbols from Go source
pub fn extract_go_symbols(content: &str, path: &str) -> FileSymbols {
    let func_re =
        GO_FUNC_RE.get_or_init(|| Regex::new(r"^func\s+(?:\(\w+\s+\*?\w+\)\s+)?(\w+)").unwrap());
    let type_re = GO_TYPE_RE.get_or_init(|| Regex::new(r"^type\s+(\w+)").unwrap());
    let import_re = GO_IMPORT_RE.get_or_init(|| Regex::new(r#"^\s*"([^"]+)"#).unwrap());

    let mut fs = FileSymbols::new(path.to_string(), "go".to_string());
    for (line_num, line) in content.lines().enumerate() {
        if let Some(cap) = func_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Function,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = type_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Class,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = import_re.captures(line) {
            fs.imports.push(cap[1].to_string());
        }
    }
    fs.token_estimate = estimate_tokens(&fs);
    fs
}

/// Extract symbols from Java source
pub fn extract_java_symbols(content: &str, path: &str) -> FileSymbols {
    let class_re = JAVA_CLASS_RE.get_or_init(|| {
        Regex::new(r"^\s*(?:public\s+)?(?:abstract\s+)?(?:class|interface)\s+(\w+)").unwrap()
    });
    let method_re = JAVA_METHOD_RE.get_or_init(|| {
        Regex::new(r"^\s*(?:public|private|protected)\s+(?:static\s+)?(?:\w+)\s+(\w+)\s*\(")
            .unwrap()
    });

    let mut fs = FileSymbols::new(path.to_string(), "java".to_string());
    for (line_num, line) in content.lines().enumerate() {
        if let Some(cap) = class_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Class,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = method_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Method,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        }
    }
    fs.token_estimate = estimate_tokens(&fs);
    fs
}

/// Extract symbols from C/C++ source
pub fn extract_c_symbols(content: &str, path: &str) -> FileSymbols {
    let func_re = C_FUNC_RE.get_or_init(|| {
        Regex::new(r"^(?:static\s+)?(?:inline\s+)?(?:\w+\s+)+(\w+)\s*\([^)]*\)\s*\{").unwrap()
    });
    let include_re =
        C_INCLUDE_RE.get_or_init(|| Regex::new(r#"^\s*#include\s+[<"]([^>"]+)"#).unwrap());

    let mut fs = FileSymbols::new(path.to_string(), "c".to_string());
    for (line_num, line) in content.lines().enumerate() {
        if let Some(cap) = func_re.captures(line) {
            fs.symbols.push(Symbol {
                name: cap[1].to_string(),
                kind: SymbolKind::Function,
                signature: line.trim().to_string(),
                line: line_num + 1,
            });
        } else if let Some(cap) = include_re.captures(line) {
            fs.imports.push(cap[1].to_string());
        }
    }
    fs.token_estimate = estimate_tokens(&fs);
    fs
}

/// Extract symbols from source file based on extension
pub fn extract_symbols(content: &str, path: &str) -> Option<FileSymbols> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    match ext {
        "py" => Some(extract_python_symbols(content, path)),
        "js" | "jsx" => Some(extract_js_symbols(content, path)),
        "ts" | "tsx" => Some(extract_js_symbols(content, path)),
        "rs" => Some(extract_rust_symbols(content, path)),
        "go" => Some(extract_go_symbols(content, path)),
        "java" => Some(extract_java_symbols(content, path)),
        "c" | "cpp" | "h" | "hpp" | "cc" => Some(extract_c_symbols(content, path)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_python_functions() {
        let code = "def foo():\n    pass\n\ndef bar(x, y):\n    return x + y";
        let symbols = extract_python_symbols(code, "test.py");

        assert_eq!(symbols.symbols.len(), 2);
        assert_eq!(symbols.symbols[0].name, "foo");
        assert_eq!(symbols.symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols.symbols[1].name, "bar");
    }

    #[test]
    fn test_extract_python_classes() {
        let code = "class MyClass:\n    def method(self):\n        pass";
        let symbols = extract_python_symbols(code, "test.py");

        assert_eq!(symbols.symbols.len(), 2);
        assert_eq!(symbols.symbols[0].name, "MyClass");
        assert_eq!(symbols.symbols[0].kind, SymbolKind::Class);
        assert_eq!(symbols.symbols[1].name, "method");
        assert_eq!(symbols.symbols[1].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_python_imports() {
        let code = "import os\nfrom pathlib import Path\nimport sys";
        let symbols = extract_python_symbols(code, "test.py");

        assert_eq!(symbols.imports.len(), 3);
        assert!(symbols.imports.contains(&"pathlib".to_string()));
    }

    #[test]
    fn test_token_estimate() {
        let code = "def foo():\n    pass";
        let symbols = extract_python_symbols(code, "test.py");

        // 5 overhead + 1 symbol * 10 = 15 tokens
        assert_eq!(symbols.token_estimate, 15);
    }

    #[test]
    fn test_extract_js_functions() {
        let code = "export function greet(name) {\n  return name;\n}\nclass App {}";
        let symbols = extract_js_symbols(code, "app.js");
        assert_eq!(symbols.symbols.len(), 2);
        assert_eq!(symbols.symbols[0].name, "greet");
        assert_eq!(symbols.symbols[1].name, "App");
    }

    #[test]
    fn test_extract_rust_symbols() {
        let code = "pub fn main() {}\nstruct Config {}\nenum State {}";
        let symbols = extract_rust_symbols(code, "lib.rs");
        assert_eq!(symbols.symbols.len(), 3);
        assert_eq!(symbols.symbols[0].name, "main");
        assert_eq!(symbols.symbols[1].name, "Config");
    }

    #[test]
    fn test_extract_go_symbols() {
        let code = "func main() {}\ntype Config struct {}\nfunc (s *Server) Start() {}";
        let symbols = extract_go_symbols(code, "main.go");
        assert!(symbols.symbols.len() >= 2);
        assert_eq!(symbols.symbols[0].name, "main");
    }

    #[test]
    fn test_unknown_extension_returns_none() {
        assert!(extract_symbols("content", "file.xyz").is_none());
    }
}
