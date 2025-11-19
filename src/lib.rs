use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule, PyString};
use pyo3::Bound;
use std::iter::Peekable;
use std::str::Chars;

struct Parser<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Parser {
            chars: source.chars().peekable(),
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            let Some(&ch) = self.chars.peek() else {
                return;
            };

            if ch.is_whitespace() {
                self.chars.next();
                continue;
            }

            if ch == '#' {
                self.consume_until_newline();
                continue;
            }

            // Markdown-style fenced code blocks: ```json ... ```
            if ch == '`' {
                let mut la = self.chars.clone();
                let first = la.next();
                let second = la.next();
                let third = la.next();
                if matches!((first, second, third), (Some('`'), Some('`'), Some('`'))) {
                    // consume the opening ```
                    self.chars.next();
                    self.chars.next();
                    self.chars.next();
                    self.consume_fence_block();
                    continue;
                }
            }

            if ch == '/' {
                let mut la = self.chars.clone();
                la.next(); // consume '/'
                match la.next() {
                    Some('/') => {
                        self.consume_until_newline();
                        continue;
                    }
                    Some('*') => {
                        self.chars.next();
                        self.chars.next();
                        self.consume_block_comment();
                        continue;
                    }
                    _ => {}
                }
            }

            return;
        }
    }

    fn consume_until_newline(&mut self) {
        while let Some(ch) = self.chars.next() {
            if ch == '\n' {
                break;
            }
        }
    }

    fn consume_block_comment(&mut self) {
        let mut last_was_star = false;
        while let Some(ch) = self.chars.next() {
            if last_was_star && ch == '/' {
                return;
            }
            last_was_star = ch == '*';
        }
    }

    fn consume_fence_block(&mut self) {
        // Skip until the next ``` or EOF. We don't try to interpret the language tag.
        let mut backtick_count = 0usize;
        while let Some(ch) = self.chars.next() {
            if ch == '`' {
                backtick_count += 1;
                if backtick_count == 3 {
                    return;
                }
            } else {
                backtick_count = 0;
            }
        }
    }

    fn parse_value(&mut self, py: Python<'a>) -> PyResult<PyObject> {
        self.skip_whitespace_and_comments();

        let Some(&ch) = self.chars.peek() else {
            return Err(PyValueError::new_err(
                "Unexpected end of input while expecting a value",
            ));
        };

        match ch {
            '{' => self.parse_object(py),
            '[' => self.parse_array(py),
            '"' | '\'' => self.parse_string(py),
            't' | 'T' => {
                if self.match_literal("true") {
                    Ok(true.into_py(py))
                } else {
                    Err(PyValueError::new_err("Invalid boolean literal"))
                }
            }
            'f' | 'F' => {
                if self.match_literal("false") {
                    Ok(false.into_py(py))
                } else {
                    Err(PyValueError::new_err("Invalid boolean literal"))
                }
            }
            'n' | 'N' => {
                if self.match_literal("null") {
                    Ok(py.None())
                } else {
                    Err(PyValueError::new_err("Invalid null literal"))
                }
            }
            '-' | '0'..='9' => self.parse_number(py),
            _ => Err(PyValueError::new_err(format!(
                "Unexpected character {ch:?} while parsing value"
            ))),
        }
    }

    fn parse_object(&mut self, py: Python<'a>) -> PyResult<PyObject> {
        let dict = PyDict::new_bound(py);
        self.chars.next(); // skip '{'

        loop {
            self.skip_whitespace_and_comments();
            let ch = self.chars.peek().copied();

            if ch.is_none() || ch == Some('}') {
                if ch == Some('}') {
                    self.chars.next();
                }
                return Ok(dict.into());
            }

            if ch == Some(',') {
                self.chars.next();
                continue;
            }

            let key_obj = self.parse_value(py)?;
            if key_obj.downcast_bound::<PyString>(py).is_err() {
                return Err(PyValueError::new_err(
                    "Object keys must be strings in json_repair_rust",
                ));
            }

            self.skip_whitespace_and_comments();
            match self.chars.peek().copied() {
                Some(':') => {
                    self.chars.next();
                }
                _ => {
                    return Err(PyValueError::new_err(
                        "Expected ':' after object key in json_repair_rust",
                    ));
                }
            }

            let value = self.parse_value(py)?;
            dict.set_item(&key_obj, value)?;

            self.skip_whitespace_and_comments();
            let ch = self.chars.peek().copied();
            if ch == Some(',') {
                self.chars.next();
                continue;
            }
            if ch == Some('}') {
                self.chars.next();
                return Ok(dict.into_py(py));
            }
            if ch.is_none() {
                return Ok(dict.into_py(py));
            }
            return Err(PyValueError::new_err(
                "Expected ',' or '}' in object in json_repair_rust",
            ));
        }
    }

    fn parse_array(&mut self, py: Python<'a>) -> PyResult<PyObject> {
        let list = PyList::empty_bound(py);
        self.chars.next(); // skip '['

        loop {
            self.skip_whitespace_and_comments();
            let ch = self.chars.peek().copied();

            if ch.is_none() || ch == Some(']') {
                if ch == Some(']') {
                    self.chars.next();
                }
                return Ok(list.into_py(py));
            }
            if ch == Some(',') {
                self.chars.next();
                continue;
            }

            let value = self.parse_value(py)?;
            list.append(value)?;

            self.skip_whitespace_and_comments();
            let ch = self.chars.peek().copied();
            if ch == Some(',') {
                self.chars.next();
                continue;
            }
            if ch == Some(']') {
                self.chars.next();
                return Ok(list.into_py(py));
            }
            if ch.is_none() {
                return Ok(list.into_py(py));
            }
            return Err(PyValueError::new_err(
                "Expected ',' or ']' in array in json_repair_rust",
            ));
        }
    }

    fn parse_string(&mut self, py: Python<'a>) -> PyResult<PyObject> {
        let quote = self.chars.next().ok_or_else(|| {
            PyValueError::new_err("Unexpected end of input while starting string")
        })?;
        let mut out = String::new();

        while let Some(ch) = self.chars.next() {
            if ch == '\\' {
                let Some(esc) = self.chars.next() else {
                    break;
                };
                match esc {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\x08'),
                    'f' => out.push('\x0c'),
                    '"' | '\'' | '\\' | '/' => out.push(esc),
                    'u' => {
                        // Stack buffer of chars; preserve all read chars on failure.
                        let mut buffer = ['\0'; 4];
                        let mut count = 0usize;
                        let mut valid_hex = true;
                        for i in 0..4 {
                            if let Some(h) = self.chars.next() {
                                if !h.is_ascii_hexdigit() {
                                    valid_hex = false;
                                }
                                buffer[i] = h;
                                count += 1;
                            } else {
                                valid_hex = false;
                                break;
                            }
                        }
                        if valid_hex && count == 4 {
                            let s: String = buffer.iter().collect();
                            if let Ok(code) = u32::from_str_radix(&s, 16) {
                                if let Some(c) = char::from_u32(code) {
                                    out.push(c);
                                    continue;
                                }
                            }
                        }
                        // Malformed unicode escape: emit "\u" plus whatever we consumed
                        out.push_str("\\u");
                        for i in 0..count {
                            out.push(buffer[i]);
                        }
                    }
                    other => {
                        // Preserve unknown escapes like \w as two characters
                        out.push('\\');
                        out.push(other);
                    }
                }
                continue;
            }

            if ch == quote {
                return Ok(PyString::new_bound(py, &out).into_py(py));
            }

            out.push(ch);
        }

        Ok(PyString::new_bound(py, &out).into_py(py))
    }

    fn parse_number(&mut self, py: Python<'a>) -> PyResult<PyObject> {
        let mut s = String::new();
        while let Some(&ch) = self.chars.peek() {
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
                s.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }

        if s.contains('.') || s.contains('e') || s.contains('E') {
            if let Ok(f) = s.parse::<f64>() {
                return Ok(f.into_py(py));
            }
        } else if let Ok(i) = s.parse::<i64>() {
            return Ok(i.into_py(py));
        } else {
            // Fallback: delegate big integers to Python's arbitrary-precision int
            let builtins = py.import_bound("builtins")?;
            let py_int = builtins.getattr("int")?.call1((s.clone(),))?;
            return Ok(py_int.into());
        }

        Err(PyValueError::new_err(format!(
            "Invalid number literal {s:?} in json_repair_rust"
        )))
    }

    fn match_literal(&mut self, expected: &str) -> bool {
        let mut cursor = self.chars.clone();
        for c in expected.chars() {
            match cursor.next() {
                Some(got) if got.to_ascii_lowercase() == c => {}
                _ => return false,
            }
        }
        // Only now advance the real iterator
        for _ in 0..expected.len() {
            if self.chars.next().is_none() {
                break;
            }
        }
        true
    }
}

#[pyfunction]
fn repair_json(py: Python<'_>, json_str: &str) -> PyResult<PyObject> {
    let mut parser = Parser::new(json_str);
    parser.parse_value(py)
}

#[pymodule]
fn json_repair_rust(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction_bound!(repair_json, py)?)?;
    Ok(())
}
