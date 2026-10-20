//! The session's status board (plugin conversion plan G3): a plugin
//! publishes values under keys, any plugin of the session reads them by the
//! publisher's id (MossTank publishes its `metaState`, MosswartMassacre
//! shows it). Scoped to the session; a plugin's values go when it stops.

use std::collections::BTreeMap;

/// The values, by (publisher id in lower case, key).
#[derive(Debug, Default)]
pub struct StatusBoard {
    values: BTreeMap<(String, String), String>,
}

impl StatusBoard {
    /// `owner` publishes `value` under `key` (none withdraws it); false
    /// for a blank key.
    pub fn publish(&mut self, owner: &str, key: &str, value: Option<String>) -> bool {
        if key.trim().is_empty() {
            return false;
        }
        let slot = (owner.to_lowercase(), key.to_owned());
        match value {
            Some(value) => {
                self.values.insert(slot, value);
            }
            None => {
                self.values.remove(&slot);
            }
        }
        true
    }

    /// What `owner` published under `key`.
    #[must_use]
    pub fn read(&self, owner: &str, key: &str) -> Option<String> {
        self.values
            .get(&(owner.to_lowercase(), key.to_owned()))
            .cloned()
    }

    /// Everything `owner` published, by key.
    #[must_use]
    pub fn capture(&self, owner: &str) -> Vec<(String, String)> {
        let owner = owner.to_lowercase();
        self.values
            .iter()
            .filter(|((o, _), _)| *o == owner)
            .map(|((_, k), v)| (k.clone(), v.clone()))
            .collect()
    }

    /// `owner` stopped: its values go.
    pub fn release(&mut self, owner: &str) {
        let owner = owner.to_lowercase();
        self.values.retain(|(o, _), _| *o != owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Plugin conversion plan G3: a value is read by its publisher's id (any
    // case), withdrawn by publishing none, and gone when the publisher stops.
    #[test]
    fn values_are_read_by_publisher_and_go_with_it() {
        let mut board = StatusBoard::default();
        assert!(board.publish("MossTank", "metaState", Some("Looting".into())));
        assert!(!board.publish("MossTank", " ", Some("x".into())));
        assert_eq!(
            board.read("mosstank", "metaState").as_deref(),
            Some("Looting")
        );
        assert_eq!(board.read("mosstank", "metastate"), None);
        board.publish("other", "k", Some("v".into()));
        assert_eq!(
            board.capture("MOSSTANK"),
            [("metaState".to_owned(), "Looting".to_owned())]
        );
        board.publish("MossTank", "metaState", None);
        assert_eq!(board.read("mosstank", "metaState"), None);
        board.release("OTHER");
        assert!(board.capture("other").is_empty());
    }
}
