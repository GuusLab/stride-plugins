//! What the site owner chose, read from storage and from the panel.

use stride_pdk::{JsonValue, Map};

pub const DEFAULT_HEADING: &str = "Related reading";
pub const DEFAULT_COLOR: &str = "#6d28d9";
pub const DEFAULT_COUNT: u32 = 3;
const MAX_COUNT: u32 = 6;
const MAX_HEADING: usize = 60;
const MAX_TOPIC_LINES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Cards,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowOn {
    /// Pages with an `<article>` element or in a folder, such as `blog/…`.
    Articles,
    /// Every page but the home page.
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// Empty for the default heading.
    pub heading: String,
    pub style: Style,
    pub count: u32,
    pub show_on: ShowOn,
    pub folders: Vec<String>,
    /// The panel's text as typed, kept so it round-trips.
    pub topics_text: String,
    /// `(slug, topics)`, parsed from `topics_text`.
    pub topics: Vec<(String, Vec<String>)>,
    /// `#rrggbb`, or empty for the default violet.
    pub color: String,
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            heading: String::new(),
            style: Style::Cards,
            count: DEFAULT_COUNT,
            show_on: ShowOn::Articles,
            folders: Vec::new(),
            topics_text: String::new(),
            topics: Vec::new(),
            color: String::new(),
            exclude: Vec::new(),
        }
    }
}

impl Style {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "cards" => Some(Style::Cards),
            "list" => Some(Style::List),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Style::Cards => "cards",
            Style::List => "list",
        }
    }
}

impl ShowOn {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "articles" => Some(ShowOn::Articles),
            "all" => Some(ShowOn::All),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            ShowOn::Articles => "articles",
            ShowOn::All => "all",
        }
    }
}

impl Settings {
    /// Whatever is stored, never fail: a field that is missing or the wrong
    /// shape falls back to its default.
    pub fn from_json(value: &JsonValue) -> Self {
        let d = Settings::default();
        let s = |key: &str| value.get(key).and_then(JsonValue::as_str);
        let topics_text = s("topics").unwrap_or_default().to_owned();
        Settings {
            enabled: value.get("enabled").and_then(JsonValue::as_bool).unwrap_or(d.enabled),
            heading: s("heading").map(clean_heading).unwrap_or_default(),
            style: s("style").and_then(Style::parse).unwrap_or(d.style),
            count: value
                .get("count")
                .and_then(JsonValue::as_u64)
                .map(|n| (n as u32).clamp(1, MAX_COUNT))
                .unwrap_or(d.count),
            show_on: s("showOn").and_then(ShowOn::parse).unwrap_or(d.show_on),
            folders: s("folders").map(parse_slugs).unwrap_or_default(),
            topics: parse_topics(&topics_text),
            topics_text,
            color: s("color").and_then(valid_color).unwrap_or_default(),
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("enabled".into(), self.enabled.into());
        map.insert("heading".into(), self.heading.clone().into());
        map.insert("style".into(), self.style.name().into());
        map.insert("count".into(), self.count.into());
        map.insert("showOn".into(), self.show_on.name().into());
        map.insert("folders".into(), self.folders.join(", ").into());
        map.insert("topics".into(), self.topics_text.clone().into());
        map.insert("color".into(), self.color.clone().into());
        map.insert("exclude".into(), self.exclude.join(", ").into());
        JsonValue::Object(map)
    }

    /// The panel's field names are the manifest's.
    pub fn to_values(&self) -> Map<String, JsonValue> {
        let mut values = Map::new();
        values.insert("enabled".into(), self.enabled.into());
        values.insert("heading".into(), self.heading.clone().into());
        values.insert("style".into(), self.style.name().into());
        values.insert("count".into(), self.count.into());
        values.insert("show-on".into(), self.show_on.name().into());
        values.insert("folders".into(), self.folders.join(", ").into());
        values.insert("topics".into(), self.topics_text.clone().into());
        if !self.color.is_empty() {
            values.insert("color".into(), self.color.clone().into());
        }
        values.insert("exclude".into(), self.exclude.join(", ").into());
        values
    }

    /// What somebody typed into the panel, checked.
    pub fn from_values(values: &Map<String, JsonValue>) -> Result<Self, String> {
        let d = Settings::default();
        let s = |key: &str| values.get(key).and_then(JsonValue::as_str);
        let color = match s("color").map(str::trim) {
            None | Some("") => String::new(),
            Some(color) => valid_color(color).ok_or_else(|| {
                "The accent colour has to look like #6d28d9. Nothing was saved.".to_owned()
            })?,
        };
        let count = match values.get("count") {
            None | Some(JsonValue::Null) => d.count,
            Some(value) => match value.as_u64() {
                Some(n) if (1..=u64::from(MAX_COUNT)).contains(&n) => n as u32,
                _ => {
                    return Err(format!(
                        "How many pages has to be a number from 1 to {MAX_COUNT}. Nothing was saved."
                    ));
                }
            },
        };
        let topics_text = s("topics").unwrap_or_default().trim().to_owned();
        for (i, line) in topics_text.lines().enumerate() {
            let line = line.trim();
            if !line.is_empty() && !line.contains(':') {
                return Err(format!(
                    "Line {} of Topics has no colon. Write the slug, a colon, then the topics, \
                     like blog/sourdough-starter: baking, bread. Nothing was saved.",
                    i + 1
                ));
            }
        }
        Ok(Settings {
            enabled: values.get("enabled").and_then(JsonValue::as_bool).unwrap_or(d.enabled),
            heading: s("heading").map(clean_heading).unwrap_or_default(),
            style: s("style").and_then(Style::parse).unwrap_or(d.style),
            count,
            show_on: s("show-on").and_then(ShowOn::parse).unwrap_or(d.show_on),
            folders: s("folders").map(parse_slugs).unwrap_or_default(),
            topics: parse_topics(&topics_text),
            topics_text,
            color,
            exclude: s("exclude").map(parse_slugs).unwrap_or_default(),
        })
    }

    pub fn effective_heading(&self) -> &str {
        if self.heading.is_empty() { DEFAULT_HEADING } else { &self.heading }
    }

    pub fn effective_color(&self) -> &str {
        if self.color.is_empty() { DEFAULT_COLOR } else { &self.color }
    }

    /// The topics given for `slug`, lowercased for comparing, with the
    /// original spelling kept for showing.
    pub fn topics_for(&self, slug: &str) -> &[String] {
        self.topics
            .iter()
            .find(|(s, _)| s == slug)
            .map(|(_, t)| t.as_slice())
            .unwrap_or(&[])
    }

    pub fn in_folders(&self, slug: &str) -> bool {
        self.folders.is_empty()
            || self
                .folders
                .iter()
                .any(|f| slug.len() > f.len() && slug.starts_with(f.as_str()) && slug.as_bytes()[f.len()] == b'/')
    }
}

fn clean_heading(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_HEADING)
        .collect()
}

