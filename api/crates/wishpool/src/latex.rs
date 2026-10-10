//! LaTeX sources through Layer 3: reading the structure for Layer 2, the
//! inlined text for review models, and compiling the PDF.

use std::{path::PathBuf, time::Duration};

use wishpool_core::{
    model::ClaimKind,
    ports::{PaperReader, ReadPaper, ReadStatement},
};
use wishpool_latex::{
    LatexError, StatementKind,
    archive::{Limits, unpack},
    compile::Compiler,
    parse,
};

fn kind(kind: StatementKind) -> ClaimKind {
    match kind {
        StatementKind::Theorem => ClaimKind::Theorem,
        StatementKind::Proposition => ClaimKind::Proposition,
        StatementKind::Lemma => ClaimKind::Lemma,
        StatementKind::Corollary => ClaimKind::Corollary,
        StatementKind::Claim => ClaimKind::Claim,
        StatementKind::Conjecture => ClaimKind::Conjecture,
        StatementKind::Question => ClaimKind::Question,
    }
}

fn reason(error: LatexError) -> String {
    match error {
        LatexError::Archive(e) => format!("the upload could not be unpacked: {e}"),
        LatexError::NoMainFile(e) => format!("no main .tex file: {e}"),
        LatexError::Compile(e) => e,
    }
}

pub struct LatexReader;

impl PaperReader for LatexReader {
    fn read(&self, bytes: &[u8], filename: &str) -> Result<ReadPaper, String> {
        let files = unpack(bytes, filename, Limits::default()).map_err(reason)?;
        let paper = parse::parse(&files).map_err(reason)?;
        Ok(ReadPaper {
            main_file: paper.main_file,
            title: paper.title,
            authors: paper.authors,
            abstract_text: paper.abstract_text,
            statements: paper
                .statements
                .into_iter()
                .map(|s| ReadStatement {
                    kind: kind(s.kind),
                    display_name: s.display_name,
                    title: s.title,
                    latex_label: s.label,
                    body: s.body,
                    has_proof: s.has_proof,
                    section: s.section,
                })
                .collect(),
            macros: paper.macros,
            warnings: paper.warnings,
        })
    }
}

/// The source with inputs inlined and comments removed, for review models.
pub fn source_text(bytes: &[u8], filename: &str) -> Result<String, String> {
    let files = unpack(bytes, filename, Limits::default()).map_err(reason)?;
    let main = parse::main_file(&files).map_err(reason)?;
    let mut warnings = Vec::new();
    Ok(parse::strip_comments(&parse::expand(
        &files,
        &main,
        &mut warnings,
    )))
}

pub struct Compile {
    /// The TeX Live bin directory.
    pub tex_bin: PathBuf,
    /// Writable cache for TeX's fonts and formats.
    pub cache_dir: PathBuf,
    /// Extra texmf tree with packages beyond the distribution.
    pub texmf_home: Option<PathBuf>,
    pub timeout: Duration,
}

