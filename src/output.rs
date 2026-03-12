use std::io::{self, Write};

/// Controls program output based on quiet mode.
///
/// When quiet is true, all output goes to `io::sink()`.
/// When quiet is false, stdout/stderr are used normally.
///
/// In tests, pass `Vec<u8>` buffers to capture output.
pub struct Output {
    pub out: Box<dyn Write>,
    pub err: Box<dyn Write>,
}

impl Output {
    pub fn new(quiet: bool) -> Self {
        if quiet {
            Self {
                out: Box::new(io::sink()),
                err: Box::new(io::sink()),
            }
        } else {
            Self {
                out: Box::new(io::stdout()),
                err: Box::new(io::stderr()),
            }
        }
    }

    /// Create an Output that captures to buffers (for testing).
    #[cfg(test)]
    pub fn capture() -> (Self, SharedBuf, SharedBuf) {
        let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = Self {
            out: Box::new(SharedWriter(out_buf.clone())),
            err: Box::new(SharedWriter(err_buf.clone())),
        };
        (output, out_buf, err_buf)
    }
}

#[cfg(test)]
type SharedBuf = std::sync::Arc<std::sync::Mutex<Vec<u8>>>;

/// A writer that writes to a shared buffer behind a mutex.
#[cfg(test)]
struct SharedWriter(SharedBuf);

#[cfg(test)]
impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_quiet_mode_suppresses_output() {
        let output = Output::new(true);
        let mut out = output.out;
        let mut err = output.err;
        // Writing to sink should succeed silently
        writeln!(out, "hello").unwrap();
        writeln!(err, "world").unwrap();
    }

    #[test]
    fn test_capture_mode() {
        let (mut output, out_buf, err_buf) = Output::capture();
        writeln!(output.out, "stdout line").unwrap();
        writeln!(output.err, "stderr line").unwrap();

        let out = String::from_utf8(out_buf.lock().unwrap().clone()).unwrap();
        let err = String::from_utf8(err_buf.lock().unwrap().clone()).unwrap();
        assert_eq!(out, "stdout line\n");
        assert_eq!(err, "stderr line\n");
    }

    #[test]
    fn test_quiet_vs_non_quiet() {
        // Quiet mode produces no output
        let (mut output, out_buf, _) = Output::capture();
        // Simulate quiet by using sink
        output.out = Box::new(std::io::sink());
        writeln!(output.out, "should vanish").unwrap();
        assert!(out_buf.lock().unwrap().is_empty());
    }
}