/// `blog/a: Baking, bread` lines into `[("blog/a", ["Baking", "bread"])]`.
pub fn parse_topics(text: &str) -> Vec<(String, Vec<String>)> {
    text.lines()
        .filter_map(|line| {
            let (slug, topics) = line.split_once(':')?;
            let slug = slug.trim().trim_matches('/').to_owned();
            let mut list: Vec<String> = Vec::new();
            for topic in topics.split(',') {
                let topic = topic.split_whitespace().collect::<Vec<_>>().join(" ");
                if !topic.is_empty()
                    && topic.chars().count() <= 40
                    && !list.iter().any(|t| t.eq_ignore_ascii_case(&topic))
                {
                    list.push(topic);
                }
            }
            (!slug.is_empty() && !list.is_empty()).then_some((slug, list))
        })
        .take(MAX_TOPIC_LINES)
        .collect()
}

/// `#rgb` or `#rrggbb`, normalised to lowercase `#rrggbb`. Anything else is
/// refused, which is what keeps the value safe to put into CSS.
pub fn valid_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        6 => Some(format!("#{hex}")),
        3 => Some(hex.chars().fold(String::from("#"), |mut out, c| {
            out.push(c);
            out.push(c);
            out
        })),
        _ => None,
    }
}

/// "home, contact /pricing" into ["home", "contact", "pricing"].
pub fn parse_slugs(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(|slug| slug.trim().trim_matches('/').to_owned())
        .filter(|slug| !slug.is_empty())
        .take(100)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let text = "blog/a: Baking, bread\nblog/b: bread";
        let s = Settings {
            enabled: true,
            heading: "Keep reading".into(),
            style: Style::List,
            count: 4,
            show_on: ShowOn::All,
            folders: vec!["blog".into()],
            topics_text: text.into(),
            topics: parse_topics(text),
            color: "#0ea5e9".into(),
            exclude: vec!["contact".into()],
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_values(&s.to_values()).unwrap(), s);
        assert_eq!(Settings::from_json(&JsonValue::Null), Settings::default());
    }

    #[test]
    fn refuses_bad_input() {
        let mut bad = Map::new();
        bad.insert("color".into(), "url(x)".into());
        assert!(Settings::from_values(&bad).is_err());
        let mut bad = Map::new();
        bad.insert("count".into(), 9.into());
        assert!(Settings::from_values(&bad).is_err());
        let mut bad = Map::new();
        bad.insert("topics".into(), "blog/a baking".into());
        assert!(Settings::from_values(&bad).unwrap_err().contains("Line 1"));
    }

    #[test]
    fn topics_and_folders() {
        let t = parse_topics("/blog/a/: Baking,  bread , baking\nnope\n x: ");
        assert_eq!(t, vec![("blog/a".to_owned(), vec!["Baking".to_owned(), "bread".to_owned()])]);
        let s = Settings { folders: vec!["blog".into()], ..Settings::default() };
        assert!(s.in_folders("blog/a"));
        assert!(!s.in_folders("blogroll"));
        assert!(!s.in_folders("blog"));
    }
}
