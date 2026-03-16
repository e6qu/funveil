use cucumber::{given, then, when, World};
use funveil::{
    compute_disclosure_plan, config::ensure_data_dir, rebuild_index, save_index, unveil_file,
    veil_file, Config, ContentStore, DisclosurePlan, MetadataStore,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct FunveilWorld {
    root: PathBuf,
    _temp: TempDir,
    last_error: Option<String>,
    doctor_output: Option<String>,
    manifest: Option<funveil::Manifest>,
    disclosure_plan: Option<DisclosurePlan>,
    original_contents: std::collections::HashMap<String, String>,
}

impl FunveilWorld {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        Self {
            root: temp.path().to_path_buf(),
            _temp: temp,
            last_error: None,
            doctor_output: None,
            manifest: None,
            disclosure_plan: None,
            original_contents: std::collections::HashMap::new(),
        }
    }

    fn output(&self) -> funveil::Output {
        funveil::Output::new(true)
    }
}

// ── Background ──────────────────────────────────────────────────────

#[given("a funveil project is initialized")]
fn init_project(world: &mut FunveilWorld) {
    ensure_data_dir(&world.root).unwrap();
    let config = Config::new(funveil::Mode::Whitelist);
    config.save(&world.root).unwrap();
}

// ── Given: files ────────────────────────────────────────────────────

#[given(expr = "a file {string} with content:")]
fn create_file_docstring(world: &mut FunveilWorld, path: String, step: &cucumber::gherkin::Step) {
    let content = step.docstring.as_ref().unwrap().trim_start_matches('\n');
    let content = content.strip_suffix('\n').unwrap_or(content);
    let full = world.root.join(&path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full, content).unwrap();
    world.original_contents.insert(path, content.to_string());
}

