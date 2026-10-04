//! The [`Store`].

use std::collections::{HashMap, HashSet, VecDeque};

use tak_core::{ChatMessage, Contact, ControlKind, TakEvent, TakObject, TakUid, Timestamp};

/// Retention settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreConfig {
    /// Maximum chat messages kept (oldest evicted first).
    pub max_chat_messages: usize,
    /// Maximum tracked contacts; further contacts are rejected (denial-of-service guard).
    pub max_contacts: usize,
    /// Maximum tracked objects; further objects are rejected (denial-of-service guard).
    pub max_objects: usize,
    /// Extra time an item is kept after its `stale` instant before `sweep`
    /// removes it, so the UI can show "stale" before "gone".
    pub stale_grace: std::time::Duration,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            max_chat_messages: 2_000,
            max_contacts: 50_000,
            max_objects: 200_000,
            stale_grace: std::time::Duration::from_secs(5 * 60),
        }
    }
}

/// Why an item left the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalReason {
    /// Explicit `t-x-d-d` delete.
    Deleted,
    /// Passed `stale` + grace at the last sweep.
    Stale,
    /// Evicted to honour a capacity limit.
    Evicted,
}

/// A mutation the store performed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StoreChange {
    /// First report for this contact.
    ContactAdded(TakUid),
    /// Newer report for a known contact.
    ContactUpdated(TakUid),
    /// Contact removed.
    ContactRemoved(TakUid, RemovalReason),
    /// First report for this object.
    ObjectAdded(TakUid),
    /// Newer report for a known object.
    ObjectUpdated(TakUid),
    /// Object removed.
    ObjectRemoved(TakUid, RemovalReason),
    /// New chat message (duplicates by `message_id` are dropped silently).
    ChatAdded(TakUid),
    /// Control message observed; nothing stored.
    Control(ControlKind),
    /// Rejected because a capacity limit was reached.
    Rejected(TakUid),
}

/// Counters for diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct StoreStats {
    /// Events passed to [`Store::apply`].
    pub events_applied: u64,
    /// Duplicate chat messages ignored.
    pub chat_duplicates: u64,
    /// Deletes for UIDs we did not know.
    pub unknown_deletes: u64,
    /// Events rejected by capacity limits.
    pub rejected: u64,
    /// Items removed as stale.
    pub swept: u64,
}

/// Current situational-awareness picture.
#[derive(Debug)]
pub struct Store {
    config: StoreConfig,
    contacts: HashMap<TakUid, Contact>,
    objects: HashMap<TakUid, TakObject>,
    chat: VecDeque<ChatMessage>,
    chat_ids: HashSet<TakUid>,
    stats: StoreStats,
}

impl Default for Store {
    fn default() -> Self {
        Self::new(StoreConfig::default())
    }
}

impl Store {
    /// Empty store.
    #[must_use]
    pub fn new(config: StoreConfig) -> Self {
        Self {
            config,
            contacts: HashMap::new(),
            objects: HashMap::new(),
            chat: VecDeque::new(),
            chat_ids: HashSet::new(),
            stats: StoreStats::default(),
        }
    }

    /// Apply one event. Returns the changes it caused (possibly none).
    pub fn apply(&mut self, event: TakEvent) -> Vec<StoreChange> {
        self.stats.events_applied += 1;
        match event {
            TakEvent::ContactUpdated(contact) => self.upsert_contact(contact),
            TakEvent::ObjectUpdated(object) => self.upsert_object(object),
            TakEvent::ObjectRemoved { uid, .. } => self.remove(&uid, RemovalReason::Deleted),
            TakEvent::ChatReceived(message) => self.add_chat(message),
            TakEvent::Control { kind, .. } => vec![StoreChange::Control(kind)],
            _ => Vec::new(),
        }
    }

    /// Remove everything whose `stale` + grace has passed at `now`.
    pub fn sweep(&mut self, now: Timestamp) -> Vec<StoreChange> {
        let grace = self.config.stale_grace;
        let expired =
            |stale_at: Timestamp| now.saturating_since(stale_at) > grace && stale_at != now;
        let mut changes = Vec::new();
        let gone: Vec<TakUid> = self
            .contacts
            .values()
            .filter(|c| c.is_stale_at(now) && expired(c.timestamps.validity.stale))
            .map(|c| c.uid.clone())
            .collect();
        for uid in gone {
            self.contacts.remove(&uid);
            changes.push(StoreChange::ContactRemoved(uid, RemovalReason::Stale));
        }
        let gone: Vec<TakUid> = self
            .objects
            .values()
            .filter(|o| o.is_stale_at(now) && expired(o.timestamps.validity.stale))
            .map(|o| o.uid.clone())
            .collect();
        for uid in gone {
            self.objects.remove(&uid);
            changes.push(StoreChange::ObjectRemoved(uid, RemovalReason::Stale));
        }
        self.stats.swept += changes.len() as u64;
        changes
    }

