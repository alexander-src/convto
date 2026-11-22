use crate::format::Format;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

pub fn convert_data(input: &str, from: Format, to: Format) -> Result<String, String> {
    let val = match from {
        Format::Json => parse_json(input)?,
        Format::Yaml => parse_yaml(input)?,
        Format::Toml => parse_toml(input)?,
        Format::Csv => parse_delimited(input, ',')?,
        Format::Tsv => parse_delimited(input, '\t')?,
        _ => return Err(format!("Unsupported structured data input format: {from}")),
    };
    match to {
        Format::Json => Ok(emit_json(&val)),
        Format::Yaml => Ok(emit_yaml(&val)),
        Format::Toml => emit_toml(&val),
        Format::Csv => emit_delimited(&val, ','),
        Format::Tsv => emit_delimited(&val, '\t'),
        _ => Err(format!("Unsupported structured data output format: {to}")),
    }
}

// ============================================================================
// JSON (RFC 8259)
// ============================================================================

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        let bytes = input.as_bytes();
        let pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            3
        } else {
            0
        };
        Self { bytes, pos }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\r' | b'\n')
        {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.expect_lit(b"null", Value::Null),
            Some(b't') => self.expect_lit(b"true", Value::Bool(true)),
            Some(b'f') => self.expect_lit(b"false", Value::Bool(false)),
            Some(b'"') => self.parse_string().map(Value::Str),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(b) => Err(format!(
                "JSON: unexpected byte 0x{b:02x} at offset {}",
                self.pos
            )),
            None => Err("JSON: unexpected end of input".to_string()),
        }
    }

    fn expect_lit(&mut self, lit: &[u8], val: Value) -> Result<Value, String> {
        if self.bytes[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            Ok(val)
        } else {
            Err(format!("JSON: invalid literal at offset {}", self.pos))
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err(format!("JSON: expected '\"' at offset {}", self.pos));
        }
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                let esc = *self
                    .bytes
                    .get(self.pos)
                    .ok_or_else(|| "JSON: unterminated escape".to_string())?;
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let code = self.parse_hex4()?;
                        if (0xd800..=0xdbff).contains(&code) {
                            if self.bytes.get(self.pos..self.pos + 2) != Some(b"\\u") {
                                return Err("JSON: missing low surrogate".to_string());
                            }
                            self.pos += 2;
                            let low = self.parse_hex4()?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err("JSON: invalid low surrogate".to_string());
                            }
                            let cp =
                                0x10000 + (((code as u32 - 0xd800) << 10) | (low as u32 - 0xdc00));
                            let ch = char::from_u32(cp)
                                .ok_or_else(|| "JSON: invalid unicode scalar".to_string())?;
                            out.push(ch);
                        } else {
                            let ch = char::from_u32(code as u32)
                                .ok_or_else(|| "JSON: invalid unicode escape".to_string())?;
                            out.push(ch);
                        }
                    }
                    _ => return Err(format!("JSON: invalid escape \\{}", esc as char)),
                }
            } else if b < 0x20 {
                return Err(format!(
                    "JSON: unescaped control character at offset {}",
                    self.pos
                ));
            } else {
                let s = std::str::from_utf8(&self.bytes[self.pos..])
                    .map_err(|_| "JSON: invalid UTF-8".to_string())?;
                let ch = s.chars().next().unwrap();
                out.push(ch);
                self.pos += ch.len_utf8();
            }
        }
        Err("JSON: unterminated string".to_string())
    }

    fn parse_hex4(&mut self) -> Result<u16, String> {
        let slice = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| "JSON: truncated \\u escape".to_string())?;
        let text = std::str::from_utf8(slice).map_err(|_| "JSON: invalid hex".to_string())?;
        let v = u16::from_str_radix(text, 16)
            .map_err(|_| format!("JSON: invalid hex escape {text}"))?;
        self.pos += 4;
        Ok(v)
    }

    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(format!("JSON: invalid number at offset {start}")),
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!(
                    "JSON: expected digit after '.' at offset {}",
                    self.pos
                ));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!(
                    "JSON: expected exponent digit at offset {}",
                    self.pos
                ));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
        if !is_float {
            if let Ok(i) = text.parse::<i64>() {
                return Ok(Value::Int(i));
            }
        }
        let f = text
            .parse::<f64>()
            .map_err(|_| format!("JSON: invalid float {text}"))?;
        if !f.is_finite() {
            return Err(format!("JSON: non-finite number {text}"));
        }
        Ok(Value::Float(f))
    }

    fn parse_array(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(format!("JSON: expected ',' or ']' at offset {}", self.pos)),
            }
        }
    }

    fn parse_object(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut entries = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(entries));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(format!("JSON: expected ':' at offset {}", self.pos));
            }
            self.pos += 1;
            let val = self.parse_value()?;
            set_key(&mut entries, key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(entries));
                }
                _ => return Err(format!("JSON: expected ',' or '}}' at offset {}", self.pos)),
            }
        }
    }
}

