//! Conversion between `tak-cot`'s wire-shaped [`CotEvent`] and the protobuf
//! [`TakMessage`].
//!
//! The schema hoists six detail elements (`contact`, `__group`,
//! `precisionlocation`, `status`, `takv`, `track`) into typed fields. A detail
//! element is hoisted only when it is *fully* representable (no unknown
//! attributes, no children, no text); otherwise it stays in `xmlDetail` so
//! nothing is lost. On the way back, hoisted fields become ordinary
//! [`DetailNode`]s again.

use tak_core::{CotType, DetailNode, Extensions, How, TakUid, Timestamp};
use tak_cot::detail::{
    ContactDetail, GroupDetail, KnownDetail as _, PrecisionLocationDetail, StatusDetail,
    TakvDetail, TrackDetail,
};
use tak_cot::{COT_VERSION, CotEvent, CotPoint, UNKNOWN_SENTINEL, parse_detail_fragment};

use crate::error::ProtoError;
use crate::message::{
    self, Contact, Detail, Group, PrecisionLocation, Status, TakMessage, Takv, Track,
};

/// Names of `CotEvent` fields that TAK Protocol cannot carry and that
/// [`to_proto`] therefore drops. Empty when the conversion is lossless.
#[must_use]
pub fn lossy_fields(event: &CotEvent) -> Vec<&'static str> {
    let mut out = Vec::new();
    if event.version != COT_VERSION {
        out.push("version");
    }
    if event.caveat.is_some() {
        out.push("caveat");
    }
    if event.releaseable_to.is_some() {
        out.push("releaseableTo");
    }
    if !event.extra_attributes.is_empty() {
        out.push("extra event attributes");
    }
    out
}

/// Convert a CoT event to a `TakMessage` carrying a protobuf `CotEvent`.
///
/// Fields listed by [`lossy_fields`] are dropped (TAK Protocol has no slot
/// for them); everything in `<detail>` is preserved.
pub fn to_proto(event: &CotEvent) -> Result<TakMessage, ProtoError> {
    let detail = detail_to_proto(&event.detail)?;
    let proto = message::CotEvent {
        r#type: event.cot_type.as_str().to_owned(),
        access: event.access.clone().unwrap_or_default(),
        qos: event.qos.clone().unwrap_or_default(),
        opex: event.opex.clone().unwrap_or_default(),
        uid: event.uid.as_str().to_owned(),
        send_time: millis("time", event.time)?,
        start_time: millis("start", event.start)?,
        stale_time: millis("stale", event.stale)?,
        how: event
            .how
            .as_ref()
            .map(|h| h.as_str().to_owned())
            .unwrap_or_default(),
        lat: event.point.lat,
        lon: event.point.lon,
        hae: event.point.hae,
        ce: event.point.ce,
        le: event.point.le,
        detail: Some(detail),
    };
    Ok(TakMessage::event(proto))
}

