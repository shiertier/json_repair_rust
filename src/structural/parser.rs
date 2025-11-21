use super::schema::{FieldLookup, SchemaNode};
use crate::utils::cursor::Cursor;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyList, PyString};
use std::fmt;

#[derive(Debug)]
pub enum ParseError {
    RecursionLimit,
    MissingField(String),
    InvalidUtf8,
    UnexpectedEof,
}

impl From<ParseError> for PyErr {
    fn from(err: ParseError) -> PyErr {
        match err {
            ParseError::RecursionLimit => {
                pyo3::exceptions::PyRecursionError::new_err("Recursion limit reached")
            }
            ParseError::MissingField(f) => {
                pyo3::exceptions::PyValueError::new_err(format!("Missing field: {}", f))
            }
            ParseError::InvalidUtf8 => pyo3::exceptions::PyValueError::new_err("Invalid UTF-8"),
            ParseError::UnexpectedEof => pyo3::exceptions::PyValueError::new_err("Unexpected EOF"),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::RecursionLimit => write!(f, "Recursion limit reached"),
            ParseError::MissingField(field) => write!(f, "Missing field: {}", field),
            ParseError::InvalidUtf8 => write!(f, "Invalid UTF-8"),
            ParseError::UnexpectedEof => write!(f, "Unexpected EOF"),
        }
    }
}

const MAX_DEPTH: usize = 128;
const MAX_STRING_LEN: usize = 1024 * 1024; // 1MB

pub fn parse_node<'py>(
    cursor: &mut Cursor,
    schema: &SchemaNode,
    py: Python<'py>,
    depth: usize,
) -> Result<PyObject, ParseError> {
    if depth > MAX_DEPTH {
        return Err(ParseError::RecursionLimit);
    }
    cursor.skip_whitespace();

    match schema {
        SchemaNode::PrimitiveString => parse_string_speculative(cursor, py),
        SchemaNode::PrimitiveNumber => parse_number_robust(cursor, py),
        SchemaNode::PrimitiveBool => parse_bool_speculative(cursor, py),
        SchemaNode::Object {
            fields,
            required,
            ac,
        } => parse_object(cursor, fields, required, ac, py, depth),
        SchemaNode::Array(inner) => parse_array(cursor, inner, py, depth),
        _ => Ok(py.None().into()), // Placeholder for Any or unimplemented types
    }
}

