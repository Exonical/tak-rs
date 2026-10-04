//! [`CotEvent`] → CoT XML.

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use tak_core::DetailNode;

use crate::detail::fmt_f64;
use crate::error::CotError;
use crate::event::CotEvent;

/// Serialisation options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteOptions {
    /// Emit `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>`.
    pub declaration: bool,
    /// Pretty-print with this many spaces per level; `None` for compact.
    pub indent: Option<usize>,
}

impl WriteOptions {
    /// Compact output with declaration, as sent on the wire.
    pub const WIRE: Self = Self {
        declaration: true,
        indent: None,
    };
    /// Indented output without declaration, for humans.
    pub const PRETTY: Self = Self {
        declaration: false,
        indent: Some(2),
    };
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self::WIRE
    }
}

/// Serialise with [`WriteOptions::WIRE`].
pub fn to_xml(event: &CotEvent) -> Result<String, CotError> {
    to_xml_with(event, &WriteOptions::WIRE)
}

/// Serialise with explicit options.
///
/// Fails if the point is out of range or if any element/attribute name or
/// value cannot be represented in XML 1.0.
pub fn to_xml_with(event: &CotEvent, opts: &WriteOptions) -> Result<String, CotError> {
    event.point.validate()?;
    let mut w = match opts.indent {
        Some(n) => Writer::new_with_indent(Vec::new(), b' ', n),
        None => Writer::new(Vec::new()),
    };
    if opts.declaration {
        w.write_event(Event::Decl(BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            Some("yes"),
        )))?;
    }

    let mut root = BytesStart::new("event");
    push_attr(&mut root, "version", &event.version)?;
    push_attr(&mut root, "uid", event.uid.as_str())?;
    push_attr(&mut root, "type", event.cot_type.as_str())?;
    if let Some(how) = &event.how {
        push_attr(&mut root, "how", how.as_str())?;
    }
    push_attr(&mut root, "time", &event.time.to_cot_string())?;
    push_attr(&mut root, "start", &event.start.to_cot_string())?;
    push_attr(&mut root, "stale", &event.stale.to_cot_string())?;
    for (key, value) in [
        ("access", &event.access),
        ("opex", &event.opex),
        ("qos", &event.qos),
        ("caveat", &event.caveat),
        ("releaseableTo", &event.releaseable_to),
    ] {
        if let Some(v) = value {
            push_attr(&mut root, key, v)?;
        }
    }
    for (k, v) in &event.extra_attributes {
        check_name(k)?;
        push_attr(&mut root, k, v)?;
    }
    w.write_event(Event::Start(root))?;

    let mut point = BytesStart::new("point");
    point.push_attribute(("lat", fmt_f64(event.point.lat).as_str()));
    point.push_attribute(("lon", fmt_f64(event.point.lon).as_str()));
    point.push_attribute(("hae", fmt_f64(event.point.hae).as_str()));
    point.push_attribute(("ce", fmt_f64(event.point.ce).as_str()));
    point.push_attribute(("le", fmt_f64(event.point.le).as_str()));
    w.write_event(Event::Empty(point))?;

    if event.detail.is_empty() {
        w.write_event(Event::Empty(BytesStart::new("detail")))?;
    } else {
        w.write_event(Event::Start(BytesStart::new("detail")))?;
        for node in event.detail.iter() {
            write_node(&mut w, node)?;
        }
        w.write_event(Event::End(BytesEnd::new("detail")))?;
    }
    w.write_event(Event::End(BytesEnd::new("event")))?;

    String::from_utf8(w.into_inner()).map_err(|e| CotError::Write(e.to_string()))
}

/// Serialise detail nodes as a root-less XML fragment (TAK Protocol `xmlDetail`).
pub fn nodes_to_xml(nodes: &[DetailNode]) -> Result<String, CotError> {
    let mut w = Writer::new(Vec::new());
    for node in nodes {
        write_node(&mut w, node)?;
    }
    String::from_utf8(w.into_inner()).map_err(|e| CotError::Write(e.to_string()))
}

/// Serialise a single detail node (useful for debugging and tests).
pub fn node_to_xml(node: &DetailNode) -> Result<String, CotError> {
    let mut w = Writer::new(Vec::new());
    write_node(&mut w, node)?;
    String::from_utf8(w.into_inner()).map_err(|e| CotError::Write(e.to_string()))
}