pub fn parse_json(input: &str) -> Result<Value, String> {
    let mut p = JsonParser::new(input);
    let val = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("JSON: trailing data at offset {}", p.pos));
    }
    Ok(val)
}

pub fn emit_json(val: &Value) -> String {
    let mut out = String::new();
    write_json(val, 0, true, &mut out);
    out.push('\n');
    out
}

fn emit_json_compact(val: &Value) -> String {
    let mut out = String::new();
    write_json(val, 0, false, &mut out);
    out
}

fn write_json(val: &Value, indent: usize, pretty: bool, out: &mut String) {
    match val {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => {
            if f.is_finite() {
                let s = f.to_string();
                out.push_str(&s);
                if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                    out.push_str(".0");
                }
            } else {
                out.push_str("null");
            }
        }
        Value::Str(s) => write_json_str(s, out),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    push_spaces(out, indent + 2);
                }
                write_json(item, indent + 2, pretty, out);
            }
            if pretty {
                out.push('\n');
                push_spaces(out, indent);
            }
            out.push(']');
        }
        Value::Object(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (idx, (k, v)) in entries.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    push_spaces(out, indent + 2);
                }
                write_json_str(k, out);
                out.push_str(if pretty { ": " } else { ":" });
                write_json(v, indent + 2, pretty, out);
            }
            if pretty {
                out.push('\n');
                push_spaces(out, indent);
            }
            out.push('}');
        }
    }
}

fn write_json_str(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ============================================================================
// YAML (clean block + flow subset)
// ============================================================================

#[derive(Debug, Clone)]
struct YamlLine {
    indent: usize,
    text: String,
    lineno: usize,
}

pub fn parse_yaml(input: &str) -> Result<Value, String> {
    let trimmed = input.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(Value::Object(Vec::new()));
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return parse_yaml_flow(trimmed);
    }

    let mut lines = Vec::new();
    for (idx, raw) in input.trim_start_matches('\u{feff}').lines().enumerate() {
        let stripped = strip_yaml_comment(raw);
        let t = stripped.trim_end();
        if t.trim().is_empty() || t.trim() == "---" || t.trim() == "..." {
            continue;
        }
        let indent = t.bytes().take_while(|&b| b == b' ').count();
        if t[indent..].starts_with('\t') {
            return Err(format!(
                "YAML line {}: tabs are not allowed for indentation",
                idx + 1
            ));
        }
        lines.push(YamlLine {
            indent,
            text: t[indent..].to_string(),
            lineno: idx + 1,
        });
    }
    if lines.is_empty() {
        return Ok(Value::Object(Vec::new()));
    }
    let mut pos = 0;
    let base_indent = lines[0].indent;
    parse_yaml_block(&lines, &mut pos, base_indent)
}

fn strip_yaml_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double && b == b'\\' {
            escaped = true;
            continue;
        }
        if b == b'"' && !in_single {
            in_double = !in_double;
        } else if b == b'\'' && !in_double {
            in_single = !in_single;
        } else if b == b'#' && !in_single && !in_double && (i == 0 || bytes[i - 1] == b' ') {
            return &line[..i];
        }
    }
    line
}

fn parse_yaml_block(lines: &[YamlLine], pos: &mut usize, indent: usize) -> Result<Value, String> {
    if *pos >= lines.len() {
        return Ok(Value::Null);
    }
    let first = &lines[*pos];
    if first.text == "-" || first.text.starts_with("- ") {
        parse_yaml_seq(lines, pos, indent)
    } else if split_yaml_kv(&first.text).is_some() {
        parse_yaml_map(lines, pos, indent)
    } else {
        *pos += 1;
        parse_yaml_scalar(&first.text)
    }
}

