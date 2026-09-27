//! Which pages are related: shared topics first, then shared title words,
//! then the same folder, newest first on a tie.

use crate::settings::Settings;
use stride_pdk::JsonValue;

/// One published page, as `documents.list` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub slug: String,
    pub title: String,
    pub published_at: String,
}

impl Candidate {
    pub fn from_json(value: &JsonValue) -> Option<Self> {
        let s = |key: &str| value.get(key).and_then(JsonValue::as_str);
        Some(Candidate {
            slug: s("slug")?.trim_matches('/').to_owned(),
            title: s("title").unwrap_or_default().trim().to_owned(),
            published_at: s("publishedAt").unwrap_or_default().to_owned(),
        })
    }
}

/// A page to link to, and the small label above its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub slug: String,
    pub title: String,
    pub label: String,
}

const TOPIC: u32 = 6;
const WORD: u32 = 2;
const FOLDER: u32 = 1;

pub fn pick(slug: &str, title: &str, pages: &[Candidate], settings: &Settings) -> Vec<Pick> {
    let own_topics: Vec<String> = settings.topics_for(slug).iter().map(|t| t.to_lowercase()).collect();
    let own_words = words(title);
    let own_folder = folder(slug);

    let mut scored: Vec<(u32, &Candidate, String)> = pages
        .iter()
        .filter(|c| c.slug != slug && c.slug != "home" && !c.slug.is_empty() && !c.title.is_empty())
        .filter(|c| !settings.exclude.iter().any(|e| *e == c.slug))
        .filter(|c| settings.in_folders(&c.slug))
        .filter_map(|c| {
            let mut score = 0;
            let mut label = String::new();
            for topic in settings.topics_for(&c.slug) {
                if own_topics.contains(&topic.to_lowercase()) {
                    score += TOPIC;
                    if label.is_empty() {
                        label = topic.clone();
                    }
                }
            }
            let theirs = words(&c.title);
            score += WORD * own_words.iter().filter(|w| theirs.contains(w)).count() as u32;
            let their_folder = folder(&c.slug);
            if own_folder.is_some() && own_folder == their_folder {
                score += FOLDER;
            }
            if label.is_empty() {
                label = their_folder.map(humanize).unwrap_or_default();
            }
            (score > 0).then_some((score, c, label))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.published_at.cmp(&a.1.published_at))
            .then_with(|| a.1.slug.cmp(&b.1.slug))
    });
    scored
        .into_iter()
        .take(settings.count as usize)
        .map(|(_, c, label)| Pick { slug: c.slug.clone(), title: c.title.clone(), label })
        .collect()
}

fn folder(slug: &str) -> Option<&str> {
    slug.rsplit_once('/').map(|(folder, _)| folder)
}

/// `writing/field-notes` into `Field notes`.
fn humanize(folder: &str) -> String {
    let last = folder.rsplit('/').next().unwrap_or(folder).replace(['-', '_'], " ");
    let mut chars = last.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The words of a title worth comparing: lowercased, short and common words
/// dropped, a plural `s` taken off so "Starters" meets "starter".
pub fn words(title: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in title.split(|c: char| !c.is_alphanumeric()) {
        let word = word.to_lowercase();
        if word.chars().count() < 3 || STOP.contains(&word.as_str()) {
            continue;
        }
        let word = match word.strip_suffix('s') {
            Some(stem) if stem.chars().count() >= 4 && !stem.ends_with('s') => stem.to_owned(),
            _ => word,
        };
        if !out.contains(&word) {
            out.push(word);
        }
    }
    out
}

/// Common English and Dutch words that say nothing about a topic.
const STOP: &[&str] = &[
    "the", "and", "for", "with", "you", "your", "our", "are", "was", "were", "how", "what", "why",
    "who", "when", "where", "which", "that", "this", "these", "those", "from", "into", "about",
    "over", "under", "out", "not", "but", "all", "any", "can", "will", "has", "have", "had", "its",
    "their", "they", "them", "one", "two", "three", "new", "more", "most", "best", "guide", "part",
    "way", "ways", "tips", "why", "use", "using", "get", "make", "de", "het", "een", "van", "voor",
    "met", "op", "en", "wat", "hoe", "waarom", "die", "dat", "niet", "over", "naar", "bij", "als",
    "ook", "uit", "aan", "door", "onze", "jouw", "uw",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn page(slug: &str, title: &str, at: &str) -> Candidate {
        Candidate { slug: slug.into(), title: title.into(), published_at: at.into() }
    }

    #[test]
    fn words_drop_noise_and_plurals() {
        assert_eq!(words("How to keep a Sourdough Starter alive"), ["keep", "sourdough", "starter", "alive"]);
        assert_eq!(words("Starters & loaves"), ["starter", "loave"]);
        assert_eq!(words("Glass"), ["glass"]);
    }

    #[test]
    fn ranks_topics_then_words_then_folder() {
        let pages = vec![
            page("home", "Home sourdough", "2026-01-01"),
            page("blog/a", "Sourdough starter basics", "2026-01-01"),
            page("blog/b", "Baking rye bread", "2026-02-01"),
            page("blog/c", "Sourdough discard crackers", "2026-03-01"),
            page("blog/d", "Our new oven", "2026-04-01"),
            page("contact", "Contact", "2026-04-01"),
            page("about", "About the sourdough bakery", "2026-01-01"),
        ];
        let settings = Settings {
            topics: crate::settings::parse_topics("blog/a: bread\nblog/b: Bread"),
            ..Settings::default()
        };
        let picks = pick("blog/a", "Sourdough starter basics", &pages, &settings);
        let slugs: Vec<_> = picks.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, ["blog/b", "blog/c", "about"]);
        assert_eq!(picks[0].label, "Bread");
        assert_eq!(picks[1].label, "Blog");
        assert_eq!(picks[2].label, "");
    }

    #[test]
    fn honours_count_exclude_and_folders() {
        let pages = vec![
            page("blog/b", "Bread", "2026-02-01"),
            page("blog/c", "Crackers", "2026-03-01"),
            page("news/d", "Bread news", "2026-04-01"),
        ];
        let settings = Settings { count: 1, ..Settings::default() };
        assert_eq!(pick("blog/a", "Bread", &pages, &settings).len(), 1);
        assert_eq!(pick("blog/a", "Bread", &pages, &settings)[0].slug, "blog/b");
        let settings = Settings {
            folders: vec!["blog".into()],
            exclude: vec!["blog/b".into()],
            ..Settings::default()
        };
        let picks = pick("blog/a", "Bread", &pages, &settings);
        assert_eq!(picks.iter().map(|p| p.slug.as_str()).collect::<Vec<_>>(), ["blog/c"]);
    }
}
