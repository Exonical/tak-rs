//! Typed views over well-known `<detail>` children.
//!
//! Each type implements [`KnownDetail`], which maps a single
//! [`DetailNode`] to a struct and back. Attributes the struct does not model
//! are kept in an `extra` list so nothing is lost on round trip. Use
//! [`DetailExt::typed`] / [`DetailExt::set_typed`] to read and write them on
//! an [`Extensions`] container.

use tak_core::{Argb, DetailNode, Extensions};

use crate::error::CotError;

/// A `<detail>` child with a known schema.
pub trait KnownDetail: Sized {
    /// Element name.
    const NAME: &'static str;

    /// Interpret a node. `node.name` is assumed to equal [`Self::NAME`].
    fn from_node(node: &DetailNode) -> Result<Self, CotError>;

    /// Produce the node.
    fn to_node(&self) -> DetailNode;
}

/// Convenience accessors for typed details on an [`Extensions`] container.
pub trait DetailExt {
    /// Decode the first child named `T::NAME`, if present.
    fn typed<T: KnownDetail>(&self) -> Result<Option<T>, CotError>;

    /// Decode every child named `T::NAME`.
    fn typed_all<T: KnownDetail>(&self) -> Result<Vec<T>, CotError>;

    /// Replace (or append) the child named `T::NAME`.
    fn set_typed<T: KnownDetail>(&mut self, value: &T);
}

impl DetailExt for Extensions {
    fn typed<T: KnownDetail>(&self) -> Result<Option<T>, CotError> {
        self.get(T::NAME).map(T::from_node).transpose()
    }

    fn typed_all<T: KnownDetail>(&self) -> Result<Vec<T>, CotError> {
        self.iter()
            .filter(|n| n.name == T::NAME)
            .map(T::from_node)
            .collect()
    }

    fn set_typed<T: KnownDetail>(&mut self, value: &T) {
        self.replace(value.to_node());
    }
}

// ---------------------------------------------------------------------------
// attribute helpers
// ---------------------------------------------------------------------------

fn req_str<'a>(node: &'a DetailNode, key: &str) -> Result<&'a str, CotError> {
    node.attr(key)
        .ok_or_else(|| CotError::missing_attr(&node.name, key))
}

fn opt_string(node: &DetailNode, key: &str) -> Option<String> {
    node.attr(key).map(str::to_owned)
}

fn opt_f64(node: &DetailNode, key: &str) -> Result<Option<f64>, CotError> {
    match node.attr(key) {
        None => Ok(None),
        Some(raw) => parse_f64(raw)
            .map(Some)
            .ok_or_else(|| CotError::invalid_attr(&node.name, key, raw, "expected a number")),
    }
}

fn opt_bool(node: &DetailNode, key: &str) -> Result<Option<bool>, CotError> {
    match node.attr(key).map(str::trim) {
        None => Ok(None),
        Some("true" | "TRUE" | "True" | "1") => Ok(Some(true)),
        Some("false" | "FALSE" | "False" | "0") => Ok(Some(false)),
        Some(raw) => Err(CotError::invalid_attr(
            &node.name,
            key,
            raw,
            "expected true/false",
        )),
    }
}

fn opt_argb(node: &DetailNode, key: &str) -> Result<Option<Argb>, CotError> {
    match node.attr(key) {
        None => Ok(None),
        Some(raw) => parse_argb(raw)
            .map(Some)
            .ok_or_else(|| CotError::invalid_attr(&node.name, key, raw, "expected ARGB integer")),
    }
}