fn parse_yaml_seq(lines: &[YamlLine], pos: &mut usize, indent: usize) -> Result<Value, String> {
    let mut out = Vec::new();
    while *pos < lines.len() {
        let line = &lines[*pos];
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(format!(
                "YAML line {}: unexpected indentation in sequence",
                line.lineno
            ));
        }
        if !(line.text == "-" || line.text.starts_with("- ")) {
            break;
        }
        let rest = if line.text == "-" {
            ""
        } else {
            line.text[2..].trim_start()
        };
        let item_indent = line.indent + 2;
        *pos += 1;
        if rest.is_empty() {
            if *pos < lines.len() && lines[*pos].indent > indent {
                let child_indent = lines[*pos].indent;
                out.push(parse_yaml_block(lines, pos, child_indent)?);
            } else {
                out.push(Value::Null);
            }
        } else if let Some((k, v)) = split_yaml_kv(rest) {
            let mut entries = Vec::new();
            let first_val = if v.is_empty() {
                if *pos < lines.len() && lines[*pos].indent > item_indent {
                    let child_indent = lines[*pos].indent;
                    parse_yaml_block(lines, pos, child_indent)?
                } else {
                    Value::Null
                }
            } else {
                parse_yaml_inline_value(v)?
            };
            set_key(&mut entries, k, first_val);
            while *pos < lines.len() && lines[*pos].indent >= item_indent {
                if lines[*pos].indent > item_indent {
                    return Err(format!(
                        "YAML line {}: unexpected indentation in sequence map",
                        lines[*pos].lineno
                    ));
                }
                if lines[*pos].text == "-" || lines[*pos].text.starts_with("- ") {
                    break;
                }
                let next_line = &lines[*pos];
                let (nk, nv) = split_yaml_kv(&next_line.text).ok_or_else(|| {
                    format!("YAML line {}: expected key: value", next_line.lineno)
                })?;
                *pos += 1;
                let nval = if nv.is_empty() {
                    if *pos < lines.len() && lines[*pos].indent > item_indent {
                        let child_indent = lines[*pos].indent;
                        parse_yaml_block(lines, pos, child_indent)?
                    } else {
                        Value::Null
                    }
                } else {
                    parse_yaml_inline_value(nv)?
                };
                set_key(&mut entries, nk, nval);
            }
            out.push(Value::Object(entries));
        } else {
            out.push(parse_yaml_inline_value(rest)?);
        }
    }
    Ok(Value::Array(out))
}

fn parse_yaml_map(lines: &[YamlLine], pos: &mut usize, indent: usize) -> Result<Value, String> {
    let mut entries = Vec::new();
    while *pos < lines.len() {
        let line = &lines[*pos];
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(format!(
                "YAML line {}: unexpected indentation in mapping",
                line.lineno
            ));
        }
        if line.text == "-" || line.text.starts_with("- ") {
            break;
        }
        let (key, rest) = split_yaml_kv(&line.text)
            .ok_or_else(|| format!("YAML line {}: expected 'key: value'", line.lineno))?;
        *pos += 1;
        let val = if rest.is_empty() {
            if *pos < lines.len()
                && (lines[*pos].indent > indent
                    || (lines[*pos].indent == indent
                        && (lines[*pos].text == "-" || lines[*pos].text.starts_with("- "))))
            {
                let child_indent = lines[*pos].indent;
                parse_yaml_block(lines, pos, child_indent)?
            } else {
                Value::Null
            }
        } else {
            parse_yaml_inline_value(rest)?
        };
        set_key(&mut entries, key, val);
    }
    Ok(Value::Object(entries))
}

fn split_yaml_kv(text: &str) -> Option<(String, &str)> {
    let bytes = text.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double && b == b'\\' {
            escaped = true;
            continue;
        }
        if b == b'"' && !in_single {
            in_double = !in_double;
        } else if b == b'\'' && !in_double {
            in_single = !in_single;
        } else if b == b':' && !in_single && !in_double {
            let at_end = i + 1 == bytes.len();
            let followed_by_space = bytes.get(i + 1) == Some(&b' ');
            if at_end || followed_by_space {
                let raw_key = text[..i].trim();
                let key = unquote_yaml_key(raw_key)?;
                let rest = if at_end { "" } else { text[i + 1..].trim() };
                return Some((key, rest));
            }
        }
    }
    None
}

fn unquote_yaml_key(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        return parse_json(raw).ok().and_then(|v| match v {
            Value::Str(s) => Some(s),
            _ => None,
        });
    }
    if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        return Some(raw[1..raw.len() - 1].replace("''", "'"));
    }
    Some(raw.to_string())
}

fn parse_yaml_inline_value(text: &str) -> Result<Value, String> {
    let t = text.trim();
    if t.starts_with('[') || t.starts_with('{') {
        parse_yaml_flow(t)
    } else {
        parse_yaml_scalar(t)
    }
}

