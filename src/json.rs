//! Bounded JSON for file-contained aliases and color settings; no I/O or evaluation.
use crate::{Error, error::Budget};

pub(crate) enum Json {
    Null,
    Bool,
    Number(f64),
    String(String),
    Array,
    Object(Vec<(String, Json)>),
}
impl Json {
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(v) => v.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn string(&self) -> Option<&str> {
        if let Self::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub fn number(&self) -> Option<f64> {
        if let Self::Number(n) = self {
            Some(*n)
        } else {
            None
        }
    }
}
pub(crate) fn parse(bytes: &[u8], budget: &mut Budget<'_>) -> Result<Json, Error> {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let mut p = Parser {
        bytes: bytes.get(..end).ok_or(Error::Allocation)?,
        pos: 0,
        budget,
    };
    let value = p.value(0)?;
    p.space();
    if p.pos != p.bytes.len() {
        return Err(p.invalid());
    }
    Ok(value)
}
struct Parser<'a, 'b, 'c> {
    bytes: &'a [u8],
    pos: usize,
    budget: &'b mut Budget<'c>,
}
impl Parser<'_, '_, '_> {
    fn invalid(&self) -> Error {
        Error::InvalidFormat {
            offset: self.pos,
            message: "invalid JSON payload",
        }
    }
    fn space(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|b| matches!(b, b' ' | b'\r' | b'\n' | b'\t'))
        {
            self.pos += 1;
        }
    }
    fn take(&mut self, token: u8) -> bool {
        self.space();
        if self.bytes.get(self.pos) == Some(&token) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn value(&mut self, depth: usize) -> Result<Json, Error> {
        Budget::check("JSON depth", depth, self.budget.limits.max_depth)?;
        self.space();
        match self.bytes.get(self.pos) {
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b'{') => {
                self.pos += 1;
                let mut fields = Vec::new();
                if self.take(b'}') {
                    return Ok(Json::Object(fields));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    if !self.take(b':') {
                        return Err(self.invalid());
                    }
                    let value = self.value(depth + 1)?;
                    self.budget.push(&mut fields, (key, value))?;
                    if self.take(b'}') {
                        break;
                    }
                    if !self.take(b',') {
                        return Err(self.invalid());
                    }
                }
                Ok(Json::Object(fields))
            }
            Some(b'[') => {
                self.pos += 1;
                if !self.take(b']') {
                    loop {
                        // Array values are validated, budgeted and discarded: neither
                        // supported CMS fields nor a path are array-valued.
                        let _ = self.value(depth + 1)?;
                        if self.take(b']') {
                            break;
                        }
                        if !self.take(b',') {
                            return Err(self.invalid());
                        }
                    }
                }
                Ok(Json::Array)
            }
            Some(b't') => {
                self.literal(b"true")?;
                Ok(Json::Bool)
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(Json::Bool)
            }
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(Json::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.invalid()),
        }
    }
    fn literal(&mut self, token: &[u8]) -> Result<(), Error> {
        let end = self.pos.checked_add(token.len()).ok_or(Error::Allocation)?;
        if self.bytes.get(self.pos..end) != Some(token) {
            return Err(self.invalid());
        }
        self.pos = end;
        Ok(())
    }
    fn digits(&mut self) -> usize {
        let start = self.pos;
        while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        self.pos - start
    }
    fn number(&mut self) -> Result<Json, Error> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        if self.bytes.get(self.pos) == Some(&b'0') {
            self.pos += 1;
        } else if !self
            .bytes
            .get(self.pos)
            .is_some_and(|b| matches!(b, b'1'..=b'9'))
            || self.digits() == 0
        {
            return Err(self.invalid());
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            if self.digits() == 0 {
                return Err(self.invalid());
            }
        }
        if self
            .bytes
            .get(self.pos)
            .is_some_and(|b| matches!(b, b'e' | b'E'))
        {
            self.pos += 1;
            if self
                .bytes
                .get(self.pos)
                .is_some_and(|b| matches!(b, b'+' | b'-'))
            {
                self.pos += 1;
            }
            if self.digits() == 0 {
                return Err(self.invalid());
            }
        }
        let s = std::str::from_utf8(self.bytes.get(start..self.pos).ok_or(Error::Allocation)?)
            .map_err(|_| self.invalid())?;
        let n = s.parse::<f64>().map_err(|_| self.invalid())?;
        if !n.is_finite() {
            return Err(self.invalid());
        }
        Ok(Json::Number(n))
    }
    fn hex(&mut self) -> Result<u32, Error> {
        let mut n = 0;
        for _ in 0..4 {
            let b = *self.bytes.get(self.pos).ok_or_else(|| self.invalid())?;
            self.pos += 1;
            let digit = char::from(b).to_digit(16).ok_or_else(|| self.invalid())?;
            n = (n << 4) | digit;
        }
        Ok(n)
    }
    fn string(&mut self) -> Result<String, Error> {
        if self.bytes.get(self.pos) != Some(&b'"') {
            return Err(self.invalid());
        }
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            let b = *self.bytes.get(self.pos).ok_or_else(|| self.invalid())?;
            self.pos += 1;
            match b {
                b'"' => return String::from_utf8(out).map_err(|_| self.invalid()),
                b'\\' => {
                    let escaped = *self.bytes.get(self.pos).ok_or_else(|| self.invalid())?;
                    self.pos += 1;
                    let byte = match escaped {
                        b'"' | b'\\' | b'/' => escaped,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'u' => {
                            let mut scalar = self.hex()?;
                            if (0xd800..=0xdbff).contains(&scalar) {
                                self.literal(b"\\u")?;
                                let lo = self.hex()?;
                                if !(0xdc00..=0xdfff).contains(&lo) {
                                    return Err(self.invalid());
                                }
                                scalar = 0x10000 + ((scalar - 0xd800) << 10) + lo - 0xdc00;
                            }
                            let ch = char::from_u32(scalar).ok_or_else(|| self.invalid())?;
                            let mut buf = [0; 4];
                            for b in ch.encode_utf8(&mut buf).bytes() {
                                self.budget.push(&mut out, b)?;
                            }
                            continue;
                        }
                        _ => return Err(self.invalid()),
                    };
                    self.budget.push(&mut out, byte)?;
                }
                0..=31 => return Err(self.invalid()),
                _ => self.budget.push(&mut out, b)?,
            }
        }
    }
}