/// Parse a float the way TAK clients write them (`1.0`, `-1`, `1E-3`).
pub(crate) fn parse_f64(raw: &str) -> Option<f64> {
    let v: f64 = raw.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

/// Parse a TAK colour: signed 32-bit decimal (Java `int`), unsigned decimal,
/// or `#AARRGGBB` / `#RRGGBB` hex.
pub(crate) fn parse_argb(raw: &str) -> Option<Argb> {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix('#') {
        return match hex.len() {
            8 => u32::from_str_radix(hex, 16).ok().map(Argb::from_u32),
            6 => u32::from_str_radix(hex, 16)
                .ok()
                .map(|rgb| Argb::from_u32(0xFF00_0000 | rgb)),
            _ => None,
        };
    }
    if let Ok(v) = raw.parse::<i32>() {
        return Some(Argb::from_i32(v));
    }
    raw.parse::<u32>().ok().map(Argb::from_u32)
}

fn extra_attrs(node: &DetailNode, known: &[&str]) -> Vec<(String, String)> {
    node.attributes
        .iter()
        .filter(|(k, _)| !known.contains(&k.as_str()))
        .cloned()
        .collect()
}

fn push_opt(node: &mut DetailNode, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        node.set_attr(key, v);
    }
}

fn push_extra(node: &mut DetailNode, extra: &[(String, String)]) {
    for (k, v) in extra {
        if node.attr(k).is_none() {
            node.set_attr(k.clone(), v.clone());
        }
    }
}

/// Format a float with at least one decimal digit, matching TAK output.
pub(crate) fn fmt_f64(v: f64) -> String {
    let s = format!("{v}");
    if s.contains(['.', 'e', 'E', 'N', 'i']) {
        s
    } else {
        format!("{s}.0")
    }
}

// ---------------------------------------------------------------------------
// <contact>
// ---------------------------------------------------------------------------

