//! A small JSON reader that keeps where every value starts (line and
//! column, both from 1) and every object's keys in file order, duplicates
//! included, so a hand-written mappings file gets errors that point at the
//! exact line ("line 7, column 15, CAM1 on Main: ...").

use std::fmt;

/// A position in the file: line and column, both counted from 1. Columns
/// count characters, not bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub column: u32,
}

/// A JSON value and where it starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub pos: Pos,
    pub value: Value,
}

/// One object member: its key, where the key starts, and its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub key: String,
    pub key_pos: Pos,
    pub node: Node,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Node>),
    Object(Vec<Member>),
}

impl Value {
    /// What kind of value this is, for "expected a string, found a number".
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "true/false",
            Value::Number(_) => "a number",
            Value::String(_) => "a string",
            Value::Array(_) => "a list",
            Value::Object(_) => "an object",
        }
    }
}

/// A syntax error and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub pos: Pos,
    pub message: String,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.pos.line, self.pos.column, self.message
        )
    }
}

/// Parses `text` as one JSON value (a leading byte order mark is skipped).
pub fn parse(text: &str) -> Result<Node, SyntaxError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut p = Parser {
        chars: text.chars().collect(),
        at: 0,
        line: 1,
        column: 1,
        depth: 0,
    };
    p.skip_space();
    let node = p.value()?;
    p.skip_space();
    if p.peek().is_some() {
        return Err(p.error("there is more after the end of the file's object"));
    }
    Ok(node)
}

/// Deeper nesting is refused rather than overflowing the stack.
const MAX_DEPTH: usize = 64;

struct Parser {
    chars: Vec<char>,
    at: usize,
    line: u32,
    column: u32,
    depth: usize,
}

