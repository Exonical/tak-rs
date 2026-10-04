//! Conversion between the CoT wire model and the `tak-core` domain model.
//!
//! Decoding ([`to_tak_event`]) classifies an event by its CoT type and
//! `<detail>` contents, consumes the well-known details it understands, and
//! keeps everything else in the resulting object's `extensions`. Encoding
//! ([`from_tak_event`] and friends) reverses the process so that a
//! decode → encode round trip preserves unknown extensions.

use tak_core::{
    Affiliation, Altitude, Callsign, ChatMessage, Contact, ControlKind, Conversation,
    ConversationKind, CotType, DetailNode, DeviceInfo, Extensions, GeoPoint, Geometry, Heading,
    How, Link, ObjectMetadata, ObjectSource, ObjectTimestamps, Precision, Role, Speed, TakEvent,
    TakObject, TakUid, Team, Timestamp, Validity,
};

use crate::detail::{
    ChatDetail, ChatGroup, ColorDetail, ContactDetail, DetailExt, EllipseShapeDetail,
    FillColorDetail, GroupDetail, KnownDetail, LinkDetail, MartiDest, MartiDetail, RemarksDetail,
    StatusDetail, StrokeColorDetail, StrokeWeightDetail, TakvDetail, TrackDetail, UidDetail,
    UserIconDetail,
};
use crate::error::CotError;
use crate::event::{CotEvent, CotPoint, UNKNOWN_SENTINEL};

/// How a CoT event will be interpreted by [`to_tak_event`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Classification {
    /// Position/presence report of a TAK user (`a-*` with `<contact>` + `<__group>`/`<takv>`).
    Contact,
    /// A map object (anything else with a position).
    Object,
    /// GeoChat (`b-t-f*`).
    Chat,
    /// Deletion request (`t-x-d-d`).
    Delete,
    /// Other tasking / control (`t-*`).
    Control,
}

/// Decide how an event maps onto the domain model.
pub fn classify(event: &CotEvent) -> Classification {
    let t = &event.cot_type;
    if t.is_within(CotType::DELETE) {
        return Classification::Delete;
    }
    if t.root() == tak_core::CotRoot::Tasking {
        return Classification::Control;
    }
    if t.is_within(CotType::GEOCHAT) {
        return Classification::Chat;
    }
    if t.is_atom()
        && event.detail.contains(ContactDetail::NAME)
        && (event.detail.contains(GroupDetail::NAME)
            || event.detail.contains(TakvDetail::NAME)
            || event
                .detail
                .get(ContactDetail::NAME)
                .is_some_and(|c| c.attr("endpoint").is_some()))
    {
        return Classification::Contact;
    }
    Classification::Object
}

/// Convert a parsed CoT event into a domain event.
///
/// `source` records where the event came from; `received_at` is stamped on
/// the resulting object's timestamps.
pub fn to_tak_event(
    event: &CotEvent,
    source: ObjectSource,
    received_at: Timestamp,
) -> Result<TakEvent, CotError> {
    match classify(event) {
        Classification::Contact => {
            to_contact(event, source, received_at).map(TakEvent::ContactUpdated)
        }
        Classification::Object => {
            to_object(event, source, received_at).map(TakEvent::ObjectUpdated)
        }
        Classification::Chat => to_chat(event).map(TakEvent::ChatReceived),
        Classification::Delete => {
            let target = event
                .detail
                .typed_all::<LinkDetail>()?
                .into_iter()
                .find_map(|l| l.uid)
                .map(TakUid::new)
                .transpose()?
                .unwrap_or_else(|| event.uid.clone());
            Ok(TakEvent::ObjectRemoved {
                uid: target,
                at: event.time,
                source,
            })
        }
        Classification::Control => Ok(TakEvent::Control {
            kind: ControlKind::from_cot_type(&event.cot_type),
            cot_type: event.cot_type.clone(),
            uid: event.uid.clone(),
            source,
        }),
    }
}

