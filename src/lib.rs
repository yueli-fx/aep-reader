//! Read-only, bounded After Effects AEP parsing in safe Rust.
//!
//! The core reads bytes and does not open files, resolve media, execute expressions,
//! render, or write AEPs. Unknown bytes remain borrowed from the input.
//!
//! ```
//! let bytes = b"RIFX\0\0\0\x04Egg!";
//! let document = aep_reader::parse(bytes)?;
//! assert!(document.project.items.is_empty());
//! # Ok::<(), aep_reader::Error>(())
//! ```

mod binary;
mod decode;
mod defaults;
mod effects;
mod error;
mod geometry;
mod gradient;
mod json;
mod metadata;
mod model;
mod panels;
mod queue;
pub mod rifx;
mod shape;
mod text;
mod time;
mod units;
mod views;

pub use error::{Error, Limits};
pub use model::*;
pub use shape::{ShapeKind, ShapeView};
pub use time::FrameRate;
pub use units::{PropertyUnit, known_property_units, property_unit};
pub use views::{AvItemRef, FootageSourceRef};

/// Original bytes and their detached semantic view. No write capability is implied.
#[derive(Debug)]
pub struct Document<'a> {
    input: &'a [u8],
    pub root: rifx::Chunk<'a>,
    pub trailing: &'a [u8],
    pub project: Project,
}

impl<'a> Document<'a> {
    /// Stored XMP trailer as UTF-8 when present; no XML interpretation or I/O.
    pub fn xmp_packet(&self) -> Option<&'a str> {
        if self.trailing.is_empty() {
            None
        } else {
            std::str::from_utf8(self.trailing).ok()
        }
    }
    /// Original bytes for a source span. Chunk spans include their header and
    /// exclude padding; fixed-width render setting spans contain just the record.
    pub fn bytes(&self, span: RawSpan) -> Option<&'a [u8]> {
        self.input
            .get(span.offset..span.offset.checked_add(span.length)?)
    }
}

/// Parse with conservative default resource limits.
pub fn parse(bytes: &[u8]) -> Result<Document<'_>, Error> {
    parse_with_limits(bytes, &Limits::default())
}

/// Read a complete btds payload (its stacked tdbs/btdk sub-LISTs) without an AEP
/// container. The data-only dictionary is never executed.
pub fn parse_text_document(bytes: &[u8]) -> Result<TextDocument, Error> {
    parse_text_document_with_limits(bytes, &Limits::default())
}
pub fn parse_text_document_with_limits(
    bytes: &[u8],
    limits: &Limits,
) -> Result<TextDocument, Error> {
    limits.validate()?;
    error::Budget::check("input bytes", bytes.len(), limits.max_input_bytes)?;
    text::decode(bytes, &mut error::Budget::new(limits))
}
/// Read the supported data-only gradient XML subset; DTDs, entities and
/// external references are rejected.
pub fn parse_gradient_xml(bytes: &[u8]) -> Result<Gradient, Error> {
    parse_gradient_xml_with_limits(bytes, &Limits::default())
}
pub fn parse_gradient_xml_with_limits(bytes: &[u8], limits: &Limits) -> Result<Gradient, Error> {
    limits.validate()?;
    error::Budget::check("input bytes", bytes.len(), limits.max_input_bytes)?;
    gradient::decode(bytes, &mut error::Budget::new(limits))
}

/// Parse with explicit, nonzero resource limits.
pub fn parse_with_limits<'a>(bytes: &'a [u8], limits: &Limits) -> Result<Document<'a>, Error> {
    limits.validate()?;
    let mut budget = error::Budget::new(limits);
    let (root, trailing) = rifx::read(bytes, &mut budget)?;
    let project = decode::project(&root, &mut budget)?;
    Ok(Document {
        input: bytes,
        root,
        trailing,
        project,
    })
}