    /// Look up a contact.
    #[must_use]
    pub fn contact(&self, uid: &TakUid) -> Option<&Contact> {
        self.contacts.get(uid)
    }

    /// All contacts, unordered.
    pub fn contacts(&self) -> impl Iterator<Item = &Contact> {
        self.contacts.values()
    }

    /// Contacts whose report is not yet stale at `now`.
    pub fn active_contacts(&self, now: Timestamp) -> impl Iterator<Item = &Contact> {
        self.contacts.values().filter(move |c| !c.is_stale_at(now))
    }

    /// Look up an object.
    #[must_use]
    pub fn object(&self, uid: &TakUid) -> Option<&TakObject> {
        self.objects.get(uid)
    }

    /// All objects, unordered.
    pub fn objects(&self) -> impl Iterator<Item = &TakObject> {
        self.objects.values()
    }

    /// Chat messages, oldest first.
    pub fn chat(&self) -> impl Iterator<Item = &ChatMessage> {
        self.chat.iter()
    }

    /// Number of contacts.
    #[must_use]
    pub fn contact_count(&self) -> usize {
        self.contacts.len()
    }

    /// Number of objects.
    #[must_use]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Diagnostics.
    #[must_use]
    pub fn stats(&self) -> StoreStats {
        self.stats
    }

    /// Retention settings in force.
    #[must_use]
    pub fn config(&self) -> &StoreConfig {
        &self.config
    }

    fn upsert_contact(&mut self, contact: Contact) -> Vec<StoreChange> {
        let uid = contact.uid.clone();
        if !self.contacts.contains_key(&uid) && self.contacts.len() >= self.config.max_contacts {
            self.stats.rejected += 1;
            tracing::warn!(%uid, limit = self.config.max_contacts, "contact limit reached; dropping report");
            return vec![StoreChange::Rejected(uid)];
        }
        // A UID is either a contact or an object; a contact report supersedes
        // an object with the same UID (and vice versa).
        let mut changes = Vec::new();
        if self.objects.remove(&uid).is_some() {
            changes.push(StoreChange::ObjectRemoved(
                uid.clone(),
                RemovalReason::Evicted,
            ));
        }
        let change = match self.contacts.insert(uid.clone(), contact) {
            Some(_) => StoreChange::ContactUpdated(uid),
            None => StoreChange::ContactAdded(uid),
        };
        changes.push(change);
        changes
    }

    fn upsert_object(&mut self, object: TakObject) -> Vec<StoreChange> {
        let uid = object.uid.clone();
        if !self.objects.contains_key(&uid) && self.objects.len() >= self.config.max_objects {
            self.stats.rejected += 1;
            tracing::warn!(%uid, limit = self.config.max_objects, "object limit reached; dropping report");
            return vec![StoreChange::Rejected(uid)];
        }
        let mut changes = Vec::new();
        if self.contacts.remove(&uid).is_some() {
            changes.push(StoreChange::ContactRemoved(
                uid.clone(),
                RemovalReason::Evicted,
            ));
        }
        let change = match self.objects.insert(uid.clone(), object) {
            Some(_) => StoreChange::ObjectUpdated(uid),
            None => StoreChange::ObjectAdded(uid),
        };
        changes.push(change);
        changes
    }

    fn remove(&mut self, uid: &TakUid, reason: RemovalReason) -> Vec<StoreChange> {
        if self.contacts.remove(uid).is_some() {
            return vec![StoreChange::ContactRemoved(uid.clone(), reason)];
        }
        if self.objects.remove(uid).is_some() {
            return vec![StoreChange::ObjectRemoved(uid.clone(), reason)];
        }
        self.stats.unknown_deletes += 1;
        Vec::new()
    }

    fn add_chat(&mut self, message: ChatMessage) -> Vec<StoreChange> {
        if self.config.max_chat_messages == 0 {
            self.stats.rejected += 1;
            return vec![StoreChange::Rejected(message.message_id)];
        }
        if !self.chat_ids.insert(message.message_id.clone()) {
            self.stats.chat_duplicates += 1;
            return Vec::new();
        }
        let id = message.message_id.clone();
        self.chat.push_back(message);
        while self.chat.len() > self.config.max_chat_messages {
            if let Some(old) = self.chat.pop_front() {
                self.chat_ids.remove(&old.message_id);
            }
        }
        vec![StoreChange::ChatAdded(id)]
    }
}