fn parse_yaml_flow(text: &str) -> Result<Value, String> {
    // Valid JSON flow collections work directly; also support unquoted YAML flow values.
    if let Ok(v) = parse_json(text) {
        return Ok(v);
    }
    let t = text.trim();
    if t.starts_with('[') && t.ends_with(']') {
        let inner = t[1..t.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }
        let mut items = Vec::new();
        for part in split_top_level(inner, ',') {
            items.push(parse_yaml_inline_value(part)?);
        }
        return Ok(Value::Array(items));
    }
    if t.starts_with('{') && t.ends_with('}') {
        let inner = t[1..t.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Value::Object(Vec::new()));
        }
        let mut entries = Vec::new();
        for part in split_top_level(inner, ',') {
            let (k, v) = split_yaml_kv(part.trim())
                .ok_or_else(|| format!("YAML: invalid flow mapping entry '{part}'"))?;
            set_key(&mut entries, k, parse_yaml_inline_value(v)?);
        }
        return Ok(Value::Object(entries));
    }
    parse_yaml_scalar(t)
}

fn parse_yaml_scalar(text: &str) -> Result<Value, String> {
    let t = text.trim();
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        return parse_json(t);
    }
    if t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2 {
        return Ok(Value::Str(t[1..t.len() - 1].replace("''", "'")));
    }
    match t {
        "null" | "Null" | "NULL" | "~" => Ok(Value::Null),
        "true" | "True" | "TRUE" => Ok(Value::Bool(true)),
        "false" | "False" | "FALSE" => Ok(Value::Bool(false)),
        _ => {
            if let Ok(i) = t.parse::<i64>() {
                return Ok(Value::Int(i));
            }
            if (t.contains('.') || t.contains('e') || t.contains('E'))
                && !t.starts_with("0x")
                && !t.starts_with("0X")
            {
                if let Ok(f) = t.parse::<f64>() {
                    if f.is_finite() {
                        return Ok(Value::Float(f));
                    }
                }
            }
            Ok(Value::Str(t.to_string()))
        }
    }
}

pub fn emit_yaml(val: &Value) -> String {
    let mut out = String::new();
    write_yaml(val, 0, &mut out);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn write_yaml(val: &Value, indent: usize, out: &mut String) {
    match val {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => {
            let s = f.to_string();
            out.push_str(&s);
            if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                out.push_str(".0");
            }
        }
        Value::Str(s) => write_yaml_str(s, out),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push('\n');
                    push_spaces(out, indent);
                }
                match item {
                    Value::Object(entries) if !entries.is_empty() => {
                        out.push_str("- ");
                        write_yaml_map(entries, indent + 2, true, out);
                    }
                    Value::Array(sub) if !sub.is_empty() => {
                        out.push_str("-\n");
                        push_spaces(out, indent + 2);
                        write_yaml(item, indent + 2, out);
                    }
                    _ => {
                        out.push_str("- ");
                        write_yaml(item, indent + 2, out);
                    }
                }
            }
        }
        Value::Object(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            write_yaml_map(entries, indent, false, out);
        }
    }
}

fn write_yaml_map(
    entries: &[(String, Value)],
    indent: usize,
    inline_first: bool,
    out: &mut String,
) {
    for (idx, (k, v)) in entries.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
            push_spaces(out, indent);
        } else if !inline_first {
            // caller already indented
        }
        write_yaml_key(k, out);
        out.push(':');
        match v {
            Value::Array(items) if !items.is_empty() => {
                out.push('\n');
                push_spaces(out, indent + 2);
                write_yaml(v, indent + 2, out);
            }
            Value::Object(sub) if !sub.is_empty() => {
                out.push('\n');
                push_spaces(out, indent + 2);
                write_yaml(v, indent + 2, out);
            }
            _ => {
                out.push(' ');
                write_yaml(v, indent + 2, out);
            }
        }
    }
}

fn write_yaml_key(k: &str, out: &mut String) {
    if !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
        && !matches!(
            k,
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "True" | "False" | "Null"
        )
        && k.parse::<f64>().is_err()
    {
        out.push_str(k);
    } else {
        write_json_str(k, out);
    }
}

fn write_yaml_str(s: &str, out: &mut String) {
    let needs_quote = s.is_empty()
        || s.starts_with(' ')
        || s.ends_with(' ')
        || s.contains('\n')
        || s.contains('\r')
        || s.contains('\t')
        || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || matches!(
            s.as_bytes()[0],
            b'#' | b'-'
                | b'?'
                | b':'
                | b'['
                | b']'
                | b'{'
                | b'}'
                | b','
                | b'&'
                | b'*'
                | b'!'
                | b'|'
                | b'>'
                | b'\''
                | b'"'
                | b'%'
                | b'@'
                | b'`'
        )
        || matches!(
            s,
            "true"
                | "false"
                | "null"
                | "True"
                | "False"
                | "Null"
                | "TRUE"
                | "FALSE"
                | "NULL"
                | "~"
                | "yes"
                | "no"
                | "on"
                | "off"
        )
        || s.parse::<i64>().is_ok()
        || s.parse::<f64>().is_ok();
    if needs_quote {
        write_json_str(s, out);
    } else {
        out.push_str(s);
    }
}