/// Convert a `TakMessage` back into a CoT event.
///
/// Returns [`ProtoError::Empty`] for control-only messages; callers handle
/// `takControl` before calling this.
pub fn from_proto(message: &TakMessage) -> Result<CotEvent, ProtoError> {
    let proto = message.cot_event.as_ref().ok_or(ProtoError::Empty)?;
    let uid = TakUid::new(proto.uid.as_str())?;
    let cot_type = CotType::new(proto.r#type.as_str())?;
    let point = CotPoint {
        lat: proto.lat,
        lon: proto.lon,
        hae: or_sentinel(proto.hae),
        ce: or_sentinel(proto.ce),
        le: or_sentinel(proto.le),
    };
    let time = timestamp("time", proto.send_time)?;
    let stale = timestamp("stale", proto.stale_time)?;
    let mut event = CotEvent::new(uid, cot_type, point, time, stale);
    event.start = if proto.start_time == 0 {
        time
    } else {
        timestamp("start", proto.start_time)?
    };
    event.how = non_empty(&proto.how).map(How::new).transpose()?;
    event.access = non_empty(&proto.access).map(str::to_owned);
    event.qos = non_empty(&proto.qos).map(str::to_owned);
    event.opex = non_empty(&proto.opex).map(str::to_owned);
    if let Some(detail) = &proto.detail {
        event.detail = detail_from_proto(detail)?;
    }
    Ok(event)
}

fn detail_to_proto(detail: &Extensions) -> Result<Detail, ProtoError> {
    let mut out = Detail::default();
    let mut rest: Vec<&DetailNode> = Vec::with_capacity(detail.len());
    for node in detail.iter() {
        if !node.children.is_empty() || node.text.is_some() {
            rest.push(node);
            continue;
        }
        let hoisted = match node.name.as_str() {
            ContactDetail::NAME if out.contact.is_none() => {
                contact_to_proto(node).map(|c| out.contact = Some(c))
            }
            GroupDetail::NAME if out.group.is_none() => {
                group_to_proto(node).map(|g| out.group = Some(g))
            }
            PrecisionLocationDetail::NAME if out.precision_location.is_none() => {
                precision_to_proto(node).map(|p| out.precision_location = Some(p))
            }
            StatusDetail::NAME if out.status.is_none() => {
                status_to_proto(node).map(|s| out.status = Some(s))
            }
            TakvDetail::NAME if out.takv.is_none() => {
                takv_to_proto(node).map(|t| out.takv = Some(t))
            }
            TrackDetail::NAME if out.track.is_none() => {
                track_to_proto(node).map(|t| out.track = Some(t))
            }
            _ => None,
        };
        if hoisted.is_none() {
            rest.push(node);
        }
    }
    if !rest.is_empty() {
        let owned: Vec<DetailNode> = rest.into_iter().cloned().collect();
        out.xml_detail = tak_cot::nodes_to_xml(&owned)?;
    }
    Ok(out)
}

fn detail_from_proto(detail: &Detail) -> Result<Extensions, ProtoError> {
    let mut nodes = Vec::new();
    if let Some(c) = &detail.contact {
        nodes.push(
            ContactDetail {
                callsign: c.callsign.clone(),
                endpoint: non_empty(&c.endpoint).map(str::to_owned),
                ..ContactDetail::default()
            }
            .to_node(),
        );
    }
    if let Some(g) = &detail.group {
        nodes.push(
            GroupDetail {
                name: g.name.clone(),
                role: g.role.clone(),
                extra: Vec::new(),
            }
            .to_node(),
        );
    }
    if let Some(p) = &detail.precision_location {
        nodes.push(
            PrecisionLocationDetail {
                geopointsrc: non_empty(&p.geopointsrc).map(str::to_owned),
                altsrc: non_empty(&p.altsrc).map(str::to_owned),
                extra: Vec::new(),
            }
            .to_node(),
        );
    }
    if let Some(s) = &detail.status {
        nodes.push(
            StatusDetail {
                battery: Some(u8::try_from(s.battery.min(100)).unwrap_or(100)),
                readiness: None,
                extra: Vec::new(),
            }
            .to_node(),
        );
    }
    if let Some(t) = &detail.takv {
        nodes.push(
            TakvDetail {
                device: non_empty(&t.device).map(str::to_owned),
                platform: non_empty(&t.platform).map(str::to_owned),
                os: non_empty(&t.os).map(str::to_owned),
                version: non_empty(&t.version).map(str::to_owned),
                extra: Vec::new(),
            }
            .to_node(),
        );
    }
    if let Some(t) = &detail.track {
        nodes.push(
            TrackDetail {
                course: Some(t.course),
                speed: Some(t.speed),
                extra: Vec::new(),
            }
            .to_node(),
        );
    }
    if !detail.xml_detail.is_empty() {
        nodes.extend(parse_detail_fragment(&detail.xml_detail)?);
    }
    Ok(Extensions::from_nodes(nodes))
}

// Hoisting helpers return `None` when the element carries anything the proto
// field cannot express, so the caller keeps it in `xmlDetail` instead.

fn only_attrs(node: &DetailNode, allowed: &[&str]) -> bool {
    node.attributes
        .iter()
        .all(|(k, _)| allowed.contains(&k.as_str()))
}

fn contact_to_proto(node: &DetailNode) -> Option<Contact> {
    if !only_attrs(node, &["callsign", "endpoint"]) {
        return None;
    }
    Some(Contact {
        endpoint: node.attr("endpoint").unwrap_or_default().to_owned(),
        callsign: node.attr("callsign")?.to_owned(),
    })
}

fn group_to_proto(node: &DetailNode) -> Option<Group> {
    if !only_attrs(node, &["name", "role"]) {
        return None;
    }
    Some(Group {
        name: node.attr("name")?.to_owned(),
        role: node.attr("role")?.to_owned(),
    })
}

fn precision_to_proto(node: &DetailNode) -> Option<PrecisionLocation> {
    if !only_attrs(node, &["geopointsrc", "altsrc"]) {
        return None;
    }
    Some(PrecisionLocation {
        geopointsrc: node.attr("geopointsrc").unwrap_or_default().to_owned(),
        altsrc: node.attr("altsrc").unwrap_or_default().to_owned(),
    })
}

fn status_to_proto(node: &DetailNode) -> Option<Status> {
    if !only_attrs(node, &["battery"]) {
        return None;
    }
    let battery = node.attr("battery")?.parse::<u32>().ok()?;
    Some(Status { battery })
}

fn takv_to_proto(node: &DetailNode) -> Option<Takv> {
    if !only_attrs(node, &["device", "platform", "os", "version"]) {
        return None;
    }
    Some(Takv {
        device: node.attr("device").unwrap_or_default().to_owned(),
        platform: node.attr("platform").unwrap_or_default().to_owned(),
        os: node.attr("os").unwrap_or_default().to_owned(),
        version: node.attr("version").unwrap_or_default().to_owned(),
    })
}

fn track_to_proto(node: &DetailNode) -> Option<Track> {
    if !only_attrs(node, &["speed", "course"]) {
        return None;
    }
    let speed = node.attr("speed")?.parse::<f64>().ok()?;
    let course = node.attr("course")?.parse::<f64>().ok()?;
    if !speed.is_finite() || !course.is_finite() {
        return None;
    }
    Some(Track { speed, course })
}

fn millis(field: &'static str, ts: Timestamp) -> Result<u64, ProtoError> {
    u64::try_from(ts.unix_millis()).map_err(|_| ProtoError::TimeOutOfRange { field, millis: 0 })
}

fn timestamp(field: &'static str, millis: u64) -> Result<Timestamp, ProtoError> {
    let signed = i64::try_from(millis).map_err(|_| ProtoError::TimeOutOfRange { field, millis })?;
    Timestamp::from_unix_millis(signed).map_err(|_| ProtoError::TimeOutOfRange { field, millis })
}

fn non_empty(s: &str) -> Option<&str> {
    if s.is_empty() { None } else { Some(s) }
}

/// Proto3 cannot distinguish "absent" from `0.0`; ATAK sends the CoT
/// sentinel explicitly, but a zero `hae`/`ce`/`le` from another producer is
/// treated as "unknown" rather than "sea level / perfect fix".
fn or_sentinel(v: f64) -> f64 {
    if v == 0.0 { UNKNOWN_SENTINEL } else { v }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    /// Hoisting loses attribute *order* (proto has none), so compare
    /// attribute-sorted renderings.
    fn canonical(detail: &Extensions) -> Vec<String> {
        fn sort_attrs(n: &mut DetailNode) {
            n.attributes.sort();
            n.children.iter_mut().for_each(sort_attrs);
        }
        let mut v: Vec<String> = detail
            .iter()
            .map(|n| {
                let mut n = n.clone();
                sort_attrs(&mut n);
                tak_cot::node_to_xml(&n).unwrap()
            })
            .collect();
        v.sort();
        v
    }

    const SA: &str = r#"<event version="2.0" uid="ANDROID-1" type="a-f-G-U-C" how="m-g" time="2024-05-01T12:00:00.000Z" start="2024-05-01T12:00:00.000Z" stale="2024-05-01T12:06:15.000Z"><point lat="38.8977" lon="-77.0365" hae="25.0" ce="9999999.0" le="9999999.0"/><detail><takv os="31" version="4.10.0.5" device="PIXEL" platform="ATAK-CIV"/><contact endpoint="192.168.1.5:4242:tcp" callsign="ALPHA"/><uid Droid="ALPHA"/><precisionlocation altsrc="GPS" geopointsrc="GPS"/><__group role="Team Member" name="Cyan"/><status battery="77"/><track course="90.5" speed="1.25"/><vendor:thing xmlns:vendor="urn:x" k="v"><inner/></vendor:thing></detail></event>"#;

    #[test]
    fn hoists_known_details_and_keeps_rest_in_xml_detail() {
        let event = tak_cot::parse(SA).unwrap();
        let msg = to_proto(&event).unwrap();
        let d = msg.cot_event.as_ref().unwrap().detail.as_ref().unwrap();
        assert_eq!(d.contact.as_ref().unwrap().callsign, "ALPHA");
        assert_eq!(d.group.as_ref().unwrap().name, "Cyan");
        assert_eq!(d.status.as_ref().unwrap().battery, 77);
        assert_eq!(d.track.as_ref().unwrap().course, 90.5);
        assert_eq!(d.takv.as_ref().unwrap().platform, "ATAK-CIV");
        assert!(d.xml_detail.contains("<uid Droid=\"ALPHA\"/>"));
        assert!(d.xml_detail.contains("<vendor:thing"));
        assert!(!d.xml_detail.contains("<contact"));
    }

    #[test]
    fn round_trips_through_bytes() {
        let event = tak_cot::parse(SA).unwrap();
        let bytes = crate::encode(&to_proto(&event).unwrap());
        let back = from_proto(&crate::decode(&bytes).unwrap()).unwrap();
        assert_eq!(back.uid, event.uid);
        assert_eq!(back.cot_type, event.cot_type);
        assert_eq!(back.how, event.how);
        assert_eq!(back.time, event.time);
        assert_eq!(back.stale, event.stale);
        assert_eq!(back.point, event.point);
        assert_eq!(canonical(&back.detail), canonical(&event.detail));
        assert!(lossy_fields(&event).is_empty());
    }

    #[test]
    fn contact_with_extra_attributes_is_not_hoisted() {
        let xml = SA.replace(
            r#"<contact endpoint="192.168.1.5:4242:tcp" callsign="ALPHA"/>"#,
            r#"<contact endpoint="192.168.1.5:4242:tcp" callsign="ALPHA" phone="555"/>"#,
        );
        let event = tak_cot::parse(&xml).unwrap();
        let msg = to_proto(&event).unwrap();
        let d = msg.cot_event.unwrap().detail.unwrap();
        assert!(d.contact.is_none());
        assert!(d.xml_detail.contains(r#"phone="555""#));
    }

    #[test]
    fn control_only_message_is_not_an_event() {
        assert!(matches!(
            from_proto(&TakMessage::control(1, 1, "")),
            Err(ProtoError::Empty)
        ));
    }

    #[test]
    fn reports_lossy_fields() {
        let mut event = tak_cot::parse(SA).unwrap();
        event.caveat = Some("X".into());
        event.extra_attributes.push(("foo".into(), "bar".into()));
        assert_eq!(
            lossy_fields(&event),
            vec!["caveat", "extra event attributes"]
        );
    }
}
