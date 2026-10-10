//! Read the structure of a LaTeX paper: main file, inlined inputs, title,
//! authors, abstract, sections and statement environments.

use std::collections::{BTreeMap, BTreeSet};

use crate::{LatexError, LatexResult, ParsedPaper, Statement, StatementKind, archive::Files};

/// Environment names treated as statements even when the preamble does not
/// declare them (document classes such as `llncs` predefine them).
const DEFAULTS: [(&str, &str); 16] = [
    ("theorem", "Theorem"),
    ("thm", "Theorem"),
    ("lemma", "Lemma"),
    ("lem", "Lemma"),
    ("proposition", "Proposition"),
    ("prop", "Proposition"),
    ("corollary", "Corollary"),
    ("cor", "Corollary"),
    ("conjecture", "Conjecture"),
    ("conj", "Conjecture"),
    ("claim", "Claim"),
    ("question", "Question"),
    ("problem", "Problem"),
    ("hypothesis", "Hypothesis"),
    ("maintheorem", "Main Theorem"),
    ("mainthm", "Main Theorem"),
];

/// Remove `%` comments, keeping escaped `\%`.
pub fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let bytes = line.as_bytes();
        let mut cut = line.len();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'%' {
                cut = i;
                break;
            }
            i += 1;
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// The content of the brace group starting at `start` (which must be `{`),
/// and the index after its closing brace.
fn braced(s: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&s[start + 1..i], i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The content of an optional `[...]` argument at `start` (after
/// whitespace), respecting nested braces, and the index after it.
fn bracketed(s: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut i = start;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\n') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'[') {
        return None;
    }
    let open = i;
    let mut depth = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b']' if depth == 0 => return Some((&s[open + 1..i], i + 1)),
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_space(s: &str, mut i: usize) -> usize {
    let bytes = s.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Pick the main file: a `.tex` with `\documentclass` and
/// `\begin{document}`, preferring conventional names.
pub fn main_file(files: &Files) -> LatexResult<String> {
    let mut candidates: Vec<&String> = files
        .iter()
        .filter(|(p, _)| p.ends_with(".tex"))
        .filter(|(_, b)| {
            let text = String::from_utf8_lossy(b);
            let text = strip_comments(&text);
            text.contains("\\documentclass") && text.contains("\\begin{document}")
        })
        .map(|(p, _)| p)
        .collect();
    candidates.sort_by_key(|p| {
        let name = p.rsplit('/').next().unwrap_or(p);
        let preferred = matches!(name, "main.tex" | "ms.tex" | "paper.tex" | "article.tex");
        (!preferred, p.matches('/').count(), (*p).clone())
    });
    candidates.first().map(|p| (*p).clone()).ok_or_else(|| {
        LatexError::NoMainFile("no .tex file has both \\documentclass and \\begin{document}".into())
    })
}

/// Inline `\input{}`, `\include{}` and `\subfile{}` recursively.
pub fn expand(files: &Files, main: &str, warnings: &mut Vec<String>) -> String {
    fn go(files: &Files, path: &str, depth: usize, warnings: &mut Vec<String>) -> String {
        let Some(bytes) = files.get(path) else {
            warnings.push(format!("missing input {path}"));
            return String::new();
        };
        let text = strip_comments(&String::from_utf8_lossy(bytes));
        if depth > 12 {
            warnings.push(format!("inputs nested too deeply at {path}"));
            return text;
        }
        let dir = path
            .rsplit_once('/')
            .map(|(d, _)| format!("{d}/"))
            .unwrap_or_default();
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(at) = ["\\input", "\\include", "\\subfile"]
            .iter()
            .filter_map(|c| rest.find(c).map(|i| (i, c.len())))
            .min()
        {
            let (i, len) = at;
            let after = i + len;
            let next = rest.as_bytes().get(after).copied();
            // `\input` must be followed by `{` (or whitespace then `{`), not a letter.
            if next.is_some_and(|c| c.is_ascii_alphabetic()) {
                out.push_str(&rest[..after]);
                rest = &rest[after..];
                continue;
            }
            let brace = skip_space(rest, after);
            match braced(rest, brace) {
                Some((name, end)) => {
                    out.push_str(&rest[..i]);
                    let name = name.trim();
                    let mut candidates = vec![format!("{dir}{name}"), name.to_owned()];
                    if !name.ends_with(".tex") {
                        candidates.insert(0, format!("{dir}{name}.tex"));
                        candidates.insert(1, format!("{name}.tex"));
                    }
                    match candidates.iter().find(|c| files.contains_key(*c)) {
                        Some(found) => out.push_str(&go(files, found, depth + 1, warnings)),
                        None => warnings.push(format!("unresolved input {name}")),
                    }
                    rest = &rest[end..];
                }
                None => {
                    out.push_str(&rest[..after]);
                    rest = &rest[after..];
                }
            }
        }
        out.push_str(rest);
        out
    }
    go(files, main, 0, warnings)
}

/// `env → printed name` from `\newtheorem`, `\newtheorem*`, `\spnewtheorem`
/// and `\declaretheorem[name=…]`.
pub fn theorem_environments(source: &str) -> BTreeMap<String, String> {
    let mut envs = BTreeMap::new();
    for command in ["\\newtheorem", "\\spnewtheorem"] {
        let mut from = 0;
        while let Some(found) = source[from..].find(command) {
            let mut i = from + found + command.len();
            from = i;
            if source.as_bytes().get(i) == Some(&b'*') {
                i += 1;
            }
            let i = skip_space(source, i);
            let Some((env, mut j)) = braced(source, i) else {
                continue;
            };
            if let Some((_, after)) = bracketed(source, j) {
                j = after;
            }
            let j = skip_space(source, j);
            if let Some((name, _)) = braced(source, j) {
                envs.insert(env.trim().to_owned(), plain(name));
            }
        }
    }
    let mut from = 0;
    while let Some(found) = source[from..].find("\\declaretheorem") {
        let i = from + found + "\\declaretheorem".len();
        from = i;
        let (options, j) = bracketed(source, i)
            .map(|(o, j)| (Some(o), j))
            .unwrap_or((None, i));
        let j = skip_space(source, j);
        let Some((env, _)) = braced(source, j) else {
            continue;
        };
        let name = options
            .and_then(|o| {
                o.split(',').find_map(|kv| {
                    kv.trim()
                        .strip_prefix("name=")
                        .map(|n| n.trim().trim_matches(['{', '}']).to_owned())
                })
            })
            .unwrap_or_else(|| {
                let mut chars = env.trim().chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            });
        envs.insert(env.trim().to_owned(), name);
    }
    envs
}

/// Text of a short LaTeX fragment for display: drop `\thanks`, `\footnote`,
/// `\inst`, line breaks and redundant whitespace; keep math.
pub fn plain(fragment: &str) -> String {
    let mut s = fragment.to_owned();
    for command in ["\\thanks", "\\footnote", "\\inst", "\\orcidlink", "\\email"] {
        while let Some(i) = s.find(command) {
            let after = skip_space(&s, i + command.len());
            match braced(&s, after) {
                Some((_, end)) => s.replace_range(i..end, ""),
                None => break,
            }
        }
    }
    s.replace("\\\\", " ")
        .replace('~', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A control word and the index after it; control symbols are not words.
fn control_word(s: &str, start: usize) -> Option<(&str, usize)> {
    if s.as_bytes().get(start) != Some(&b'\\') {
        return None;
    }
    let mut end = start + 1;
    while s.as_bytes().get(end).is_some_and(u8::is_ascii_alphabetic) {
        end += 1;
    }
    (end > start + 1).then(|| (&s[start + 1..end], end))
}

/// A delimited math span, with its content and end. Never interpret its commands.
fn math_span(s: &str, start: usize) -> Option<(&str, usize)> {
    let rest = &s[start..];
    let (open, close) = [("$$", "$$"), ("$", "$"), (r"\(", r"\)"), (r"\[", r"\]")]
        .into_iter()
        .find(|(open, _)| rest.starts_with(open))?;
    let content = start + open.len();
    let mut i = content;
    while i < s.len() {
        if s[i..].starts_with(close) {
            return Some((&s[content..i], i + close.len()));
        }
        if s.as_bytes()[i] == b'\\' {
            i += 1;
        }
        i += s[i..].chars().next()?.len_utf8();
    }
    None
}

/// Remove layout containers around a shared names/affiliations block.
fn unwrap_author_layout(mut entry: &str) -> &str {
    loop {
        entry = entry.trim();
        if let Some((body, end)) = braced(entry, 0)
            && end == entry.len()
        {
            entry = body;
            continue;
        }
        let Some((env, mut after)) = entry
            .strip_prefix("\\begin")
            .and_then(|_| braced(entry, skip_space(entry, "\\begin".len())))
        else {
            return entry;
        };
        if !matches!(env, "center" | "tabular" | "tabular*") {
            return entry;
        }
        if env == "tabular*" {
            let Some((_, end)) = braced(entry, skip_space(entry, after)) else {
                return entry;
            };
            after = end;
        }
        if env != "center" {
            if let Some((_, end)) = bracketed(entry, after) {
                after = end;
            }
            let Some((_, end)) = braced(entry, skip_space(entry, after)) else {
                return entry;
            };
            after = end;
        }
        let Some(body) = entry[after..].strip_suffix(&format!("\\end{{{env}}}")) else {
            return entry;
        };
        entry = body;
    }
}

/// Remove name annotations before splitting: a superscript or thanks may itself
/// contain commas, `and`, or line breaks which are not author separators.
fn strip_author_marks(entry: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < entry.len() {
        if let Some((content, end)) = math_span(entry, i) {
            let marker = content.trim().strip_prefix("{}").unwrap_or(content.trim());
            if !marker.trim_start().starts_with('^') {
                out.push_str(&entry[i..end]);
            }
            i = end;
            continue;
        }
        if let Some((command, after)) = control_word(entry, i)
            && matches!(
                command,
                "thanks"
                    | "footnote"
                    | "inst"
                    | "orcidlink"
                    | "email"
                    | "textsuperscript"
                    | "footnotemark"
            )
        {
            let mut end = after;
            if let Some((_, after)) = bracketed(entry, end) {
                end = after;
            }
            if let Some((_, after)) = braced(entry, skip_space(entry, end)) {
                i = after;
                continue;
            }
            if command == "footnotemark" {
                i = end;
                continue;
            }
        }
        let c = entry[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
        if c == '\\'
            && let Some(escaped) = entry[i..].chars().next()
        {
            out.push(escaped);
            i += escaped.len_utf8();
        }
    }
    out
}

/// Split entries on `\and`, then names on spacing commands, commas, `and`
/// and table columns. Brace groups and control-word suffixes stay intact.
fn author_parts(entry: &str, names: bool) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < entry.len() {
        let c = entry[i..].chars().next().unwrap();
        let mut end = i + c.len_utf8();
        let separator = if let Some((command, after)) = control_word(entry, i) {
            end = after;
            command == "and" || (names && matches!(command, "quad" | "qquad"))
        } else if let Some((_, after)) = braced(entry, i) {
            end = after;
            false
        } else if names
            && entry[i..].starts_with("and")
            && entry[..i]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
            && entry[i + 3..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
        {
            end = i + 3;
            true
        } else {
            names && matches!(c, ',' | '&')
        };
        if separator {
            parts.push(entry[start..i].trim());
            start = end;
        }
        i = end;
    }
    parts.push(entry[start..].trim());
    parts
}

/// Names occupy the first row of each `\and` entry; later rows contain
/// affiliations, addresses, ORCID or e-mail lines.
fn author_names(entry: &str) -> Vec<String> {
    author_parts(unwrap_author_layout(entry), false)
        .into_iter()
        .flat_map(|entry| {
            let clean = strip_author_marks(unwrap_author_layout(entry));
            let row = clean.split("\\\\").next().unwrap_or_default();
            author_parts(row, true)
                .into_iter()
                .map(plain)
                .collect::<Vec<_>>()
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// Titles become plain UI labels. Render citation keys and text grouping, while
/// retaining math (including its braces) and unknown commands as LaTeX source.
fn statement_title(fragment: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < fragment.len() {
        if let Some((_, end)) = math_span(fragment, i) {
            out.push_str(&fragment[i..end]);
            i = end;
            continue;
        }
        if let Some((command, after)) = control_word(fragment, i) {
            let mut arg_start = skip_space(fragment, after);
            let mut optional = None;
            if let Some((option, end)) = bracketed(fragment, arg_start) {
                optional = Some(option);
                arg_start = skip_space(fragment, end);
            }
            if let Some((arg, end)) = braced(fragment, arg_start) {
                if matches!(command, "cite" | "citep" | "citet") {
                    out.push('[');
                    out.push_str(&arg.split(',').map(str::trim).collect::<Vec<_>>().join(", "));
                    if let Some(option) = optional {
                        out.push_str(", ");
                        out.push_str(&plain(option));
                    }
                    out.push(']');
                } else {
                    out.push_str(&fragment[i..end]);
                }
                i = end;
                continue;
            }
            out.push_str(&fragment[i..after]);
            i = after;
            continue;
        }
        let c = fragment[i..].chars().next().unwrap();
        if !matches!(c, '{' | '}') {
            out.push(c);
        }
        i += c.len_utf8();
        if c == '\\'
            && let Some(escaped) = fragment[i..].chars().next()
        {
            out.push(escaped);
            i += escaped.len_utf8();
        }
    }
    plain(&out)
}

/// Remove every `\label{..}` and cleveref `\label[type]{..}`.
fn strip_labels(body: &str) -> String {
    let mut s = body.to_owned();
    let mut from = 0;
    while let Some(found) = s[from..].find("\\label") {
        let i = from + found;
        let mut j = i + "\\label".len();
        if s.as_bytes().get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
            from = j;
            continue;
        }
        if let Some((_, after)) = bracketed(&s, j) {
            j = after;
        }
        match braced(&s, skip_space(&s, j)) {
            Some((_, end)) => s.replace_range(i..end, ""),
            None => from = j,
        }
    }
    s
}

/// Labels named in the optional argument of a `proof` environment, as in
/// `\begin{proof}[Proof of Theorem~\ref{thm:main}]`: proofs given later.
fn deferred_proofs(source: &str) -> BTreeSet<String> {
    let mut labels = BTreeSet::new();
    let mut from = 0;
    while let Some(found) = source[from..].find("\\begin{proof}") {
        let after = from + found + "\\begin{proof}".len();
        from = after;
        let Some((option, _)) = bracketed(source, after) else {
            continue;
        };
        for command in ["\\ref", "\\cref", "\\Cref", "\\autoref"] {
            for label in all_arguments(option, command) {
                labels.extend(label.split(',').map(|l| l.trim().to_owned()));
            }
        }
    }
    labels
}

fn command_argument<'a>(source: &'a str, command: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(found) = source[from..].find(command) {
        let i = from + found + command.len();
        from = i;
        if source
            .as_bytes()
            .get(i)
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            continue;
        }
        let mut j = i;
        if let Some((_, after)) = bracketed(source, j) {
            j = after;
        }
        let j = skip_space(source, j);
        if let Some((arg, _)) = braced(source, j) {
            return Some(arg);
        }
    }
    None
}

fn all_arguments(source: &str, command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = source[from..].find(command) {
        let i = from + found + command.len();
        from = i;
        if source
            .as_bytes()
            .get(i)
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            continue;
        }
        let mut j = i;
        if let Some((_, after)) = bracketed(source, j) {
            j = after;
        }
        let j = skip_space(source, j);
        if let Some((arg, end)) = braced(source, j) {
            out.push(arg.to_owned());
            from = end;
        }
    }
    out
}

fn environment_body<'a>(source: &'a str, env: &str) -> Option<&'a str> {
    let open = format!("\\begin{{{env}}}");
    let close = format!("\\end{{{env}}}");
    let start = source.find(&open)? + open.len();
    let end = source[start..].find(&close)? + start;
    Some(&source[start..end])
}

const MAX_MACROS: usize = 500;
const MAX_MACRO_BODY: usize = 2_000;

/// Math macros the preamble defines, for rendering statements outside TeX:
/// `\newcommand`, `\renewcommand`, `\providecommand` (without an optional
/// default argument) and `\DeclareMathOperator(*)`. Names map to bodies in
/// KaTeX's macro syntax (`#1` for arguments). Counters, lengths and page
/// styles are not math and are skipped.
pub fn macros(preamble: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut add = |name: &str, body: String| {
        let name = name.trim();
        let valid = name.len() > 1
            && name.starts_with('\\')
            && name[1..].chars().all(|c| c.is_ascii_alphabetic())
            && !name.starts_with("\\the")
            && !name.contains("rule")
            && body.len() <= MAX_MACRO_BODY;
        if valid && out.len() < MAX_MACROS {
            out.insert(name.to_owned(), body);
        }
    };
    for command in ["\\newcommand", "\\renewcommand", "\\providecommand"] {
        let mut from = 0;
        while let Some(found) = preamble[from..].find(command) {
            let mut i = from + found + command.len();
            from = i;
            if preamble.as_bytes().get(i) == Some(&b'*') {
                i += 1;
            }
            let i = skip_space(preamble, i);
            let (name, mut j) = match braced(preamble, i) {
                Some((n, e)) => (n.to_owned(), e),
                None => {
                    // `\newcommand\name{...}`
                    let end = preamble[i + 1..]
                        .find(|c: char| !c.is_ascii_alphabetic())
                        .map(|k| i + 1 + k)
                        .unwrap_or(preamble.len());
                    (preamble[i..end].to_owned(), end)
                }
            };
            if let Some((_, after)) = bracketed(preamble, j) {
                j = after;
                // An optional default argument has no KaTeX equivalent.
                if bracketed(preamble, j).is_some() {
                    continue;
                }
            }
            if let Some((body, end)) = braced(preamble, skip_space(preamble, j)) {
                add(&name, body.trim().to_owned());
                from = end;
            }
        }
    }
    let mut from = 0;
    while let Some(found) = preamble[from..].find("\\DeclareMathOperator") {
        let mut i = from + found + "\\DeclareMathOperator".len();
        from = i;
        let star = preamble.as_bytes().get(i) == Some(&b'*');
        if star {
            i += 1;
        }
        let Some((name, j)) = braced(preamble, skip_space(preamble, i)) else {
            continue;
        };
        let Some((text, end)) = braced(preamble, skip_space(preamble, j)) else {
            continue;
        };
        let op = if star {
            "\\operatorname*"
        } else {
            "\\operatorname"
        };
        add(name, format!("{op}{{{}}}", text.trim()));
        from = end;
    }
    out
}

/// Parse an unpacked paper.
pub fn parse(files: &Files) -> LatexResult<ParsedPaper> {
    let main = main_file(files)?;
    let mut warnings = Vec::new();
    let source = expand(files, &main, &mut warnings);
    let mut envs = theorem_environments(&source);
    for (env, name) in DEFAULTS {
        envs.entry(env.to_owned())
            .or_insert_with(|| name.to_owned());
    }
    let body_start = source.find("\\begin{document}").unwrap_or(0);
    let macros = macros(&source[..body_start]);
    let deferred = deferred_proofs(&source);

    let title = command_argument(&source, "\\title")
        .map(plain)
        .filter(|t| !t.is_empty());
    let mut authors: Vec<String> = all_arguments(&source, "\\author")
        .iter()
        .flat_map(|a| author_names(a))
        .collect();
    authors.dedup();
    let abstract_text = environment_body(&source, "abstract")
        .map(|a| a.trim().to_owned())
        .filter(|a| !a.is_empty());

    // Sections in document order, for locating statements.
    let mut sections: Vec<(usize, String)> = Vec::new();
    for command in ["\\section", "\\section*"] {
        let mut from = body_start;
        while let Some(found) = source[from..].find(command) {
            let i = from + found;
            let after = i + command.len();
            from = after;
            if command == "\\section" && source.as_bytes().get(after) == Some(&b'*') {
                continue;
            }
            let mut j = after;
            if let Some((_, k)) = bracketed(&source, j) {
                j = k;
            }
            let j = skip_space(&source, j);
            if let Some((name, _)) = braced(&source, j) {
                sections.push((i, plain(name)));
            }
        }
    }
    sections.sort();

    let mut statements = Vec::new();
    let mut from = body_start;
    while let Some(found) = source[from..].find("\\begin{") {
        let open = from + found;
        let name_start = open + "\\begin".len();
        let Some((env, after_name)) = braced(&source, name_start) else {
            from = open + 1;
            continue;
        };
        let env = env.trim().to_owned();
        let Some(display) = envs.get(&env).cloned() else {
            from = after_name;
            continue;
        };
        let Some(kind) = StatementKind::from_name(&display) else {
            from = after_name;
            continue;
        };
        let (title, content_start) = match bracketed(&source, after_name) {
            Some((t, end)) => (Some(statement_title(t)), end),
            None => (None, after_name),
        };
        // Matching \end, allowing nested environments of the same name.
        let open_tag = format!("\\begin{{{env}}}");
        let close_tag = format!("\\end{{{env}}}");
        let mut depth = 1;
        let mut cursor = content_start;
        let mut close_at = None;
        while depth > 0 {
            let next_open = source[cursor..].find(&open_tag).map(|i| i + cursor);
            let Some(next_close) = source[cursor..].find(&close_tag).map(|i| i + cursor) else {
                break;
            };
            match next_open {
                Some(o) if o < next_close => {
                    depth += 1;
                    cursor = o + open_tag.len();
                }
                _ => {
                    depth -= 1;
                    cursor = next_close + close_tag.len();
                    if depth == 0 {
                        close_at = Some(next_close);
                    }
                }
            }
        }
        let Some(close) = close_at else {
            warnings.push(format!("unterminated {env} environment"));
            from = after_name;
            continue;
        };
        let raw = &source[content_start..close];
        let label = all_arguments(raw, "\\label")
            .into_iter()
            .next()
            .map(|l| l.trim().to_owned());
        let body = strip_labels(raw);
        let after_end = cursor;
        let mut k = skip_space(&source, after_end);
        while source[k..].starts_with("\\label") {
            k = braced(&source, k + "\\label".len())
                .map(|(_, e)| skip_space(&source, e))
                .unwrap_or(k + 1);
        }
        let has_proof = source[k..].starts_with("\\begin{proof}")
            || label.as_ref().is_some_and(|l| deferred.contains(l));
        let section = sections
            .iter()
            .rev()
            .find(|(at, _)| *at < open)
            .map(|(_, name)| name.clone());
        statements.push(Statement {
            index: statements.len() + 1,
            kind,
            environment: env,
            display_name: display,
            title: title.filter(|t| !t.is_empty()),
            label,
            body: body.trim().to_owned(),
            has_proof,
            section,
        });
        from = after_end;
    }

    Ok(ParsedPaper {
        main_file: main,
        title,
        authors,
        abstract_text,
        statements,
        macros,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> Files {
        pairs
            .iter()
            .map(|(p, b)| (p.to_string(), b.as_bytes().to_vec()))
            .collect()
    }

    const MAIN: &str = r#"\documentclass{amsart}
\newtheorem{thm}{Theorem}[section]
\newtheorem{lem}[thm]{Lemma}
\newtheorem*{conj*}{Conjecture}
\theoremstyle{definition}
\newtheorem{defn}[thm]{Definition}
\title[Short]{Zero runs of Raney numbers\thanks{Supported by X.}}
\author{Sen-Peng Eu \and Wenlin Zhang}
\begin{document}
\begin{abstract}
We classify the zero runs. % a comment with \begin{thm}
\end{abstract}
\maketitle
\section{Introduction}
\begin{thm}[Main theorem]\label{thm:main}
For all $p \geq 2$, the spectrum is $\{1, 2\}$.
\end{thm}
\begin{proof} Combine Lemma~\ref{lem:a}. \end{proof}
\input{sections/two}
\end{document}
"#;

    const TWO: &str = r#"\section{Lemmas and open questions}
\begin{defn} A run is a block. \end{defn}
\begin{lem}\label{lem:a}
Every run has length at most $2$, and 100\% of them are short.
\end{lem}
\begin{conj*}
The spectrum is $\{1,2\}$ for every $r$.
\end{conj*}
"#;

    #[test]
    fn reads_structure() {
        let paper = parse(&files(&[
            ("paper/main.tex", MAIN),
            ("paper/sections/two.tex", TWO),
            ("other.tex", "\\section{x}"),
        ]))
        .unwrap();
        assert_eq!(paper.main_file, "paper/main.tex");
        assert_eq!(paper.title.as_deref(), Some("Zero runs of Raney numbers"));
        assert_eq!(paper.authors, ["Sen-Peng Eu", "Wenlin Zhang"]);
        assert_eq!(
            paper.abstract_text.as_deref(),
            Some("We classify the zero runs.")
        );
        let kinds: Vec<_> = paper.statements.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                StatementKind::Theorem,
                StatementKind::Lemma,
                StatementKind::Conjecture
            ],
            "definitions are skipped"
        );
        let main = &paper.statements[0];
        assert_eq!(main.title.as_deref(), Some("Main theorem"));
        assert_eq!(main.label.as_deref(), Some("thm:main"));
        assert!(main.has_proof);
        assert!(!main.body.contains("\\label"));
        assert_eq!(main.section.as_deref(), Some("Introduction"));
        let lemma = &paper.statements[1];
        assert!(
            lemma.body.contains("100\\% of them"),
            "escaped percent survives comment stripping"
        );
        assert!(!lemma.has_proof);
        assert_eq!(lemma.section.as_deref(), Some("Lemmas and open questions"));
        assert!(paper.statements[2].kind.is_open());
        assert!(paper.warnings.is_empty(), "{:?}", paper.warnings);
    }

    #[test]
    fn preamble_macros() {
        let preamble = r#"\newcommand{\rep}{\operatorname{rep}}
\newcommand\Z{\mathbb{Z}}
\newcommand{\code}[1]{\texttt{#1}}
\newcommand{\opt}[2][x]{#1+#2}
\renewcommand{\headrulewidth}{0.3pt}
\renewcommand{\thesection}{S\arabic{section}}
\DeclareMathOperator{\lcm}{lcm}
\DeclareMathOperator*{\argmax}{arg\,max}"#;
        let m = macros(preamble);
        assert_eq!(m["\\rep"], "\\operatorname{rep}");
        assert_eq!(m["\\Z"], "\\mathbb{Z}");
        assert_eq!(m["\\code"], "\\texttt{#1}");
        assert_eq!(m["\\lcm"], "\\operatorname{lcm}");
        assert_eq!(m["\\argmax"], "\\operatorname*{arg\\,max}");
        assert!(
            !m.contains_key("\\opt")
                && !m.contains_key("\\headrulewidth")
                && !m.contains_key("\\thesection")
        );
    }

    #[test]
    fn affiliations_cleveref_labels_and_deferred_proofs() {
        let source = r#"\documentclass{article}
\newtheorem{theorem}{Theorem}
\newtheorem{lemma}{Lemma}
\title{A description\\of a recurrence}
\author{
Ada Author\\[-2pt]
\small ORCID \href{https://orcid.org/0000}{\texttt{0000}}
\and
Bo Builder\\[-2pt] \small Some University\\[-2pt] \small\texttt{b@u.edu}
}
\begin{document}
\begin{theorem}\label{thm:main} Every term is bounded. \end{theorem}
\begin{lemma}\label[lemma]{lem:a} A step. \end{lemma}
\begin{proof} Direct. \end{proof}
\begin{proof}[Proof of Theorem~\ref{thm:main}] By Lemma~\ref{lem:a}. \end{proof}
\end{document}"#;
        let paper = parse(&files(&[("main.tex", source)])).unwrap();
        assert_eq!(
            paper.title.as_deref(),
            Some("A description of a recurrence")
        );
        assert_eq!(paper.authors, ["Ada Author", "Bo Builder"]);
        let (theorem, lemma) = (&paper.statements[0], &paper.statements[1]);
        assert!(theorem.has_proof, "the proof given later names the theorem");
        assert_eq!(lemma.label.as_deref(), Some("lem:a"));
        assert_eq!(lemma.body, "A step.");
    }

    #[test]
    fn tabular_authors_from_arxiv_2609_25128() {
        let source = r#"\documentclass{article}
\author{\begin{tabular}{c}
Haobo Ma$^{1,2}$ \qquad Rafik Sahbi$^3$ \qquad Wenlin Zhang$^{1,4}$\\[6pt]
\small $^1$The Omega Institute\\
\small $^2$ChronoAI Pte Ltd\\
\small $^3$Department of Fundamental Science and Technology\\
\small National Higher School of Advanced Technologies\\
\small B.P. 474, Martyrs Square, Algiers 16001, Algeria\\
\small $^4$National University of Singapore\\
\small 21 Lower Kent Ridge Road, Singapore 119077\\[4pt]
\small \texttt{auric@aelf.io}; \texttt{r.sahbi@g.essa-alger.edu.dz}\\
\small \texttt{e1327962@u.nus.edu}
\end{tabular}}
\begin{document}\end{document}"#;
        let paper = parse(&files(&[("main.tex", source)])).unwrap();
        assert_eq!(paper.authors, ["Haobo Ma", "Rafik Sahbi", "Wenlin Zhang"]);
    }

    #[test]
    fn author_layouts_separators_and_annotations() {
        let cases = [
            (
                r"\begin{center}\begin{tabular}[t]{ccc}
Ada Author\textsuperscript{1,2} & Bo Builder\thanks{Support from A and B, Inc.\\Thanks.} & Chloé Chen\inst{3}
\\[4pt]University, City and Country\\\texttt{a@example.org}
\end{tabular}\end{center}",
                vec!["Ada Author", "Bo Builder", "Chloé Chen"],
            ),
            (
                r"\begin{center}Ada Author$^\dagger$ \quad Bo Builder${}^{2}$ and Chloé Chen\footnotemark[3]\\Institute\end{center}",
                vec!["Ada Author", "Bo Builder", "Chloé Chen"],
            ),
            (
                r"Ada Author\footnotemark and Bo Builder\footnote{Contact, address} , Chloé Chen\inst{1,2}",
                vec!["Ada Author", "Bo Builder", "Chloé Chen"],
            ),
            (
                r"Ada Author\\University, City \and Bo Builder\\Institute and Address",
                vec!["Ada Author", "Bo Builder"],
            ),
            (
                r"\begin{tabular*}{\textwidth}{c}Ada Author \and Bo Builder\end{tabular*}",
                vec!["Ada Author", "Bo Builder"],
            ),
        ];
        for (entry, expected) in cases {
            let source = format!(
                "\\documentclass{{article}}\\author{{{entry}}}\\begin{{document}}\\end{{document}}"
            );
            let paper = parse(&files(&[("main.tex", &source)])).unwrap();
            assert_eq!(paper.authors, expected, "{entry}");
        }
        assert_eq!(author_names(r"Ada \anderson"), [r"Ada \anderson"]);
    }

    #[test]
    fn statement_titles_with_citations_keep_math() {
        let source = r#"\documentclass{article}
\begin{document}
\begin{theorem}[Equivalent form of the grid $3$-path-cover formula {\cite{Bresar2013,JakovacTaranenko2013}}]
\label{thm:gridbeta} A grid formula.
\end{theorem}
\begin{lemma}[Estimate for $x^{2}+\frac{a}{b}$ {{\citep[Thm.~2]{alpha, beta}}}]
An estimate.
\end{lemma}
\end{document}"#;
        let paper = parse(&files(&[("main.tex", source)])).unwrap();
        assert_eq!(
            paper.statements[0].title.as_deref(),
            Some(
                "Equivalent form of the grid $3$-path-cover formula [Bresar2013, JakovacTaranenko2013]"
            )
        );
        assert_eq!(
            paper.statements[1].title.as_deref(),
            Some(r"Estimate for $x^{2}+\frac{a}{b}$ [alpha, beta, Thm. 2]")
        );
        assert_eq!(statement_title(r"By \citet{alpha}"), "By [alpha]");
        assert_eq!(
            statement_title(r"An unknown \custom{a,b} and \(x^{2}\)"),
            r"An unknown \custom{a,b} and \(x^{2}\)"
        );
    }

    #[test]
    fn defaults_declaretheorem_and_nesting() {
        let source = r#"\documentclass{llncs}
\declaretheorem[name=Main Theorem]{mainresult}
\begin{document}
\begin{theorem} Outer \begin{theorem} inner \end{theorem} still outer. \end{theorem}
\begin{mainresult} Big. \end{mainresult}
\begin{question} Is it? \end{question}
\input{missing}
\end{document}"#;
        let paper = parse(&files(&[("x.tex", source)])).unwrap();
        assert_eq!(paper.statements.len(), 3);
        assert!(paper.statements[0].body.contains("still outer"));
        assert_eq!(paper.statements[1].display_name, "Main Theorem");
        assert_eq!(paper.statements[2].kind, StatementKind::Question);
        assert_eq!(paper.warnings, ["unresolved input missing"]);
    }

    #[test]
    fn no_main_file_is_an_error() {
        assert!(parse(&files(&[("a.tex", "\\section{x}")])).is_err());
    }

    #[test]
    fn kinds_from_names() {
        assert_eq!(
            StatementKind::from_name("Main Theorem"),
            Some(StatementKind::Theorem)
        );
        assert_eq!(
            StatementKind::from_name("Open Problem"),
            Some(StatementKind::Question)
        );
        assert_eq!(StatementKind::from_name("Definition"), None);
        assert_eq!(StatementKind::from_name("Remark"), None);
    }
}