// ============================================================================
// TOML
// ============================================================================

pub fn parse_toml(input: &str) -> Result<Value, String> {
    let mut root: Vec<(String, Value)> = Vec::new();
    let mut current_path: Vec<String> = Vec::new();
    let mut current_is_array = false;
    let raw_lines: Vec<&str> = input.trim_start_matches('\u{feff}').lines().collect();
    let mut idx = 0;

    while idx < raw_lines.len() {
        let lineno = idx + 1;
        let line = strip_toml_comment(raw_lines[idx]).trim();
        idx += 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[[") && line.ends_with("]]") {
            let header = &line[2..line.len() - 2];
            let path = parse_toml_key_path(header)?;
            ensure_array_table(&mut root, &path)?;
            current_path = path;
            current_is_array = true;
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let header = &line[1..line.len() - 1];
            let path = parse_toml_key_path(header)?;
            ensure_table(&mut root, &path)?;
            current_path = path;
            current_is_array = false;
            continue;
        }
        let eq = find_toml_eq(line)
            .ok_or_else(|| format!("TOML line {lineno}: expected 'key = value'"))?;
        let key_path = parse_toml_key_path(&line[..eq])?;
        let mut val_text = line[eq + 1..].trim().to_string();
        // Join multiline arrays.
        while bracket_balance(&val_text) > 0 && idx < raw_lines.len() {
            let next_part = strip_toml_comment(raw_lines[idx]).trim();
            val_text.push(' ');
            val_text.push_str(next_part);
            idx += 1;
        }
        let value = parse_toml_value(&val_text).map_err(|e| format!("TOML line {lineno}: {e}"))?;
        let target = resolve_current_table(&mut root, &current_path, current_is_array)?;
        insert_dotted(target, &key_path, value)?;
    }

    Ok(Value::Object(root))
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (i, b) in line.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double && b == b'\\' {
            escaped = true;
            continue;
        }
        if b == b'"' && !in_single {
            in_double = !in_double;
        } else if b == b'\'' && !in_double {
            in_single = !in_single;
        } else if b == b'#' && !in_single && !in_double {
            return &line[..i];
        }
    }
    line
}

fn bracket_balance(s: &str) -> i32 {
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for b in s.bytes() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double && b == b'\\' {
            escaped = true;
            continue;
        }
        if b == b'"' && !in_single {
            in_double = !in_double;
        } else if b == b'\'' && !in_double {
            in_single = !in_single;
        } else if !in_single && !in_double {
            match b {
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
        }
    }
    depth
}

fn find_toml_eq(line: &str) -> Option<usize> {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (i, b) in line.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double && b == b'\\' {
            escaped = true;
            continue;
        }
        if b == b'"' && !in_single {
            in_double = !in_double;
        } else if b == b'\'' && !in_double {
            in_single = !in_single;
        } else if b == b'=' && !in_single && !in_double {
            return Some(i);
        }
    }
    None
}

fn parse_toml_key_path(raw: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    for seg in split_top_level(raw.trim(), '.') {
        let t = seg.trim();
        if t.is_empty() {
            return Err("empty key segment".to_string());
        }
        if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
            let Value::Str(s) = parse_json(t)? else {
                return Err("invalid quoted key".to_string());
            };
            parts.push(s);
        } else if t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2 {
            parts.push(t[1..t.len() - 1].to_string());
        } else {
            parts.push(t.to_string());
        }
    }
    if parts.is_empty() {
        return Err("empty key".to_string());
    }
    Ok(parts)
}

