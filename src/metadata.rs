use crate::cas::ContentStore;
use crate::config::{is_supported_source, Config};
use crate::error::{FunveilError, Result};
use crate::parser::{ClassKind, ParsedFile, Symbol, TreeSitterParser, Visibility};
use crate::types::{ConfigKey, ContentHash};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
pub struct FileMetadata {
    pub language: String,
    pub path: String,
    pub symbols: Vec<SymbolMeta>,
    pub imports: Vec<ImportMeta>,
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
        match hash.path_components() {
            Ok((a, b, c)) => self
                .root
                .join(METADATA_DIR)
                .join(a)
                .join(b)
                .join(c)
                .exists(),
            Err(_) => false,
        }
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
    }
}

pub fn rebuild_index(root: &Path, config: &Config) -> Result<MetadataIndex> {
    let metadata_store = MetadataStore::new(root);
    let mut index = MetadataIndex::default();

    for (key, meta) in &config.objects {
        let parsed_key = ConfigKey::parse(key);
        let file = parsed_key.file();

        // Only process full-veil and original entries (skip range keys)
        match parsed_key {
            ConfigKey::FullVeil { .. } => {}
            ConfigKey::Original { .. } => continue,
            ConfigKey::Range { .. } => continue,
        }

        let hash = match ContentHash::from_string(meta.hash.clone()) {
            Ok(h) => h,
            Err(_) => continue,
        };

        if let Ok(file_meta) = metadata_store.retrieve(&hash) {
            index.files.insert(
                file.to_string(),
                FileIndexEntry {
                    path: file.to_string(),
                    hash: hash.full().to_string(),
                    language: file_meta.language.clone(),
                    symbol_count: file_meta.symbols.len(),
                },
            );

            for sym in &file_meta.symbols {
                let entry = SymbolIndexEntry {
                    name: sym.name.clone(),
                    kind: sym.kind.clone(),
                    file: file.to_string(),
                    hash: hash.full().to_string(),
                    line_start: sym.line_start,
                    line_end: sym.line_end,
                    signature: sym.signature.clone(),
                };
                index
                    .symbols
                    .entry(sym.name.clone())
                    .or_default()
                    .push(entry);

                // Also index methods
                for method in &sym.methods {
                    let method_entry = SymbolIndexEntry {
                        name: method.name.clone(),
                        kind: method.kind.clone(),
                        file: file.to_string(),
                        hash: hash.full().to_string(),
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
    }

    Ok(index)
}

pub fn save_index(root: &Path, index: &MetadataIndex) -> Result<()> {
    let path = root.join(INDEX_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
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

pub fn build_call_graph_from_metadata(
    root: &Path,
    config: &Config,
) -> Result<crate::analysis::CallGraph> {
    let store = ContentStore::new(root);
    let parser = TreeSitterParser::new()?;
    let mut parsed_files = Vec::new();

    for (key, meta) in &config.objects {
        let parsed_key = ConfigKey::parse(key);
        match parsed_key {
            ConfigKey::FullVeil { file } => {
                if !is_supported_source(Path::new(file)) {
                    continue;
                }
                let hash = ContentHash::from_string(meta.hash.clone())?;
                let content = store.retrieve(&hash)?;
                let content_str = String::from_utf8_lossy(&content);
                if let Ok(parsed) = parser.parse_file(Path::new(file), &content_str) {
                    parsed_files.push(parsed);
                }
            }
            _ => continue,
        }
    }

    Ok(crate::CallGraphBuilder::from_files(&parsed_files))
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
        let symbol = Symbol::Class {
            name: "MyStruct".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 10).unwrap(),
            kind: ClassKind::Struct,
        };

        let meta = symbol_to_meta(&symbol);
        assert_eq!(meta.name, "MyStruct");
        assert!(matches!(meta.kind, SymbolKind::Struct));
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