impl Parser {
    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            column: self.column,
        }
    }

    fn error(&self, message: impl Into<String>) -> SyntaxError {
        SyntaxError {
            pos: self.pos(),
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.bump();
        }
    }

    fn expect(&mut self, want: char, what: &str) -> Result<(), SyntaxError> {
        match self.peek() {
            Some(c) if c == want => {
                self.bump();
                Ok(())
            }
            Some(c) => Err(self.error(format!("expected {what}, found {}", shown(c)))),
            None => Err(self.error(format!("expected {what}, but the file ends"))),
        }
    }

    fn value(&mut self) -> Result<Node, SyntaxError> {
        let pos = self.pos();
        let value = match self.peek() {
            None => return Err(self.error("the file ends where a value should be")),
            Some('{') => self.object()?,
            Some('[') => self.array()?,
            Some('"') => Value::String(self.string()?),
            Some('t') => self.word("true", Value::Bool(true))?,
            Some('f') => self.word("false", Value::Bool(false))?,
            Some('n') => self.word("null", Value::Null)?,
            Some(c) if c == '-' || c.is_ascii_digit() => self.number()?,
            Some('\'') => {
                return Err(
                    self.error("text needs double quotes (\"like this\"), not single quotes")
                );
            }
            Some(c) => {
                return Err(self.error(format!(
                    "expected a value, found {}{}",
                    shown(c),
                    if c.is_alphabetic() {
                        " (text needs double quotes)"
                    } else {
                        ""
                    }
                )));
            }
        };
        Ok(Node { pos, value })
    }

    fn word(&mut self, word: &str, value: Value) -> Result<Value, SyntaxError> {
        let start = self.pos();
        for want in word.chars() {
            if self.peek() != Some(want) {
                return Err(SyntaxError {
                    pos: start,
                    message: format!(
                        "expected a value (text needs double quotes; did you mean {word}?)"
                    ),
                });
            }
            self.bump();
        }
        Ok(value)
    }

    fn nest(&mut self) -> Result<(), SyntaxError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("the file nests too deeply"));
        }
        Ok(())
    }

    fn object(&mut self) -> Result<Value, SyntaxError> {
        self.nest()?;
        self.bump();
        let mut members = Vec::new();
        self.skip_space();
        if self.peek() == Some('}') {
            self.bump();
            self.depth -= 1;
            return Ok(Value::Object(members));
        }
        loop {
            self.skip_space();
            let key_pos = self.pos();
            match self.peek() {
                Some('"') => {}
                Some('}') => return Err(self.error("there is a comma before this }: remove it")),
                Some(c) => {
                    return Err(self.error(format!(
                        "expected a name in double quotes, found {}",
                        shown(c)
                    )));
                }
                None => return Err(self.error("the file ends inside an object: a } is missing")),
            }
            let key = self.string()?;
            self.skip_space();
            self.expect(':', "a : after the name")?;
            self.skip_space();
            let node = self.value()?;
            members.push(Member { key, key_pos, node });
            self.skip_space();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some('}') => {
                    self.bump();
                    self.depth -= 1;
                    return Ok(Value::Object(members));
                }
                Some('"') => return Err(self.error("a comma is missing before this name")),
                Some(c) => return Err(self.error(format!("expected , or }}, found {}", shown(c)))),
                None => return Err(self.error("the file ends inside an object: a } is missing")),
            }
        }
    }

    fn array(&mut self) -> Result<Value, SyntaxError> {
        self.nest()?;
        self.bump();
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(']') {
            self.bump();
            self.depth -= 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_space();
            if self.peek() == Some(']') {
                return Err(self.error("there is a comma before this ]: remove it"));
            }
            items.push(self.value()?);
            self.skip_space();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    self.depth -= 1;
                    return Ok(Value::Array(items));
                }
                Some(c) => return Err(self.error(format!("expected , or ], found {}", shown(c)))),
                None => return Err(self.error("the file ends inside a list: a ] is missing")),
            }
        }
    }

    fn string(&mut self) -> Result<String, SyntaxError> {
        self.bump();
        let mut out = String::new();
        loop {
            let pos = self.pos();
            match self.bump() {
                None => return Err(self.error("the file ends inside text: a \" is missing")),
                Some('"') => return Ok(out),
                Some('\n') => {
                    return Err(SyntaxError {
                        pos,
                        message: "text runs past the end of the line: a \" is missing".into(),
                    });
                }
                Some('\\') => {
                    let escaped = match self.bump() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('/') => '/',
                        Some('b') => '\u{8}',
                        Some('f') => '\u{c}',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('u') => self.unicode_escape(pos)?,
                        _ => {
                            return Err(SyntaxError {
                                pos,
                                message: "a backslash in text must be doubled (write C:\\\\Tools, not C:\\Tools)".into(),
                            });
                        }
                    };
                    out.push(escaped);
                }
                Some(c) if (c as u32) < 0x20 => {
                    return Err(SyntaxError {
                        pos,
                        message: "text cannot contain a tab or control character; write \\t".into(),
                    });
                }
                Some(c) => out.push(c),
            }
        }
    }

    fn hex4(&mut self, pos: Pos) -> Result<u32, SyntaxError> {
        let mut n = 0;
        for _ in 0..4 {
            let digit = self
                .bump()
                .and_then(|c| c.to_digit(16))
                .ok_or(SyntaxError {
                    pos,
                    message: "\\u needs four hex digits".into(),
                })?;
            n = n * 16 + digit;
        }
        Ok(n)
    }

    fn unicode_escape(&mut self, pos: Pos) -> Result<char, SyntaxError> {
        let bad = || SyntaxError {
            pos,
            message: "\\u is not a valid character".into(),
        };
        let first = self.hex4(pos)?;
        let code = if (0xD800..0xDC00).contains(&first) {
            if self.bump() != Some('\\') || self.bump() != Some('u') {
                return Err(bad());
            }
            let second = self.hex4(pos)?;
            if !(0xDC00..0xE000).contains(&second) {
                return Err(bad());
            }
            0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
        } else {
            first
        };
        char::from_u32(code).ok_or_else(bad)
    }

    fn number(&mut self) -> Result<Value, SyntaxError> {
        let start = self.pos();
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let valid = {
            let body = text.strip_prefix('-').unwrap_or(&text);
            let int_end = body.find(['.', 'e', 'E']).unwrap_or(body.len());
            let int = &body[..int_end];
            !int.is_empty()
                && int.chars().all(|c| c.is_ascii_digit())
                && (int == "0" || !int.starts_with('0'))
        };
        match text.parse::<f64>() {
            Ok(n) if valid && n.is_finite() => Ok(Value::Number(n)),
            _ => Err(SyntaxError {
                pos: start,
                message: format!("{text} is not a number"),
            }),
        }
    }
}