/// `<contact callsign=".." endpoint=".." phone=".." emailAddress=".."/>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContactDetail {
    /// Display name.
    pub callsign: String,
    /// Direct-connect endpoint, e.g. `*:-1:stcp` or `192.168.1.5:4242:tcp`.
    pub endpoint: Option<String>,
    /// Phone number.
    pub phone: Option<String>,
    /// E-mail address.
    pub email: Option<String>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for ContactDetail {
    const NAME: &'static str = "contact";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            callsign: req_str(node, "callsign")?.to_owned(),
            endpoint: opt_string(node, "endpoint"),
            phone: opt_string(node, "phone"),
            email: opt_string(node, "emailAddress"),
            extra: extra_attrs(node, &["callsign", "endpoint", "phone", "emailAddress"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME).with_attr("callsign", &self.callsign);
        push_opt(&mut n, "endpoint", self.endpoint.as_deref());
        push_opt(&mut n, "phone", self.phone.as_deref());
        push_opt(&mut n, "emailAddress", self.email.as_deref());
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <__group>
// ---------------------------------------------------------------------------

/// `<__group name="Cyan" role="Team Member"/>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupDetail {
    /// Team colour name.
    pub name: String,
    /// Role within the team.
    pub role: String,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for GroupDetail {
    const NAME: &'static str = "__group";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            name: req_str(node, "name")?.to_owned(),
            role: req_str(node, "role")?.to_owned(),
            extra: extra_attrs(node, &["name", "role"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME)
            .with_attr("name", &self.name)
            .with_attr("role", &self.role);
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <takv>
// ---------------------------------------------------------------------------

/// `<takv device=".." platform=".." os=".." version=".."/>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TakvDetail {
    /// Hardware description.
    pub device: Option<String>,
    /// Client software name.
    pub platform: Option<String>,
    /// Operating system / API level.
    pub os: Option<String>,
    /// Client software version.
    pub version: Option<String>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for TakvDetail {
    const NAME: &'static str = "takv";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            device: opt_string(node, "device"),
            platform: opt_string(node, "platform"),
            os: opt_string(node, "os"),
            version: opt_string(node, "version"),
            extra: extra_attrs(node, &["device", "platform", "os", "version"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        push_opt(&mut n, "device", self.device.as_deref());
        push_opt(&mut n, "platform", self.platform.as_deref());
        push_opt(&mut n, "os", self.os.as_deref());
        push_opt(&mut n, "version", self.version.as_deref());
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <status>
// ---------------------------------------------------------------------------

/// `<status battery="88" readiness="true"/>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusDetail {
    /// Battery percentage.
    pub battery: Option<u8>,
    /// Readiness flag (used by some tasking workflows).
    pub readiness: Option<bool>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for StatusDetail {
    const NAME: &'static str = "status";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        let battery = match node.attr("battery") {
            None => None,
            Some(raw) => {
                let v = parse_f64(raw).ok_or_else(|| {
                    CotError::invalid_attr(&node.name, "battery", raw, "expected a percentage")
                })?;
                // Clients occasionally report >100 or negative; clamp instead of failing.
                Some(v.clamp(0.0, 100.0).round() as u8)
            }
        };
        Ok(Self {
            battery,
            readiness: opt_bool(node, "readiness")?,
            extra: extra_attrs(node, &["battery", "readiness"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        if let Some(b) = self.battery {
            n.set_attr("battery", b.to_string());
        }
        if let Some(r) = self.readiness {
            n.set_attr("readiness", r.to_string());
        }
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <track>
// ---------------------------------------------------------------------------

/// `<track course="123.4" speed="1.5"/>` (degrees true, metres/second).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackDetail {
    /// Course over ground in degrees.
    pub course: Option<f64>,
    /// Speed over ground in m/s.
    pub speed: Option<f64>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for TrackDetail {
    const NAME: &'static str = "track";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            course: opt_f64(node, "course")?,
            speed: opt_f64(node, "speed")?,
            extra: extra_attrs(node, &["course", "speed"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        if let Some(c) = self.course {
            n.set_attr("course", fmt_f64(c));
        }
        if let Some(s) = self.speed {
            n.set_attr("speed", fmt_f64(s));
        }
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <precisionlocation>
// ---------------------------------------------------------------------------

/// `<precisionlocation geopointsrc="GPS" altsrc="GPS"/>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrecisionLocationDetail {
    /// Horizontal fix source.
    pub geopointsrc: Option<String>,
    /// Altitude source.
    pub altsrc: Option<String>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for PrecisionLocationDetail {
    const NAME: &'static str = "precisionlocation";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            geopointsrc: opt_string(node, "geopointsrc"),
            altsrc: opt_string(node, "altsrc"),
            extra: extra_attrs(node, &["geopointsrc", "altsrc"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        push_opt(&mut n, "geopointsrc", self.geopointsrc.as_deref());
        push_opt(&mut n, "altsrc", self.altsrc.as_deref());
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <remarks>
// ---------------------------------------------------------------------------

/// `<remarks source=".." to=".." time="..">free text</remarks>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemarksDetail {
    /// The text.
    pub text: String,
    /// Chat: originating client identifier (`BAO.F.ATAK.<uid>`).
    pub source: Option<String>,
    /// Chat: destination room/contact.
    pub to: Option<String>,
    /// Chat: send time.
    pub time: Option<String>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for RemarksDetail {
    const NAME: &'static str = "remarks";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            text: node.text.clone().unwrap_or_default(),
            source: opt_string(node, "source"),
            to: opt_string(node, "to"),
            time: opt_string(node, "time"),
            extra: extra_attrs(node, &["source", "to", "time"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        push_opt(&mut n, "source", self.source.as_deref());
        push_opt(&mut n, "to", self.to.as_deref());
        push_opt(&mut n, "time", self.time.as_deref());
        push_extra(&mut n, &self.extra);
        if !self.text.is_empty() {
            n.text = Some(self.text.clone());
        }
        n
    }
}

// ---------------------------------------------------------------------------
// colours / stroke
// ---------------------------------------------------------------------------

/// `<color argb="-1"/>` (marker colour).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorDetail(pub Argb);

impl KnownDetail for ColorDetail {
    const NAME: &'static str = "color";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        // ATAK writes `argb`; some tools write `value`.
        let c = opt_argb(node, "argb")?
            .or(opt_argb(node, "value")?)
            .ok_or_else(|| CotError::missing_attr(&node.name, "argb"))?;
        Ok(Self(c))
    }

    fn to_node(&self) -> DetailNode {
        DetailNode::new(Self::NAME).with_attr("argb", self.0.to_i32().to_string())
    }
}

macro_rules! value_detail {
    ($(#[$m:meta])* $name:ident, $elem:literal, Argb) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name(pub Argb);

        impl KnownDetail for $name {
            const NAME: &'static str = $elem;

            fn from_node(node: &DetailNode) -> Result<Self, CotError> {
                let c = opt_argb(node, "value")?
                    .ok_or_else(|| CotError::missing_attr(&node.name, "value"))?;
                Ok(Self(c))
            }

            fn to_node(&self) -> DetailNode {
                DetailNode::new(Self::NAME).with_attr("value", self.0.to_i32().to_string())
            }
        }
    };
    ($(#[$m:meta])* $name:ident, $elem:literal, f64) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $name(pub f64);

        impl KnownDetail for $name {
            const NAME: &'static str = $elem;

            fn from_node(node: &DetailNode) -> Result<Self, CotError> {
                let v = opt_f64(node, "value")?
                    .ok_or_else(|| CotError::missing_attr(&node.name, "value"))?;
                Ok(Self(v))
            }

            fn to_node(&self) -> DetailNode {
                DetailNode::new(Self::NAME).with_attr("value", fmt_f64(self.0))
            }
        }
    };
}

value_detail!(
    /// `<strokeColor value="-1"/>` (shape outline colour).
    StrokeColorDetail,
    "strokeColor",
    Argb
);
value_detail!(
    /// `<fillColor value="-1"/>` (shape fill colour).
    FillColorDetail,
    "fillColor",
    Argb
);
value_detail!(
    /// `<strokeWeight value="4.0"/>` (shape outline width).
    StrokeWeightDetail,
    "strokeWeight",
    f64
);

// ---------------------------------------------------------------------------
// <usericon>
// ---------------------------------------------------------------------------

/// `<usericon iconsetpath="COT_MAPPING_2525C/a-f/a-f-G"/>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserIconDetail {
    /// Icon set path.
    pub iconsetpath: String,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for UserIconDetail {
    const NAME: &'static str = "usericon";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            iconsetpath: req_str(node, "iconsetpath")?.to_owned(),
            extra: extra_attrs(node, &["iconsetpath"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME).with_attr("iconsetpath", &self.iconsetpath);
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <uid Droid="..."/>
// ---------------------------------------------------------------------------

/// `<uid Droid="CALLSIGN"/>` (legacy ATAK self-marker detail).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UidDetail {
    /// The `Droid` attribute (usually the callsign).
    pub droid: Option<String>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl KnownDetail for UidDetail {
    const NAME: &'static str = "uid";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            droid: opt_string(node, "Droid"),
            extra: extra_attrs(node, &["Droid"]),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        push_opt(&mut n, "Droid", self.droid.as_deref());
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <link>
// ---------------------------------------------------------------------------

/// `<link uid=".." type=".." relation=".." parent_callsign=".." point="lat,lon,hae"/>`.
///
/// Links serve two purposes in TAK: relating an object to another (`uid` +
/// `relation`, e.g. `p-p` parent) and listing shape vertices (`point`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkDetail {
    /// Related object UID.
    pub uid: Option<String>,
    /// CoT type of the related object.
    pub link_type: Option<String>,
    /// Relation code (`p-p`, `c`, `none`, ...).
    pub relation: Option<String>,
    /// Callsign of the parent.
    pub parent_callsign: Option<String>,
    /// Production time.
    pub production_time: Option<String>,
    /// Vertex position as `lat,lon[,hae]`.
    pub point: Option<String>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
}

impl LinkDetail {
    /// Decode the `point` attribute into `(lat, lon, hae)`.
    pub fn point_coords(&self) -> Result<Option<(f64, f64, Option<f64>)>, CotError> {
        let Some(raw) = self.point.as_deref() else {
            return Ok(None);
        };
        let err = || CotError::invalid_attr("link", "point", raw, "expected `lat,lon[,hae]`");
        let mut parts = raw.split(',');
        let lat = parts.next().and_then(parse_f64).ok_or_else(err)?;
        let lon = parts.next().and_then(parse_f64).ok_or_else(err)?;
        let hae = match parts.next() {
            None => None,
            Some(h) if h.trim().is_empty() => None,
            Some(h) => Some(parse_f64(h).ok_or_else(err)?),
        };
        if parts.next().is_some() {
            return Err(err());
        }
        Ok(Some((lat, lon, hae)))
    }

    /// A vertex link.
    pub fn vertex(lat: f64, lon: f64, hae: Option<f64>) -> Self {
        let point = match hae {
            Some(h) => format!("{},{},{}", fmt_f64(lat), fmt_f64(lon), fmt_f64(h)),
            None => format!("{},{}", fmt_f64(lat), fmt_f64(lon)),
        };
        Self {
            point: Some(point),
            ..Self::default()
        }
    }
}

impl KnownDetail for LinkDetail {
    const NAME: &'static str = "link";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            uid: opt_string(node, "uid"),
            link_type: opt_string(node, "type"),
            relation: opt_string(node, "relation"),
            parent_callsign: opt_string(node, "parent_callsign"),
            production_time: opt_string(node, "production_time"),
            point: opt_string(node, "point"),
            extra: extra_attrs(
                node,
                &[
                    "uid",
                    "type",
                    "relation",
                    "parent_callsign",
                    "production_time",
                    "point",
                ],
            ),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        push_opt(&mut n, "uid", self.uid.as_deref());
        push_opt(&mut n, "type", self.link_type.as_deref());
        push_opt(&mut n, "relation", self.relation.as_deref());
        push_opt(&mut n, "parent_callsign", self.parent_callsign.as_deref());
        push_opt(&mut n, "production_time", self.production_time.as_deref());
        push_opt(&mut n, "point", self.point.as_deref());
        push_extra(&mut n, &self.extra);
        n
    }
}

// ---------------------------------------------------------------------------
// <shape><ellipse/></shape>
// ---------------------------------------------------------------------------

/// `<shape><ellipse major=".." minor=".." angle=".."/></shape>`.
///
/// ATAK writes the circle *radius* into both `major` and `minor`; this crate
/// follows that convention (values are semi-axes in metres).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EllipseShapeDetail {
    /// Semi-major axis, metres.
    pub major: f64,
    /// Semi-minor axis, metres.
    pub minor: f64,
    /// Orientation of the major axis, degrees true.
    pub angle: f64,
    /// Any other `<shape>` children (e.g. `<polyline>`), preserved verbatim.
    pub other_children: Vec<DetailNode>,
}

impl KnownDetail for EllipseShapeDetail {
    const NAME: &'static str = "shape";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        let ellipse = node
            .child("ellipse")
            .ok_or_else(|| CotError::NotRepresentable {
                target: "EllipseShapeDetail",
                reason: "<shape> has no <ellipse> child".into(),
            })?;
        Ok(Self {
            major: opt_f64(ellipse, "major")?
                .ok_or_else(|| CotError::missing_attr("ellipse", "major"))?,
            minor: opt_f64(ellipse, "minor")?
                .ok_or_else(|| CotError::missing_attr("ellipse", "minor"))?,
            angle: opt_f64(ellipse, "angle")?.unwrap_or(0.0),
            other_children: node
                .children
                .iter()
                .filter(|c| c.name != "ellipse")
                .cloned()
                .collect(),
        })
    }

    fn to_node(&self) -> DetailNode {
        let ellipse = DetailNode::new("ellipse")
            .with_attr("major", fmt_f64(self.major))
            .with_attr("minor", fmt_f64(self.minor))
            .with_attr("angle", fmt_f64(self.angle));
        let mut n = DetailNode::new(Self::NAME).with_child(ellipse);
        n.children.extend(self.other_children.iter().cloned());
        n
    }
}

// ---------------------------------------------------------------------------
// <__chat>
// ---------------------------------------------------------------------------

/// `<chatgrp uid0=".." uid1=".." id=".."/>`: participants of a chat.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatGroup {
    /// Group id (room id or destination contact UID).
    pub id: Option<String>,
    /// `uid0`, `uid1`, ... in index order. `uid0` is the sender.
    pub uids: Vec<String>,
}

/// `<__chat parent=".." groupOwner=".." messageId=".." chatroom=".." id=".." senderCallsign=".."/>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatDetail {
    /// Parent group (`RootContactGroup`, `TeamGroups`, ...).
    pub parent: Option<String>,
    /// Whether the sender owns the group.
    pub group_owner: Option<bool>,
    /// Message identifier.
    pub message_id: Option<String>,
    /// Human-readable room name.
    pub chatroom: String,
    /// Room identifier (contact UID for direct messages).
    pub id: String,
    /// Sender's callsign.
    pub sender_callsign: String,
    /// `<chatgrp>` children.
    pub groups: Vec<ChatGroup>,
    /// Unmodelled attributes.
    pub extra: Vec<(String, String)>,
    /// Unmodelled children (e.g. `<hierarchy>`), preserved verbatim.
    pub other_children: Vec<DetailNode>,
}

impl KnownDetail for ChatDetail {
    const NAME: &'static str = "__chat";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        let mut groups = Vec::new();
        let mut other_children = Vec::new();
        for child in &node.children {
            if child.name == "chatgrp" {
                let mut indexed: Vec<(usize, String)> = child
                    .attributes
                    .iter()
                    .filter_map(|(k, v)| {
                        k.strip_prefix("uid")
                            .and_then(|i| i.parse::<usize>().ok())
                            .map(|i| (i, v.clone()))
                    })
                    .collect();
                indexed.sort_by_key(|(i, _)| *i);
                groups.push(ChatGroup {
                    id: opt_string(child, "id"),
                    uids: indexed.into_iter().map(|(_, v)| v).collect(),
                });
            } else {
                other_children.push(child.clone());
            }
        }
        Ok(Self {
            parent: opt_string(node, "parent"),
            group_owner: opt_bool(node, "groupOwner")?,
            message_id: opt_string(node, "messageId"),
            chatroom: req_str(node, "chatroom")?.to_owned(),
            id: req_str(node, "id")?.to_owned(),
            sender_callsign: req_str(node, "senderCallsign")?.to_owned(),
            groups,
            extra: extra_attrs(
                node,
                &[
                    "parent",
                    "groupOwner",
                    "messageId",
                    "chatroom",
                    "id",
                    "senderCallsign",
                ],
            ),
            other_children,
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        push_opt(&mut n, "parent", self.parent.as_deref());
        if let Some(o) = self.group_owner {
            n.set_attr("groupOwner", o.to_string());
        }
        push_opt(&mut n, "messageId", self.message_id.as_deref());
        n.set_attr("chatroom", &self.chatroom);
        n.set_attr("id", &self.id);
        n.set_attr("senderCallsign", &self.sender_callsign);
        push_extra(&mut n, &self.extra);
        for g in &self.groups {
            let mut c = DetailNode::new("chatgrp");
            for (i, uid) in g.uids.iter().enumerate() {
                c.set_attr(format!("uid{i}"), uid);
            }
            push_opt(&mut c, "id", g.id.as_deref());
            n.children.push(c);
        }
        n.children.extend(self.other_children.iter().cloned());
        n
    }
}

// ---------------------------------------------------------------------------
// <marti>
// ---------------------------------------------------------------------------

/// `<marti><dest callsign=".."/><dest uid=".."/></marti>`: server-side routing hints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MartiDetail {
    /// Destinations.
    pub dests: Vec<MartiDest>,
}

/// One `<dest>` inside `<marti>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MartiDest {
    /// Destination callsign.
    pub callsign: Option<String>,
    /// Destination UID.
    pub uid: Option<String>,
    /// Destination mission name.
    pub mission: Option<String>,
}

impl KnownDetail for MartiDetail {
    const NAME: &'static str = "marti";

    fn from_node(node: &DetailNode) -> Result<Self, CotError> {
        Ok(Self {
            dests: node
                .children_named("dest")
                .map(|d| MartiDest {
                    callsign: opt_string(d, "callsign"),
                    uid: opt_string(d, "uid"),
                    mission: opt_string(d, "mission"),
                })
                .collect(),
        })
    }

    fn to_node(&self) -> DetailNode {
        let mut n = DetailNode::new(Self::NAME);
        for d in &self.dests {
            let mut c = DetailNode::new("dest");
            push_opt(&mut c, "callsign", d.callsign.as_deref());
            push_opt(&mut c, "uid", d.uid.as_deref());
            push_opt(&mut c, "mission", d.mission.as_deref());
            n.children.push(c);
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_round_trip_keeps_extra_attributes() {
        let node = DetailNode::new("contact")
            .with_attr("callsign", "ALPHA")
            .with_attr("endpoint", "*:-1:stcp")
            .with_attr("xmppUsername", "alpha@example");
        let c = ContactDetail::from_node(&node).unwrap();
        assert_eq!(c.callsign, "ALPHA");
        assert_eq!(
            c.extra,
            vec![("xmppUsername".into(), "alpha@example".into())]
        );
        let back = c.to_node();
        assert_eq!(back.attr("xmppUsername"), Some("alpha@example"));
        assert_eq!(back.attr("endpoint"), Some("*:-1:stcp"));
    }

    #[test]
    fn contact_requires_callsign() {
        let node = DetailNode::new("contact").with_attr("endpoint", "x");
        assert!(matches!(
            ContactDetail::from_node(&node),
            Err(CotError::MissingAttribute { .. })
        ));
    }

    #[test]
    fn status_clamps_battery() {
        let node = DetailNode::new("status").with_attr("battery", "140");
        assert_eq!(StatusDetail::from_node(&node).unwrap().battery, Some(100));
        let node = DetailNode::new("status").with_attr("battery", "abc");
        assert!(StatusDetail::from_node(&node).is_err());
    }

    #[test]
    fn colour_parsing_accepts_java_ints_and_hex() {
        assert_eq!(parse_argb("-1"), Some(Argb::WHITE));
        assert_eq!(parse_argb("4294967295"), Some(Argb::WHITE));
        assert_eq!(parse_argb("#FFFFFFFF"), Some(Argb::WHITE));
        assert_eq!(parse_argb("#FFFFFF"), Some(Argb::WHITE));
        assert_eq!(parse_argb("-65536"), Some(Argb::new(0xFF, 0xFF, 0, 0)));
        assert_eq!(parse_argb("red"), None);
    }

    #[test]
    fn link_point_parsing() {
        let l = LinkDetail::vertex(1.5, -2.25, Some(10.0));
        assert_eq!(l.point.as_deref(), Some("1.5,-2.25,10.0"));
        assert_eq!(l.point_coords().unwrap(), Some((1.5, -2.25, Some(10.0))));
        let bad = LinkDetail {
            point: Some("1,2,3,4".into()),
            ..LinkDetail::default()
        };
        assert!(bad.point_coords().is_err());
        let two = LinkDetail {
            point: Some("1,2".into()),
            ..LinkDetail::default()
        };
        assert_eq!(two.point_coords().unwrap(), Some((1.0, 2.0, None)));
    }

    #[test]
    fn chat_groups_sort_uids_by_index() {
        let node = DetailNode::new("__chat")
            .with_attr("chatroom", "All Chat Rooms")
            .with_attr("id", "All Chat Rooms")
            .with_attr("senderCallsign", "ALPHA")
            .with_child(
                DetailNode::new("chatgrp")
                    .with_attr("uid1", "All Chat Rooms")
                    .with_attr("uid0", "ANDROID-1")
                    .with_attr("id", "All Chat Rooms"),
            )
            .with_child(DetailNode::new("hierarchy"));
        let chat = ChatDetail::from_node(&node).unwrap();
        assert_eq!(chat.groups[0].uids, vec!["ANDROID-1", "All Chat Rooms"]);
        assert_eq!(chat.other_children.len(), 1);
        let back = chat.to_node();
        assert_eq!(
            back.child("chatgrp").unwrap().attr("uid0"),
            Some("ANDROID-1")
        );
        assert!(back.child("hierarchy").is_some());
    }

    #[test]
    fn fmt_f64_always_has_fraction() {
        assert_eq!(fmt_f64(1.0), "1.0");
        assert_eq!(fmt_f64(9_999_999.0), "9999999.0");
        assert_eq!(fmt_f64(-0.5), "-0.5");
        assert!(fmt_f64(1e300).ends_with(".0"));
        assert_eq!(fmt_f64(1.5e-7), "0.00000015");
    }
}
