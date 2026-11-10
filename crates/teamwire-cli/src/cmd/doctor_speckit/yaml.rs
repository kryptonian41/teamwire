//! A small YAML reader for spec-kit's `.specify/extensions.yml` and its
//! extension manifests: block mappings and lists, quoted and plain scalars,
//! block scalars, comments, a BOM and CRLF line ends. The workspace has no
//! YAML parser, so this reads the forms spec-kit writes and the hand-written
//! forms `PyYAML` reads the same way.
//!
//! Never: guess. Anchors, aliases, tags, a second document or an indent it
//! cannot follow give an error, never an empty value.

// A path, so the test crates that include this file by path find it too.
#[path = "yaml_flow.rs"]
mod yaml_flow;

use yaml_flow::flow;

/// A YAML value, as far as the spec-kit checks need one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Yaml {
    Map(Vec<(String, Self)>),
    List(Vec<Self>),
    /// A scalar's text, and whether it was quoted (a quoted `false` is a string).
    Scalar(String, bool),
}

impl Yaml {
    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            Self::List(_) | Self::Scalar(..) => None,
        }
    }

    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::Scalar(text, _) => Some(text),
            Self::Map(_) | Self::List(_) => None,
        }
    }

    /// The plain scalars `PyYAML`, which spec-kit reads with, takes as false.
    pub(crate) fn is_false(&self) -> bool {
        matches!(self, Self::Scalar(t, false)
            if ["false", "False", "FALSE", "no", "No", "NO", "off", "Off", "OFF"].contains(&t.as_str()))
    }

    /// An empty value, or a plain scalar `PyYAML` takes as null.
    pub(crate) fn is_null(&self) -> bool {
        matches!(self, Self::Scalar(t, false) if ["", "~", "null", "Null", "NULL"].contains(&t.as_str()))
    }
}

/// One line with its comment cut off: its number, indent and text.
#[derive(Debug, Clone)]
struct Line {
    no: usize,
    indent: usize,
    text: String,
}

/// True when a quote at byte `at` of `text` opens a quoted scalar: it starts
/// a value, not the middle of a plain word such as `it's`.
fn opens_quote(text: &str, at: usize) -> bool {
    text.get(..at)
        .and_then(|before| before.chars().last())
        .is_none_or(|c| c == ' ' || "[{,:-".contains(c))
}

/// The text of a line without its comment.
fn strip_comment(text: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut prev = ' ';
    for (at, c) in text.char_indices() {
        match quote {
            Some('"') if c == '\\' => {}
            Some(q) if c == q && !(q == '"' && prev == '\\') => quote = None,
            Some(_) => {}
            None if (c == '"' || c == '\'') && opens_quote(text, at) => quote = Some(c),
            None if c == '#' && prev == ' ' => return text.get(..at).unwrap_or(text),
            None => {}
        }
        prev = if prev == '\\' && c == '\\' { ' ' } else { c };
    }
    text
}

fn lines_of(text: &str) -> Result<Vec<Line>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = Vec::new();
    for (i, raw) in text.split('\n').enumerate() {
        let no = i + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let body = raw.trim_start_matches(' ');
        if body.starts_with('\t') {
            return Err(format!("line {no} is indented with a tab"));
        }
        let content = strip_comment(body).trim_end();
        if content.is_empty() {
            continue;
        }
        if content == "---" && lines.is_empty() {
            continue;
        }
        if content == "---" || content == "..." || content.starts_with('%') {
            return Err(format!(
                "line {no} starts another YAML document or a directive"
            ));
        }
        lines.push(Line {
            no,
            indent: raw.len() - body.len(),
            text: content.to_owned(),
        });
    }
    Ok(lines)
}

/// Reads `text` as YAML. An empty file is null.
pub(crate) fn parse_yaml(text: &str) -> Result<Yaml, String> {
    let mut reader = Reader {
        lines: lines_of(text)?,
        at: 0,
    };
    let Some(first) = reader.lines.first().map(|l| l.indent) else {
        return Ok(Yaml::Scalar(String::new(), false));
    };
    let value = reader.node(first)?;
    match reader.lines.get(reader.at) {
        Some(line) => Err(format!("line {} is not read: {}", line.no, line.text)),
        None => Ok(value),
    }
}

struct Reader {
    lines: Vec<Line>,
    at: usize,
}

fn is_item(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

/// The key and the rest of a `key: value` line, or `None` for another line.
fn split_key(text: &str) -> Option<(String, &str)> {
    if text.starts_with(['[', '{']) || is_item(text) {
        return None;
    }
    let (key, rest) = if let Some(q) = text.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let (key, used) = quoted(text, q).ok()?;
        (key, text.get(used..)?)
    } else {
        let at = text.match_indices(':').map(|(i, _)| i).find(|&i| {
            text.get(i + 1..)
                .is_some_and(|r| r.is_empty() || r.starts_with(' '))
        })?;
        (text.get(..at)?.trim().to_owned(), text.get(at..)?)
    };
    let rest = rest.strip_prefix(':')?;
    (rest.is_empty() || rest.starts_with(' ')).then(|| (key, rest.trim_start()))
}

impl Reader {
    fn peek(&self) -> Option<&Line> {
        self.lines.get(self.at)
    }