fn shown(c: char) -> String {
    match c {
        '\n' => "the end of the line".into(),
        c => format!("{c:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, column: u32) -> Pos {
        Pos { line, column }
    }

    #[test]
    fn values_keep_their_line_and_column_and_key_order() {
        let node =
            parse("{\n  \"b\": 1,\n  \"a\": [true, null, \"x\\u00e9\"],\n  \"b\": -2.5e1\n}")
                .unwrap();
        let Value::Object(members) = node.value else {
            panic!()
        };
        let keys: Vec<_> = members.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["b", "a", "b"]);
        assert_eq!(members[0].key_pos, at(2, 3));
        assert_eq!(members[0].node.pos, at(2, 8));
        assert_eq!(members[2].node.value, Value::Number(-25.0));
        let Value::Array(items) = &members[1].node.value else {
            panic!()
        };
        assert_eq!(items[2].value, Value::String("xé".into()));
        assert_eq!(items[2].pos, at(3, 21));
    }

    #[test]
    fn columns_count_characters_and_a_bom_is_skipped() {
        let node = parse("\u{feff}{\"é\": \"ü\", \"k\": 1}").unwrap();
        let Value::Object(m) = node.value else {
            panic!()
        };
        assert_eq!(m[1].node.pos, at(1, 17));
    }

    #[test]
    fn syntax_errors_say_where_and_what() {
        let cases = [
            ("{\"a\": 1,}", at(1, 9), "comma before this }"),
            ("{\"a\": 1 \"b\": 2}", at(1, 9), "comma is missing"),
            ("{\"a\": 'x'}", at(1, 7), "double quotes"),
            ("{\"a\": \"C:\\Tools\"}", at(1, 10), "backslash"),
            ("{\"a\": open}", at(1, 7), "double quotes"),
            ("{\"a\": 01}", at(1, 7), "not a number"),
            ("{\"a\": 1", at(1, 8), "} is missing"),
            ("{\"a\": 1}}", at(1, 9), "more after the end"),
            ("", at(1, 1), "file ends"),
            ("{\"a\": \"x\n\"}", at(1, 9), "past the end of the line"),
            ("[1, 2,]", at(1, 7), "comma before this ]"),
        ];
        for (text, pos, part) in cases {
            let e = parse(text).unwrap_err();
            assert_eq!(e.pos, pos, "{text}: {e}");
            assert!(e.message.contains(part), "{text}: {e}");
        }
    }

    #[test]
    fn agrees_with_serde_json_on_valid_files() {
        let text = r#"{"x": [1, 2.5, -0, 1e3, "a\"b\\c\/\n", {"y": false}], "z": null}"#;
        fn plain(n: &Node) -> serde_json::Value {
            match &n.value {
                Value::Null => serde_json::Value::Null,
                Value::Bool(b) => (*b).into(),
                Value::Number(x) => serde_json::json!(*x),
                Value::String(s) => s.clone().into(),
                Value::Array(a) => a.iter().map(plain).collect(),
                Value::Object(m) => m
                    .iter()
                    .map(|m| (m.key.clone(), plain(&m.node)))
                    .collect::<serde_json::Map<_, _>>()
                    .into(),
            }
        }
        let ours = plain(&parse(text).unwrap());
        let theirs: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            ours.to_string().replace(".0", ""),
            theirs.to_string().replace(".0", "")
        );
        let deep = "[".repeat(200);
        assert!(parse(&deep).unwrap_err().message.contains("too deeply"));
    }
}
