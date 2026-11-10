//! The flow forms of the YAML reader: `[bug, git]` and `{id: git}`, on one
//! line or joined from several.

use super::{Yaml, quoted};

/// A flow list or mapping, such as `[bug, git]` or `{id: git}`.
pub(super) fn flow(text: &str) -> Result<Yaml, String> {
    let mut parser = Flow { text, at: 0 };
    let value = parser.node()?;
    parser.space();
    if parser.at < text.len() {
        return Err("text after a [list] or {mapping}".to_owned());
    }
    Ok(value)
}

struct Flow<'t> {
    text: &'t str,
    at: usize,
}

impl Flow<'_> {
    fn rest(&self) -> &str {
        self.text.get(self.at..).unwrap_or("")
    }

    fn space(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, c: char) -> bool {
        self.space();
        let found = self.rest().starts_with(c);
        if found {
            self.at += c.len_utf8();
        }
        found
    }

    fn node(&mut self) -> Result<Yaml, String> {
        self.space();
        match self.rest().chars().next() {
            Some('[') => {
                self.at += 1;
                let mut items = Vec::new();
                while !self.eat(']') {
                    items.push(self.node()?);
                    if !self.eat(',') && !self.rest().trim_start().starts_with(']') {
                        return Err("a [list] is not closed".to_owned());
                    }
                }
                Ok(Yaml::List(items))
            }
            Some('{') => {
                self.at += 1;
                let mut entries = Vec::new();
                while !self.eat('}') {
                    let key = self.scalar(true)?;
                    let value = if self.eat(':') {
                        self.node()?
                    } else {
                        Yaml::Scalar(String::new(), false)
                    };
                    entries.push((key.text().unwrap_or("").to_owned(), value));
                    if !self.eat(',') && !self.rest().trim_start().starts_with('}') {
                        return Err("a {mapping} is not closed".to_owned());
                    }
                }
                Ok(Yaml::Map(entries))
            }
            Some('&' | '*' | '!') => Err("an anchor, alias or tag".to_owned()),
            Some(_) => self.scalar(false),
            None => Err("a [list] or {mapping} is not closed".to_owned()),
        }
    }

    fn scalar(&mut self, key: bool) -> Result<Yaml, String> {
        self.space();
        if let Some(q) = self
            .rest()
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')
        {
            let (value, used) = quoted(self.rest(), q)?;
            self.at += used;
            return Ok(Yaml::Scalar(value, true));
        }
        let rest = self.rest();
        let end = rest
            .char_indices()
            .find(|&(i, c)| {
                ",[]{}".contains(c)
                    || (key
                        && c == ':'
                        && rest
                            .get(i + 1..)
                            .is_some_and(|r| r.starts_with([' ', ',', '}']) || r.is_empty()))
            })
            .map_or(rest.len(), |(i, _)| i);
        let text = rest.get(..end).unwrap_or("").trim().to_owned();
        self.at += end;
        Ok(Yaml::Scalar(text, false))
    }
}
