use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordSwap {
    from: String,
    to: String,
}

impl KeywordSwap {
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> Result<Self, PostProcessingError> {
        let from = from.into();

        if from.trim().is_empty() {
            return Err(PostProcessingError::EmptyKeyword);
        }

        Ok(Self {
            from,
            to: to.into(),
        })
    }

    pub fn from(&self) -> &str {
        &self.from
    }

    pub fn to(&self) -> &str {
        &self.to
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostProcessingSettings {
    keyword_swaps: Vec<KeywordSwap>,
}

impl PostProcessingSettings {
    pub fn new(keyword_swaps: Vec<KeywordSwap>) -> Self {
        Self { keyword_swaps }
    }

    pub fn with_keyword_swap(mut self, keyword_swap: KeywordSwap) -> Self {
        self.keyword_swaps.push(keyword_swap);
        self
    }

    pub fn add_keyword_swap(&mut self, keyword_swap: KeywordSwap) {
        self.keyword_swaps.push(keyword_swap);
    }

    pub fn keyword_swaps(&self) -> &[KeywordSwap] {
        &self.keyword_swaps
    }

    pub fn apply(&self, text: &str) -> String {
        self.keyword_swaps
            .iter()
            .fold(text.to_string(), |current, swap| {
                apply_keyword_swap(&current, swap)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostProcessingError {
    EmptyKeyword,
}

impl fmt::Display for PostProcessingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyKeyword => write!(formatter, "keyword swap source cannot be empty"),
        }
    }
}

impl std::error::Error for PostProcessingError {}

fn apply_keyword_swap(text: &str, swap: &KeywordSwap) -> String {
    let from = swap.from();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;

    while cursor < text.len() {
        let Some(start) = find_keyword_match(text, from, cursor) else {
            output.push_str(&text[cursor..]);
            break;
        };

        let end = start + from.len();

        if is_keyword_boundary(text, start, end) {
            output.push_str(&text[cursor..start]);
            output.push_str(swap.to());
            cursor = end;
        } else {
            let next = next_char_boundary(text, start);
            output.push_str(&text[cursor..next]);
            cursor = next;
        }
    }

    output
}

fn find_keyword_match(text: &str, keyword: &str, cursor: usize) -> Option<usize> {
    text[cursor..].char_indices().find_map(|(offset, _)| {
        let start = cursor + offset;
        let end = start + keyword.len();

        if text.is_char_boundary(end) && text[start..end].eq_ignore_ascii_case(keyword) {
            Some(start)
        } else {
            None
        }
    })
}

fn is_keyword_boundary(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();

    !is_keyword_char(before) && !is_keyword_char(after)
}

fn is_keyword_char(character: Option<char>) -> bool {
    character.is_some_and(|character| character.is_alphanumeric() || character == '_')
}

fn next_char_boundary(text: &str, start: usize) -> usize {
    text[start..]
        .chars()
        .next()
        .map(|character| start + character.len_utf8())
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::{KeywordSwap, PostProcessingSettings};

    #[test]
    fn applies_keyword_swaps_in_order() {
        let settings = PostProcessingSettings::new(vec![
            KeywordSwap::new("peers", "PRs").unwrap(),
            KeywordSwap::new("ship it", "merge it").unwrap(),
        ]);

        assert_eq!(
            settings.apply("please ask peers to ship it"),
            "please ask PRs to merge it"
        );
    }

    #[test]
    fn keeps_keyword_swaps_to_token_boundaries() {
        let settings = PostProcessingSettings::new(vec![KeywordSwap::new("peer", "PR").unwrap()]);

        assert_eq!(
            settings.apply("peer appears in peer_review, but peer should change"),
            "PR appears in peer_review, but PR should change"
        );
    }

    #[test]
    fn preserves_surrounding_punctuation() {
        let settings = PostProcessingSettings::new(vec![KeywordSwap::new("peers", "PRs").unwrap()]);

        assert_eq!(settings.apply("peers, then peers."), "PRs, then PRs.");
    }

    #[test]
    fn matches_ascii_keywords_case_insensitively() {
        let settings = PostProcessingSettings::new(vec![KeywordSwap::new("peers", "PRs").unwrap()]);

        assert_eq!(settings.apply("Peers approved it"), "PRs approved it");
    }

    #[test]
    fn rejects_empty_keyword_sources() {
        assert_eq!(
            KeywordSwap::new("  ", "replacement")
                .unwrap_err()
                .to_string(),
            "keyword swap source cannot be empty"
        );
    }
}
