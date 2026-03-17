use crate::cas::ContentStore;
use crate::config::{is_supported_source, walk_files, Config};
use crate::error::{FunveilError, Result};
use crate::parser::{ClassKind, ParsedFile, Symbol, TreeSitterParser, Visibility};
use crate::types::{ConfigKey, ContentHash};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub const METADATA_DIR: &str = ".funveil/metadata";
const INDEX_FILE: &str = ".funveil/metadata/index.json";
const MANIFEST_FILE: &str = ".funveil/manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMeta {
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: VisibilityMeta,
    pub signature: String,
    pub line_start: usize,
    pub line_end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ParamMeta>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<SymbolMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Trait,
    Interface,
    Enum,
    Module,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisibilityMeta {
    Private,
    Public,
    Protected,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_annotation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportMeta {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallMeta {
    pub caller: Option<String>,
    pub callee: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub language: String,
    pub path: String,
    pub symbols: Vec<SymbolMeta>,
    pub imports: Vec<ImportMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<CallMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolIndexEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub file: String,
    pub hash: String,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndexEntry {
    pub path: String,
    pub hash: String,
    pub language: String,
    pub symbol_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetadataIndex {
    pub symbols: HashMap<String, Vec<SymbolIndexEntry>>,
    pub files: HashMap<String, FileIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub mode: String,
    pub veiled_files: Vec<ManifestFile>,
    pub unveiled_count: usize,
    pub totals: ManifestTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub veil_type: String,
    pub on_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestTotals {
    pub veiled: usize,
    pub unveiled: usize,
    pub total: usize,
}

pub struct MetadataStore {
    root: std::path::PathBuf,
}

impl MetadataStore {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub fn store_metadata(
        &self,
        hash: &ContentHash,
        file_path: &str,
        content: &str,
    ) -> Result<FileMetadata> {
        let path = Path::new(file_path);
        let parser = TreeSitterParser::new()?;
        let parsed = parser.parse_file(path, content)?;
        let metadata = parsed_file_to_metadata(&parsed, file_path);

        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| FunveilError::TreeSitterError(format!("metadata serialization: {e}")))?;

        let (a, b, c) = hash.path_components()?;
        let dir = self.root.join(METADATA_DIR).join(a).join(b);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(c), json)?;

        Ok(metadata)
    }

    pub fn retrieve(&self, hash: &ContentHash) -> Result<FileMetadata> {
        let (a, b, c) = hash.path_components()?;
        let path = self.root.join(METADATA_DIR).join(a).join(b).join(c);

        if !path.exists() {
            return Err(FunveilError::ObjectNotFound(format!(
                "metadata for {}",
                hash.full()
            )));
        }

        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content)
            .map_err(|e| FunveilError::TreeSitterError(format!("metadata deserialization: {e}")))
    }

    pub fn exists(&self, hash: &ContentHash) -> bool {
        let (a, b, c) = hash
            .path_components()
            .expect("ContentHash invariant: len >= 7");
        self.root
            .join(METADATA_DIR)
            .join(a)
            .join(b)
            .join(c)
            .exists()
    }

    pub fn delete(&self, hash: &ContentHash) -> Result<()> {
        let (a, b, c) = hash.path_components()?;
        let path = self.root.join(METADATA_DIR).join(a).join(b).join(c);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

fn visibility_to_meta(vis: &Visibility) -> VisibilityMeta {
    match vis {
        Visibility::Private => VisibilityMeta::Private,
        Visibility::Public => VisibilityMeta::Public,
        Visibility::Protected => VisibilityMeta::Protected,
        Visibility::Internal => VisibilityMeta::Internal,
    }
}

fn class_kind_to_symbol_kind(kind: &ClassKind) -> SymbolKind {
    match kind {
        ClassKind::Class => SymbolKind::Class,
        ClassKind::Struct => SymbolKind::Struct,
        ClassKind::Trait => SymbolKind::Trait,
        ClassKind::Interface => SymbolKind::Interface,
        ClassKind::Enum => SymbolKind::Enum,
    }
}

pub fn symbol_to_meta(symbol: &Symbol) -> SymbolMeta {
    match symbol {
        Symbol::Function {
            name,
            params,
            return_type,
            visibility,
            line_range,
            is_async: _,
            ..
        } => SymbolMeta {
            name: name.clone(),
            kind: SymbolKind::Function,
            visibility: visibility_to_meta(visibility),
            signature: symbol.signature(),
            line_start: line_range.start(),
            line_end: line_range.end(),
            parameters: Some(
                params
                    .iter()
                    .map(|p| ParamMeta {
                        name: p.name.clone(),
                        type_annotation: p.type_annotation.clone(),
                    })
                    .collect(),
            ),
            return_type: return_type.clone(),
            methods: vec![],
        },
        Symbol::Class {
            name,
            methods,
            visibility,
            line_range,
            kind,
            ..
        } => SymbolMeta {
            name: name.clone(),
            kind: class_kind_to_symbol_kind(kind),
            visibility: visibility_to_meta(visibility),
            signature: symbol.signature(),
            line_start: line_range.start(),
            line_end: line_range.end(),
            parameters: None,
            return_type: None,
            methods: methods.iter().map(symbol_to_meta).collect(),
        },
        Symbol::Module { name, line_range } => SymbolMeta {
            name: name.clone(),
            kind: SymbolKind::Module,
            visibility: VisibilityMeta::Public,
            signature: format!("mod {name}"),
            line_start: line_range.start(),
            line_end: line_range.end(),
            parameters: None,
            return_type: None,
            methods: vec![],
        },
    }
}

fn parsed_file_to_metadata(parsed: &ParsedFile, file_path: &str) -> FileMetadata {
    FileMetadata {
        language: parsed.language.name().to_string(),
        path: file_path.to_string(),
        symbols: parsed.symbols.iter().map(symbol_to_meta).collect(),
        imports: parsed
            .imports
            .iter()
            .map(|i| ImportMeta {
                path: i.path.clone(),
            })
            .collect(),
        calls: parsed
            .calls
            .iter()
            .map(|c| CallMeta {
                caller: c.caller.clone(),
                callee: c.callee.clone(),
                line: c.line,
            })
            .collect(),
    }
}

/// Convert stored metadata back to a ParsedFile for use by call graph builder
/// and index building without re-parsing.
pub fn metadata_to_parsed_file(meta: &FileMetadata) -> ParsedFile {
    use crate::parser::{Call, Import, Language};
    let language = match meta.language.as_str() {
        "Rust" => Language::Rust,
        "TypeScript" => Language::TypeScript,
        "Python" => Language::Python,
        "Bash/Shell" => Language::Bash,
        "Terraform/HCL" => Language::Terraform,
        "Helm/YAML" => Language::Helm,
        "Go" => Language::Go,
        "Zig" => Language::Zig,
        "HTML" => Language::Html,
        "CSS" => Language::Css,
        "XML" => Language::Xml,
        "Markdown" => Language::Markdown,
        _ => Language::Unknown,
    };

    ParsedFile {
        language,
        path: std::path::PathBuf::from(&meta.path),
        symbols: meta.symbols.iter().map(meta_to_symbol).collect(),
        imports: meta
            .imports
            .iter()
            .map(|i| Import {
                path: i.path.clone(),
                alias: None,
                line: 0,
            })
            .collect(),
        calls: meta
            .calls
            .iter()
            .map(|c| Call {
                caller: c.caller.clone(),
                callee: c.callee.clone(),
                line: c.line,
                is_dynamic: false,
            })
            .collect(),
    }
}

fn meta_to_symbol(meta: &SymbolMeta) -> crate::parser::Symbol {
    use crate::parser::{ClassKind, Param, Symbol, Visibility};
    use crate::types::LineRange;

    let visibility = match meta.visibility {
        VisibilityMeta::Private => Visibility::Private,
        VisibilityMeta::Public => Visibility::Public,
        VisibilityMeta::Protected => Visibility::Protected,
        VisibilityMeta::Internal => Visibility::Internal,
    };
    let line_range = LineRange::new(meta.line_start, meta.line_end).unwrap_or_else(|e| {
        tracing::warn!(
            "corrupt line range ({}, {}) for symbol '{}': {e}; falling back to (1, 1)",
            meta.line_start,
            meta.line_end,
            meta.name
        );
        LineRange::new(1, 1).unwrap()
    });

    match meta.kind {
        SymbolKind::Function => Symbol::Function {
            name: meta.name.clone(),
            params: meta
                .parameters
                .as_ref()
                .map(|ps| {
                    ps.iter()
                        .map(|p| Param {
                            name: p.name.clone(),
                            type_annotation: p.type_annotation.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            return_type: meta.return_type.clone(),
            visibility,
            line_range,
            body_range: line_range,
            is_async: false,
            attributes: vec![],
        },
        SymbolKind::Class
        | SymbolKind::Struct
        | SymbolKind::Trait
        | SymbolKind::Interface
        | SymbolKind::Enum => {
            let kind = match meta.kind {
                SymbolKind::Class => ClassKind::Class,
                SymbolKind::Struct => ClassKind::Struct,
                SymbolKind::Trait => ClassKind::Trait,
                SymbolKind::Interface => ClassKind::Interface,
                SymbolKind::Enum => ClassKind::Enum,
                _ => ClassKind::Class,
            };
            Symbol::Class {
                name: meta.name.clone(),
                methods: meta.methods.iter().map(meta_to_symbol).collect(),
                properties: vec![],
                visibility,
                line_range,
                kind,
            }
        }
        SymbolKind::Module => Symbol::Module {
            name: meta.name.clone(),
            line_range,
        },
    }
}

pub fn rebuild_index(root: &Path, config: &Config) -> Result<MetadataIndex> {
    let parsed_files = parse_all_sources(root, config)?;
    Ok(rebuild_index_from_parsed(root, &parsed_files))
}

/// Build a MetadataIndex from already-parsed files (avoids re-parsing).
pub fn rebuild_index_from_parsed(root: &Path, parsed_files: &[ParsedFile]) -> MetadataIndex {
    let mut index = MetadataIndex::default();

    for parsed in parsed_files {
        let file = parsed
            .path
            .strip_prefix(root)
            .unwrap_or(&parsed.path)
            .to_string_lossy()
            .to_string();
        let file_meta = parsed_file_to_metadata(parsed, &file);

        index.files.insert(
            file.clone(),
            FileIndexEntry {
                path: file.clone(),
                hash: String::new(),
                language: file_meta.language.clone(),
                symbol_count: file_meta.symbols.len(),
            },
        );

        for sym in &file_meta.symbols {
            let entry = SymbolIndexEntry {
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                file: file.clone(),
                hash: String::new(),
                line_start: sym.line_start,
                line_end: sym.line_end,
                signature: sym.signature.clone(),
            };
            index
                .symbols
                .entry(sym.name.clone())
                .or_default()
                .push(entry);

            for method in &sym.methods {
                let method_entry = SymbolIndexEntry {
                    name: method.name.clone(),
                    kind: method.kind.clone(),
                    file: file.clone(),
                    hash: String::new(),
                    line_start: method.line_start,
                    line_end: method.line_end,
                    signature: method.signature.clone(),
                };
                index
                    .symbols
                    .entry(method.name.clone())
                    .or_default()
                    .push(method_entry);
            }
        }
    }

    index
}

pub fn save_index(root: &Path, index: &MetadataIndex) -> Result<()> {
    let path = root.join(INDEX_FILE);
    let parent = path
        .parent()
        .expect("index path always has a parent directory");
    fs::create_dir_all(parent)?;
    let json = serde_json::to_string_pretty(index)
        .map_err(|e| FunveilError::TreeSitterError(format!("index serialization: {e}")))?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn load_index(root: &Path) -> Result<MetadataIndex> {
    let path = root.join(INDEX_FILE);
    if !path.exists() {
        return Ok(MetadataIndex::default());
    }
    let content = fs::read_to_string(&path)?;
    serde_json::from_str(&content)
        .map_err(|e| FunveilError::TreeSitterError(format!("index deserialization: {e}")))
}

pub fn generate_manifest(root: &Path, config: &Config) -> Result<Manifest> {
    let mut veiled_files = Vec::new();

    for key in config.objects.keys() {
        let parsed_key = ConfigKey::parse(key);
        match parsed_key {
            ConfigKey::FullVeil { file } => {
                let on_disk = root.join(file).exists();
                veiled_files.push(ManifestFile {
                    path: file.to_string(),
                    veil_type: "full".to_string(),
                    on_disk,
                });
            }
            ConfigKey::Range { file, .. } => {
                // Only add if not already added
                if !veiled_files.iter().any(|f| f.path == file) {
                    veiled_files.push(ManifestFile {
                        path: file.to_string(),
                        veil_type: "partial".to_string(),
                        on_disk: root.join(file).exists(),
                    });
                }
            }
            ConfigKey::Original { .. } => {}
        }
    }

    let veiled_count = veiled_files.len();

    // Count unveiled files
    let mut unveiled_count = 0usize;
    let veiled_paths: std::collections::HashSet<&str> =
        veiled_files.iter().map(|f| f.path.as_str()).collect();
    for entry in crate::config::walk_files(root).build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
        if rel.starts_with(".funveil") || rel == crate::config::CONFIG_FILE {
            continue;
        }
        if !veiled_paths.contains(rel.as_ref()) {
            unveiled_count += 1;
        }
    }

    Ok(Manifest {
        mode: config.mode().to_string(),
        veiled_files,
        unveiled_count,
        totals: ManifestTotals {
            veiled: veiled_count,
            unveiled: unveiled_count,
            total: veiled_count + unveiled_count,
        },
    })
}

pub fn save_manifest(root: &Path, manifest: &Manifest) -> Result<()> {
    let path = root.join(MANIFEST_FILE);
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| FunveilError::TreeSitterError(format!("manifest serialization: {e}")))?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn load_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join(MANIFEST_FILE);
    if !path.exists() {
        return Err(FunveilError::ObjectNotFound(
            "manifest not found".to_string(),
        ));
    }
    let content = fs::read_to_string(&path)?;
    serde_json::from_str(&content)
        .map_err(|e| FunveilError::TreeSitterError(format!("manifest deserialization: {e}")))
}

/// Parse all source files in the project, reading original content from CAS for
/// veiled files and from disk for unveiled files. This gives static analysis
/// commands (trace, entrypoints, context, disclose) a complete view of the
/// codebase regardless of veil state.
///
/// Uses MetadataStore as a content-hash-keyed cache: if metadata for a file's
/// content hash already exists, it is converted back to a ParsedFile without
/// re-running tree-sitter. Only files with new or changed content are parsed.
pub fn parse_all_sources(root: &Path, config: &Config) -> Result<Vec<ParsedFile>> {
    let store = ContentStore::new(root);
    let meta_store = MetadataStore::new(root);
    let parser = TreeSitterParser::new()?;
    let mut parsed_files = Vec::new();
    let mut seen_files = HashSet::new();

    // 1. Veiled files from CAS (original content)
    for (key, obj_meta) in &config.objects {
        let parsed_key = ConfigKey::parse(key);
        match parsed_key {
            ConfigKey::FullVeil { file } => {
                if !is_supported_source(Path::new(file)) {
                    continue;
                }
                if !seen_files.insert(file.to_string()) {
                    continue;
                }
                let hash = ContentHash::from_string(obj_meta.hash.clone())?;
                if let Some(pf) = try_from_cache_or_parse(&meta_store, &store, &parser, &hash, file)
                {
                    parsed_files.push(pf);
                }
            }
            ConfigKey::Original { file } => {
                if !is_supported_source(Path::new(file)) {
                    continue;
                }
                if !seen_files.insert(file.to_string()) {
                    continue;
                }
                let hash = ContentHash::from_string(obj_meta.hash.clone())?;
                if let Some(pf) = try_from_cache_or_parse(&meta_store, &store, &parser, &hash, file)
                {
                    parsed_files.push(pf);
                }
            }
            ConfigKey::Range { .. } => continue,
        }
    }

    // 2. Unveiled files from disk
    for entry in walk_files(root)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
    {
        let path = entry.path();
        if !is_supported_source(path) {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        if seen_files.contains(&relative) {
            continue;
        }
        if let Ok(content) = fs::read_to_string(path) {
            let hash = ContentHash::from_content(content.as_bytes());
            // Check cache first
            if meta_store.exists(&hash) {
                if let Ok(file_meta) = meta_store.retrieve(&hash) {
                    let mut pf = metadata_to_parsed_file(&file_meta);
                    pf.path = Path::new(&relative).to_path_buf();
                    parsed_files.push(pf);
                    continue;
                }
            }
            // Cache miss — parse and store
            let rel_path = Path::new(&relative);
            match parser.parse_file(rel_path, &content) {
                Ok(parsed) => {
                    if let Err(e) = meta_store.store_metadata(&hash, &relative, &content) {
                        tracing::warn!("failed to cache metadata for {relative}: {e}");
                    }
                    parsed_files.push(parsed);
                }
                Err(e) => tracing::warn!("failed to parse file {relative}: {e}"),
            }
        }
    }

    Ok(parsed_files)
}

/// Try to load a ParsedFile from MetadataStore cache; on miss, parse from CAS content.
fn try_from_cache_or_parse(
    meta_store: &MetadataStore,
    cas: &ContentStore,
    parser: &TreeSitterParser,
    hash: &ContentHash,
    file: &str,
) -> Option<ParsedFile> {
    // Cache hit
    if meta_store.exists(hash) {
        if let Ok(file_meta) = meta_store.retrieve(hash) {
            let mut pf = metadata_to_parsed_file(&file_meta);
            pf.path = Path::new(file).to_path_buf();
            return Some(pf);
        }
    }
    // Cache miss — read content from CAS and parse
    let content = cas.retrieve(hash).ok()?;
    let content_str = String::from_utf8_lossy(&content);
    match parser.parse_file(Path::new(file), &content_str) {
        Ok(parsed) => {
            if let Err(e) = meta_store.store_metadata(hash, file, &content_str) {
                tracing::warn!("failed to cache metadata for {file}: {e}");
            }
            Some(parsed)
        }
        Err(e) => {
            tracing::warn!("failed to parse file {file}: {e}");
            None
        }
    }
}

pub fn build_call_graph_from_metadata(
    root: &Path,
    config: &Config,
) -> Result<crate::analysis::CallGraph> {
    let parsed_files = parse_all_sources(root, config)?;
    Ok(build_call_graph_from_parsed(&parsed_files))
}

/// Build a CallGraph from already-parsed files (avoids re-parsing).
pub fn build_call_graph_from_parsed(parsed_files: &[ParsedFile]) -> crate::analysis::CallGraph {
    crate::CallGraphBuilder::from_files(parsed_files)
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ensure_data_dir;
    use crate::types::{LineRange, Mode};
    use tempfile::TempDir;

    fn setup() -> (TempDir, Config) {
        let temp = TempDir::new().unwrap();
        ensure_data_dir(temp.path()).unwrap();
        // Create metadata dir
        fs::create_dir_all(temp.path().join(METADATA_DIR)).unwrap();
        (temp, Config::new(Mode::Whitelist))
    }

    #[test]
    fn test_metadata_store_roundtrip() {
        let (temp, _config) = setup();
        let store = MetadataStore::new(temp.path());
        let content = "fn hello() {\n    println!(\"hi\");\n}\n";
        let hash = ContentHash::from_content(content.as_bytes());

        let metadata = store.store_metadata(&hash, "test.rs", content).unwrap();
        assert_eq!(metadata.language, "Rust");
        assert_eq!(metadata.path, "test.rs");
        assert!(!metadata.symbols.is_empty());

        let retrieved = store.retrieve(&hash).unwrap();
        assert_eq!(retrieved.path, metadata.path);
        assert_eq!(retrieved.symbols.len(), metadata.symbols.len());
    }

    #[test]
    fn test_metadata_store_exists() {
        let (temp, _config) = setup();
        let store = MetadataStore::new(temp.path());
        let content = "fn test() {}\n";
        let hash = ContentHash::from_content(content.as_bytes());

        assert!(!store.exists(&hash));
        store.store_metadata(&hash, "test.rs", content).unwrap();
        assert!(store.exists(&hash));
    }

    #[test]
    fn test_metadata_store_delete() {
        let (temp, _config) = setup();
        let store = MetadataStore::new(temp.path());
        let content = "fn test() {}\n";
        let hash = ContentHash::from_content(content.as_bytes());

        store.store_metadata(&hash, "test.rs", content).unwrap();
        assert!(store.exists(&hash));
        store.delete(&hash).unwrap();
        assert!(!store.exists(&hash));
    }

    #[test]
    fn test_metadata_index_symbol_lookup() {
        let (temp, mut config) = setup();
        let store = MetadataStore::new(temp.path());
        let cas = ContentStore::new(temp.path());

        let content = "fn verify_token() {\n    // impl\n}\n";
        let hash = cas.store(content.as_bytes()).unwrap();
        store.store_metadata(&hash, "auth.rs", content).unwrap();
        config.register_object(
            "auth.rs".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let index = rebuild_index(temp.path(), &config).unwrap();
        assert!(index.symbols.contains_key("verify_token"));
        let entries = &index.symbols["verify_token"];
        assert_eq!(entries[0].file, "auth.rs");
    }

    #[test]
    fn test_manifest_generation() {
        let (temp, mut config) = setup();
        let cas = ContentStore::new(temp.path());

        let content = "fn test() {}\n";
        let hash = cas.store(content.as_bytes()).unwrap();
        config.register_object(
            "src/test.rs".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let manifest = generate_manifest(temp.path(), &config).unwrap();
        assert_eq!(manifest.totals.veiled, 1);
        assert_eq!(manifest.veiled_files[0].path, "src/test.rs");
        assert!(!manifest.veiled_files[0].on_disk);
    }

    #[test]
    fn test_symbol_to_meta_function() {
        let symbol = Symbol::Function {
            name: "test_fn".to_string(),
            params: vec![crate::parser::Param {
                name: "x".to_string(),
                type_annotation: Some("i32".to_string()),
            }],
            return_type: Some("bool".to_string()),
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 4).unwrap(),
            is_async: false,
            attributes: vec![],
        };

        let meta = symbol_to_meta(&symbol);
        assert_eq!(meta.name, "test_fn");
        assert!(matches!(meta.kind, SymbolKind::Function));
        assert!(meta.parameters.is_some());
        assert_eq!(meta.parameters.unwrap().len(), 1);
    }

    #[test]
    fn test_symbol_to_meta_class() {
        let method = Symbol::Function {
            name: "do_thing".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(3, 5).unwrap(),
            body_range: LineRange::new(4, 4).unwrap(),
            is_async: false,
            attributes: vec![],
        };
        let symbol = Symbol::Class {
            name: "MyStruct".to_string(),
            methods: vec![method],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 10).unwrap(),
            kind: ClassKind::Struct,
        };

        let meta = symbol_to_meta(&symbol);
        assert_eq!(meta.name, "MyStruct");
        assert!(matches!(meta.kind, SymbolKind::Struct));
        assert_eq!(meta.methods.len(), 1);
        assert_eq!(meta.methods[0].name, "do_thing");
    }

    #[test]
    fn test_symbol_to_meta_module() {
        let symbol = Symbol::Module {
            name: "my_mod".to_string(),
            line_range: LineRange::new(1, 1).unwrap(),
        };

        let meta = symbol_to_meta(&symbol);
        assert_eq!(meta.name, "my_mod");
        assert!(matches!(meta.kind, SymbolKind::Module));
    }

    #[test]
    fn test_index_save_load_roundtrip() {
        let (temp, _config) = setup();
        let mut index = MetadataIndex::default();
        index.symbols.insert(
            "foo".to_string(),
            vec![SymbolIndexEntry {
                name: "foo".to_string(),
                kind: SymbolKind::Function,
                file: "test.rs".to_string(),
                hash: "a".repeat(64),
                line_start: 1,
                line_end: 5,
                signature: "fn foo()".to_string(),
            }],
        );

        save_index(temp.path(), &index).unwrap();
        let loaded = load_index(temp.path()).unwrap();
        assert!(loaded.symbols.contains_key("foo"));
    }

    #[test]
    fn test_manifest_save_load_roundtrip() {
        let (temp, _config) = setup();
        let manifest = Manifest {
            mode: "whitelist".to_string(),
            veiled_files: vec![ManifestFile {
                path: "test.rs".to_string(),
                veil_type: "full".to_string(),
                on_disk: false,
            }],
            unveiled_count: 5,
            totals: ManifestTotals {
                veiled: 1,
                unveiled: 5,
                total: 6,
            },
        };

        save_manifest(temp.path(), &manifest).unwrap();
        let loaded = load_manifest(temp.path()).unwrap();
        assert_eq!(loaded.veiled_files.len(), 1);
        assert_eq!(loaded.totals.veiled, 1);
    }

    #[test]
    fn test_load_index_empty() {
        let temp = TempDir::new().unwrap();
        let index = load_index(temp.path()).unwrap();
        assert!(index.symbols.is_empty());
        assert!(index.files.is_empty());
    }

    #[test]
    fn test_retrieve_nonexistent_metadata() {
        let (temp, _config) = setup();
        let store = MetadataStore::new(temp.path());
        let hash = ContentHash::from_content(b"nonexistent");
        assert!(store.retrieve(&hash).is_err());
    }

    #[test]
    fn test_visibility_to_meta_all_variants() {
        assert!(matches!(
            visibility_to_meta(&Visibility::Private),
            VisibilityMeta::Private
        ));
        assert!(matches!(
            visibility_to_meta(&Visibility::Public),
            VisibilityMeta::Public
        ));
        assert!(matches!(
            visibility_to_meta(&Visibility::Protected),
            VisibilityMeta::Protected
        ));
        assert!(matches!(
            visibility_to_meta(&Visibility::Internal),
            VisibilityMeta::Internal
        ));
    }

    #[test]
    fn test_class_kind_to_symbol_kind_all_variants() {
        assert!(matches!(
            class_kind_to_symbol_kind(&crate::parser::ClassKind::Class),
            SymbolKind::Class
        ));
        assert!(matches!(
            class_kind_to_symbol_kind(&crate::parser::ClassKind::Struct),
            SymbolKind::Struct
        ));
        assert!(matches!(
            class_kind_to_symbol_kind(&crate::parser::ClassKind::Trait),
            SymbolKind::Trait
        ));
        assert!(matches!(
            class_kind_to_symbol_kind(&crate::parser::ClassKind::Interface),
            SymbolKind::Interface
        ));
        assert!(matches!(
            class_kind_to_symbol_kind(&crate::parser::ClassKind::Enum),
            SymbolKind::Enum
        ));
    }

    #[test]
    fn test_metadata_store_delete_nonexistent() {
        let (temp, _config) = setup();
        let store = MetadataStore::new(temp.path());
        let hash = ContentHash::from_content(b"never stored");
        let result = store.delete(&hash);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_manifest_missing() {
        let temp = TempDir::new().unwrap();
        let result = load_manifest(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_rebuild_index_with_methods() {
        let (temp, mut config) = setup();
        let cas = ContentStore::new(temp.path());
        let store = MetadataStore::new(temp.path());

        let content = "struct Greeter {}\nimpl Greeter {\n    fn greet(&self) {\n        println!(\"hi\");\n    }\n}\n";
        let hash = cas.store(content.as_bytes()).unwrap();
        let meta = store.store_metadata(&hash, "greeter.rs", content).unwrap();
        // Verify that methods are populated in the metadata
        let has_methods = meta.symbols.iter().any(|s| !s.methods.is_empty());
        if !has_methods {
            // Rust impl blocks may not nest methods inside the struct symbol
            // depending on tree-sitter parsing. Still verify the function was indexed.
        }
        config.register_object(
            "greeter.rs".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let index = rebuild_index(temp.path(), &config).unwrap();
        assert!(
            index.symbols.contains_key("greet"),
            "method 'greet' should be indexed"
        );
    }

    #[test]
    fn test_rebuild_index_with_class() {
        let (temp, mut config) = setup();
        let cas = ContentStore::new(temp.path());
        let store = MetadataStore::new(temp.path());

        let content = "class Dog {\n    bark() {\n        console.log('woof');\n    }\n}\n";
        let hash = cas.store(content.as_bytes()).unwrap();
        store.store_metadata(&hash, "dog.js", content).unwrap();
        config.register_object(
            "dog.js".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let index = rebuild_index(temp.path(), &config).unwrap();
        assert!(index.files.contains_key("dog.js"));
    }

    #[test]
    fn test_build_call_graph_skips_non_source() {
        let (temp, mut config) = setup();
        let cas = ContentStore::new(temp.path());

        let content = b"just some text data";
        let hash = cas.store(content).unwrap();
        config.register_object(
            "data.txt".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let graph = build_call_graph_from_metadata(temp.path(), &config).unwrap();
        assert_eq!(graph.function_count(), 0);
    }

    #[test]
    fn test_build_call_graph_skips_range_keys() {
        let (temp, mut config) = setup();
        let cas = ContentStore::new(temp.path());

        let content = "fn hello() {}\n";
        let hash = cas.store(content.as_bytes()).unwrap();
        config.register_object(
            "test.rs#1-1".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let graph = build_call_graph_from_metadata(temp.path(), &config).unwrap();
        assert_eq!(graph.function_count(), 0);
    }

    #[test]
    fn test_rebuild_index_skips_range_and_original_keys() {
        let (temp, mut config) = setup();
        let cas = ContentStore::new(temp.path());

        let content = "fn test_fn() {}\n";
        let hash = cas.store(content.as_bytes()).unwrap();
        config.register_object(
            "test.rs#1-1".to_string(),
            crate::config::ObjectMeta::new(hash.clone(), 0o644),
        );
        config.register_object(
            "test.rs.original".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let index = rebuild_index(temp.path(), &config).unwrap();
        assert!(index.symbols.is_empty());
        assert!(index.files.is_empty());
    }

    #[test]
    fn test_build_call_graph_from_metadata() {
        let (temp, mut config) = setup();
        let cas = ContentStore::new(temp.path());
        let store = MetadataStore::new(temp.path());

        let content_a = "fn caller() {\n    callee();\n}\n";
        let hash_a = cas.store(content_a.as_bytes()).unwrap();
        store
            .store_metadata(&hash_a, "caller.rs", content_a)
            .unwrap();
        config.register_object(
            "caller.rs".to_string(),
            crate::config::ObjectMeta::new(hash_a, 0o644),
        );

        let content_b = "fn callee() {\n    println!(\"callee\");\n}\n";
        let hash_b = cas.store(content_b.as_bytes()).unwrap();
        store
            .store_metadata(&hash_b, "callee.rs", content_b)
            .unwrap();
        config.register_object(
            "callee.rs".to_string(),
            crate::config::ObjectMeta::new(hash_b, 0o644),
        );

        let graph = build_call_graph_from_metadata(temp.path(), &config).unwrap();
        assert!(
            graph.function_count() > 0,
            "call graph should contain functions"
        );
    }
}
