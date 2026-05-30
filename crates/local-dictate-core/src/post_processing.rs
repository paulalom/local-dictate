use std::{fmt, ops::Range};

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
    cleanup_disfluencies: bool,
    keyword_swaps: Vec<KeywordSwap>,
}

impl PostProcessingSettings {
    pub fn new(keyword_swaps: Vec<KeywordSwap>) -> Self {
        Self {
            cleanup_disfluencies: false,
            keyword_swaps,
        }
    }

    pub fn with_cleanup_disfluencies(mut self, cleanup_disfluencies: bool) -> Self {
        self.cleanup_disfluencies = cleanup_disfluencies;
        self
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

    pub fn cleanup_disfluencies(&self) -> bool {
        self.cleanup_disfluencies
    }

    pub fn apply(&self, text: &str) -> String {
        let text = if self.cleanup_disfluencies {
            cleanup_disfluencies(text)
        } else {
            text.to_string()
        };

        self.keyword_swaps
            .iter()
            .fold(text, |current, swap| apply_keyword_swap(&current, swap))
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

fn cleanup_disfluencies(text: &str) -> String {
    let text = cleanup_stuttered_hyphens(text);
    let text = remove_filler_words(&text);
    let text = collapse_repeated_phrases(&text);

    normalize_spacing(&text)
}

fn cleanup_stuttered_hyphens(text: &str) -> String {
    let mut current = text.to_string();

    for _ in 0..4 {
        let next = cleanup_stuttered_hyphens_once(&current);

        if next == current {
            break;
        }

        current = next;
    }

    current
}

fn cleanup_stuttered_hyphens_once(text: &str) -> String {
    let words = word_spans(text);

    for pair in words.windows(2) {
        let prefix = pair[0];
        let word = pair[1];

        if &text[prefix.end..word.start] == "-"
            && is_stutter_prefix(&text[prefix.start..prefix.end], &text[word.start..word.end])
        {
            let mut output = String::with_capacity(text.len());
            output.push_str(&text[..prefix.start]);
            output.push_str(&text[word.start..]);
            return output;
        }
    }

    text.to_string()
}

fn is_stutter_prefix(prefix: &str, word: &str) -> bool {
    let prefix = prefix.to_ascii_lowercase();
    let word = word.to_ascii_lowercase();
    let prefix_len = prefix.chars().count();

    if prefix.is_empty()
        || prefix_len > 2
        || !prefix
            .chars()
            .all(|character| character.is_ascii_alphabetic())
        || !word.starts_with(&prefix)
    {
        return false;
    }

    prefix_len == 1 || is_common_stutter_cluster(&prefix)
}

fn is_common_stutter_cluster(prefix: &str) -> bool {
    matches!(
        prefix,
        "bl" | "br"
            | "ch"
            | "cl"
            | "cr"
            | "dr"
            | "fl"
            | "fr"
            | "gl"
            | "gr"
            | "pl"
            | "pr"
            | "sc"
            | "sh"
            | "sk"
            | "sl"
            | "sm"
            | "sn"
            | "sp"
            | "st"
            | "sw"
            | "th"
            | "tr"
            | "wh"
    )
}

fn remove_filler_words(text: &str) -> String {
    let mut ranges = Vec::new();

    for word in word_spans(text) {
        if is_filler_word(&text[word.start..word.end]) {
            ranges.push(expanded_filler_range(text, word));
        }
    }

    replace_ranges_with_space(text, ranges)
}

fn is_filler_word(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "ah" | "er" | "erm" | "hm" | "hmm" | "mm" | "uh" | "um"
    )
}

fn expanded_filler_range(text: &str, word: Span) -> Range<usize> {
    let mut start = consume_whitespace_back(text, word.start);

    if let Some((punctuation_start, character)) = previous_char(text, start)
        && is_soft_punctuation(character)
    {
        start = consume_whitespace_back(text, punctuation_start);
    }

    let mut end = consume_whitespace_forward(text, word.end);

    if let Some((punctuation_start, character)) = next_char(text, end)
        && is_cleanup_punctuation(character)
    {
        end = consume_whitespace_forward(text, punctuation_start + character.len_utf8());
    }

    start..end
}

fn collapse_repeated_phrases(text: &str) -> String {
    let mut current = text.to_string();

    loop {
        let words = word_spans(&current);
        let mut removal = None;

        'search: for index in 0..words.len() {
            let max_phrase_len = ((words.len() - index) / 2).min(4);

            for phrase_len in (1..=max_phrase_len).rev() {
                if repeated_phrase_at(&current, &words, index, phrase_len) {
                    removal = Some(
                        words[index + phrase_len - 1].end..words[index + (phrase_len * 2) - 1].end,
                    );
                    break 'search;
                }
            }
        }

        let Some(removal) = removal else {
            break;
        };

        current.replace_range(removal, " ");
    }

    current
}

fn repeated_phrase_at(text: &str, words: &[Span], index: usize, phrase_len: usize) -> bool {
    let first_phrase_end = index + phrase_len;
    let second_phrase_end = index + (phrase_len * 2);

    if second_phrase_end > words.len() {
        return false;
    }

    if contains_sentence_boundary(
        text,
        words[first_phrase_end - 1].end..words[first_phrase_end].start,
    ) {
        return false;
    }

    (0..phrase_len).all(|offset| {
        word_eq_ignore_ascii_case(
            text,
            words[index + offset],
            words[first_phrase_end + offset],
        )
    })
}

fn word_eq_ignore_ascii_case(text: &str, left: Span, right: Span) -> bool {
    text[left.start..left.end].eq_ignore_ascii_case(&text[right.start..right.end])
}

fn contains_sentence_boundary(text: &str, range: Range<usize>) -> bool {
    text[range]
        .chars()
        .any(|character| matches!(character, '.' | '!' | '?'))
}

fn replace_ranges_with_space(text: &str, mut ranges: Vec<Range<usize>>) -> String {
    if ranges.is_empty() {
        return text.to_string();
    }

    ranges.sort_by_key(|range| range.start);

    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;

    for range in ranges {
        let start = range.start.max(cursor);
        let end = range.end.max(start);

        if start > cursor {
            output.push_str(&text[cursor..start]);
        }

        if !ends_with_whitespace(&output) {
            output.push(' ');
        }

        cursor = end;
    }

    output.push_str(&text[cursor..]);
    output
}

fn normalize_spacing(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending_space = false;

    for character in text.chars() {
        if character.is_whitespace() {
            pending_space = true;
            continue;
        }

        if is_cleanup_punctuation(character) {
            while ends_with_whitespace(&output) {
                output.pop();
            }

            output.push(character);
            pending_space = true;
            continue;
        }

        if pending_space && !output.is_empty() {
            output.push(' ');
        }

        output.push(character);
        pending_space = false;
    }

    trim_dangling_soft_punctuation(output.trim())
}

fn trim_dangling_soft_punctuation(text: &str) -> String {
    text.trim_matches(|character: char| character.is_whitespace() || is_soft_punctuation(character))
        .to_string()
}

fn consume_whitespace_back(text: &str, mut index: usize) -> usize {
    while let Some((previous_index, character)) = previous_char(text, index) {
        if !character.is_whitespace() {
            break;
        }

        index = previous_index;
    }

    index
}

fn consume_whitespace_forward(text: &str, mut index: usize) -> usize {
    while let Some((next_index, character)) = next_char(text, index) {
        if !character.is_whitespace() {
            break;
        }

        index = next_index + character.len_utf8();
    }

    index
}

fn previous_char(text: &str, index: usize) -> Option<(usize, char)> {
    text[..index].char_indices().next_back()
}

fn next_char(text: &str, index: usize) -> Option<(usize, char)> {
    text[index..]
        .char_indices()
        .next()
        .map(|(offset, character)| (index + offset, character))
}

fn is_soft_punctuation(character: char) -> bool {
    matches!(character, ',' | ';' | ':')
}

fn is_cleanup_punctuation(character: char) -> bool {
    matches!(character, ',' | '.' | '!' | '?' | ';' | ':')
}

fn ends_with_whitespace(text: &str) -> bool {
    text.chars().next_back().is_some_and(char::is_whitespace)
}

fn word_spans(text: &str) -> Vec<Span> {
    let mut words = Vec::new();
    let mut start = None;

    for (index, character) in text.char_indices() {
        if is_spoken_word_char(character) {
            start.get_or_insert(index);
        } else if let Some(word_start) = start.take() {
            words.push(Span {
                start: word_start,
                end: index,
            });
        }
    }

    if let Some(word_start) = start {
        words.push(Span {
            start: word_start,
            end: text.len(),
        });
    }

    words
}

fn is_spoken_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '\''
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
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

