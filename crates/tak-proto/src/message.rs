//! Hand-written mirrors of the TAK Protocol v1 `.proto` definitions.
//!
//! Field numbers and names follow the reference schema vendored under
//! `proto/` (see ADR 0007). `prost` derives the wire codec, so no `protoc`
//! is needed at build time.

/// Top-level message carried in every TAK Protocol frame.
#[derive(Clone, PartialEq, prost::Message)]
pub struct TakMessage {
    /// Protocol negotiation payload, present only on control messages.
    #[prost(message, optional, tag = "1")]
    pub tak_control: Option<TakControl>,
    /// A CoT event.
    #[prost(message, optional, tag = "2")]
    pub cot_event: Option<CotEvent>,
}

/// Protocol control: version range supported by the sender.
#[derive(Clone, PartialEq, prost::Message)]
pub struct TakControl {
    /// Lowest supported TAK Protocol version (0 = unspecified).
    #[prost(uint32, tag = "1")]
    pub min_proto_version: u32,
    /// Highest supported TAK Protocol version (0 = unspecified).
    #[prost(uint32, tag = "2")]
    pub max_proto_version: u32,
    /// UID of the contact sending this control message (mesh SA only).
    #[prost(string, tag = "3")]
    pub contact_uid: String,
}

/// Protobuf rendering of a CoT `<event>` and its `<point>`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct CotEvent {
    /// `event/@type`.
    #[prost(string, tag = "1")]
    pub r#type: String,
    /// `event/@access`.
    #[prost(string, tag = "2")]
    pub access: String,
    /// `event/@qos`.
    #[prost(string, tag = "3")]
    pub qos: String,
    /// `event/@opex`.
    #[prost(string, tag = "4")]
    pub opex: String,
    /// `event/@uid`.
    #[prost(string, tag = "5")]
    pub uid: String,
    /// `event/@time` as milliseconds since the Unix epoch.
    #[prost(uint64, tag = "6")]
    pub send_time: u64,
    /// `event/@start` as milliseconds since the Unix epoch.
    #[prost(uint64, tag = "7")]
    pub start_time: u64,
    /// `event/@stale` as milliseconds since the Unix epoch.
    #[prost(uint64, tag = "8")]
    pub stale_time: u64,
    /// `event/@how`.
    #[prost(string, tag = "9")]
    pub how: String,
    /// `point/@lat`.
    #[prost(double, tag = "10")]
    pub lat: f64,
    /// `point/@lon`.
    #[prost(double, tag = "11")]
    pub lon: f64,
    /// `point/@hae`.
    #[prost(double, tag = "12")]
    pub hae: f64,
    /// `point/@ce`.
    #[prost(double, tag = "13")]
    pub ce: f64,
    /// `point/@le`.
    #[prost(double, tag = "14")]
    pub le: f64,
    /// `event/detail`.
    #[prost(message, optional, tag = "15")]
    pub detail: Option<Detail>,
}

/// `<detail>`: a few hoisted children plus the remainder as XML text.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Detail {
    /// Children of `<detail>` not represented by the typed fields, as an
    /// XML fragment (no enclosing `<detail>` element).
    #[prost(string, tag = "1")]
    pub xml_detail: String,
    /// `<contact>`.
    #[prost(message, optional, tag = "2")]
    pub contact: Option<Contact>,
    /// `<__group>`.
    #[prost(message, optional, tag = "3")]
    pub group: Option<Group>,
    /// `<precisionlocation>`.
    #[prost(message, optional, tag = "4")]
    pub precision_location: Option<PrecisionLocation>,
    /// `<status>`.
    #[prost(message, optional, tag = "5")]
    pub status: Option<Status>,
    /// `<takv>`.
    #[prost(message, optional, tag = "6")]
    pub takv: Option<Takv>,
    /// `<track>`.
    #[prost(message, optional, tag = "7")]
    pub track: Option<Track>,
}

/// `<contact endpoint callsign>`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Contact {
    /// `contact/@endpoint`.
    #[prost(string, tag = "1")]
    pub endpoint: String,
    /// `contact/@callsign`.
    #[prost(string, tag = "2")]
    pub callsign: String,
}

/// `<__group name role>`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Group {
    /// `__group/@name`.
    #[prost(string, tag = "1")]
    pub name: String,
    /// `__group/@role`.
    #[prost(string, tag = "2")]
    pub role: String,
}

/// `<precisionlocation geopointsrc altsrc>`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct PrecisionLocation {
    /// `precisionlocation/@geopointsrc`.
    #[prost(string, tag = "1")]
    pub geopointsrc: String,
    /// `precisionlocation/@altsrc`.
    #[prost(string, tag = "2")]
    pub altsrc: String,
}

/// `<status battery>`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Status {
    /// `status/@battery` (percent).
    #[prost(uint32, tag = "1")]
    pub battery: u32,
}

/// `<takv device platform os version>`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Takv {
    /// `takv/@device`.
    #[prost(string, tag = "1")]
    pub device: String,
    /// `takv/@platform`.
    #[prost(string, tag = "2")]
    pub platform: String,
    /// `takv/@os`.
    #[prost(string, tag = "3")]
    pub os: String,
    /// `takv/@version`.
    #[prost(string, tag = "4")]
    pub version: String,
}

/// `<track speed course>`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Track {
    /// `track/@speed` in metres per second.
    #[prost(double, tag = "1")]
    pub speed: f64,
    /// `track/@course` in degrees true.
    #[prost(double, tag = "2")]
    pub course: f64,
}