    /// The node whose first line is the current one, at `indent`.
    fn node(&mut self, indent: usize) -> Result<Yaml, String> {
        let Some(line) = self.peek().cloned() else {
            return Ok(Yaml::Scalar(String::new(), false));
        };
        if is_item(&line.text) {
            self.list(indent)
        } else if split_key(&line.text).is_some() {
            self.map(indent)
        } else {
            self.at += 1;
            self.inline(&line.text, indent.saturating_sub(1), line.no)
        }
    }

    fn map(&mut self, indent: usize) -> Result<Yaml, String> {
        let mut entries = Vec::new();
        while let Some(line) = self.peek().cloned() {
            if line.indent < indent {
                break;
            }
            let Some((key, rest)) = split_key(&line.text).filter(|_| line.indent == indent) else {
                return Err(format!(
                    "line {} does not continue the mapping above it",
                    line.no
                ));
            };
            if key.starts_with('?') || key.starts_with(['&', '*', '!']) {
                return Err(format!(
                    "line {} has a key form this reader does not read",
                    line.no
                ));
            }
            self.at += 1;
            let value = self.value(rest, indent, true, line.no)?;
            entries.push((key, value));
        }
        Ok(Yaml::Map(entries))
    }

    fn list(&mut self, indent: usize) -> Result<Yaml, String> {
        let mut items = Vec::new();
        while let Some(line) = self.peek().cloned() {
            if line.indent < indent || (line.indent == indent && !is_item(&line.text)) {
                break;
            }
            if line.indent > indent {
                return Err(format!(
                    "line {} does not continue the list above it",
                    line.no
                ));
            }
            let rest = line.text.get(1..).unwrap_or("");
            let body = rest.trim_start();
            let offset = indent + 1 + rest.len() - body.len();
            if body.is_empty() {
                self.at += 1;
                items.push(self.value("", indent, false, line.no)?);
            } else if is_item(body) || split_key(body).is_some() {
                // `- key: value` or `- - item`: the rest of the line starts a node one level in.
                if let Some(slot) = self.lines.get_mut(self.at) {
                    *slot = Line {
                        no: line.no,
                        indent: offset,
                        text: body.to_owned(),
                    };
                }
                items.push(self.node(offset)?);
            } else {
                self.at += 1;
                items.push(self.inline(body, indent, line.no)?);
            }
        }
        Ok(Yaml::List(items))
    }

    /// The value after `key:` or `-`: on the same line, or a node below.
    fn value(&mut self, rest: &str, indent: usize, map: bool, no: usize) -> Result<Yaml, String> {
        if !rest.is_empty() {
            return self.inline(rest, indent, no);
        }
        match self.peek() {
            Some(next) if next.indent > indent => {
                let at = next.indent;
                self.node(at)
            }
            // A list may sit at its key's own indent.
            Some(next) if map && next.indent == indent && is_item(&next.text) => self.list(indent),
            _ => Ok(Yaml::Scalar(String::new(), false)),
        }
    }

    /// A value written on one line, or continued on the lines below that are
    /// indented more than `indent`.
    fn inline(&mut self, rest: &str, indent: usize, no: usize) -> Result<Yaml, String> {
        if rest.starts_with(['&', '*', '!']) {
            return Err(format!("line {no} has an anchor, alias or tag"));
        }
        if rest.starts_with(['|', '>']) {
            let mut text = Vec::new();
            while let Some(next) = self.peek().filter(|l| l.indent > indent) {
                text.push(next.text.clone());
                self.at += 1;
            }
            return Ok(Yaml::Scalar(text.join("\n"), true));
        }
        let mut text = rest.to_owned();
        let closed = |t: &str| -> bool {
            match t.chars().next() {
                Some('[' | '{') => flow(t).is_ok(),
                Some(q @ ('"' | '\'')) => quoted(t, q).is_ok(),
                _ => true,
            }
        };
        let plain = !rest.starts_with(['[', '{', '"', '\'']);
        while let Some(next) = self.peek().filter(|l| l.indent > indent) {
            if !plain && closed(&text) {
                break;
            }
            // PyYAML refuses a `key: value` that continues a plain value.
            if plain && split_key(&next.text).is_some() {
                return Err(format!("line {} continues a value with a key", next.no));
            }
            text.push(' ');
            text.push_str(&next.text);
            self.at += 1;
        }
        match text.chars().next() {
            Some('[' | '{') => flow(&text).map_err(|e| format!("line {no}: {e}")),
            Some(q @ ('"' | '\'')) => {
                let (value, used) = quoted(&text, q).map_err(|e| format!("line {no}: {e}"))?;
                if text.get(used..).is_some_and(|r| !r.trim().is_empty()) {
                    return Err(format!("line {no} has text after a quoted value"));
                }
                Ok(Yaml::Scalar(value, true))
            }
            _ => Ok(Yaml::Scalar(text.trim().to_owned(), false)),
        }
    }
}

/// The quoted scalar at the start of `text`, and the bytes it used.
pub(super) fn quoted(text: &str, q: char) -> Result<(String, usize), String> {
    let mut out = String::new();
    let mut chars = text.char_indices().skip(1).peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            '\'' if q == '\'' => {
                if chars.peek().is_some_and(|(_, n)| *n == '\'') {
                    chars.next();
                    out.push('\'');
                } else {
                    return Ok((out, at + 1));
                }
            }
            '"' if q == '"' => return Ok((out, at + 1)),
            '\\' if q == '"' => match chars.next().map(|(_, e)| e) {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(e) => out.push(e),
                None => break,
            },
            c => out.push(c),
        }
    }
    Err("a quoted value is not closed".to_owned())
}
