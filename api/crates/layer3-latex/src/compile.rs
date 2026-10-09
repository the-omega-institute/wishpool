//! Compile an unpacked paper to PDF with TeX Live, as arXiv does.
//!
//! The engine is `pdflatex` unless the source's `00README.json` (arXiv's
//! processing instructions) names `xelatex` or `lualatex`. Passes repeat
//! while the log asks for a rerun, with one `bibtex` run when the source
//! cites a `.bib` database and ships no `.bbl`.
//!
//! Untrusted input: shell escape is off, TeX may read and write only
//! inside the work directory (`openin_any`/`openout_any` paranoid), the
//! child's environment is cleared so no deployment secret is visible to it,
//! and every pass shares one deadline.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{LatexError, LatexResult, archive::Files};

const MAX_PASSES: usize = 4;

pub struct Compiler {
    /// The directory holding `pdflatex`, `xelatex`, `lualatex` and `bibtex`.
    pub bin_dir: PathBuf,
    /// A writable directory for TeX's font and format caches.
    pub cache_dir: PathBuf,
    /// An extra read-only texmf tree with packages beyond the distribution.
    pub texmf_home: Option<PathBuf>,
    pub timeout: Duration,
}

pub struct Compiled {
    pub pdf: Vec<u8>,
    /// The engine that produced it.
    pub engine: String,
}

fn write_tree(root: &Path, files: &Files) -> LatexResult<()> {
    for (path, bytes) in files {
        let target = root.join(path);
        if !target.starts_with(root) {
            return Err(LatexError::Archive(format!(
                "path {path} leaves the work directory"
            )));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| LatexError::Compile(e.to_string()))?;
        }
        std::fs::write(&target, bytes).map_err(|e| LatexError::Compile(e.to_string()))?;
    }
    Ok(())
}

/// The engine named by arXiv's `00README.json`, if any.
pub fn requested_engine(files: &Files) -> Option<&'static str> {
    let readme = files.get("00README.json")?;
    let text = std::str::from_utf8(readme).ok()?;
    let compiler = text.split("\"compiler\"").nth(1)?;
    let value = compiler.split('"').nth(1)?.trim().to_ascii_lowercase();
    Some(match value.as_str() {
        "xelatex" => "xelatex",
        "lualatex" => "lualatex",
        _ => "pdflatex",
    })
}

/// The lines of a TeX log that explain a failure: from the first error
/// line (`! ...`) to the memory statistics, at most 20.
fn failure(log: &str) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with('!'))
        .unwrap_or(lines.len().saturating_sub(20));
    lines[start..]
        .iter()
        .take_while(|l| !l.starts_with("Here is how much"))
        .take(20)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
}

impl Compiler {
    fn command(&self, program: &str, dir: &Path) -> Command {
        let mut command = Command::new(self.bin_dir.join(program));
        command
            .env_clear()
            .env("PATH", &self.bin_dir)
            .env("HOME", dir)
            .env("TEXMFVAR", &self.cache_dir)
            .env("TEXMFCACHE", &self.cache_dir)
            .env("openin_any", "p")
            .env("openout_any", "p")
            .env("shell_escape", "f")
            .env("TEXMFHOME", self.texmf_home.as_deref().unwrap_or(dir))
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    fn run(&self, mut command: Command, deadline: Instant) -> LatexResult<bool> {
        let mut child = command
            .spawn()
            .map_err(|e| LatexError::Compile(format!("cannot start TeX: {e}")))?;
        loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| LatexError::Compile(e.to_string()))?
            {
                return Ok(status.success());
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(LatexError::Compile(format!(
                    "timed out after {:?}",
                    self.timeout
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn compile(&self, files: &Files, main: &str, work: &Path) -> LatexResult<Compiled> {
        write_tree(work, files)?;
        let main_path = work.join(main);
        let dir = main_path.parent().unwrap_or(work).to_path_buf();
        let file = main_path
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| LatexError::Compile("bad main file name".into()))?
            .to_owned();
        let job = file.trim_end_matches(".tex").to_owned();
        let engine = requested_engine(files).unwrap_or("pdflatex");
        let deadline = Instant::now() + self.timeout;
        let log_path = dir.join(format!("{job}.log"));
        let mut bibtex_done = false;
        for pass in 0..MAX_PASSES {
            let mut command = self.command(engine, &dir);
            command.args([
                "-interaction=nonstopmode",
                "-halt-on-error",
                "-no-shell-escape",
                "-file-line-error",
                &file,
            ]);
            let ok = self.run(command, deadline)?;
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            if !ok {
                return Err(LatexError::Compile(failure(&log)));
            }
            let aux = std::fs::read_to_string(dir.join(format!("{job}.aux"))).unwrap_or_default();
            if !bibtex_done && aux.contains("\\bibdata") && !dir.join(format!("{job}.bbl")).exists()
            {
                bibtex_done = true;
                let mut bibtex = self.command("bibtex", &dir);
                bibtex.arg(&job);
                self.run(bibtex, deadline)?;
                continue;
            }
            let rerun = log.contains("Rerun to get")
                || log.contains("Label(s) may have changed")
                || (pass == 0 && aux.contains("\\newlabel"));
            if !rerun {
                break;
            }
        }
        let pdf = std::fs::read(dir.join(format!("{job}.pdf")))
            .map_err(|_| LatexError::Compile(format!("{engine} wrote no PDF")))?;
        Ok(Compiled {
            pdf,
            engine: engine.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_follows_arxiv_instructions() {
        let mut files = Files::new();
        assert_eq!(requested_engine(&files), None);
        files.insert(
            "00README.json".into(),
            br#"{"process": {"compiler": "xelatex"}, "sources": []}"#.to_vec(),
        );
        assert_eq!(requested_engine(&files), Some("xelatex"));
        files.insert(
            "00README.json".into(),
            br#"{"process": {"compiler": "pdflatex"}}"#.to_vec(),
        );
        assert_eq!(requested_engine(&files), Some("pdflatex"));
    }

    #[test]
    fn failures_start_at_the_error() {
        let log = "This is pdfTeX\n(./paper.tex\n! Undefined control sequence.\nl.12 \\foo\n\nHere is how much of TeX's memory you used:\n 1777 strings";
        assert_eq!(failure(log), "! Undefined control sequence.\nl.12 \\foo");
    }

    /// Runs when `WISHPOOL_TEST_TEX_BIN` names a TeX Live bin directory.
    #[test]
    fn compiles_and_refuses_to_read_outside() {
        let Ok(bin) = std::env::var("WISHPOOL_TEST_TEX_BIN") else {
            eprintln!("WISHPOOL_TEST_TEX_BIN not set; skipping");
            return;
        };
        let work = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let compiler = Compiler {
            bin_dir: bin.into(),
            cache_dir: cache.path().into(),
            texmf_home: None,
            timeout: Duration::from_secs(120),
        };
        let mut files = Files::new();
        files.insert(
            "main.tex".into(),
            br"\documentclass{article}\begin{document}See \ref{x}.\section{A}\label{x}\end{document}".to_vec(),
        );
        let out = compiler.compile(&files, "main.tex", work.path()).unwrap();
        assert!(out.pdf.starts_with(b"%PDF"));

        let other = tempfile::tempdir().unwrap();
        let mut files = Files::new();
        files.insert(
            "main.tex".into(),
            br"\documentclass{article}\begin{document}\input{/etc/hosts}\end{document}".to_vec(),
        );
        let error = compiler
            .compile(&files, "main.tex", other.path())
            .err()
            .unwrap();
        assert!(matches!(error, LatexError::Compile(_)));
    }
}