fn parse_object<'py>(
    cursor: &mut Cursor,
    fields: &FieldLookup,
    required: &ahash::AHashSet<Vec<u8>>,
    ac: &aho_corasick::AhoCorasick,
    py: Python<'py>,
    depth: usize,
) -> Result<PyObject, ParseError> {
    let dict = PyDict::new(py);
    let mut found_keys = ahash::AHashSet::new(); // 记录找到的 keys

    // 容错：如果没找到 '{'，我们假设已经在里面了（上下文推断），
    // 但标准情况是必须有 '{'
    if cursor.matches(b"{") {
        cursor.advance(1);
    }

    loop {
        cursor.skip_whitespace();

        if cursor.matches(b"}") || cursor.remaining().is_empty() {
            cursor.advance(1);
            break;
        }

        // === 核心推测逻辑 (Aho-Corasick) ===
        // 使用 AC 自动机在剩余文本中搜索所有可能的 Key
        let input = cursor.remaining();
        let mut found_match = false;

        // 限制搜索范围，避免在超大文本中搜索太久
        // 假设 Key 不会离当前位置太远（例如 1KB 内）
        // 注意：AC 搜索是流式的，但为了简单起见，我们先在 slice 上搜
        // 实际上 AC 很快，搜整个 remaining 通常也没问题，除非 remaining 巨大
        // 为了安全起见，我们可以限制搜索窗口，但如果 JSON 结构非常松散，限制窗口可能导致找不到
        // 鉴于 AC 的高性能，我们先尝试搜索整个 remaining (或者一个较大的窗口)

        // 迭代查找所有匹配项
        // println!("DEBUG: Searching in input: {:?}", String::from_utf8_lossy(input));
        for mat in ac.find_iter(input) {
            let _pattern_id = mat.pattern();
            let end = mat.end();
            // println!("DEBUG: Found match at {:?}-{:?}", mat.start(), mat.end());

            // mat.start() 是相对于 input 的偏移
            // 检查匹配项后面是否紧跟着 ':' (允许中间有空格)
            let after_match_idx = end;
            let after_match = &input[after_match_idx..];

            let mut colon_idx = 0;
            while colon_idx < after_match.len() && after_match[colon_idx].is_ascii_whitespace() {
                colon_idx += 1;
            }

            if colon_idx < after_match.len() && after_match[colon_idx] == b':' {
                // 找到了合法的 Key: Value 结构！
                // 1. 移动游标到 Value 开始处
                let value_start_offset = after_match_idx + colon_idx + 1;
                cursor.advance(value_start_offset);

                // 2. 获取 Key 内容
                // pattern_id 对应 compiler.rs 中构建的 patterns
                // patterns 顺序: "key1", 'key1', "key2", 'key2', ...
                // 偶数是双引号，奇数是单引号
                // 实际上我们不需要反查 pattern table，直接从 input 提取即可
                // mat.start() .. mat.end() 是引号包含的 Key
                let key_quote_content = &input[mat.start()..mat.end()];
                // 去掉引号
                let key_content = &key_quote_content[1..key_quote_content.len() - 1];

                // 3. 解析 Value
                if let Some(sub_schema) = fields.get(key_content) {
                    let val = parse_node(cursor, sub_schema, py, depth + 1)?;

                    // 安全的 UTF-8 转换
                    let key_str = String::from_utf8_lossy(key_content);
                    dict.set_item(key_str, val)
                        .map_err(|_| ParseError::InvalidUtf8)?;
                    found_keys.insert(key_content.to_vec());

                    found_match = true;
                    break; // 处理完一个 Key 后，跳出搜索循环，继续外层循环寻找下一个 Key
                }
            }
        }

        if !found_match {
            // 找不到任何已知的 Key 了
            break;
        }

        cursor.skip_whitespace();
        if cursor.matches(b",") {
            cursor.advance(1);
        }
    }

    // === 审计阶段 ===
    for req in required {
        if !found_keys.contains(req) {
            return Err(ParseError::MissingField(
                String::from_utf8_lossy(req).to_string(),
            ));
        }
    }

    Ok(dict.into())
}

fn parse_array<'py>(
    cursor: &mut Cursor,
    inner: &SchemaNode,
    py: Python<'py>,
    depth: usize,
) -> Result<PyObject, ParseError> {
    let list = PyList::empty(py);

    if cursor.matches(b"[") {
        cursor.advance(1);
    }

    loop {
        cursor.skip_whitespace();
        if cursor.matches(b"]") || cursor.remaining().is_empty() {
            cursor.advance(1);
            break;
        }

        let start_pos = cursor.pos;
        let val = parse_node(cursor, inner, py, depth + 1)?;
        list.append(val).map_err(|_| ParseError::InvalidUtf8)?;

        if cursor.pos == start_pos {
            // Stuck! Force advance to avoid infinite loop
            if !cursor.remaining().is_empty() {
                cursor.advance(1);
            } else {
                break;
            }
        }

        cursor.skip_whitespace();
        if cursor.matches(b",") {
            cursor.advance(1);
        }
    }

    Ok(list.into())
}