impl Compile {
    /// Compile an uploaded source in a fresh temporary directory. Runs on a
    /// blocking thread. `Err` carries the message for the author.
    pub async fn pdf(&self, bytes: Vec<u8>, filename: String) -> Result<Vec<u8>, String> {
        let compiler = Compiler {
            bin_dir: self.tex_bin.clone(),
            cache_dir: self.cache_dir.clone(),
            texmf_home: self.texmf_home.clone(),
            timeout: self.timeout,
        };
        tokio::task::spawn_blocking(move || {
            let files = unpack(&bytes, &filename, Limits::default()).map_err(reason)?;
            let main = parse::main_file(&files).map_err(reason)?;
            let work = tempfile::tempdir().map_err(|e| e.to_string())?;
            compiler
                .compile(&files, &main, work.path())
                .map(|c| c.pdf)
                .map_err(reason)
        })
        .await
        .map_err(|e| format!("the compiler task failed: {e}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_single_file_paper() {
        let source = br"\documentclass{article}
\newtheorem{theorem}{Theorem}
\title{Gaps}\author{A. Author}
\begin{document}\maketitle
\begin{theorem}\label{thm:main} Every gap is finite. \end{theorem}
\begin{proof} Trivial. \end{proof}
\end{document}";
        let read = LatexReader.read(source, "paper.tex").unwrap();
        assert_eq!(read.title.as_deref(), Some("Gaps"));
        assert_eq!(read.statements.len(), 1);
        assert_eq!(read.statements[0].kind, ClaimKind::Theorem);
        assert!(read.statements[0].has_proof);
        assert!(LatexReader.read(b"not latex", "paper.tex").is_err());
    }
    #[tokio::test]
    async fn typed_conjecture_generates_stored_tex_and_a_confirmable_main_claim() {
        use std::{collections::BTreeSet, sync::Arc};
        use wishpool_core::{
            app::{App, Upload},
            memory::MemoryStores,
            model::*,
            policy::Policy,
            ports::SystemClock,
        };
        let stores = Arc::new(MemoryStores::default());
        let app = App::new(
            stores.ports(Arc::new(SystemClock), Arc::new(LatexReader)),
            Policy::default(),
            BTreeSet::new(),
        );
        let author = app
            .ensure_service_account(&"author".into(), "A. Author", Role::Endorser)
            .await
            .unwrap();
        let new: NewPaper = serde_json::from_value(serde_json::json!({
            "kind":"conjecture", "ai_disclosure":{"level":"none","statement":"No AI used."}, "authors":[{"name":"A. Author"}],
            "typed_conjecture":{"title":"A conjecture: 100% & x_y","statement":"For every $n > 1$, $f(n) > 0$.", "background":"Let $f$ be the given function.", "origin":"my own"}
        })).unwrap();
        let paper = app
            .submit_paper(
                &author,
                new,
                Upload {
                    filename: String::new(),
                    bytes: vec![],
                },
            )
            .await
            .unwrap();
        assert_eq!(paper.title, "A conjecture: 100% & x_y");
        assert_eq!(paper.extracted.len(), 1);
        assert_eq!(paper.extracted[0].kind, ClaimKind::Conjecture);
        assert_eq!(paper.extracted[0].role, ClaimRole::Main);
        assert_eq!(paper.status, SubmissionStatus::Draft);
        let source = app
            .paper_file(Some(&author), &paper.id, None, false)
            .await
            .unwrap();
        assert_eq!(source.filename, "conjecture.tex");
        let text = String::from_utf8(source.bytes.clone()).unwrap();
        assert!(text.contains(r"\begin{conjecture}"));
        assert!(text.contains("my own"));
        let reread = LatexReader.read(&source.bytes, &source.filename).unwrap();
        assert_eq!(reread.statements.len(), 1);
        assert_eq!(reread.statements[0].kind, ClaimKind::Conjecture);
        let confirmed = app
            .confirm_claims(
                &author,
                &paper.id,
                vec![ClaimConfirmation {
                    depends_on_conjectures: vec![],
                    id: "C1".into(),
                    kind: ClaimKind::Conjecture,
                    role: ClaimRole::Main,
                    depends_on: vec![],
                    settles: None,
                    excluded: false,
                }],
            )
            .await
            .unwrap();
        assert_eq!(
            confirmed.claims[0].statement,
            "For every $n > 1$, $f(n) > 0$."
        );
        assert_eq!(confirmed.status, SubmissionStatus::InReview);
        if let Ok(bin) = std::env::var("WISHPOOL_TEST_TEX_BIN") {
            let cache = tempfile::tempdir().unwrap();
            let compile = Compile {
                tex_bin: bin.into(),
                cache_dir: cache.path().into(),
                texmf_home: None,
                timeout: Duration::from_secs(60),
            };
            assert!(
                compile
                    .pdf(source.bytes, source.filename)
                    .await
                    .unwrap()
                    .starts_with(b"%PDF")
            );
        }
    }
}
