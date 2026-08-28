use memchr::memchr_iter;
use memchr::memmem::Finder;

use crate::{PatchError, UpdateChunk};

#[derive(Debug)]
pub(crate) struct IndexedText {
    text: String,
    line_starts: Vec<usize>,
}

impl IndexedText {
    pub fn new(contents: &str) -> Self {
        let text = contents.replace("\r\n", "\n").replace('\r', "\n");
        let line_starts = if text.is_empty() {
            Vec::new()
        } else {
            let mut starts = vec![0];
            starts.extend(
                memchr_iter(b'\n', text.as_bytes())
                    .map(|position| position + 1)
                    .filter(|position| *position < text.len()),
            );
            starts
        };
        Self { text, line_starts }
    }

    pub fn len(&self) -> usize {
        self.line_starts.len()
    }

    pub fn line(&self, index: usize) -> &str {
        let start = self.line_starts[index];
        let end = self
            .line_starts
            .get(index + 1)
            .map_or(self.text.len(), |next| next - 1);
        let end = if end == self.text.len() && self.text.ends_with('\n') {
            end - 1
        } else {
            end
        };
        &self.text[start..end]
    }

    pub fn to_lines(&self) -> Vec<String> {
        (0..self.len())
            .map(|index| self.line(index).to_owned())
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Replacement {
    pub start: usize,
    pub old_length: usize,
    pub new_lines: Vec<String>,
}

fn lines_equal(text: &IndexedText, pattern: &[String], start: usize) -> bool {
    pattern
        .iter()
        .enumerate()
        .all(|(offset, expected)| text.line(start + offset) == expected)
}

fn seek_sequence(
    text: &IndexedText,
    pattern: &[String],
    start: usize,
    end_of_file: bool,
) -> Option<usize> {
    if pattern.is_empty() {
        return Some(start);
    }
    if pattern.len() > text.len() || start + pattern.len() > text.len() {
        return None;
    }

    if end_of_file {
        let candidate = text.len() - pattern.len();
        return (candidate >= start && lines_equal(text, pattern, candidate)).then_some(candidate);
    }

    let needle = pattern.join("\n");
    if needle.is_empty() {
        return (start..=text.len() - pattern.len())
            .find(|candidate| lines_equal(text, pattern, *candidate));
    }

    let finder = Finder::new(needle.as_bytes());
    let bytes = text.text.as_bytes();
    let mut search_offset = text.line_starts[start];

    while search_offset <= bytes.len().saturating_sub(needle.len()) {
        let relative = finder.find(&bytes[search_offset..])?;
        let candidate_byte = search_offset + relative;
        let candidate_line = match text.line_starts.binary_search(&candidate_byte) {
            Ok(index) => index,
            Err(_) => {
                search_offset = candidate_byte + 1;
                continue;
            }
        };
        let candidate_end = candidate_byte + needle.len();
        let ends_at_line_boundary = candidate_end == bytes.len()
            || bytes.get(candidate_end).is_some_and(|byte| *byte == b'\n');
        if ends_at_line_boundary
            && candidate_line + pattern.len() <= text.len()
            && lines_equal(text, pattern, candidate_line)
        {
            return Some(candidate_line);
        }
        search_offset = candidate_byte + 1;
    }
    None
}

pub(crate) fn compute_replacements(
    original: &IndexedText,
    path: &str,
    chunks: &[UpdateChunk],
) -> Result<Vec<Replacement>, PatchError> {
    let mut replacements = Vec::new();
    let mut line_index = 0;

    for chunk in chunks {
        if let Some(context) = &chunk.context {
            let context_pattern = [context.clone()];
            let context_index = seek_sequence(original, &context_pattern, line_index, false)
                .ok_or_else(|| {
                    PatchError::new(format!("Failed to find context '{context}' in {path}"))
                })?;
            line_index = context_index + 1;
        }

        if chunk.old_lines.is_empty() {
            replacements.push(Replacement {
                start: original.len(),
                old_length: 0,
                new_lines: chunk.new_lines.clone(),
            });
            continue;
        }

        let found = seek_sequence(original, &chunk.old_lines, line_index, chunk.end_of_file)
            .ok_or_else(|| {
                PatchError::new(format!(
                    "Failed to find expected lines in {path}:\n{}",
                    chunk.old_lines.join("\n")
                ))
            })?;
        replacements.push(Replacement {
            start: found,
            old_length: chunk.old_lines.len(),
            new_lines: chunk.new_lines.clone(),
        });
        line_index = found + chunk.old_lines.len();
    }

    replacements.sort_by_key(|replacement| replacement.start);
    Ok(replacements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_exact_sequence_at_end() {
        let text = IndexedText::new("one\ntwo\nthree\n");
        let pattern = vec!["two".to_owned(), "three".to_owned()];
        assert_eq!(seek_sequence(&text, &pattern, 0, true), Some(1));
    }

    #[test]
    fn rejects_non_exact_unicode_punctuation() {
        let text = IndexedText::new("“hello”\n");
        let pattern = vec!["\"hello\"".to_owned()];
        assert_eq!(seek_sequence(&text, &pattern, 0, false), None);
    }
}