/// 鲁棒的数字解析
fn parse_number_robust<'py>(cursor: &mut Cursor, py: Python<'py>) -> Result<PyObject, ParseError> {
    let _start = cursor.pos;
    let input = cursor.remaining();
    let mut end = 0;

    // 贪婪匹配所有可能组成数字的字符
    // 容忍 '1,000' 中的逗号
    while end < input.len() {
        match input[end] {
            b'0'..=b'9' | b'.' | b'-' | b'+' | b'e' | b'E' | b',' => end += 1,
            _ => break,
        }
    }

    cursor.advance(end);
    let raw_bytes = &input[..end];

    // 优化：先检查是否存在逗号。memchr 极快。
    let has_comma = memchr::memchr(b',', raw_bytes).is_some();

    let float_val = if !has_comma {
        // 快乐路径：完全零拷贝
        // 安全性：我们在上面的循环里只允许了 [0-9.-+eE]
        let s = unsafe { std::str::from_utf8_unchecked(raw_bytes) };
        s.parse::<f64>().unwrap_or(0.0)
    } else {
        // 悲伤路径：只有遇到逗号才分配内存
        let s = String::from_utf8_lossy(raw_bytes);
        s.replace(',', "").parse::<f64>().unwrap_or(0.0)
    };

    Ok(PyFloat::new(py, float_val).into())
}

/// 推测性字符串解析
fn parse_string_speculative<'py>(
    cursor: &mut Cursor,
    py: Python<'py>,
) -> Result<PyObject, ParseError> {
    let start_quote = if cursor.matches(b"\"") {
        Some(b'"')
    } else if cursor.matches(b"'") {
        Some(b'\'')
    } else if cursor.matches("＂".as_bytes()) {
        Some(b'\x82') // Marker for fullwidth quote (last byte of EF BC 82)
    } else {
        None
    };

    if let Some(quote_type) = start_quote {
        if quote_type == b'"' || quote_type == b'\'' {
            cursor.advance(1);
        } else {
            cursor.advance(3); // Fullwidth quote is 3 bytes
        }

        // Quoted string mode: STRICT
        let input = cursor.remaining();
        let mut len = 0;
        let mut escape = false;

        while len < input.len() {
            if len > MAX_STRING_LEN {
                // String too long
                return Ok(PyString::new(py, &String::from_utf8_lossy(&input[..len])).into());
            }

            let b = input[len];
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if (quote_type == b'"' && b == b'"') || (quote_type == b'\'' && b == b'\'') {
                // Found potential closing quote
                // LOOKAHEAD: Is this really the end?
                // Rule: It's the end if followed by:
                // 1. Whitespace + Separator (:, }, ])
                // 2. Whitespace + Comma + (Key or End)

                let rest = &input[len + 1..];
                if is_structural_closure(rest) {
                    cursor.advance(len + 1);
                    let s = String::from_utf8_lossy(&input[..len]);
                    return Ok(PyString::new(py, &s).into());
                }
                // Else: Treat as content
            } else if quote_type == b'\x82' && b == b'"' {
                // Allow standard quote to close fullwidth quote if followed by closure
                let rest = &input[len + 1..];
                if is_structural_closure(rest) {
                    cursor.advance(len + 1);
                    let s = String::from_utf8_lossy(&input[..len]);
                    return Ok(PyString::new(py, &s).into());
                }
            } else if quote_type == b'\x82'
                && b == 0xEF
                && len + 2 < input.len()
                && input[len + 1] == 0xBC
                && input[len + 2] == 0x82
            {
                // Found potential fullwidth closing quote
                let rest = &input[len + 3..];
                if is_structural_closure(rest) {
                    cursor.advance(len + 3);
                    let s = String::from_utf8_lossy(&input[..len]);
                    return Ok(PyString::new(py, &s).into());
                }
            }
            len += 1;
        }

        // Hit EOF without closing quote -> Error
        return Err(ParseError::UnexpectedEof);
    } else {
        // Unquoted string mode: ROBUST / HEURISTIC
        // Consume until a separator is found
        let input = cursor.remaining();
        let mut len = 0;
        while len < input.len() {
            if len > MAX_STRING_LEN {
                break;
            }
            let b = input[len];
            // Stop at separators: , } ] or whitespace
            // Also check for fullwidth closing brace ｝ (EF BC 9D)
            if b == b',' || b == b'}' || b == b']' || b.is_ascii_whitespace() {
                break;
            }
            // Check for fullwidth comma ， (EF BC 8C) or fullwidth brace ｝
            if b == 0xEF && len + 2 < input.len() && input[len + 1] == 0xBC {
                let last = input[len + 2];
                if last == 0x8C || last == 0x9D {
                    // ， or ｝
                    break;
                }
            }
            len += 1;
        }

        cursor.advance(len);
        let s = String::from_utf8_lossy(&input[..len]);

        // Special handling for null -> None
        if s == "null" {
            return Ok(py.None().into());
        }

        Ok(PyString::new(py, &s).into())
    }
}

