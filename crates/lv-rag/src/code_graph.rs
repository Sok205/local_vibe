use lv_core::Result;
use lv_core::traits::CodeGraph;
use lv_core::types::{Location, Span, Symbol, SymbolId, SymbolKind};
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use tree_sitter::Language;

pub struct TreeSitterGraph {
    symbols: FxHashMap<PathBuf, Vec<Symbol>>,
    languages: FxHashMap<String, Language>,
}

impl TreeSitterGraph {
    pub fn new(language_names: &[String]) -> Self {
        let mut languages: FxHashMap<String, Language> = FxHashMap::default();
        for name in language_names {
            match name.as_str() {
                "rust" => {
                    languages.insert("rust".to_string(), tree_sitter_rust::LANGUAGE.into());
                }
                "typescript" => {
                    languages.insert(
                        "typescript".to_string(),
                        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                    );
                }
                "python" => {
                    languages.insert("python".to_string(), tree_sitter_python::LANGUAGE.into());
                }
                _ => {}
            }
        }
        Self {
            symbols: FxHashMap::default(),
            languages,
        }
    }

    fn ext_to_lang(ext: &str) -> Option<&'static str> {
        match ext {
            "rs" => Some("rust"),
            "ts" | "tsx" => Some("typescript"),
            "py" => Some("python"),
            _ => None,
        }
    }

    fn node_kind_to_symbol_kind(kind: &str) -> Option<SymbolKind> {
        match kind {
            "function_item" | "function_definition" => Some(SymbolKind::Function),
            "struct_item" => Some(SymbolKind::Struct),
            "trait_item" => Some(SymbolKind::Trait),
            "impl_item" => Some(SymbolKind::Impl),
            "enum_item" => Some(SymbolKind::Enum),
            "const_item" => Some(SymbolKind::Const),
            "mod_item" | "module" => Some(SymbolKind::Module),
            "use_declaration" | "import_statement" | "import_from_statement" => {
                Some(SymbolKind::Import)
            }
            "class_definition" => Some(SymbolKind::Struct),
            "decorated_definition" => Some(SymbolKind::Function),
            _ => None,
        }
    }

    fn extract_name<'a>(node: &tree_sitter::Node, content: &'a str) -> Option<&'a str> {
        // Try named child with field "name"
        if let Some(name_node) = node.child_by_field_name("name") {
            let start = name_node.start_byte();
            let end = name_node.end_byte();
            return content.get(start..end);
        }
        // Fallback: first identifier child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "type_identifier" {
                let start = child.start_byte();
                let end = child.end_byte();
                return content.get(start..end);
            }
        }
        None
    }

    fn first_line(text: &str) -> String {
        text.lines().next().unwrap_or("").trim_end().to_string()
    }
}

impl CodeGraph for TreeSitterGraph {
    fn index_file(&mut self, path: &Path, content: &str) -> Result<()> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();

        let lang_name = match Self::ext_to_lang(ext) {
            Some(l) => l,
            None => return Ok(()),
        };

        let language = match self.languages.get(lang_name) {
            Some(l) => l.clone(),
            None => return Ok(()),
        };

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .map_err(|e| lv_core::error::VibeError::CodeGraph(e.to_string()))?;

        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return Ok(()),
        };

        let root = tree.root_node();
        let mut symbols = Vec::new();

        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            let kind = child.kind();
            let sym_kind = match Self::node_kind_to_symbol_kind(kind) {
                Some(k) => k,
                None => continue,
            };

            let name = Self::extract_name(&child, content)
                .unwrap_or_default()
                .to_string();

            let start = child.start_position();
            let end = child.end_position();
            let span = Span {
                start_line: start.row,
                end_line: end.row,
                start_col: start.column,
                end_col: end.column,
            };

            let node_text = content
                .get(child.start_byte()..child.end_byte())
                .unwrap_or_default();
            let signature = Self::first_line(node_text);

            symbols.push(Symbol {
                id: SymbolId {
                    file_path: path.to_path_buf(),
                    name,
                    kind: sym_kind,
                },
                span,
                signature,
            });
        }

        self.symbols.insert(path.to_path_buf(), symbols);
        Ok(())
    }

    fn symbols(&self, path: &Path) -> Vec<Symbol> {
        self.symbols.get(path).cloned().unwrap_or_default()
    }

    fn references(&self, symbol: &SymbolId) -> Vec<Location> {
        let mut locs = Vec::new();
        for (file_path, syms) in &self.symbols {
            if file_path == &symbol.file_path {
                continue;
            }
            for sym in syms {
                if sym.signature.contains(&symbol.name) {
                    locs.push(Location {
                        file_path: file_path.clone(),
                        span: sym.span.clone(),
                    });
                }
            }
        }
        locs
    }

    fn dependents(&self, path: &Path) -> Vec<PathBuf> {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if stem.is_empty() {
            return Vec::new();
        }

        let mut deps = Vec::new();
        for (file_path, syms) in &self.symbols {
            if file_path == path {
                continue;
            }
            let is_dep = syms.iter().any(|sym| {
                matches!(sym.id.kind, SymbolKind::Import) && sym.signature.contains(stem)
            });
            if is_dep {
                deps.push(file_path.clone());
            }
        }
        deps
    }

    fn repo_map(&self, _root: &Path) -> String {
        let mut paths: Vec<&PathBuf> = self.symbols.keys().collect();
        paths.sort();

        let mut out = String::new();
        for path in paths {
            let syms = &self.symbols[path];
            out.push_str(&format!("{}:\n", path.display()));
            for sym in syms {
                out.push_str(&format!(
                    "  {:?} {} (line {})\n",
                    sym.id.kind, sym.id.name, sym.span.start_line
                ));
            }
        }
        out
    }
}
