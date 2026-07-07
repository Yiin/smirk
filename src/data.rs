use emojis::{Emoji, Group};

pub const GROUPS: &[(&str, Group)] = &[
    ("😀", Group::SmileysAndEmotion),
    ("🧑", Group::PeopleAndBody),
    ("🐻", Group::AnimalsAndNature),
    ("🍕", Group::FoodAndDrink),
    ("✈️", Group::TravelAndPlaces),
    ("⚽", Group::Activities),
    ("💡", Group::Objects),
    ("🔣", Group::Symbols),
    ("🏁", Group::Flags),
];

pub struct Entry {
    pub emoji: &'static Emoji,
    name_lc: String,
    shortcodes_lc: Vec<String>,
}

pub struct Catalog {
    pub entries: Vec<Entry>,
}

impl Catalog {
    pub fn load() -> Self {
        let entries = emojis::iter()
            .map(|e| Entry {
                emoji: e,
                name_lc: e.name().to_lowercase(),
                shortcodes_lc: e.shortcodes().map(|s| s.to_lowercase()).collect(),
            })
            .collect();
        Catalog { entries }
    }

    /// Rank-ordered search. Empty query returns the full catalog
    /// (optionally restricted to one group) in CLDR order.
    pub fn search(&self, query: &str, group: Option<Group>) -> Vec<&'static Emoji> {
        let q = query.trim().to_lowercase();

        if q.is_empty() {
            return self
                .entries
                .iter()
                .filter(|e| group.is_none_or(|g| e.emoji.group() == g))
                .map(|e| e.emoji)
                .collect();
        }

        let mut scored: Vec<(u8, usize, &'static Emoji)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| group.is_none_or(|g| e.emoji.group() == g))
            .filter_map(|(i, e)| {
                let score = if e.name_lc.starts_with(&q) {
                    0
                } else if e.name_lc.split_whitespace().any(|w| w.starts_with(&q)) {
                    1
                } else if e.shortcodes_lc.iter().any(|s| s.starts_with(&q)) {
                    2
                } else if e.name_lc.contains(&q) {
                    3
                } else if e.shortcodes_lc.iter().any(|s| s.contains(&q)) {
                    4
                } else {
                    return None;
                };
                Some((score, i, e.emoji))
            })
            .collect();

        scored.sort_by_key(|(score, i, _)| (*score, *i));
        scored.into_iter().map(|(_, _, e)| e).collect()
    }
}