fn parse_bool_speculative<'py>(
    cursor: &mut Cursor,
    py: Python<'py>,
) -> Result<PyObject, ParseError> {
    if cursor.matches(b"true") {
        cursor.advance(4);
        Ok(PyBool::new(py, true).into())
    } else if cursor.matches(b"false") {
        cursor.advance(5);
        Ok(PyBool::new(py, false).into())
    } else {
        // 也许是 "True" 或 "False" (Python 风格)
        if cursor.matches(b"True") {
            cursor.advance(4);
            Ok(PyBool::new(py, true).into())
        } else if cursor.matches(b"False") {
            cursor.advance(5);
            Ok(PyBool::new(py, false).into())
        } else {
            Ok(py.None().into())
        }
    }
}

fn is_structural_closure(input: &[u8]) -> bool {
    let mut idx = 0;
    // Skip whitespace
    while idx < input.len() && input[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx >= input.len() {
        return true; // End of input -> assume valid closure (or EOF)
    }

    let b = input[idx];
    if b == b':' || b == b'}' || b == b']' {
        return true;
    }

    // Check for fullwidth closing brace ｝ (EF BC 9D)
    if b == 0xEF && idx + 2 < input.len() && input[idx + 1] == 0xBC && input[idx + 2] == 0x9D {
        return true;
    }

    if b == b',' {
        // Comma found. Check what's after comma.
        let after_comma = &input[idx + 1..];
        let mut next_idx = 0;
        while next_idx < after_comma.len() && after_comma[next_idx].is_ascii_whitespace() {
            next_idx += 1;
        }
        if next_idx >= after_comma.len() {
            return true; // Trailing comma at EOF
        }
        let next_b = after_comma[next_idx];
        if next_b == b'"' || next_b == b'}' {
            return true;
        }
        // Fullwidth quote or brace
        if next_b == 0xEF && next_idx + 2 < after_comma.len() && after_comma[next_idx + 1] == 0xBC {
            let last = after_comma[next_idx + 2];
            if last == 0x82 || last == 0x9D {
                // ＂ or ｝
                return true;
            }
        }

        return false; // Comma followed by garbage -> Treat previous quote as content
    }

    // Fullwidth comma ， (EF BC 8C)
    if b == 0xEF && idx + 2 < input.len() && input[idx + 1] == 0xBC && input[idx + 2] == 0x8C {
        let after_comma = &input[idx + 3..];
        let mut next_idx = 0;
        while next_idx < after_comma.len() && after_comma[next_idx].is_ascii_whitespace() {
            next_idx += 1;
        }
        if next_idx >= after_comma.len() {
            return true;
        }
        let next_b = after_comma[next_idx];
        if next_b == b'"' || next_b == b'}' {
            return true;
        }
        if next_b == 0xEF && next_idx + 2 < after_comma.len() && after_comma[next_idx + 1] == 0xBC {
            let last = after_comma[next_idx + 2];
            if last == 0x82 || last == 0x9D {
                return true;
            }
        }
        return false;
    }

    false
}