fn parse_toml_value(raw: &str) -> Result<Value, String> {
    let t = raw.trim();
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        return parse_json(t);
    }
    if t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2 {
        return Ok(Value::Str(t[1..t.len() - 1].to_string()));
    }
    if t == "true" {
        return Ok(Value::Bool(true));
    }
    if t == "false" {
        return Ok(Value::Bool(false));
    }
    if t.starts_with('[') && t.ends_with(']') {
        let inner = t[1..t.len() - 1].trim();
        let mut items = Vec::new();
        if !inner.is_empty() {
            for part in split_top_level(inner, ',') {
                let p = part.trim();
                if p.is_empty() {
                    continue;
                }
                items.push(parse_toml_value(p)?);
            }
        }
        return Ok(Value::Array(items));
    }
    if t.starts_with('{') && t.ends_with('}') {
        let inner = t[1..t.len() - 1].trim();
        let mut entries = Vec::new();
        if !inner.is_empty() {
            for part in split_top_level(inner, ',') {
                let p = part.trim();
                if p.is_empty() {
                    continue;
                }
                let eq =
                    find_toml_eq(p).ok_or_else(|| format!("invalid inline table entry '{p}'"))?;
                let kp = parse_toml_key_path(&p[..eq])?;
                let v = parse_toml_value(&p[eq + 1..])?;
                insert_dotted(&mut entries, &kp, v)?;
            }
        }
        return Ok(Value::Object(entries));
    }
    let clean_num = t.replace('_', "");
    if let Some(hex) = clean_num.strip_prefix("0x") {
        if let Ok(i) = i64::from_str_radix(hex, 16) {
            return Ok(Value::Int(i));
        }
    }
    if let Ok(i) = clean_num.parse::<i64>() {
        return Ok(Value::Int(i));
    }
    if let Ok(f) = clean_num.parse::<f64>() {
        if f.is_finite() {
            return Ok(Value::Float(f));
        }
    }
    // Preserve ISO dates/times as strings so conversion to JSON/YAML stays lossless.
    if looks_like_toml_datetime(t) {
        return Ok(Value::Str(t.to_string()));
    }
    Err(format!("unsupported TOML value '{t}'"))
}

fn looks_like_toml_datetime(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10 && b[4] == b'-' && b[7] == b'-' && b[0..4].iter().all(|c| c.is_ascii_digit())
}

fn ensure_table(root: &mut Vec<(String, Value)>, path: &[String]) -> Result<(), String> {
    let mut cur = root;
    for seg in path {
        let idx = match cur.iter().position(|(k, _)| k == seg) {
            Some(i) => i,
            None => {
                cur.push((seg.clone(), Value::Object(Vec::new())));
                cur.len() - 1
            }
        };
        cur = match &mut cur[idx].1 {
            Value::Object(map) => map,
            Value::Array(arr) => match arr.last_mut() {
                Some(Value::Object(map)) => map,
                _ => return Err(format!("TOML: '{seg}' is not a table")),
            },
            _ => return Err(format!("TOML: '{seg}' is not a table")),
        };
    }
    Ok(())
}

fn ensure_array_table(root: &mut Vec<(String, Value)>, path: &[String]) -> Result<(), String> {
    let (last, parents) = path
        .split_last()
        .ok_or_else(|| "TOML: empty header".to_string())?;
    let mut cur = root;
    for seg in parents {
        let idx = match cur.iter().position(|(k, _)| k == seg) {
            Some(i) => i,
            None => {
                cur.push((seg.clone(), Value::Object(Vec::new())));
                cur.len() - 1
            }
        };
        cur = match &mut cur[idx].1 {
            Value::Object(map) => map,
            Value::Array(arr) => match arr.last_mut() {
                Some(Value::Object(map)) => map,
                _ => return Err(format!("TOML: '{seg}' is not a table")),
            },
            _ => return Err(format!("TOML: '{seg}' is not a table")),
        };
    }
    match cur.iter_mut().find(|(k, _)| k == last) {
        Some((_, Value::Array(arr))) => {
            arr.push(Value::Object(Vec::new()));
            Ok(())
        }
        Some(_) => Err(format!("TOML: '{last}' is not an array of tables")),
        None => {
            cur.push((last.clone(), Value::Array(vec![Value::Object(Vec::new())])));
            Ok(())
        }
    }
}

fn resolve_current_table<'a>(
    root: &'a mut Vec<(String, Value)>,
    path: &[String],
    is_array: bool,
) -> Result<&'a mut Vec<(String, Value)>, String> {
    let mut cur = root;
    for (i, seg) in path.iter().enumerate() {
        let is_last = i + 1 == path.len();
        let idx = cur
            .iter()
            .position(|(k, _)| k == seg)
            .ok_or_else(|| format!("TOML: missing table '{seg}'"))?;
        cur = match &mut cur[idx].1 {
            Value::Object(map) if !(is_last && is_array) => map,
            Value::Array(arr) => match arr.last_mut() {
                Some(Value::Object(map)) => map,
                _ => return Err(format!("TOML: '{seg}' has no active table element")),
            },
            _ => return Err(format!("TOML: '{seg}' is not a table")),
        };
    }
    Ok(cur)
}

