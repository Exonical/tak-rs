//! CoT XML → [`CotEvent`].
//!
//! The parser is designed for untrusted input from the network:
//!
//! * hard limits on document size, nesting depth, element count and
//!   attributes per element ([`ParseLimits`]);
//! * `<!DOCTYPE>` is rejected outright, so no entity expansion is possible;
//! * only the five predefined entities and numeric character references are
//!   resolved;
//! * any `<detail>` child is accepted and preserved verbatim as a
//!   [`DetailNode`] tree; only `<event>` and `<point>` attributes are
//!   interpreted.

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use tak_core::{CotType, DetailNode, Extensions, How, TakUid, Timestamp};

use crate::error::CotError;
use crate::event::{COT_VERSION, CotEvent, CotPoint};

/// Resource limits applied while parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseLimits {
    /// Maximum document size in bytes.
    pub max_bytes: usize,
    /// Maximum element nesting depth below `<event>`.
    pub max_depth: usize,
    /// Maximum number of elements in the document.
    pub max_nodes: usize,
    /// Maximum attributes on a single element.
    pub max_attributes: usize,
}

impl ParseLimits {
    /// Defaults suitable for TAK traffic: 1 MiB, depth 32, 10 000 elements,
    /// 256 attributes.
    pub const DEFAULT: Self = Self {
        max_bytes: 1024 * 1024,
        max_depth: 32,
        max_nodes: 10_000,
        max_attributes: 256,
    };
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Parse a single CoT document with [`ParseLimits::DEFAULT`].
pub fn parse(xml: &str) -> Result<CotEvent, CotError> {
    parse_with_limits(xml, &ParseLimits::DEFAULT)
}

/// Parse a single CoT document with explicit limits.
pub fn parse_with_limits(xml: &str, limits: &ParseLimits) -> Result<CotEvent, CotError> {
    if xml.len() > limits.max_bytes {
        return Err(CotError::TooLarge {
            max: limits.max_bytes,
            actual: xml.len(),
        });
    }
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    let mut parser = Parser {
        reader,
        limits,
        nodes: 0,
    };
    parser.document()
}

struct Parser<'a, 'l> {
    reader: Reader<&'a [u8]>,
    limits: &'l ParseLimits,
    nodes: usize,
}

impl<'a> Parser<'a, '_> {
    fn next(&mut self) -> Result<Event<'a>, CotError> {
        Ok(self.reader.read_event()?)
    }

    fn document(&mut self) -> Result<CotEvent, CotError> {
        loop {
            match self.next()? {
                Event::Decl(_) | Event::PI(_) | Event::Comment(_) => {}
                Event::DocType(_) => return Err(CotError::DoctypeNotAllowed),
                Event::Text(t) if t.xml10_content().trim().is_empty() => {}
                Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                    return Err(CotError::UnexpectedText("document".into()));
                }
                Event::Start(start) => {
                    let name = start.name();
                    if name.as_ref() != "event" {
                        return Err(CotError::UnexpectedRoot(name.as_ref().to_owned()));
                    }
                    let event = self.event(&start)?;
                    self.trailing()?;
                    return Ok(event);
                }
                Event::Empty(_) | Event::End(_) => {
                    // Cannot happen with `expand_empty_elements`; unmatched
                    // ends are rejected by quick-xml itself.
                    return Err(CotError::MissingEvent);
                }
                Event::Eof => return Err(CotError::MissingEvent),
            }
        }
    }

    fn trailing(&mut self) -> Result<(), CotError> {
        loop {
            match self.next()? {
                Event::Eof => return Ok(()),
                Event::Comment(_) | Event::PI(_) => {}
                Event::Text(t) if t.xml10_content().trim().is_empty() => {}
                _ => return Err(CotError::TrailingContent),
            }
        }
    }

    fn attributes(&self, start: &BytesStart<'_>) -> Result<Vec<(String, String)>, CotError> {
        let element = start.name();
        let mut out = Vec::new();
        for attr in start.attributes() {
            let attr = attr.map_err(quick_xml::Error::InvalidAttr)?;
            if out.len() >= self.limits.max_attributes {
                return Err(CotError::TooManyAttributes {
                    element: element.as_ref().to_owned(),
                    max: self.limits.max_attributes,
                });
            }
            let value = attr.normalized_value(XmlVersion::Implicit1_0)?;
            out.push((attr.key.as_ref().to_owned(), value.into_owned()));
        }
        Ok(out)
    }

    #[allow(clippy::too_many_lines)]
    fn event(&mut self, start: &BytesStart<'_>) -> Result<CotEvent, CotError> {
        let mut version = None;
        let mut uid = None;
        let mut cot_type = None;
        let mut how = None;
        let mut time = None;
        let mut start_t = None;
        let mut stale = None;
        let mut access = None;
        let mut opex = None;
        let mut qos = None;
        let mut caveat = None;
        let mut releaseable_to = None;
        let mut extra = Vec::new();

        for (key, value) in self.attributes(start)? {
            match key.as_str() {
                "version" => version = Some(value),
                "uid" => uid = Some(value),
                "type" => cot_type = Some(value),
                "how" => how = Some(value),
                "time" => time = Some(value),
                "start" => start_t = Some(value),
                "stale" => stale = Some(value),
                "access" => access = Some(value),
                "opex" => opex = Some(value),
                "qos" => qos = Some(value),
                "caveat" => caveat = Some(value),
                "releaseableTo" => releaseable_to = Some(value),
                _ => extra.push((key, value)),
            }
        }

        let uid = required("event", "uid", uid)?;
        let uid = TakUid::new(uid.as_str())
            .map_err(|e| CotError::invalid_attr("event", "uid", &uid, e.to_string()))?;
        let cot_type = required("event", "type", cot_type)?;
        let cot_type = CotType::new(cot_type.as_str())
            .map_err(|e| CotError::invalid_attr("event", "type", &cot_type, e.to_string()))?;
        let how = match how {
            None => None,
            Some(h) if h.trim().is_empty() => None,
            Some(h) => Some(
                How::new(h.as_str())
                    .map_err(|e| CotError::invalid_attr("event", "how", &h, e.to_string()))?,
            ),
        };
        let time = parse_time("time", &required("event", "time", time)?)?;
        let start_t = parse_time("start", &required("event", "start", start_t)?)?;
        let stale = parse_time("stale", &required("event", "stale", stale)?)?;

        let mut point = None;
        let mut detail = None;
        loop {
            match self.next()? {
                Event::Start(child) => {
                    let name = child.name().as_ref().to_owned();
                    match name.as_str() {
                        "point" => {
                            if point.is_some() {
                                return Err(CotError::Duplicate(name));
                            }
                            point = Some(self.point(&child)?);
                        }
                        "detail" => {
                            if detail.is_some() {
                                return Err(CotError::Duplicate(name));
                            }
                            let node = self.subtree(&child, 1)?;
                            detail = Some(Extensions::from_nodes(node.children));
                        }
                        _ => {
                            return Err(CotError::UnexpectedElement {
                                parent: "event".into(),
                                child: name,
                            });
                        }
                    }
                }
                Event::End(_) => break,
                Event::Text(t) if t.xml10_content().trim().is_empty() => {}
                Event::Comment(_) | Event::PI(_) => {}
                Event::DocType(_) => return Err(CotError::DoctypeNotAllowed),
                Event::Text(_)
                | Event::CData(_)
                | Event::GeneralRef(_)
                | Event::Decl(_)
                | Event::Empty(_) => {
                    return Err(CotError::UnexpectedText("event".into()));
                }
                Event::Eof => return Err(CotError::Truncated("event".into())),
            }
        }

        let point = point.ok_or(CotError::MissingPoint)?;
        Ok(CotEvent {
            version: version.unwrap_or_else(|| COT_VERSION.to_owned()),
            uid,
            cot_type,
            how,
            time,
            start: start_t,
            stale,
            access,
            opex,
            qos,
            caveat,
            releaseable_to,
            extra_attributes: extra,
            point,
            detail: detail.unwrap_or_default(),
        })
    }

    fn point(&mut self, start: &BytesStart<'_>) -> Result<CotPoint, CotError> {
        let mut lat = None;
        let mut lon = None;
        let mut hae = None;
        let mut ce = None;
        let mut le = None;
        for (key, value) in self.attributes(start)? {
            let slot = match key.as_str() {
                "lat" => &mut lat,
                "lon" => &mut lon,
                "hae" => &mut hae,
                "ce" => &mut ce,
                "le" => &mut le,
                _ => continue,
            };
            let v = crate::detail::parse_f64(&value).ok_or_else(|| {
                CotError::invalid_attr("point", &key, &value, "expected a number")
            })?;
            *slot = Some(v);
        }
        // Consume children up to </point>; the schema allows none, and we
        // deliberately ignore anything an emitter might put there.
        self.reader.read_to_end(start.name())?;
        let point = CotPoint {
            lat: required("point", "lat", lat)?,
            lon: required("point", "lon", lon)?,
            hae: hae.unwrap_or(crate::event::UNKNOWN_SENTINEL),
            ce: ce.unwrap_or(crate::event::UNKNOWN_SENTINEL),
            le: le.unwrap_or(crate::event::UNKNOWN_SENTINEL),
        };
        point.validate()?;
        Ok(point)
    }

    fn subtree(&mut self, start: &BytesStart<'_>, depth: usize) -> Result<DetailNode, CotError> {
        if depth > self.limits.max_depth {
            return Err(CotError::TooDeep {
                max: self.limits.max_depth,
                depth,
            });
        }
        self.nodes += 1;
        if self.nodes > self.limits.max_nodes {
            return Err(CotError::TooManyNodes {
                max: self.limits.max_nodes,
            });
        }
        let name = start.name().as_ref().to_owned();
        let mut node = DetailNode::new(name.clone());
        node.attributes = self.attributes(start)?;
        let mut text = String::new();
        loop {
            match self.next()? {
                Event::Start(child) => {
                    let child = self.subtree(&child, depth + 1)?;
                    node.children.push(child);
                }
                Event::End(_) => break,
                Event::Text(t) => text.push_str(&t.xml10_content()),
                Event::CData(c) => text.push_str(&c.xml10_content()),
                Event::GeneralRef(r) => {
                    if r.is_char_ref() {
                        match r.resolve_char_ref()? {
                            Some(ch) => text.push(ch),
                            None => {
                                return Err(CotError::UnknownEntity(r.into_inner().into_owned()));
                            }
                        }
                    } else {
                        let entity = r.into_inner();
                        match resolve_predefined_entity(&entity) {
                            Some(s) => text.push_str(s),
                            None => return Err(CotError::UnknownEntity(entity.into_owned())),
                        }
                    }
                }
                Event::Comment(_) | Event::PI(_) => {}
                Event::DocType(_) => return Err(CotError::DoctypeNotAllowed),
                Event::Decl(_) | Event::Empty(_) => {
                    return Err(CotError::UnexpectedText(name));
                }
                Event::Eof => return Err(CotError::Truncated(name)),
            }
        }
        if !text.trim().is_empty() {
            node.text = Some(text);
        }
        Ok(node)
    }
}

