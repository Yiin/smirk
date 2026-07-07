use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const MAX_RECENTS: usize = 32;

#[derive(Serialize, Deserialize, Clone)]
struct Item {
    emoji: String,
    count: u64,
    last_used: u64,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Recents {
    items: Vec<Item>,
    #[serde(default)]
    seq: u64,
}

fn state_path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/state"));
    base.join("smirk/recents.json")
}

impl Recents {
    pub fn load() -> Self {
        fs::read(state_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn push(&mut self, emoji: &str) {
        self.seq += 1;
        match self.items.iter_mut().find(|i| i.emoji == emoji) {
            Some(item) => {
                item.count += 1;
                item.last_used = self.seq;
            }
            None => self.items.push(Item {
                emoji: emoji.to_string(),
                count: 1,
                last_used: self.seq,
            }),
        }
        // Frequency first, recency as tie-breaker.
        self.items
            .sort_by_key(|i| (std::cmp::Reverse(i.count), std::cmp::Reverse(i.last_used)));
        self.items.truncate(MAX_RECENTS);
        self.save();
    }

    pub fn emojis(&self) -> Vec<&str> {
        self.items.iter().map(|i| i.emoji.as_str()).collect()
    }

    fn save(&self) {
        let path = state_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}