/// Convert a domain event into a CoT event ready for serialisation.
///
/// `now` is used for control/delete events, which carry no validity of
/// their own.
pub fn from_tak_event(event: &TakEvent, now: Timestamp) -> Result<CotEvent, CotError> {
    match event {
        TakEvent::ContactUpdated(c) => from_contact(c),
        TakEvent::ObjectUpdated(o) => from_object(o),
        TakEvent::ChatReceived(m) => from_chat(m),
        TakEvent::ObjectRemoved { uid, at, .. } => Ok(delete_event(uid, *at)),
        TakEvent::Control { cot_type, uid, .. } => {
            Ok(control_event(uid.clone(), cot_type.clone(), now))
        }
        _ => Err(CotError::NotRepresentable {
            target: "CotEvent",
            reason: "unsupported TakEvent variant".into(),
        }),
    }
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

fn position(point: &CotPoint) -> Result<GeoPoint, CotError> {
    Ok(GeoPoint::from_degrees(point.lat, point.lon)?)
}

fn altitude(point: &CotPoint) -> Result<Option<Altitude>, CotError> {
    Ok(point.hae_metres().map(Altitude::hae).transpose()?)
}

fn precision(point: &CotPoint) -> Result<Precision, CotError> {
    Ok(Precision::new(point.ce_metres(), point.le_metres())?)
}

fn how_or_default(event: &CotEvent) -> How {
    event.how.clone().unwrap_or_else(How::human_gigo)
}

fn timestamps(event: &CotEvent, received_at: Timestamp) -> Result<ObjectTimestamps, CotError> {
    Ok(ObjectTimestamps::from_validity(event.validity()?).received_at(received_at))
}

fn point_from(position: GeoPoint, altitude: Option<Altitude>, precision: Precision) -> CotPoint {
    CotPoint {
        lat: position.lat.degrees(),
        lon: position.lon.degrees(),
        hae: altitude.map_or(UNKNOWN_SENTINEL, Altitude::metres),
        ce: precision.ce_m.unwrap_or(UNKNOWN_SENTINEL),
        le: precision.le_m.unwrap_or(UNKNOWN_SENTINEL),
    }
}

fn remaining(detail: &Extensions, consumed: &[&str]) -> Extensions {
    Extensions::from_nodes(
        detail
            .iter()
            .filter(|n| !consumed.contains(&n.name.as_str()))
            .cloned()
            .collect(),
    )
}

fn append_extensions(detail: &mut Extensions, extensions: &Extensions) {
    for node in extensions.iter() {
        if !detail.contains(&node.name) {
            detail.push(node.clone());
        }
    }
}

fn base_event(
    uid: TakUid,
    cot_type: CotType,
    how: How,
    validity: &Validity,
    point: CotPoint,
) -> CotEvent {
    let mut ev = CotEvent::new(uid, cot_type, point, validity.time, validity.stale).with_how(how);
    ev.start = validity.start;
    ev
}

fn speed_from(track: Option<&TrackDetail>) -> Result<Option<Speed>, CotError> {
    // Some clients emit negative speed for "unknown"; treat that as absent.
    match track.and_then(|t| t.speed).filter(|s| *s >= 0.0) {
        Some(s) => Ok(Some(Speed::mps(s)?)),
        None => Ok(None),
    }
}

fn heading_from(track: Option<&TrackDetail>) -> Result<Option<Heading>, CotError> {
    match track.and_then(|t| t.course) {
        Some(c) => Ok(Some(Heading::degrees(c)?)),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Contact
// ---------------------------------------------------------------------------

const CONTACT_CONSUMED: &[&str] = &[
    ContactDetail::NAME,
    GroupDetail::NAME,
    TakvDetail::NAME,
    StatusDetail::NAME,
    TrackDetail::NAME,
    UidDetail::NAME,
];

/// Interpret an event as a [`Contact`] (presence report).
pub fn to_contact(
    event: &CotEvent,
    source: ObjectSource,
    received_at: Timestamp,
) -> Result<Contact, CotError> {
    let contact: ContactDetail =
        event
            .detail
            .typed()?
            .ok_or_else(|| CotError::NotRepresentable {
                target: "Contact",
                reason: "missing <contact> detail".into(),
            })?;
    let group: Option<GroupDetail> = event.detail.typed()?;
    let takv: Option<TakvDetail> = event.detail.typed()?;
    let status: Option<StatusDetail> = event.detail.typed()?;
    let track: Option<TrackDetail> = event.detail.typed()?;

    let device = takv.map(|t| DeviceInfo {
        device: t.device,
        platform: t.platform,
        os: t.os,
        version: t.version,
    });

    Ok(Contact {
        uid: event.uid.clone(),
        callsign: Callsign::new(&contact.callsign)?,
        cot_type: event.cot_type.clone(),
        affiliation: event.cot_type.affiliation().unwrap_or(Affiliation::Unknown),
        team: group.as_ref().map(|g| Team::parse(&g.name)).transpose()?,
        role: group.as_ref().map(|g| Role::parse(&g.role)).transpose()?,
        device: device.filter(|d| !d.is_empty()),
        position: position(&event.point)?,
        altitude: altitude(&event.point)?,
        precision: precision(&event.point)?,
        speed: speed_from(track.as_ref())?,
        heading: heading_from(track.as_ref())?,
        how: how_or_default(event),
        battery_percent: status.and_then(|s| s.battery),
        endpoint: contact.endpoint,
        phone: contact.phone,
        source,
        timestamps: timestamps(event, received_at)?,
        extensions: remaining(&event.detail, CONTACT_CONSUMED),
    })
}

/// Encode a [`Contact`] as a presence report.
pub fn from_contact(contact: &Contact) -> Result<CotEvent, CotError> {
    let mut detail = Extensions::new();
    if let Some(d) = &contact.device {
        detail.set_typed(&TakvDetail {
            device: d.device.clone(),
            platform: d.platform.clone(),
            os: d.os.clone(),
            version: d.version.clone(),
            extra: Vec::new(),
        });
    }
    detail.set_typed(&ContactDetail {
        callsign: contact.callsign.as_str().to_owned(),
        endpoint: contact.endpoint.clone(),
        phone: contact.phone.clone(),
        email: None,
        extra: Vec::new(),
    });
    detail.set_typed(&UidDetail {
        droid: Some(contact.callsign.as_str().to_owned()),
        extra: Vec::new(),
    });
    if contact.team.is_some() || contact.role.is_some() {
        detail.set_typed(&GroupDetail {
            name: contact
                .team
                .as_ref()
                .map_or("Cyan", Team::as_str)
                .to_owned(),
            role: contact
                .role
                .as_ref()
                .map_or("Team Member", Role::as_str)
                .to_owned(),
            extra: Vec::new(),
        });
    }
    if let Some(b) = contact.battery_percent {
        detail.set_typed(&StatusDetail {
            battery: Some(b),
            readiness: None,
            extra: Vec::new(),
        });
    }
    if contact.speed.is_some() || contact.heading.is_some() {
        detail.set_typed(&TrackDetail {
            course: contact.heading.map(Heading::as_degrees),
            speed: contact.speed.map(Speed::as_mps),
            extra: Vec::new(),
        });
    }
    append_extensions(&mut detail, &contact.extensions);

    Ok(base_event(
        contact.uid.clone(),
        contact.cot_type.clone(),
        contact.how.clone(),
        &contact.timestamps.validity,
        point_from(contact.position, contact.altitude, contact.precision),
    )
    .with_detail(detail))
}

// ---------------------------------------------------------------------------
// TakObject
// ---------------------------------------------------------------------------

const OBJECT_CONSUMED: &[&str] = &[
    ContactDetail::NAME,
    RemarksDetail::NAME,
    ColorDetail::NAME,
    StrokeColorDetail::NAME,
    FillColorDetail::NAME,
    StrokeWeightDetail::NAME,
    UserIconDetail::NAME,
    LinkDetail::NAME,
    EllipseShapeDetail::NAME,
    "archive",
];

/// Interpret an event as a [`TakObject`] (marker, shape, route, ...).
pub fn to_object(
    event: &CotEvent,
    source: ObjectSource,
    received_at: Timestamp,
) -> Result<TakObject, CotError> {
    let links = event.detail.typed_all::<LinkDetail>()?;
    let anchor = position(&event.point)?;
    let (geometry, shape_consumed) = geometry_from(event, &links, anchor)?;

    let mut consumed: Vec<&str> = OBJECT_CONSUMED.to_vec();
    if !shape_consumed {
        consumed.retain(|n| *n != EllipseShapeDetail::NAME);
    }

    let contact: Option<ContactDetail> = event.detail.typed()?;
    let remarks: Option<RemarksDetail> = event.detail.typed()?;
    let color: Option<ColorDetail> = event.detail.typed()?;
    let stroke: Option<StrokeColorDetail> = event.detail.typed()?;
    let fill: Option<FillColorDetail> = event.detail.typed()?;
    let weight: Option<StrokeWeightDetail> = event.detail.typed()?;
    let icon: Option<UserIconDetail> = event.detail.typed()?;

    let relation_links = links
        .iter()
        .filter(|l| l.point.is_none())
        .filter_map(|l| {
            let uid = TakUid::new(l.uid.as_deref()?).ok()?;
            Some(Link {
                uid,
                relation: l.relation.clone().unwrap_or_default(),
                cot_type: l.link_type.as_deref().and_then(|t| CotType::new(t).ok()),
                callsign: l
                    .parent_callsign
                    .as_deref()
                    .and_then(|c| Callsign::new(c).ok()),
            })
        })
        .collect();

    let metadata = ObjectMetadata {
        callsign: contact.and_then(|c| Callsign::new(&c.callsign).ok()),
        remarks: remarks.map(|r| r.text).filter(|t| !t.is_empty()),
        color: color.map(|c| c.0).or(stroke.map(|s| s.0)),
        fill_color: fill.map(|f| f.0),
        stroke_weight: weight.map(|w| w.0),
        icon: icon.map(|i| i.iconsetpath),
        links: relation_links,
        archive: event.detail.contains("archive"),
        extensions: remaining(&event.detail, &consumed),
    };

    Ok(TakObject {
        uid: event.uid.clone(),
        cot_type: event.cot_type.clone(),
        affiliation: event.cot_type.affiliation().unwrap_or(Affiliation::Unknown),
        geometry,
        altitude: altitude(&event.point)?,
        precision: precision(&event.point)?,
        how: how_or_default(event),
        metadata,
        source,
        timestamps: timestamps(event, received_at)?,
    })
}

/// Returns the geometry and whether `<shape>` was consumed.
fn geometry_from(
    event: &CotEvent,
    links: &[LinkDetail],
    anchor: GeoPoint,
) -> Result<(Geometry, bool), CotError> {
    let t = &event.cot_type;
    let mut vertices = Vec::new();
    for l in links {
        if let Some((lat, lon, _)) = l.point_coords()? {
            vertices.push(GeoPoint::from_degrees(lat, lon)?);
        }
    }
    if vertices.len() > 3 && vertices.first() == vertices.last() {
        vertices.pop();
    }

    if t.is_within(CotType::SHAPE_CIRCLE) {
        if let Some(e) = event.detail.typed::<EllipseShapeDetail>()? {
            let g = if (e.major - e.minor).abs() < f64::EPSILON {
                Geometry::circle(anchor, e.major)?
            } else {
                Geometry::ellipse(anchor, e.major, e.minor, e.angle)?
            };
            return Ok((g, true));
        }
    }
    if t.is_within(CotType::SHAPE_RECTANGLE) && vertices.len() == 4 {
        let mut it = vertices.into_iter();
        let corners = [
            it.next().unwrap_or(anchor),
            it.next().unwrap_or(anchor),
            it.next().unwrap_or(anchor),
            it.next().unwrap_or(anchor),
        ];
        return Ok((Geometry::Rectangle { corners }, false));
    }
    if (t.is_within(CotType::SHAPE_POLYGON) || t.is_within(CotType::SHAPE_RECTANGLE))
        && vertices.len() >= 3
    {
        return Ok((Geometry::polygon(vertices)?, false));
    }
    if t.is_within(CotType::ROUTE) && vertices.len() >= 2 {
        return Ok((Geometry::polyline(vertices)?, false));
    }
    Ok((Geometry::Point(anchor), false))
}

/// Encode a [`TakObject`].
pub fn from_object(object: &TakObject) -> Result<CotEvent, CotError> {
    let mut detail = Extensions::new();
    let m = &object.metadata;

    if let Some(cs) = &m.callsign {
        detail.set_typed(&ContactDetail {
            callsign: cs.as_str().to_owned(),
            ..ContactDetail::default()
        });
    }
    for link in &m.links {
        detail.push(
            LinkDetail {
                uid: Some(link.uid.as_str().to_owned()),
                link_type: link.cot_type.as_ref().map(|t| t.as_str().to_owned()),
                relation: Some(link.relation.clone()),
                parent_callsign: link.callsign.as_ref().map(|c| c.as_str().to_owned()),
                ..LinkDetail::default()
            }
            .to_node(),
        );
    }
    if let Some(r) = &m.remarks {
        detail.set_typed(&RemarksDetail {
            text: r.clone(),
            ..RemarksDetail::default()
        });
    }
    if let Some(c) = m.color {
        detail.set_typed(&ColorDetail(c));
    }
    if let Some(i) = &m.icon {
        detail.set_typed(&UserIconDetail {
            iconsetpath: i.clone(),
            extra: Vec::new(),
        });
    }
    if m.archive {
        detail.push(DetailNode::new("archive"));
    }

    let is_shape = !matches!(object.geometry, Geometry::Point(_));
    if is_shape {
        if let Some(c) = m.color {
            detail.set_typed(&StrokeColorDetail(c));
        }
        if let Some(f) = m.fill_color {
            detail.set_typed(&FillColorDetail(f));
        }
        if let Some(w) = m.stroke_weight {
            detail.set_typed(&StrokeWeightDetail(w));
        }
    }
    match &object.geometry {
        Geometry::Point(_) => {}
        Geometry::Circle { radius_m, .. } => detail.set_typed(&EllipseShapeDetail {
            major: *radius_m,
            minor: *radius_m,
            angle: 360.0,
            other_children: Vec::new(),
        }),
        Geometry::Ellipse {
            major_m,
            minor_m,
            angle_deg,
            ..
        } => detail.set_typed(&EllipseShapeDetail {
            major: *major_m,
            minor: *minor_m,
            angle: *angle_deg,
            other_children: Vec::new(),
        }),
        Geometry::Polyline(v) => push_vertices(&mut detail, v, false),
        Geometry::Polygon(v) => push_vertices(&mut detail, v, true),
        Geometry::Rectangle { corners } => push_vertices(&mut detail, corners, true),
    }
    append_extensions(&mut detail, &m.extensions);

    Ok(base_event(
        object.uid.clone(),
        object.cot_type.clone(),
        object.how.clone(),
        &object.timestamps.validity,
        point_from(object.geometry.anchor(), object.altitude, object.precision),
    )
    .with_detail(detail))
}

fn push_vertices(detail: &mut Extensions, vertices: &[GeoPoint], close: bool) {
    for v in vertices {
        detail.push(LinkDetail::vertex(v.lat.degrees(), v.lon.degrees(), None).to_node());
    }
    if close {
        if let Some(first) = vertices.first() {
            detail
                .push(LinkDetail::vertex(first.lat.degrees(), first.lon.degrees(), None).to_node());
        }
    }
}

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

/// Interpret a `b-t-f` event as a [`ChatMessage`].
pub fn to_chat(event: &CotEvent) -> Result<ChatMessage, CotError> {
    let chat: ChatDetail = event
        .detail
        .typed()?
        .ok_or_else(|| CotError::NotRepresentable {
            target: "ChatMessage",
            reason: "missing <__chat> detail".into(),
        })?;
    let remarks: Option<RemarksDetail> = event.detail.typed()?;
    let links = event.detail.typed_all::<LinkDetail>()?;
    let marti: Option<MartiDetail> = event.detail.typed()?;

    let sender_uid = chat
        .groups
        .iter()
        .find_map(|g| g.uids.first().cloned())
        .or_else(|| links.iter().find_map(|l| l.uid.clone()))
        .ok_or_else(|| CotError::NotRepresentable {
            target: "ChatMessage",
            reason: "cannot determine sender UID (no <chatgrp uid0> or <link uid>)".into(),
        })?;

    let kind = if chat.id == Conversation::ALL_CHAT_ROOMS
        || chat.chatroom == Conversation::ALL_CHAT_ROOMS
    {
        ConversationKind::Broadcast
    } else if chat
        .groups
        .iter()
        .any(|g| g.uids.len() == 2 && g.uids[1] == chat.id)
    {
        ConversationKind::Direct
    } else {
        ConversationKind::Group
    };

    let mut recipients: Vec<TakUid> = Vec::new();
    for g in &chat.groups {
        for uid in g.uids.iter().skip(1) {
            if uid != Conversation::ALL_CHAT_ROOMS {
                if let Ok(u) = TakUid::new(uid.as_str()) {
                    if !recipients.contains(&u) {
                        recipients.push(u);
                    }
                }
            }
        }
    }
    if let Some(m) = &marti {
        for d in &m.dests {
            if let Some(u) = d.uid.as_deref().and_then(|u| TakUid::new(u).ok()) {
                if !recipients.contains(&u) {
                    recipients.push(u);
                }
            }
        }
    }

    let sent_at = remarks
        .as_ref()
        .and_then(|r| r.time.as_deref())
        .and_then(|t| Timestamp::parse_rfc3339(t).ok())
        .unwrap_or(event.time);

    Ok(ChatMessage {
        message_id: match chat.message_id.as_deref() {
            Some(id) => TakUid::new(id)?,
            None => event.uid.clone(),
        },
        sender: TakUid::new(sender_uid)?,
        sender_callsign: Callsign::new(&chat.sender_callsign)?,
        conversation: Conversation {
            id: chat.id,
            name: chat.chatroom,
            kind,
        },
        recipients,
        text: remarks.map(|r| r.text).unwrap_or_default(),
        sent_at,
    })
}

/// Encode a [`ChatMessage`] as a GeoChat event (ATAK conventions).
pub fn from_chat(message: &ChatMessage) -> Result<CotEvent, CotError> {
    let conv = &message.conversation;
    let uid = TakUid::new(format!(
        "GeoChat.{}.{}.{}",
        message.sender.as_str(),
        conv.id,
        message.message_id.as_str()
    ))?;
    let stale = message
        .sent_at
        .checked_add(tak_core::Duration::days(1))
        .unwrap_or(message.sent_at);

    let mut detail = Extensions::new();
    detail.set_typed(&ChatDetail {
        parent: Some(match conv.kind {
            ConversationKind::Broadcast | ConversationKind::Direct => "RootContactGroup".into(),
            ConversationKind::Group => "UserGroups".into(),
        }),
        group_owner: Some(false),
        message_id: Some(message.message_id.as_str().to_owned()),
        chatroom: conv.name.clone(),
        id: conv.id.clone(),
        sender_callsign: message.sender_callsign.as_str().to_owned(),
        groups: vec![ChatGroup {
            id: Some(conv.id.clone()),
            uids: {
                let mut u = vec![message.sender.as_str().to_owned()];
                match conv.kind {
                    ConversationKind::Broadcast => u.push(Conversation::ALL_CHAT_ROOMS.to_owned()),
                    ConversationKind::Direct => u.push(conv.id.clone()),
                    ConversationKind::Group => {
                        u.extend(message.recipients.iter().map(|r| r.as_str().to_owned()));
                    }
                }
                u
            },
        }],
        extra: Vec::new(),
        other_children: Vec::new(),
    });
    detail.push(
        LinkDetail {
            uid: Some(message.sender.as_str().to_owned()),
            link_type: Some(CotType::FRIENDLY_GROUND_UNIT.to_owned()),
            relation: Some("p-p".to_owned()),
            ..LinkDetail::default()
        }
        .to_node(),
    );
    detail.set_typed(&RemarksDetail {
        text: message.text.clone(),
        source: Some(format!("BAO.F.ATAK.{}", message.sender.as_str())),
        to: Some(conv.id.clone()),
        time: Some(message.sent_at.to_cot_string()),
        extra: Vec::new(),
    });
    if conv.kind == ConversationKind::Direct {
        detail.set_typed(&MartiDetail {
            dests: vec![MartiDest {
                callsign: Some(conv.name.clone()),
                uid: None,
                mission: None,
            }],
        });
    }

    let mut ev = CotEvent::new(
        uid,
        CotType::new(CotType::GEOCHAT)?,
        CotPoint::NULL_ISLAND,
        message.sent_at,
        stale,
    )
    .with_how(How::human_gigo())
    .with_detail(detail);
    ev.start = message.sent_at;
    Ok(ev)
}

// ---------------------------------------------------------------------------
// Delete / control
// ---------------------------------------------------------------------------

/// Build a `t-x-d-d` deletion request for `target`.
pub fn delete_event(target: &TakUid, at: Timestamp) -> CotEvent {
    let uid = TakUid::new(uuid::Uuid::new_v4().to_string()).unwrap_or_else(|_| target.clone());
    let detail = Extensions::from_nodes(vec![
        LinkDetail {
            uid: Some(target.as_str().to_owned()),
            link_type: Some("none".to_owned()),
            relation: Some("none".to_owned()),
            ..LinkDetail::default()
        }
        .to_node(),
        DetailNode::new("__forcedelete"),
    ]);
    let stale = at.checked_add(tak_core::Duration::minutes(1)).unwrap_or(at);
    CotEvent::new(
        uid,
        CotType::new(CotType::DELETE).unwrap_or_else(|_| unreachable_type()),
        CotPoint::NULL_ISLAND,
        at,
        stale,
    )
    .with_how(How::human_gigo())
    .with_detail(detail)
}

/// Build a bare control event (ping, protocol negotiation, ...).
pub fn control_event(uid: TakUid, cot_type: CotType, now: Timestamp) -> CotEvent {
    let stale = now
        .checked_add(tak_core::Duration::minutes(1))
        .unwrap_or(now);
    CotEvent::new(uid, cot_type, CotPoint::NULL_ISLAND, now, stale).with_how(How::human_gigo())
}

/// Build a `t-x-c-t` ping with the given UID.
pub fn ping_event(uid: TakUid, now: Timestamp) -> CotEvent {
    control_event(
        uid,
        CotType::new(CotType::PING).unwrap_or_else(|_| unreachable_type()),
        now,
    )
}

fn unreachable_type() -> CotType {
    // The constants in `CotType` are valid by construction; this fallback
    // only exists to avoid `expect` in library code.
    CotType::new("t-x").unwrap_or_else(|_| unreachable_type())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use crate::write::to_xml;

    const ATAK_SA: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><event version="2.0" uid="ANDROID-359975090666199" type="a-f-G-U-C" time="2024-05-01T12:00:00.000Z" start="2024-05-01T12:00:00.000Z" stale="2024-05-01T12:06:15.000Z" how="m-g"><point lat="38.8977" lon="-77.0365" hae="22.7" ce="4.9" le="9999999.0"/><detail><takv os="34" version="5.1.0.5 (abc).1700000000-CIV" device="GOOGLE PIXEL 7" platform="ATAK-CIV"/><status battery="87"/><uid Droid="ALPHA"/><contact callsign="ALPHA" endpoint="*:-1:stcp" phone="+15555550100"/><__group role="Team Member" name="Cyan"/><track course="123.4" speed="1.2"/><precisionlocation altsrc="GPS" geopointsrc="GPS"/><__vendor_ext foo="bar"><nested>x</nested></__vendor_ext></detail></event>"#;

    fn src() -> ObjectSource {
        ObjectSource::remote(
            tak_core::TransportId::new("test"),
            tak_core::WireEncoding::CotXml,
        )
    }

    #[test]
    fn atak_sa_becomes_contact_and_round_trips_extensions() {
        let ev = parse(ATAK_SA).unwrap();
        assert_eq!(classify(&ev), Classification::Contact);
        let now = Timestamp::now();
        let TakEvent::ContactUpdated(c) = to_tak_event(&ev, src(), now).unwrap() else {
            panic!("expected contact");
        };
        assert_eq!(c.callsign.as_str(), "ALPHA");
        assert_eq!(c.team, Some(Team::Cyan));
        assert_eq!(c.role, Some(Role::TeamMember));
        assert_eq!(c.battery_percent, Some(87));
        assert_eq!(c.endpoint.as_deref(), Some("*:-1:stcp"));
        assert_eq!(
            c.device.as_ref().unwrap().platform.as_deref(),
            Some("ATAK-CIV")
        );
        assert_eq!(c.speed.unwrap().as_mps(), 1.2);
        assert_eq!(c.heading.unwrap().as_degrees(), 123.4);
        assert_eq!(c.altitude.unwrap().metres(), 22.7);
        assert_eq!(c.precision.ce_m, Some(4.9));
        assert_eq!(c.precision.le_m, None);
        assert_eq!(c.timestamps.received, Some(now));
        // unknown + unmodelled details survive
        assert!(c.extensions.contains("__vendor_ext"));
        assert!(c.extensions.contains("precisionlocation"));
        assert!(!c.extensions.contains("contact"));

        let back = from_contact(&c).unwrap();
        assert_eq!(back.uid, ev.uid);
        assert_eq!(back.point, ev.point);
        assert!(back.detail.contains("__vendor_ext"));
        assert_eq!(
            back.detail.get("contact").unwrap().attr("callsign"),
            Some("ALPHA")
        );
        assert_eq!(
            back.detail.get("__group").unwrap().attr("name"),
            Some("Cyan")
        );
        assert_eq!(back.detail.get("track").unwrap().attr("speed"), Some("1.2"));
        // and the re-encoded XML parses to the same contact again
        let xml = to_xml(&back).unwrap();
        let TakEvent::ContactUpdated(c2) = to_tak_event(&parse(&xml).unwrap(), src(), now).unwrap()
        else {
            panic!("expected contact");
        };
        assert_eq!(c2, c);
    }

    #[test]
    fn marker_becomes_object() {
        let xml = ATAK_SA
            .replace("a-f-G-U-C", "a-h-G")
            .replace(r#"<contact callsign="ALPHA" endpoint="*:-1:stcp" phone="+15555550100"/><__group role="Team Member" name="Cyan"/>"#, r#"<contact callsign="Enemy Tank"/><remarks>seen at 1200</remarks><color argb="-65536"/><archive/><link uid="ANDROID-1" type="a-f-G-U-C" relation="p-p" parent_callsign="ALPHA"/>"#)
            .replace(r#"<takv os="34" version="5.1.0.5 (abc).1700000000-CIV" device="GOOGLE PIXEL 7" platform="ATAK-CIV"/>"#, "");
        let ev = parse(&xml).unwrap();
        assert_eq!(classify(&ev), Classification::Object);
        let TakEvent::ObjectUpdated(o) = to_tak_event(&ev, src(), Timestamp::now()).unwrap() else {
            panic!("expected object");
        };
        assert_eq!(o.affiliation, Affiliation::Hostile);
        assert_eq!(o.metadata.callsign.as_ref().unwrap().as_str(), "Enemy Tank");
        assert_eq!(o.metadata.remarks.as_deref(), Some("seen at 1200"));
        assert_eq!(
            o.metadata.color,
            Some(tak_core::Argb::new(0xFF, 0xFF, 0, 0))
        );
        assert!(o.metadata.archive);
        assert_eq!(o.metadata.links.len(), 1);
        assert_eq!(o.metadata.links[0].relation, "p-p");
        assert!(matches!(o.geometry, Geometry::Point(_)));
        let back = from_object(&o).unwrap();
        assert!(back.detail.contains("archive"));
        assert_eq!(
            back.detail.get("color").unwrap().attr("argb"),
            Some("-65536")
        );
        assert!(back.detail.contains("__vendor_ext"));
    }

    #[test]
    fn polygon_and_circle_geometry() {
        let base = ATAK_SA.replace("a-f-G-U-C", "u-d-f");
        let poly = base.replace("<detail>", r#"<detail><link point="38.9,-77.1,0.0"/><link point="38.9,-77.0,0.0"/><link point="38.8,-77.0,0.0"/><link point="38.9,-77.1,0.0"/><strokeColor value="-1"/><fillColor value="16711680"/><strokeWeight value="3.0"/>"#);
        let ev = parse(&poly).unwrap();
        let TakEvent::ObjectUpdated(o) = to_tak_event(&ev, src(), Timestamp::now()).unwrap() else {
            panic!("expected object");
        };
        let Geometry::Polygon(v) = &o.geometry else {
            panic!("expected polygon, got {:?}", o.geometry);
        };
        assert_eq!(v.len(), 3);
        assert_eq!(o.metadata.stroke_weight, Some(3.0));
        assert_eq!(o.metadata.color, Some(tak_core::Argb::WHITE));
        let back = from_object(&o).unwrap();
        assert_eq!(back.detail.typed_all::<LinkDetail>().unwrap().len(), 4);

        let circle = ATAK_SA.replace("a-f-G-U-C", "u-d-c-c").replace(
            "<detail>",
            r#"<detail><shape><ellipse major="250.0" minor="250.0" angle="360"/></shape>"#,
        );
        let ev = parse(&circle).unwrap();
        let TakEvent::ObjectUpdated(o) = to_tak_event(&ev, src(), Timestamp::now()).unwrap() else {
            panic!("expected object");
        };
        assert!(matches!(o.geometry, Geometry::Circle { radius_m, .. } if radius_m == 250.0));
        assert!(!o.metadata.extensions.contains("shape"));
        let back = from_object(&o).unwrap();
        assert_eq!(
            back.detail
                .get("shape")
                .unwrap()
                .child("ellipse")
                .unwrap()
                .attr("major"),
            Some("250.0")
        );
    }

    #[test]
    fn geochat_round_trip() {
        let xml = r#"<event version="2.0" uid="GeoChat.ANDROID-1.All Chat Rooms.msg-1" type="b-t-f" how="h-g-i-g-o" time="2024-05-01T12:00:00.000Z" start="2024-05-01T12:00:00.000Z" stale="2024-05-02T12:00:00.000Z"><point lat="0.0" lon="0.0" hae="9999999.0" ce="9999999.0" le="9999999.0"/><detail><__chat parent="RootContactGroup" groupOwner="false" messageId="msg-1" chatroom="All Chat Rooms" id="All Chat Rooms" senderCallsign="ALPHA"><chatgrp uid0="ANDROID-1" uid1="All Chat Rooms" id="All Chat Rooms"/></__chat><link uid="ANDROID-1" type="a-f-G-U-C" relation="p-p"/><remarks source="BAO.F.ATAK.ANDROID-1" to="All Chat Rooms" time="2024-05-01T12:00:00.000Z">hello &amp; welcome</remarks></detail></event>"#;
        let ev = parse(xml).unwrap();
        assert_eq!(classify(&ev), Classification::Chat);
        let TakEvent::ChatReceived(m) = to_tak_event(&ev, src(), Timestamp::now()).unwrap() else {
            panic!("expected chat");
        };
        assert_eq!(m.text, "hello & welcome");
        assert_eq!(m.sender.as_str(), "ANDROID-1");
        assert_eq!(m.sender_callsign.as_str(), "ALPHA");
        assert_eq!(m.conversation.kind, ConversationKind::Broadcast);
        assert_eq!(m.message_id.as_str(), "msg-1");
        assert_eq!(m.recipients, Vec::new());

        let back = from_chat(&m).unwrap();
        assert_eq!(back.uid.as_str(), "GeoChat.ANDROID-1.All Chat Rooms.msg-1");
        let again = to_chat(&parse(&to_xml(&back).unwrap()).unwrap()).unwrap();
        assert_eq!(again, m);
    }

    #[test]
    fn direct_chat_is_classified_direct() {
        let xml = r#"<event version="2.0" uid="GeoChat.A.B.m" type="b-t-f" how="h-g-i-g-o" time="2024-05-01T12:00:00.000Z" start="2024-05-01T12:00:00.000Z" stale="2024-05-02T12:00:00.000Z"><point lat="0" lon="0" hae="0" ce="0" le="0"/><detail><__chat parent="RootContactGroup" groupOwner="false" messageId="m" chatroom="BRAVO" id="B" senderCallsign="ALPHA"><chatgrp uid0="A" uid1="B" id="B"/></__chat><remarks>hi</remarks><marti><dest callsign="BRAVO"/></marti></detail></event>"#;
        let m = to_chat(&parse(xml).unwrap()).unwrap();
        assert_eq!(m.conversation.kind, ConversationKind::Direct);
        assert_eq!(m.recipients.len(), 1);
        assert_eq!(m.recipients[0].as_str(), "B");
        let back = from_chat(&m).unwrap();
        assert!(back.detail.contains("marti"));
    }

    #[test]
    fn delete_and_control() {
        let target = TakUid::new("ANDROID-9").unwrap();
        let now = Timestamp::now();
        let del = delete_event(&target, now);
        let xml = to_xml(&del).unwrap();
        let TakEvent::ObjectRemoved { uid, .. } =
            to_tak_event(&parse(&xml).unwrap(), src(), now).unwrap()
        else {
            panic!("expected removal");
        };
        assert_eq!(uid, target);

        let ping = ping_event(TakUid::new("p").unwrap(), now);
        let TakEvent::Control { kind, .. } = to_tak_event(&ping, src(), now).unwrap() else {
            panic!("expected control");
        };
        assert_eq!(kind, ControlKind::Ping);
        let back = from_tak_event(
            &TakEvent::Control {
                kind,
                cot_type: ping.cot_type.clone(),
                uid: ping.uid.clone(),
                source: src(),
            },
            now,
        )
        .unwrap();
        assert_eq!(back.cot_type, ping.cot_type);
    }

    #[test]
    fn contact_without_callsign_is_not_representable() {
        let xml = ATAK_SA.replace(r#"callsign="ALPHA" "#, "");
        let ev = parse(&xml).unwrap();
        // <contact> without callsign fails the typed decode
        assert!(matches!(
            to_tak_event(&ev, src(), Timestamp::now()),
            Err(CotError::MissingAttribute { .. })
        ));
    }

    #[test]
    fn inverted_validity_is_rejected() {
        let xml = ATAK_SA.replace(
            r#"stale="2024-05-01T12:06:15.000Z""#,
            r#"stale="2024-05-01T11:00:00.000Z""#,
        );
        let ev = parse(&xml).unwrap();
        assert!(matches!(
            to_tak_event(&ev, src(), Timestamp::now()),
            Err(CotError::Validation(_))
        ));
    }
}
