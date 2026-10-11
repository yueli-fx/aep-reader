use std::fmt;

/// Parsing failures have machine-readable categories and source offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidFormat {
        offset: usize,
        message: &'static str,
    },
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    InvalidLimits(&'static str),
    Allocation,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat { offset, message } => {
                write!(f, "invalid AEP at byte {offset}: {message}")
            }
            Self::ResourceLimit {
                resource,
                limit,
                actual,
            } => write!(f, "{resource} limit exceeded: {actual} > {limit}"),
            Self::InvalidLimits(s) => write!(f, "invalid parser limits: {s}"),
            Self::Allocation => f.write_str("parser allocation failed"),
        }
    }
}
impl std::error::Error for Error {}

/// Limits apply to the whole parse. Zero values are invalid, not unlimited.
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_chunk_bytes: usize,
    pub max_chunks: usize,
    pub max_depth: usize,
    pub max_keyframes: usize,
    pub max_allocated_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 256 * 1024 * 1024,
            max_chunk_bytes: 256 * 1024 * 1024,
            max_chunks: 200_000,
            max_depth: 64,
            max_keyframes: 1_000_000,
            max_allocated_bytes: 128 * 1024 * 1024,
        }
    }
}
impl Limits {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if [
            self.max_input_bytes,
            self.max_chunk_bytes,
            self.max_chunks,
            self.max_depth,
            self.max_keyframes,
            self.max_allocated_bytes,
        ]
        .contains(&0)
        {
            return Err(Error::InvalidLimits("all limits must be nonzero"));
        }
        if self.max_depth > 128 {
            return Err(Error::InvalidLimits("max_depth must be <= 128"));
        }
        Ok(())
    }
}

pub(crate) struct Budget<'a> {
    pub limits: &'a Limits,
    pub chunks: usize,
    pub keyframes: usize,
    allocated: usize,
}
impl<'a> Budget<'a> {
    pub fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            chunks: 0,
            keyframes: 0,
            allocated: 0,
        }
    }
    pub fn check(resource: &'static str, actual: usize, limit: usize) -> Result<(), Error> {
        if actual > limit {
            Err(Error::ResourceLimit {
                resource,
                limit,
                actual,
            })
        } else {
            Ok(())
        }
    }
    pub fn charge(&mut self, n: usize) -> Result<(), Error> {
        let actual = self.allocated.checked_add(n).ok_or(Error::Allocation)?;
        Self::check(
            "decoded allocation bytes",
            actual,
            self.limits.max_allocated_bytes,
        )?;
        self.allocated = actual;
        Ok(())
    }
    pub fn push<T>(&mut self, v: &mut Vec<T>, x: T) -> Result<(), Error> {
        if v.len() == v.capacity() {
            // Large semantic records commonly occur in singleton groups. Four
            // eagerly reserved slots would spend most of the budget on unused
            // capacity; retain geometric growth once the vector is populated.
            let initial = if std::mem::size_of::<T>() >= 512 {
                1
            } else {
                4
            };
            let additional = v.capacity().max(initial);
            self.charge(
                additional
                    .checked_mul(std::mem::size_of::<T>())
                    .ok_or(Error::Allocation)?,
            )?;
            v.try_reserve_exact(additional)
                .map_err(|_| Error::Allocation)?;
        }
        v.push(x);
        Ok(())
    }
    pub fn text(&mut self, bytes: &[u8]) -> Result<String, Error> {
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        let bytes = bytes.get(..end).ok_or(Error::Allocation)?;
        // Valid UTF-8 needs one exact-sized owned copy. Only malformed text
        // needs replacement expansion and a temporary lossy buffer.
        if let Ok(text) = std::str::from_utf8(bytes) {
            return self.owned(text);
        }
        // UTF-8 replacement may expand a bad source byte to three bytes.
        let capacity = bytes.len().checked_mul(3).ok_or(Error::Allocation)?;
        self.charge(capacity.checked_mul(2).ok_or(Error::Allocation)?)?;
        let mut s = String::new();
        s.try_reserve_exact(capacity)
            .map_err(|_| Error::Allocation)?;
        s.push_str(&String::from_utf8_lossy(bytes));
        Ok(s)
    }
    pub fn owned(&mut self, s: &str) -> Result<String, Error> {
        self.charge(s.len())?;
        let mut out = String::new();
        out.try_reserve_exact(s.len())
            .map_err(|_| Error::Allocation)?;
        out.push_str(s);
        Ok(out)
    }
}