    #[test]
    fn leaves_disfluencies_unchanged_by_default() {
        assert_eq!(
            PostProcessingSettings::default().apply("Um, I, I want to w-write."),
            "Um, I, I want to w-write."
        );
    }

    #[test]
    fn cleans_common_dictation_disfluencies_when_enabled() {
        let settings = PostProcessingSettings::default().with_cleanup_disfluencies(true);

        assert_eq!(
            settings.apply("Um, I, I want to w-write this."),
            "I want to write this."
        );
    }

    #[test]
    fn collapses_repeated_phrases_when_cleanup_is_enabled() {
        let settings = PostProcessingSettings::default().with_cleanup_disfluencies(true);

        assert_eq!(
            settings.apply("We should test it, we should test it before release."),
            "We should test it before release."
        );
    }

    #[test]
    fn runs_disfluency_cleanup_before_keyword_swaps() {
        let settings = PostProcessingSettings::new(vec![KeywordSwap::new("peers", "PRs").unwrap()])
            .with_cleanup_disfluencies(true);

        assert_eq!(settings.apply("uh peers peers"), "PRs");
    }

    #[test]
    fn keeps_intentional_sentence_repetition() {
        let settings = PostProcessingSettings::default().with_cleanup_disfluencies(true);

        assert_eq!(settings.apply("No. No changes."), "No. No changes.");
    }

    #[test]
    fn keeps_common_non_stutter_hyphenated_words() {
        let settings = PostProcessingSettings::default().with_cleanup_disfluencies(true);

        assert_eq!(
            settings.apply("Please re-record the sample."),
            "Please re-record the sample."
        );
    }
}