fn insert_dotted(
    table: &mut Vec<(String, Value)>,
    path: &[String],
    val: Value,
) -> Result<(), String> {
    let (last, parents) = path.split_last().ok_or_else(|| "empty key".to_string())?;
    let mut cur = table;
    for seg in parents {
        let idx = match cur.iter().position(|(k, _)| k == seg) {
            Some(i) => i,
            None => {
                cur.push((seg.clone(), Value::Object(Vec::new())));
                cur.len() - 1
            }
        };
        cur = match &mut cur[idx].1 {
            Value::Object(map) => map,
            _ => return Err(format!("cannot index into non-table key '{seg}'")),
        };
    }
    set_key(cur, last.clone(), val);
    Ok(())
}

pub fn emit_toml(val: &Value) -> Result<String, String> {
    let entries = match val {
        Value::Object(entries) => entries.clone(),
        Value::Array(_) => vec![("items".to_string(), val.clone())],
        _ => return Err("TOML requires an object/table root (or array of tables)".to_string()),
    };
    let mut out = String::new();
    write_toml_table(&[], &entries, &mut out)?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn write_toml_table(
    prefix: &[String],
    entries: &[(String, Value)],
    out: &mut String,
) -> Result<(), String> {
    // Pass 1: scalar & non-table-array fields first.
    for (k, v) in entries {
        match v {
            Value::Object(_) => continue,
            Value::Array(items)
                if !items.is_empty() && items.iter().all(|x| matches!(x, Value::Object(_))) =>
            {
                continue;
            }
            Value::Null => continue,
            _ => {
                write_toml_key(k, out);
                out.push_str(" = ");
                write_toml_inline_val(v, out)?;
                out.push('\n');
            }
        }
    }
    // Pass 2: nested tables and arrays of tables.
    for (k, v) in entries {
        let mut path = prefix.to_vec();
        path.push(k.clone());
        match v {
            Value::Object(sub) => {
                if !out.is_empty() && !out.ends_with("\n\n") {
                    out.push('\n');
                }
                out.push('[');
                write_toml_path(&path, out);
                out.push_str("]\n");
                write_toml_table(&path, sub, out)?;
            }
            Value::Array(items)
                if !items.is_empty() && items.iter().all(|x| matches!(x, Value::Object(_))) =>
            {
                for item in items {
                    if let Value::Object(sub) = item {
                        if !out.is_empty() && !out.ends_with("\n\n") {
                            out.push('\n');
                        }
                        out.push_str("[[");
                        write_toml_path(&path, out);
                        out.push_str("]]\n");
                        write_toml_table(&path, sub, out)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn write_toml_path(path: &[String], out: &mut String) {
    for (i, seg) in path.iter().enumerate() {
        if i > 0 {
            out.push('.');
        }
        write_toml_key(seg, out);
    }
}

fn write_toml_key(k: &str, out: &mut String) {
    if !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        out.push_str(k);
    } else {
        write_json_str(k, out);
    }
}

fn write_toml_inline_val(val: &Value, out: &mut String) -> Result<(), String> {
    match val {
        Value::Null => return Err("TOML does not support null values inside arrays".to_string()),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => {
            let s = f.to_string();
            out.push_str(&s);
            if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                out.push_str(".0");
            }
        }
        Value::Str(s) => write_json_str(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_toml_inline_val(item, out)?;
            }
            out.push(']');
        }
        Value::Object(entries) => {
            out.push_str("{ ");
            let mut first = true;
            for (k, v) in entries {
                if matches!(v, Value::Null) {
                    continue;
                }
                if !first {
                    out.push_str(", ");
                }
                first = false;
                write_toml_key(k, out);
                out.push_str(" = ");
                write_toml_inline_val(v, out)?;
            }
            out.push_str(" }");
        }
    }
    Ok(())
}

// ============================================================================
// CSV / TSV (RFC 4180)
// ============================================================================

pub fn parse_delimited(input: &str, delim: char) -> Result<Value, String> {
    let text = input.trim_start_matches('\u{feff}');
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' && field.is_empty() {
            in_quotes = true;
        } else if ch == delim {
            row.push(std::mem::take(&mut field));
        } else if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            if !(row.len() == 1 && row[0].is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else if ch == '\n' {
            row.push(std::mem::take(&mut field));
            if !(row.len() == 1 && row[0].is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            field.push(ch);
        }
    }
    if in_quotes {
        return Err("CSV/TSV: unterminated quoted field".to_string());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    if rows.is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    let headers = rows[0].clone();
    let mut out = Vec::with_capacity(rows.len().saturating_sub(1));
    for r in rows.into_iter().skip(1) {
        let mut obj = Vec::with_capacity(headers.len());
        for (i, h) in headers.iter().enumerate() {
            let cell = r.get(i).cloned().unwrap_or_default();
            obj.push((h.clone(), Value::Str(cell)));
        }
        out.push(Value::Object(obj));
    }
    Ok(Value::Array(out))
}

pub fn emit_delimited(val: &Value, delim: char) -> Result<String, String> {
    let rows: Vec<Vec<(String, Value)>> = match val {
        Value::Array(items) => {
            if items.is_empty() {
                return Ok(String::new());
            }
            let mut objs = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Object(map) => objs.push(map.clone()),
                    _ => {
                        return Err(
                            "CSV/TSV output requires an array of objects or a single object"
                                .to_string(),
                        );
                    }
                }
            }
            objs
        }
        Value::Object(map) => {
            // If an object wraps a single array of objects (like TOML `[[items]]`), unwrap it.
            if map.len() == 1 {
                if let Value::Array(items) = &map[0].1 {
                    if !items.is_empty() && items.iter().all(|x| matches!(x, Value::Object(_))) {
                        return emit_delimited(&map[0].1, delim);
                    }
                }
            }
            vec![map.clone()]
        }
        _ => return Err("CSV/TSV output requires an object or array of objects".to_string()),
    };

    let mut headers: Vec<String> = Vec::new();
    for obj in &rows {
        for (k, _) in obj {
            if !headers.contains(k) {
                headers.push(k.clone());
            }
        }
    }

    let mut out = String::new();
    write_delim_row(headers.iter().map(|s| s.as_str()), delim, &mut out);
    for obj in &rows {
        let cells: Vec<String> = headers
            .iter()
            .map(|h| match obj.iter().find(|(k, _)| k == h).map(|(_, v)| v) {
                None | Some(Value::Null) => String::new(),
                Some(Value::Str(s)) => s.clone(),
                Some(Value::Bool(b)) => b.to_string(),
                Some(Value::Int(i)) => i.to_string(),
                Some(Value::Float(f)) => f.to_string(),
                Some(other) => emit_json_compact(other),
            })
            .collect();
        write_delim_row(cells.iter().map(|s| s.as_str()), delim, &mut out);
    }
    Ok(out)
}

fn write_delim_row<'a>(cells: impl Iterator<Item = &'a str>, delim: char, out: &mut String) {
    for (i, cell) in cells.enumerate() {
        if i > 0 {
            out.push(delim);
        }
        if cell.contains(delim) || cell.contains('"') || cell.contains('\n') || cell.contains('\r')
        {
            out.push('"');
            for ch in cell.chars() {
                if ch == '"' {
                    out.push_str("\"\"");
                } else {
                    out.push(ch);
                }
            }
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push('\n');
}

// ============================================================================
// Helpers
// ============================================================================

fn set_key(entries: &mut Vec<(String, Value)>, key: String, val: Value) {
    if let Some((_, existing)) = entries.iter_mut().find(|(k, _)| k == &key) {
        *existing = val;
    } else {
        entries.push((key, val));
    }
}

fn push_spaces(out: &mut String, count: usize) {
    for _ in 0..count {
        out.push(' ');
    }
}

fn split_top_level(text: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (i, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
        } else if ch == '\'' && !in_double {
            in_single = !in_single;
        } else if !in_single && !in_double {
            match ch {
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                c if c == sep && depth == 0 => {
                    parts.push(&text[start..i]);
                    start = i + ch.len_utf8();
                }
                _ => {}
            }
        }
    }
    parts.push(&text[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_yaml_toml_roundtrip() {
        let src = r#"{"name":"convto","version":1,"enabled":true,"tags":["linux","dolphin"],"server":{"host":"127.0.0.1","port":8080}}"#;
        let yaml = convert_data(src, Format::Json, Format::Yaml).unwrap();
        let toml = convert_data(&yaml, Format::Yaml, Format::Toml).unwrap();
        let back = convert_data(&toml, Format::Toml, Format::Json).unwrap();
        assert_eq!(parse_json(src).unwrap(), parse_json(&back).unwrap());
    }

    #[test]
    fn csv_tsv_json_roundtrip() {
        let csv = "name,note,zip\n\"Alice\",\"hello, \"\"world\"\"\",01234\nBob,plain,90210\n";
        let json = convert_data(csv, Format::Csv, Format::Json).unwrap();
        let tsv = convert_data(&json, Format::Json, Format::Tsv).unwrap();
        let back = convert_data(&tsv, Format::Tsv, Format::Csv).unwrap();
        assert_eq!(
            parse_delimited(csv, ',').unwrap(),
            parse_delimited(&back, ',').unwrap()
        );
    }
}
