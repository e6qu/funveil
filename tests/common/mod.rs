#![allow(dead_code)]

use funveil::{run_command, Cli, Commands, Mode, Output, VeilMode};
use std::io::Write;

pub struct TestWriter(pub std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for TestWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

pub fn run_in_temp(command: Commands) -> (String, String, anyhow::Result<()>) {
    let temp = tempfile::TempDir::new().unwrap();
    run_in_dir(temp.path(), command)
}

pub fn run_in_dir(
    dir: &std::path::Path,
    command: Commands,
) -> (String, String, anyhow::Result<()>) {
    let cli = Cli {
        quiet: false,
        log_level: None,
        json: false,
        command,
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, dir, &mut output).map(|_| ());
    let stdout = String::from_utf8(out_buf.lock().unwrap().clone()).unwrap();
    let stderr = String::from_utf8(err_buf.lock().unwrap().clone()).unwrap();
    (stdout, stderr, result)
}

pub struct TestEnv {
    temp: tempfile::TempDir,
}

impl TestEnv {
    pub fn new() -> Self {
        Self {
            temp: tempfile::TempDir::new().unwrap(),
        }
    }

    pub fn init(mode: Mode) -> Self {
        let env = Self::new();
        let _ = run_in_dir(env.dir(), Commands::Init { mode });
        env
    }

    pub fn dir(&self) -> &std::path::Path {
        self.temp.path()
    }

    pub fn write_file(&self, path: &str, content: &str) -> &Self {
        let full = self.dir().join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(full, content).unwrap();
        self
    }

    pub fn run(&self, command: Commands) -> (String, String, anyhow::Result<()>) {
        run_in_dir(self.dir(), command)
    }

    pub fn veil(&self, file: &str) -> (String, String, anyhow::Result<()>) {
        self.run(Commands::Veil {
            pattern: file.into(),
            mode: VeilMode::Full,
            dry_run: false,
            symbol: None,
            unreachable_from: None,
            reachable_from: None,
            level: None,
        })
    }

    pub fn unveil(&self, file: &str) -> (String, String, anyhow::Result<()>) {
        self.run(Commands::Unveil {
            pattern: Some(file.into()),
            all: false,
            dry_run: false,
            symbol: None,
            callers_of: None,
            callees_of: None,
            level: None,
            unreachable_from: None,
            reachable_from: None,
        })
    }

    pub fn unveil_all(&self) -> (String, String, anyhow::Result<()>) {
        self.run(Commands::Unveil {
            pattern: None,
            all: true,
            dry_run: false,
            symbol: None,
            callers_of: None,
            callees_of: None,
            level: None,
            unreachable_from: None,
            reachable_from: None,
        })
    }
}
