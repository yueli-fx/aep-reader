//! Borrowed RIFX tree. Chunk payloads and opaque text data are never copied.
use crate::{Error, binary, error::Budget};

#[derive(Debug)]
pub struct Chunk<'a> {
    pub id: [u8; 4],
    pub form: Option<[u8; 4]>,
    pub offset: usize,
    pub data: &'a [u8],
    pub children: Vec<Chunk<'a>>,
    pub opaque: bool,
}
impl<'a> Chunk<'a> {
    /// Saved-order depth-first traversal, including this chunk. The traversal
    /// stack is allocated by the caller-facing query, outside parser budgets.
    pub fn walk(&self) -> impl Iterator<Item = &Self> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let node = stack.pop()?;
            stack.extend(node.children.iter().rev());
            Some(node)
        })
    }
    pub fn find_all(&self, id: &[u8; 4]) -> impl Iterator<Item = &Self> {
        let id = *id;
        self.walk().filter(move |n| n.id == id)
    }
    pub fn find_all_lists(&self, form: &[u8; 4]) -> impl Iterator<Item = &Self> {
        let form = *form;
        self.walk().filter(move |n| n.form == Some(form))
    }
    pub fn u8(&self, offset: usize) -> Option<u8> {
        self.data.get(offset).copied()
    }
    pub fn u16(&self, offset: usize) -> Option<u16> {
        binary::u16(self.data, offset)
    }
    pub fn u32(&self, offset: usize) -> Option<u32> {
        binary::u32(self.data, offset)
    }
    pub fn child(&self, id: &[u8; 4]) -> Option<&Self> {
        self.children.iter().find(|c| &c.id == id)
    }
    pub fn list(&self, form: &[u8; 4]) -> Option<&Self> {
        self.children.iter().find(|c| c.form.as_ref() == Some(form))
    }
    /// Recursive lookup bounded by the parser's validated tree depth.
    pub fn find(&self, id: &[u8; 4]) -> Option<&Self> {
        if &self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }
    pub fn find_list(&self, form: &[u8; 4]) -> Option<&Self> {
        if self.form.as_ref() == Some(form) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find_list(form))
    }
    pub fn text(&self) -> std::borrow::Cow<'a, str> {
        let end = self
            .data
            .iter()
            .position(|x| *x == 0)
            .unwrap_or(self.data.len());
        String::from_utf8_lossy(self.data.get(..end).unwrap_or_default())
    }
}

pub(crate) fn read<'a>(
    bytes: &'a [u8],
    budget: &mut Budget<'_>,
) -> Result<(Chunk<'a>, &'a [u8]), Error> {
    Budget::check("input bytes", bytes.len(), budget.limits.max_input_bytes)?;
    if bytes.get(..4) != Some(b"RIFX") {
        return Err(invalid(0, "expected RIFX magic (binary AEP only)"));
    }
    if bytes.get(8..12) != Some(b"Egg!") {
        return Err(invalid(8, "expected Egg! form"));
    }
    let (root, end) = chunk(bytes, 0, bytes.len(), 0, budget)?;
    let trailing = bytes
        .get(end..)
        .ok_or_else(|| invalid(end, "invalid trailing range"))?;
    Ok((root, trailing))
}
fn invalid(offset: usize, message: &'static str) -> Error {
    Error::InvalidFormat { offset, message }
}
fn chunk<'a>(
    bytes: &'a [u8],
    offset: usize,
    parent_end: usize,
    depth: usize,
    budget: &mut Budget<'_>,
) -> Result<(Chunk<'a>, usize), Error> {
    Budget::check("container depth", depth, budget.limits.max_depth)?;
    budget.chunks = budget.chunks.checked_add(1).ok_or(Error::Allocation)?;
    Budget::check("chunk count", budget.chunks, budget.limits.max_chunks)?;
    let start = offset
        .checked_add(8)
        .ok_or_else(|| invalid(offset, "header offset overflow"))?;
    if start > parent_end {
        return Err(invalid(offset, "incomplete chunk header"));
    }
    let id = bytes
        .get(offset..offset.checked_add(4).ok_or(Error::Allocation)?)
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| invalid(offset, "missing chunk id"))?;
    let size = usize::try_from(
        binary::u32(bytes, offset.checked_add(4).ok_or(Error::Allocation)?)
            .ok_or_else(|| invalid(offset, "missing chunk size"))?,
    )
    .map_err(|_| Error::Allocation)?;
    Budget::check("chunk bytes", size, budget.limits.max_chunk_bytes)?;
    let end = start
        .checked_add(size)
        .ok_or_else(|| invalid(offset, "chunk size overflow"))?;
    let next = end
        .checked_add(size % 2)
        .ok_or_else(|| invalid(offset, "padding overflow"))?;
    if next > parent_end || next > bytes.len() {
        return Err(invalid(offset, "chunk exceeds containing payload"));
    }
    let payload = bytes
        .get(start..end)
        .ok_or_else(|| invalid(offset, "missing payload"))?;
    let mut c = Chunk {
        id,
        form: None,
        offset,
        data: payload,
        children: Vec::new(),
        opaque: false,
    };
    if id == *b"LIST" || id == *b"RIFX" {
        let form: [u8; 4] = payload
            .get(..4)
            .and_then(|v| v.try_into().ok())
            .ok_or_else(|| invalid(offset, "container lacks form type"))?;
        c.form = Some(form);
        c.data = payload
            .get(4..)
            .ok_or_else(|| invalid(offset, "container lacks payload"))?;
        c.opaque = form == *b"btds" || form == *b"btdk";
        if !c.opaque {
            let mut pos = start.checked_add(4).ok_or(Error::Allocation)?;
            while pos < end {
                let (child, after) = chunk(bytes, pos, end, depth + 1, budget)?;
                budget.push(&mut c.children, child)?;
                pos = after;
            }
        }
    }
    Ok((c, next))
}
