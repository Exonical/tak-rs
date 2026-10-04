//! Error type for the CoT codec.

use tak_core::ValidationError;

/// Everything that can go wrong while parsing, serialising or adapting CoT.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CotError {
    /// Input is larger than the configured limit.
    #[error("CoT document is {actual} bytes, limit is {max}")]
    TooLarge {
        /// Configured maximum.
        max: usize,
        /// Observed size.
        actual: usize,
    },
    /// Element nesting exceeded the configured limit.
    #[error("element nesting depth {depth} exceeds limit {max}")]
    TooDeep {
        /// Configured maximum.
        max: usize,
        /// Depth at which the limit was hit.
        depth: usize,
    },
    /// Too many elements in one document.
    #[error("document contains more than {max} elements")]
    TooManyNodes {
        /// Configured maximum.
        max: usize,
    },
    /// Too many attributes on one element.
    #[error("element <{element}> has more than {max} attributes")]
    TooManyAttributes {
        /// Element name.
        element: String,
        /// Configured maximum.
        max: usize,
    },
    /// `<!DOCTYPE>` is never allowed (prevents entity-expansion attacks).
    #[error("DTD declarations are not permitted in CoT")]
    DoctypeNotAllowed,
    /// An entity reference that is neither predefined nor a character reference.
    #[error("unknown entity reference `&{0};`")]
    UnknownEntity(String),
    /// The underlying XML tokenizer rejected the input.
    #[error("malformed XML: {0}")]
    Xml(#[from] quick_xml::Error),
    /// Document ended before the root element was closed.
    #[error("document ended before <{0}> was closed")]
    Truncated(String),
    /// Root element is not `<event>`.
    #[error("expected root element <event>, found <{0}>")]
    UnexpectedRoot(String),
    /// No root element at all.
    #[error("document contains no <event> element")]
    MissingEvent,
    /// `<event>` has no `<point>` child.
    #[error("<event> is missing its <point> child")]
    MissingPoint,
    /// A child that may only appear once appeared twice.
    #[error("duplicate <{0}> element")]
    Duplicate(String),
    /// A child element that the CoT schema does not allow at this position.
    #[error("unexpected element <{child}> inside <{parent}>")]
    UnexpectedElement {
        /// Parent element name.
        parent: String,
        /// Offending child name.
        child: String,
    },
    /// Non-whitespace character data where only elements are allowed.
    #[error("unexpected text content inside <{0}>")]
    UnexpectedText(String),
    /// Content after the closing `</event>`.
    #[error("unexpected content after </event>")]
    TrailingContent,
    /// A required attribute is absent.
    #[error("<{element}> is missing required attribute `{attribute}`")]
    MissingAttribute {
        /// Element name.
        element: String,
        /// Attribute name.
        attribute: String,
    },
    /// An attribute value could not be interpreted.
    #[error("invalid value for <{element} {attribute}=\"{value}\">: {reason}")]
    InvalidAttribute {
        /// Element name.
        element: String,
        /// Attribute name.
        attribute: String,
        /// Offending value (truncated for display).
        value: String,
        /// Human-readable reason.
        reason: String,
    },
    /// A name or value cannot be represented in XML 1.0.
    #[error("`{0}` is not a legal XML name or contains characters illegal in XML 1.0")]
    IllegalXml(String),
    /// Domain-model validation failed while adapting.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// A CoT event cannot be expressed as the requested domain type.
    #[error("CoT event is not representable as {target}: {reason}")]
    NotRepresentable {
        /// Target domain type name.
        target: &'static str,
        /// Why.
        reason: String,
    },
    /// Serialisation failed (should be unreachable when writing to memory).
    #[error("failed to write XML: {0}")]
    Write(String),
}

impl From<std::io::Error> for CotError {
    fn from(e: std::io::Error) -> Self {
        Self::Write(e.to_string())
    }
}

impl CotError {
    pub(crate) fn invalid_attr(
        element: &str,
        attribute: &str,
        value: &str,
        reason: impl Into<String>,
    ) -> Self {
        let mut value = value.to_owned();
        if value.len() > 64 {
            let mut cut = 64;
            while !value.is_char_boundary(cut) {
                cut -= 1;
            }
            value.truncate(cut);
            value.push('…');
        }
        Self::InvalidAttribute {
            element: element.to_owned(),
            attribute: attribute.to_owned(),
            value,
            reason: reason.into(),
        }
    }

    pub(crate) fn missing_attr(element: &str, attribute: &str) -> Self {
        Self::MissingAttribute {
            element: element.to_owned(),
            attribute: attribute.to_owned(),
        }
    }
}