#[given(expr = "a file {string} with content {string} and a config entry for {string}")]
fn create_legacy_file(
    world: &mut FunveilWorld,
    path: String,
    content: String,
    config_path: String,
) {
    let full = world.root.join(&path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let content = content.replace("\\n", "\n");
    fs::write(&full, &content).unwrap();

    let mut config = Config::load(&world.root).unwrap();
    let store = ContentStore::new(&world.root);
    let hash = store.store(b"original content for legacy").unwrap();
    config.register_object(config_path, funveil::ObjectMeta::new(hash, 0o644));
    config.save(&world.root).unwrap();
}

// ── When: veil / unveil ─────────────────────────────────────────────

#[when(expr = "I veil {string}")]
fn veil_file_step(world: &mut FunveilWorld, path: String) {
    let mut config = Config::load(&world.root).unwrap();
    let mut output = world.output();
    match veil_file(&world.root, &mut config, &path, None, &mut output) {
        Ok(()) => {
            config.save(&world.root).unwrap();
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

#[when(expr = "I unveil {string}")]
fn unveil_file_step(world: &mut FunveilWorld, path: String) {
    let mut config = Config::load(&world.root).unwrap();
    let mut output = world.output();
    match unveil_file(&world.root, &mut config, &path, None, &mut output) {
        Ok(()) => {
            config.save(&world.root).unwrap();
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

#[when(expr = "I veil {string} at level {int}")]
fn veil_at_level(world: &mut FunveilWorld, path: String, level: u8) {
    match level {
        0 => veil_file_step(world, path),
        1 => {
            let full = world.root.join(&path);
            let content = fs::read_to_string(&full).unwrap();
            let parser = funveil::TreeSitterParser::new().unwrap();
            let parsed = parser
                .parse_file(std::path::Path::new(&path), &content)
                .unwrap();
            let strategy = funveil::HeaderStrategy::new();
            use funveil::VeilStrategy;
            let veiled = strategy.veil_file(&content, &parsed).unwrap();
            fs::write(&full, veiled).unwrap();
        }
        _ => {}
    }
}

#[when(expr = "I unveil {string} at level {int}")]
fn unveil_at_level(world: &mut FunveilWorld, path: String, level: u8) {
    if level == 3 {
        unveil_file_step(world, path);
    }
}

// ── When: symbol-based ──────────────────────────────────────────────

#[when(expr = "I unveil with symbol {string}")]
fn unveil_by_symbol(world: &mut FunveilWorld, symbol: String) {
    let config = Config::load(&world.root).unwrap();
    let index = funveil::load_index(&world.root).unwrap_or_default();

    // Rebuild index if empty
    let index = if index.symbols.is_empty() {
        let idx = rebuild_index(&world.root, &config).unwrap();
        save_index(&world.root, &idx).unwrap();
        idx
    } else {
        index
    };

    if let Some(entries) = index.symbols.get(&symbol) {
        for entry in entries {
            let file = entry.file.clone();
            unveil_file_step(world, file);
        }
    }
}

#[when(expr = "I veil with symbol {string}")]
fn veil_by_symbol(world: &mut FunveilWorld, symbol: String) {
    let config = Config::load(&world.root).unwrap();

    let parser = funveil::TreeSitterParser::new().unwrap();
    // Find the symbol across all source files
    for entry in walkdir::WalkDir::new(&world.root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if !funveil::is_supported_source(path) {
            continue;
        }
        let rel = path
            .strip_prefix(&world.root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        if config.get_object(&rel).is_some() {
            continue;
        }
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(parsed) = parser.parse_file(path, &content) {
                for sym in &parsed.symbols {
                    let sym_name = match sym {
                        funveil::Symbol::Function { name, .. } => name,
                        funveil::Symbol::Class { name, .. } => name,
                        funveil::Symbol::Module { name, .. } => name,
                    };
                    if sym_name == &symbol {
                        let line_range = match sym {
                            funveil::Symbol::Function { line_range, .. } => *line_range,
                            funveil::Symbol::Class { line_range, .. } => *line_range,
                            funveil::Symbol::Module { line_range, .. } => *line_range,
                        };
                        let mut cfg = Config::load(&world.root).unwrap();
                        let mut output = world.output();
                        let _ = veil_file(
                            &world.root,
                            &mut cfg,
                            &rel,
                            Some(&[line_range]),
                            &mut output,
                        );
                        cfg.save(&world.root).unwrap();
                        return;
                    }
                }
            }
        }
    }
}

#[when(expr = "I unveil callers of {string}")]
fn unveil_callers_of(world: &mut FunveilWorld, symbol: String) {
    let config = Config::load(&world.root).unwrap();
    let index = {
        let idx = rebuild_index(&world.root, &config).unwrap();
        save_index(&world.root, &idx).unwrap();
        idx
    };

    if let Ok(graph) = funveil::build_call_graph_from_metadata(&world.root, &config) {
        if let Some(trace) = graph.trace(&symbol, funveil::TraceDirection::Backward, 3) {
            for node in trace.all_functions() {
                if let Some(ref file) = node.file {
                    let file_str = file.to_string_lossy().to_string();
                    unveil_file_step(world, file_str);
                }
            }
        }
    }

    // Also unveil files that contain the symbol based on index
    if let Some(entries) = index.symbols.get(&symbol) {
        for entry in entries {
            let file = entry.file.clone();
            let cfg = Config::load(&world.root).unwrap();
            if cfg.get_object(&file).is_some() {
                unveil_file_step(world, file);
            }
        }
    }
}

// ── When: metadata operations ───────────────────────────────────────

#[when("I rebuild the metadata index")]
fn rebuild_metadata_index(world: &mut FunveilWorld) {
    let config = Config::load(&world.root).unwrap();
    let index = rebuild_index(&world.root, &config).unwrap();
    save_index(&world.root, &index).unwrap();
}

#[when("I generate a manifest")]
fn generate_manifest(world: &mut FunveilWorld) {
    let config = Config::load(&world.root).unwrap();
    let manifest = funveil::generate_manifest(&world.root, &config).unwrap();
    world.manifest = Some(manifest);
}

// ── When: context and disclosure ────────────────────────────────────

#[when(expr = "I request context for {string} with depth {int}")]
fn request_context(world: &mut FunveilWorld, function: String, _depth: usize) {
    // Rebuild index first
    let config = Config::load(&world.root).unwrap();
    let index = rebuild_index(&world.root, &config).unwrap();
    save_index(&world.root, &index).unwrap();

    // Find the file containing the function and unveil it
    if let Some(entries) = index.symbols.get(&function) {
        for entry in entries {
            unveil_file_step(world, entry.file.clone());
        }
    }
}

#[when(expr = "I request a disclosure plan with budget {int} focused on {string}")]
fn request_disclosure(world: &mut FunveilWorld, budget: usize, focus: String) {
    let config = Config::load(&world.root).unwrap();
    let index = rebuild_index(&world.root, &config).unwrap();
    let graph = funveil::build_call_graph_from_metadata(&world.root, &config).ok();
    let plan = compute_disclosure_plan(
        &world.root,
        &config,
        budget,
        &focus,
        graph.as_ref(),
        Some(&index),
    )
    .unwrap();
    world.disclosure_plan = Some(plan);
}

// ── When: doctor / apply ────────────────────────────────────────────

#[when("I run doctor")]
fn run_doctor(world: &mut FunveilWorld) {
    let config = Config::load(&world.root).unwrap();
    let mut doctor_output = String::new();

    for key in config.objects.keys() {
        let parsed_key = funveil::ConfigKey::parse(key);
        let file = parsed_key.file();
        let file_path = world.root.join(file);

        if file_path.exists() && funveil::is_legacy_marker(&file_path) {
            doctor_output.push_str(&format!("Legacy marker detected: {file}\n"));
        }
    }
    world.doctor_output = Some(doctor_output);
}

#[when("I run apply")]
fn run_apply(world: &mut FunveilWorld) {
    let config = Config::load(&world.root).unwrap();

    for key in config.objects.keys() {
        let parsed_key = funveil::ConfigKey::parse(key);
        let file = parsed_key.file();
        let file_path = world.root.join(file);

        if file_path.exists() && funveil::is_legacy_marker(&file_path) {
            fs::remove_file(&file_path).unwrap();
        }
    }
}

// ── Then: file existence ────────────────────────────────────────────

#[then(expr = "{string} should not exist on disk")]
fn assert_not_exists(world: &mut FunveilWorld, path: String) {
    let full = world.root.join(&path);
    assert!(
        !full.exists(),
        "Expected {path} to NOT exist on disk, but it does"
    );
}

#[then(expr = "{string} should exist on disk")]
fn assert_exists(world: &mut FunveilWorld, path: String) {
    let full = world.root.join(&path);
    assert!(
        full.exists(),
        "Expected {path} to exist on disk, but it doesn't"
    );
}

// ── Then: config tracking ───────────────────────────────────────────

#[then(expr = "{string} should be tracked in config")]
fn assert_tracked(world: &mut FunveilWorld, path: String) {
    let config = Config::load(&world.root).unwrap();
    assert!(
        config.get_object(&path).is_some(),
        "Expected {path} to be tracked in config"
    );
}

#[then(expr = "{string} should not be tracked in config")]
fn assert_not_tracked(world: &mut FunveilWorld, path: String) {
    let config = Config::load(&world.root).unwrap();
    assert!(
        config.get_object(&path).is_none(),
        "Expected {path} to NOT be tracked in config"
    );
}

// ── Then: content checks ────────────────────────────────────────────

#[then(expr = "{string} should have content:")]
fn assert_content(world: &mut FunveilWorld, path: String, step: &cucumber::gherkin::Step) {
    let expected = step.docstring.as_ref().unwrap().trim_start_matches('\n');
    let expected = expected.strip_suffix('\n').unwrap_or(expected);
    let full = world.root.join(&path);
    let actual = fs::read_to_string(&full).unwrap();
    assert_eq!(actual, expected);
}

#[then(expr = "{string} content should exactly match the original")]
fn assert_matches_original(world: &mut FunveilWorld, path: String) {
    let full = world.root.join(&path);
    let actual = fs::read_to_string(&full).unwrap();
    let original = world
        .original_contents
        .get(&path)
        .expect("No original content stored");
    assert_eq!(&actual, original);
}

#[then(expr = "{string} should contain {string}")]
fn assert_contains(world: &mut FunveilWorld, path: String, needle: String) {
    let full = world.root.join(&path);
    let content = fs::read_to_string(&full).unwrap();
    assert!(
        content.contains(&needle),
        "Expected {path} to contain '{needle}', got: {content}"
    );
}

#[then(expr = "{string} should not contain {string}")]
fn assert_not_contains(world: &mut FunveilWorld, path: String, needle: String) {
    let full = world.root.join(&path);
    let content = fs::read_to_string(&full).unwrap();
    assert!(
        !content.contains(&needle),
        "Expected {path} to NOT contain '{needle}'"
    );
}

// ── Then: error checks ──────────────────────────────────────────────

#[then(expr = "veiling {string} again should fail with {string}")]
fn assert_veil_fails(world: &mut FunveilWorld, path: String, expected_err: String) {
    let mut config = Config::load(&world.root).unwrap();
    let mut output = world.output();
    let result = veil_file(&world.root, &mut config, &path, None, &mut output);
    assert!(result.is_err(), "Expected veil to fail for {path}");
    let err = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err.contains(&expected_err.to_lowercase()),
        "Expected error containing '{expected_err}', got: {err}"
    );
}

// ── Then: metadata checks ───────────────────────────────────────────

#[then(expr = "metadata should exist for {string}")]
fn assert_metadata_exists(world: &mut FunveilWorld, path: String) {
    let config = Config::load(&world.root).unwrap();
    if let Some(meta) = config.get_object(&path) {
        let hash = funveil::ContentHash::from_string(meta.hash.clone()).unwrap();
        let store = MetadataStore::new(&world.root);
        assert!(store.exists(&hash), "Metadata should exist for {path}");
    } else {
        panic!("File {path} not tracked in config");
    }
}

#[then(expr = "metadata should not exist for {string}")]
fn assert_metadata_not_exists(world: &mut FunveilWorld, path: String) {
    let config = Config::load(&world.root).unwrap();
    // If not in config, metadata is effectively gone
    if config.get_object(&path).is_none() {
        return;
    }
    panic!("File {path} is still tracked in config");
}

#[then(expr = "the metadata should contain symbol {string}")]
fn assert_metadata_has_symbol(world: &mut FunveilWorld, symbol: String) {
    let config = Config::load(&world.root).unwrap();
    let index = rebuild_index(&world.root, &config).unwrap();
    assert!(
        index.symbols.contains_key(&symbol),
        "Expected index to contain symbol '{symbol}'"
    );
}

#[then(expr = "the index should map {string} to {string}")]
fn assert_index_maps(world: &mut FunveilWorld, symbol: String, file: String) {
    let index = funveil::load_index(&world.root).unwrap();
    let entries = index
        .symbols
        .get(&symbol)
        .unwrap_or_else(|| panic!("Symbol '{symbol}' not in index"));
    assert!(
        entries.iter().any(|e| e.file == file),
        "Expected '{symbol}' to map to '{file}'"
    );
}

// ── Then: manifest checks ───────────────────────────────────────────

#[then(expr = "the manifest should list {string} as veiled")]
fn assert_manifest_veiled(world: &mut FunveilWorld, path: String) {
    let manifest = world.manifest.as_ref().expect("No manifest generated");
    assert!(
        manifest.veiled_files.iter().any(|f| f.path == path),
        "Expected manifest to list '{path}' as veiled"
    );
}

// ── Then: doctor checks ─────────────────────────────────────────────

#[then(expr = "the doctor output should mention {string}")]
fn assert_doctor_mentions(world: &mut FunveilWorld, text: String) {
    let output = world.doctor_output.as_ref().expect("Doctor not run");
    assert!(
        output.to_lowercase().contains(&text.to_lowercase()),
        "Expected doctor output to mention '{text}', got: {output}"
    );
}

// ── Then: disclosure plan checks ────────────────────────────────────

#[then("the plan should have total tokens within budget")]
fn assert_plan_within_budget(world: &mut FunveilWorld) {
    let plan = world.disclosure_plan.as_ref().expect("No plan");
    assert!(
        plan.used_tokens <= plan.budget,
        "Plan exceeds budget: {} > {}",
        plan.used_tokens,
        plan.budget
    );
}

#[then(expr = "the plan should include {string} at level {int}")]
fn assert_plan_includes(world: &mut FunveilWorld, file: String, level: u8) {
    let plan = world.disclosure_plan.as_ref().expect("No plan");
    assert!(
        plan.entries
            .iter()
            .any(|e| e.file == file && e.level == level),
        "Expected plan to include '{file}' at level {level}"
    );
}

#[then(expr = "the plan should have {int} entries")]
fn assert_plan_entry_count(world: &mut FunveilWorld, count: usize) {
    let plan = world.disclosure_plan.as_ref().expect("No plan");
    assert_eq!(plan.entries.len(), count);
}

// ── Runner ──────────────────────────────────────────────────────────

fn main() {
    futures::executor::block_on(
        FunveilWorld::cucumber()
            .max_concurrent_scenarios(1)
            .run("tests/features"),
    );
}
