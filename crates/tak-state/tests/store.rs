//! Feed real CoT fixtures through the adapter into the store.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use tak_core::{ObjectSource, TakEvent, TakUid, Timestamp, TransportId, WireEncoding};
use tak_cot::adapter::to_tak_event;
use tak_state::{RemovalReason, Store, StoreChange, StoreConfig};

fn fixture(name: &str) -> TakEvent {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tak-cot/tests/fixtures/valid")
        .join(name);
    let xml = std::fs::read_to_string(path).unwrap();
    let event = tak_cot::parse(&xml).unwrap();
    let src = ObjectSource::remote(TransportId::new("test"), WireEncoding::CotXml);
    to_tak_event(
        &event,
        src,
        Timestamp::parse_rfc3339("2024-05-01T12:00:30Z").unwrap(),
    )
    .unwrap()
}

#[test]
fn contacts_add_update_delete() {
    let mut store = Store::default();
    let sa = fixture("atak_sa.xml");
    let uid = sa.uid().clone();
    assert_eq!(
        store.apply(sa.clone()),
        vec![StoreChange::ContactAdded(uid.clone())]
    );
    assert_eq!(
        store.apply(sa),
        vec![StoreChange::ContactUpdated(uid.clone())]
    );
    assert_eq!(store.contact_count(), 1);
    assert_eq!(store.contact(&uid).unwrap().callsign.as_str(), "ALPHA");

    let delete = TakEvent::ObjectRemoved {
        uid: uid.clone(),
        at: Timestamp::now(),
        source: ObjectSource::LOCAL,
    };
    assert_eq!(
        store.apply(delete.clone()),
        vec![StoreChange::ContactRemoved(uid, RemovalReason::Deleted)]
    );
    assert_eq!(store.apply(delete), Vec::new());
    assert_eq!(store.stats().unknown_deletes, 1);
}

#[test]
fn objects_and_mixed_fixtures() {
    let mut store = Store::default();
    for name in [
        "marker_hostile.xml",
        "shape_circle.xml",
        "shape_polygon.xml",
        "route.xml",
        "wintak_sa.xml",
        "itak_sa.xml",
    ] {
        let changes = store.apply(fixture(name));
        assert!(
            matches!(
                changes[..],
                [StoreChange::ObjectAdded(_) | StoreChange::ContactAdded(_)]
            ),
            "{name}: {changes:?}"
        );
    }
    assert_eq!(store.object_count(), 4);
    assert_eq!(store.contact_count(), 2);
    assert!(matches!(
        store.apply(fixture("ping.xml"))[..],
        [StoreChange::Control(_)]
    ));
}

#[test]
fn sweep_removes_stale_after_grace() {
    let mut store = Store::new(StoreConfig {
        stale_grace: std::time::Duration::from_secs(60),
        ..StoreConfig::default()
    });
    let sa = fixture("atak_sa.xml");
    let uid = sa.uid().clone();
    store.apply(sa);
    let stale_at = store.contact(&uid).unwrap().timestamps.validity.stale;
    // Still within grace: kept (and visible as stale).
    let just_after = Timestamp::from_unix_millis(stale_at.unix_millis() + 10_000).unwrap();
    assert_eq!(store.sweep(just_after), Vec::new());
    assert!(store.contact(&uid).unwrap().is_stale_at(just_after));
    assert_eq!(store.active_contacts(just_after).count(), 0);
    // Past grace: removed.
    let later = Timestamp::from_unix_millis(stale_at.unix_millis() + 61_000).unwrap();
    assert_eq!(
        store.sweep(later),
        vec![StoreChange::ContactRemoved(uid, RemovalReason::Stale)]
    );
    assert_eq!(store.contact_count(), 0);
}

#[test]
fn chat_dedupes_and_caps() {
    let mut store = Store::new(StoreConfig {
        max_chat_messages: 1,
        ..StoreConfig::default()
    });
    let msg = fixture("geochat_all_chat_rooms.xml");
    let id = msg.uid().clone();
    assert_eq!(store.apply(msg.clone()), vec![StoreChange::ChatAdded(id)]);
    assert_eq!(store.apply(msg), Vec::new());
    assert_eq!(store.stats().chat_duplicates, 1);
    let direct = fixture("geochat_direct.xml");
    let direct_id = direct.uid().clone();
    assert_eq!(
        store.apply(direct),
        vec![StoreChange::ChatAdded(direct_id.clone())]
    );
    let ids: Vec<_> = store.chat().map(|m| m.message_id.clone()).collect();
    assert_eq!(ids, vec![direct_id]);
}

#[test]
fn capacity_limit_rejects_new_contacts() {
    let mut store = Store::new(StoreConfig {
        max_contacts: 1,
        ..StoreConfig::default()
    });
    store.apply(fixture("atak_sa.xml"));
    let changes = store.apply(fixture("wintak_sa.xml"));
    assert!(
        matches!(changes[..], [StoreChange::Rejected(_)]),
        "{changes:?}"
    );
    assert_eq!(store.stats().rejected, 1);
    // Updates to existing contacts still work at the limit.
    assert!(matches!(
        store.apply(fixture("atak_sa.xml"))[..],
        [StoreChange::ContactUpdated(_)]
    ));
    let _ = TakUid::new("unused").unwrap();
}