fn required<T>(element: &str, attribute: &str, v: Option<T>) -> Result<T, CotError> {
    v.ok_or_else(|| CotError::missing_attr(element, attribute))
}

fn parse_time(attribute: &str, raw: &str) -> Result<Timestamp, CotError> {
    Timestamp::parse_rfc3339(raw.trim())
        .map_err(|e| CotError::invalid_attr("event", attribute, raw, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"<event version="2.0" uid="X-1" type="a-f-G-U-C" how="m-g" time="2024-05-01T12:00:00.000Z" start="2024-05-01T12:00:00.000Z" stale="2024-05-01T12:05:00.000Z"><point lat="38.8977" lon="-77.0365" hae="20.0" ce="5.0" le="9999999.0"/><detail/></event>"#;

    #[test]
    fn parses_minimal_event() {
        let e = parse(MINIMAL).unwrap();
        assert_eq!(e.uid.as_str(), "X-1");
        assert_eq!(e.cot_type.as_str(), "a-f-G-U-C");
        assert_eq!(e.how.as_ref().map(How::as_str), Some("m-g"));
        assert_eq!(e.point.lat, 38.8977);
        assert_eq!(e.point.hae_metres(), Some(20.0));
        assert_eq!(e.point.le_metres(), None);
        assert!(e.detail.is_empty());
    }

    #[test]
    fn accepts_declaration_bom_and_whitespace() {
        let doc = format!(
            "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n  {MINIMAL}\n\n"
        );
        assert!(parse(&doc).is_ok());
    }

    #[test]
    fn rejects_doctype() {
        let doc = format!("<!DOCTYPE event [<!ENTITY x \"y\">]>{MINIMAL}");
        assert!(matches!(parse(&doc), Err(CotError::DoctypeNotAllowed)));
    }

    #[test]
    fn rejects_unknown_entity_but_resolves_predefined() {
        let doc = MINIMAL.replace(
            "<detail/>",
            "<detail><remarks>a &amp; b &lt; c &#x41;&#66;</remarks></detail>",
        );
        let e = parse(&doc).unwrap();
        assert_eq!(
            e.detail.get("remarks").unwrap().text.as_deref(),
            Some("a & b < c AB")
        );
        let doc = MINIMAL.replace("<detail/>", "<detail><remarks>&bogus;</remarks></detail>");
        assert!(matches!(parse(&doc), Err(CotError::UnknownEntity(ref n)) if n == "bogus"));
    }

    #[test]
    fn rejects_out_of_range_latitude() {
        let doc = MINIMAL.replace("lat=\"38.8977\"", "lat=\"91\"");
        assert!(matches!(
            parse(&doc),
            Err(CotError::InvalidAttribute { .. })
        ));
    }

    #[test]
    fn rejects_missing_point_and_uid() {
        let doc = MINIMAL.replace(
            r#"<point lat="38.8977" lon="-77.0365" hae="20.0" ce="5.0" le="9999999.0"/>"#,
            "",
        );
        assert!(matches!(parse(&doc), Err(CotError::MissingPoint)));
        let doc = MINIMAL.replace(r#"uid="X-1" "#, "");
        assert!(matches!(
            parse(&doc),
            Err(CotError::MissingAttribute { .. })
        ));
    }

    #[test]
    fn rejects_wrong_root_and_trailing_content() {
        assert!(matches!(parse("<foo/>"), Err(CotError::UnexpectedRoot(_))));
        assert!(matches!(parse(""), Err(CotError::MissingEvent)));
        let doc = format!("{MINIMAL}<event/>");
        assert!(matches!(parse(&doc), Err(CotError::TrailingContent)));
    }

    #[test]
    fn rejects_truncated_document() {
        let cut = &MINIMAL[..MINIMAL.len() - 10];
        assert!(parse(cut).is_err());
    }

    #[test]
    fn enforces_depth_and_node_limits() {
        let deep = format!("<detail>{}{}</detail>", "<a>".repeat(40), "</a>".repeat(40));
        let doc = MINIMAL.replace("<detail/>", &deep);
        assert!(matches!(parse(&doc), Err(CotError::TooDeep { .. })));
        let wide = format!("<detail>{}</detail>", "<a/>".repeat(20_000));
        let doc = MINIMAL.replace("<detail/>", &wide);
        assert!(matches!(parse(&doc), Err(CotError::TooManyNodes { .. })));
        let limits = ParseLimits {
            max_bytes: 10,
            ..ParseLimits::DEFAULT
        };
        assert!(matches!(
            parse_with_limits(MINIMAL, &limits),
            Err(CotError::TooLarge { .. })
        ));
    }

    #[test]
    fn preserves_unknown_detail_and_event_attributes() {
        let doc = MINIMAL
            .replace("<detail/>", r#"<detail><__vendor a="1"><inner>t</inner></__vendor><contact callsign="A"/></detail>"#)
            .replace("how=\"m-g\"", "how=\"m-g\" foo=\"bar\"");
        let e = parse(&doc).unwrap();
        assert_eq!(
            e.extra_attributes,
            vec![("foo".to_owned(), "bar".to_owned())]
        );
        let vendor = e.detail.get("__vendor").unwrap();
        assert_eq!(vendor.attr("a"), Some("1"));
        assert_eq!(vendor.child("inner").unwrap().text.as_deref(), Some("t"));
        assert_eq!(e.detail.len(), 2);
    }

    #[test]
    fn how_is_optional_and_version_defaults() {
        let doc = MINIMAL
            .replace("version=\"2.0\" ", "")
            .replace("how=\"m-g\" ", "");
        let e = parse(&doc).unwrap();
        assert_eq!(e.version, "2.0");
        assert!(e.how.is_none());
    }
}
