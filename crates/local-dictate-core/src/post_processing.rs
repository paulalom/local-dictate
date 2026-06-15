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
    cleanup_revisions: bool,
    keyword_swaps: Vec<KeywordSwap>,
}

impl PostProcessingSettings {
    pub fn new(keyword_swaps: Vec<KeywordSwap>) -> Self {
        Self {
            cleanup_disfluencies: false,
            cleanup_revisions: false,
            keyword_swaps,
        }
    }

    pub fn with_cleanup_disfluencies(mut self, cleanup_disfluencies: bool) -> Self {
        self.cleanup_disfluencies = cleanup_disfluencies;
        self
    }

    pub fn with_cleanup_revisions(mut self, cleanup_revisions: bool) -> Self {
        self.cleanup_revisions = cleanup_revisions;
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

    pub fn cleanup_revisions(&self) -> bool {
        self.cleanup_revisions
    }

    pub fn apply(&self, text: &str) -> String {
        let mut text = if self.cleanup_disfluencies {
            cleanup_disfluencies(text)
        } else {
            text.to_string()
        };

        if self.cleanup_revisions {
            text = cleanup_revision_segments(&text);
        }

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
    let words = word_spans(text);

    for (index, word) in words.iter().copied().enumerate() {
        if is_filler_word(&text[word.start..word.end])
            || is_contextual_like_filler(text, &words, index)
        {
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

fn is_contextual_like_filler(text: &str, words: &[Span], index: usize) -> bool {
    let word = words[index];

    if !text[word.start..word.end].eq_ignore_ascii_case("like") {
        return false;
    }

    let previous = index
        .checked_sub(1)
        .map(|previous_index| lowercase_word(text, words[previous_index]));
    let next = words
        .get(index + 1)
        .map(|next_word| lowercase_word(text, *next_word));
    let previous = previous.as_deref();
    let next = next.as_deref();

    if is_protected_like_context(previous, next) {
        return false;
    }

    let has_soft_punctuation = previous_non_whitespace_char(text, word.start)
        .is_some_and(is_soft_punctuation)
        || next_non_whitespace_char(text, word.end).is_some_and(is_soft_punctuation);

    if has_soft_punctuation {
        return true;
    }

    previous.is_some_and(is_like_filler_lead_in)
        && next.is_some_and(|next| !is_protected_like_follower(next))
}

fn is_protected_like_context(previous: Option<&str>, next: Option<&str>) -> bool {
    previous.is_some_and(is_like_content_predecessor)
        || previous.is_some_and(is_would_like_lead_in) && next == Some("to")
}

fn is_like_content_predecessor(word: &str) -> bool {
    matches!(
        word,
        "about"
            | "called"
            | "filler"
            | "fillers"
            | "for"
            | "of"
            | "phrase"
            | "removal"
            | "term"
            | "token"
            | "word"
            | "feel"
            | "feeling"
            | "feels"
            | "felt"
            | "look"
            | "looked"
            | "looking"
            | "looks"
            | "seem"
            | "seemed"
            | "seeming"
            | "seems"
            | "smell"
            | "smelled"
            | "smelling"
            | "smells"
            | "sound"
            | "sounded"
            | "sounding"
            | "sounds"
            | "taste"
            | "tasted"
            | "tasting"
            | "tastes"
    )
}

fn is_would_like_lead_in(word: &str) -> bool {
    matches!(
        word,
        "could" | "couldn't" | "should" | "shouldn't" | "would" | "wouldn't"
    )
}

fn is_like_filler_lead_in(word: &str) -> bool {
    matches!(
        word,
        "and"
            | "but"
            | "or"
            | "so"
            | "can"
            | "can't"
            | "cannot"
            | "could"
            | "couldn't"
            | "gonna"
            | "gotta"
            | "may"
            | "might"
            | "must"
            | "should"
            | "shouldn't"
            | "will"
            | "won't"
            | "would"
            | "wouldn't"
    )
}

fn is_protected_like_follower(word: &str) -> bool {
    matches!(
        word,
        "a" | "an"
            | "her"
            | "hers"
            | "him"
            | "his"
            | "it"
            | "its"
            | "me"
            | "my"
            | "our"
            | "that"
            | "the"
            | "their"
            | "them"
            | "these"
            | "this"
            | "those"
            | "to"
            | "us"
            | "you"
            | "your"
    )
}

fn lowercase_word(text: &str, word: Span) -> String {
    text[word.start..word.end].to_ascii_lowercase()
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

fn cleanup_revision_segments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut sentence_start = 0;

    for (index, character) in text.char_indices() {
        if is_sentence_punctuation(character) {
            let sentence_end = index + character.len_utf8();
            output.push_str(&cleanup_revision_sentence(
                &text[sentence_start..sentence_end],
            ));
            sentence_start = sentence_end;
        }
    }

    if sentence_start < text.len() {
        output.push_str(&cleanup_revision_sentence(&text[sentence_start..]));
    }

    output
}

fn cleanup_revision_sentence(sentence: &str) -> String {
    let clauses = clause_spans(sentence);
    let Some(removal) = revision_prefix_removal_range(sentence, &clauses) else {
        return sentence.to_string();
    };

    let mut output = String::with_capacity(sentence.len());
    output.push_str(&sentence[..removal.start]);
    output.push_str(&sentence[removal.end..]);
    output
}

fn clause_spans(text: &str) -> Vec<Span> {
    let mut clauses = Vec::new();
    let mut start = 0;

    for (index, character) in text.char_indices() {
        if is_soft_punctuation(character) {
            if let Some(clause) = trim_span(text, start..index) {
                clauses.push(clause);
            }

            start = index + character.len_utf8();
        }
    }

    if let Some(clause) = trim_span(text, start..text.len()) {
        clauses.push(clause);
    }

    clauses
}

fn revision_prefix_removal_range(text: &str, clauses: &[Span]) -> Option<Range<usize>> {
    let (target_clause, prefix_clauses) = clauses.split_last()?;

    if prefix_clauses.is_empty() {
        return None;
    }

    let target = RevisionClause::new(text, *target_clause);

    if !target.is_rewrite_target() {
        return None;
    }

    let mut strong_matches = 0;

    for clause in prefix_clauses {
        let clause = RevisionClause::new(text, *clause);

        if clause.is_low_information() {
            continue;
        }

        if clause.is_strong_revision_of(&target) {
            strong_matches += 1;
            continue;
        }

        return None;
    }

    if strong_matches >= 2 {
        Some(prefix_clauses[0].start..target_clause.start)
    } else {
        None
    }
}

fn trim_span(text: &str, range: Range<usize>) -> Option<Span> {
    let mut start = range.start;
    let mut end = range.end;

    while let Some((index, character)) = next_char(text, start) {
        if index >= end || !character.is_whitespace() {
            break;
        }

        start = index + character.len_utf8();
    }

    while let Some((index, character)) = previous_char(text, end) {
        if index < start || !character.is_whitespace() {
            break;
        }

        end = index;
    }

    if start < end {
        Some(Span { start, end })
    } else {
        None
    }
}

fn is_sentence_punctuation(character: char) -> bool {
    matches!(character, '.' | '!' | '?')
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RevisionClause {
    word_count: usize,
    tokens: Vec<String>,
}

impl RevisionClause {
    fn new(text: &str, span: Span) -> Self {
        let words = word_spans(&text[span.start..span.end]);
        let mut tokens = Vec::new();

        for word in &words {
            let word_text = &text[(span.start + word.start)..(span.start + word.end)];
            let Some(token) = canonical_revision_token(word_text) else {
                continue;
            };

            if !tokens.contains(&token) {
                tokens.push(token);
            }
        }

        Self {
            word_count: words.len(),
            tokens,
        }
    }

    fn is_rewrite_target(&self) -> bool {
        self.word_count >= 4 && self.word_count <= 16 && self.tokens.len() >= 3
    }

    fn is_low_information(&self) -> bool {
        self.word_count <= 3 && self.tokens.len() <= 1
    }

    fn is_strong_revision_of(&self, target: &Self) -> bool {
        if self.word_count > 7 || self.tokens.is_empty() || self.tokens.len() > target.tokens.len()
        {
            return false;
        }

        let overlap = self
            .tokens
            .iter()
            .filter(|token| target.tokens.contains(token))
            .count();

        overlap >= 2 && overlap * 2 >= self.tokens.len()
    }
}

fn canonical_revision_token(word: &str) -> Option<String> {
    let lower = word.to_ascii_lowercase();
    let compact = word
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();

    let token = match lower.as_str() {
        "i'm" | "i've" | "i'd" | "i'll" => "i",
        "you're" | "you've" | "you'd" | "you'll" => "you",
        "we're" | "we've" | "we'd" | "we'll" => "we",
        "they're" | "they've" | "they'd" | "they'll" => "they",
        "he's" | "he'd" | "he'll" => "he",
        "she's" | "she'd" | "she'll" => "she",
        "it's" | "it'd" | "it'll" => "it",
        _ => match compact.as_str() {
            "" => return None,
            "gonna" => "going",
            "wanna" => "want",
            "gotta" => "got",
            "kinda" => "kind",
            "sorta" => "sort",
            token if is_revision_stop_token(token) => return None,
            token => token,
        },
    };

    Some(stem_revision_token(token))
}

fn is_revision_stop_token(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "basically"
            | "be"
            | "been"
            | "but"
            | "by"
            | "for"
            | "from"
            | "in"
            | "is"
            | "just"
            | "like"
            | "mean"
            | "meant"
            | "of"
            | "on"
            | "or"
            | "really"
            | "said"
            | "say"
            | "so"
            | "that"
            | "the"
            | "to"
            | "was"
            | "were"
            | "with"
    )
}

fn stem_revision_token(token: &str) -> String {
    if token.len() > 5
        && let Some(stemmed) = token.strip_suffix("ing")
    {
        return stemmed.to_string();
    }

    if token.len() > 4 {
        if let Some(stemmed) = token.strip_suffix("ed") {
            return stemmed.to_string();
        }

        if let Some(stemmed) = token.strip_suffix("es") {
            return stemmed.to_string();
        }

        if let Some(stemmed) = token.strip_suffix('s') {
            return stemmed.to_string();
        }
    }

    token.to_string()
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

fn previous_non_whitespace_char(text: &str, mut index: usize) -> Option<char> {
    while let Some((previous_index, character)) = previous_char(text, index) {
        if !character.is_whitespace() {
            return Some(character);
        }

        index = previous_index;
    }

    None
}

fn next_non_whitespace_char(text: &str, mut index: usize) -> Option<char> {
    while let Some((next_index, character)) = next_char(text, index) {
        if !character.is_whitespace() {
            return Some(character);
        }

        index = next_index + character.len_utf8();
    }

    None
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
    fn removes_contextual_like_fillers_when_cleanup_is_enabled() {
        let settings = PostProcessingSettings::default().with_cleanup_disfluencies(true);

        assert_eq!(
            settings.apply(
                "Can we improve the removal of fillers, or like focus on improving the removal of like when appropriate?"
            ),
            "Can we improve the removal of fillers, or focus on improving the removal of like when appropriate?"
        );
        assert_eq!(
            settings.apply("I'm gonna like test the preview."),
            "I'm gonna test the preview."
        );
        assert_eq!(
            settings.apply("Like, we should ship this."),
            "we should ship this."
        );
        assert_eq!(
            settings.apply("I was, like, going to test this."),
            "I was going to test this."
        );
    }

    #[test]
    fn keeps_content_like_uses_when_cleanup_is_enabled() {
        let settings = PostProcessingSettings::default().with_cleanup_disfluencies(true);

        assert_eq!(settings.apply("I like this."), "I like this.");
        assert_eq!(settings.apply("It looks like this."), "It looks like this.");
        assert_eq!(
            settings.apply("I would like to test this."),
            "I would like to test this."
        );
        assert_eq!(
            settings.apply("Focus on the removal of like when appropriate."),
            "Focus on the removal of like when appropriate."
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
    fn cleans_abandoned_similar_revision_segments() {
        let settings = PostProcessingSettings::default().with_cleanup_revisions(true);

        assert_eq!(
            settings.apply(
                "I said, this is now, I'm gonna like test, I want to test, this is, I'm going to test this now."
            ),
            "I'm going to test this now."
        );
    }

    #[test]
    fn leaves_revision_segments_when_only_disfluency_cleanup_is_enabled() {
        let settings = PostProcessingSettings::default().with_cleanup_disfluencies(true);

        assert_eq!(
            settings.apply(
                "This is now, I'm gonna like test, I want to test, I'm going to test this now."
            ),
            "This is now, I'm gonna test, I want to test, I'm going to test this now."
        );
    }

    #[test]
    fn leaves_cleanup_candidates_unchanged_when_cleanup_flags_are_disabled() {
        let settings = PostProcessingSettings::default();
        let text = "Um, I said, this is now, I'm gonna like test, I want to test, this is, I'm going to test this now.";

        assert_eq!(settings.apply(text), text);
    }

    #[test]
    fn keeps_literal_short_intro_before_final_clause() {
        let settings = PostProcessingSettings::default().with_cleanup_revisions(true);

        assert_eq!(
            settings.apply("I said, I'm going to test this now."),
            "I said, I'm going to test this now."
        );
    }

    #[test]
    fn keeps_distinct_comma_separated_thoughts() {
        let settings = PostProcessingSettings::default().with_cleanup_revisions(true);

        assert_eq!(
            settings.apply("I opened settings, changed the model, and saved the file."),
            "I opened settings, changed the model, and saved the file."
        );
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