fn write_node(w: &mut Writer<Vec<u8>>, node: &DetailNode) -> Result<(), CotError> {
    check_name(&node.name)?;
    let mut start = BytesStart::new(node.name.as_str());
    for (k, v) in &node.attributes {
        check_name(k)?;
        push_attr(&mut start, k, v)?;
    }
    if node.text.is_none() && node.children.is_empty() {
        w.write_event(Event::Empty(start))?;
        return Ok(());
    }
    w.write_event(Event::Start(start))?;
    if let Some(text) = &node.text {
        check_chars(text)?;
        w.write_event(Event::Text(BytesText::new(text)))?;
    }
    for child in &node.children {
        write_node(w, child)?;
    }
    w.write_event(Event::End(BytesEnd::new(node.name.as_str())))?;
    Ok(())
}

fn push_attr(start: &mut BytesStart<'_>, key: &str, value: &str) -> Result<(), CotError> {
    check_chars(value)?;
    start.push_attribute((key, value));
    Ok(())
}

/// XML 1.0 `Name` production (simplified to the characters TAK uses; letters
/// may be any Unicode alphabetic character).
fn check_name(name: &str) -> Result<(), CotError> {
    let mut chars = name.chars();
    let ok_first = chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == ':');
    let ok_rest = chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.'));
    if ok_first && ok_rest {
        Ok(())
    } else {
        Err(CotError::IllegalXml(name.to_owned()))
    }
}

/// Reject characters that XML 1.0 cannot carry even when escaped.
fn check_chars(s: &str) -> Result<(), CotError> {
    let bad = s.chars().any(|c| {
        matches!(c, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}')
    });
    if bad {
        Err(CotError::IllegalXml(s.chars().take(32).collect()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use tak_core::{CotType, Extensions, How, TakUid, Timestamp};

    fn sample() -> CotEvent {
        let t = Timestamp::parse_rfc3339("2024-05-01T12:00:00Z").unwrap();
        let stale = Timestamp::parse_rfc3339("2024-05-01T12:05:00Z").unwrap();
        CotEvent::new(
            TakUid::new("X-1").unwrap(),
            CotType::new("a-f-G-U-C").unwrap(),
            crate::CotPoint::new(38.8977, -77.0365).unwrap(),
            t,
            stale,
        )
        .with_how(How::machine_gps())
        .with_detail(Extensions::from_nodes(vec![
            DetailNode::new("contact").with_attr("callsign", "A & B \"quoted\""),
            DetailNode::new("remarks").with_text("<not a tag> & more"),
            DetailNode::new("__vendor").with_child(DetailNode::new("inner").with_attr("k", "v")),
        ]))
    }

    #[test]
    fn writes_wire_format() {
        let xml = to_xml(&sample()).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>"));
        assert!(xml.contains(r#"<event version="2.0" uid="X-1" type="a-f-G-U-C" how="m-g" time="2024-05-01T12:00:00.000Z""#));
        assert!(xml.contains(
            r#"<point lat="38.8977" lon="-77.0365" hae="9999999.0" ce="9999999.0" le="9999999.0"/>"#
        ));
        assert!(xml.contains("callsign=\"A &amp; B &quot;quoted&quot;\""));
        assert!(xml.contains("<remarks>&lt;not a tag&gt; &amp; more</remarks>"));
        assert!(!xml.contains('\n'));
    }

    #[test]
    fn round_trips_through_parser() {
        let ev = sample();
        let xml = to_xml(&ev).unwrap();
        let back = parse(&xml).unwrap();
        assert_eq!(back, ev);
        let pretty = to_xml_with(&ev, &WriteOptions::PRETTY).unwrap();
        assert!(pretty.contains('\n'));
        assert_eq!(parse(&pretty).unwrap(), ev);
    }

    #[test]
    fn rejects_illegal_names_and_chars() {
        let mut ev = sample();
        ev.detail = Extensions::from_nodes(vec![DetailNode::new("bad name")]);
        assert!(matches!(to_xml(&ev), Err(CotError::IllegalXml(_))));
        let mut ev = sample();
        ev.detail = Extensions::from_nodes(vec![DetailNode::new("x").with_attr("a", "\u{1}")]);
        assert!(matches!(to_xml(&ev), Err(CotError::IllegalXml(_))));
        let mut ev = sample();
        ev.detail = Extensions::from_nodes(vec![DetailNode::new("x").with_attr("<a", "1")]);
        assert!(matches!(to_xml(&ev), Err(CotError::IllegalXml(_))));
        let mut ev = sample();
        ev.point.lat = 100.0;
        assert!(matches!(
            to_xml(&ev),
            Err(CotError::InvalidAttribute { .. })
        ));
    }
}
