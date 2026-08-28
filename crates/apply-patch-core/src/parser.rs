use crate::PatchError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateChunk {
    pub context: Option<String>,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
    pub end_of_file: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchOperation {
    Add {
        path: String,
        lines: Vec<String>,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        chunks: Vec<UpdateChunk>,
    },
}

impl PatchOperation {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Delete { .. } => "delete",
            Self::Update { .. } => "update",
        }
    }

    pub fn path(&self) -> &str {
        match self {
            Self::Add { path, .. } | Self::Delete { path } | Self::Update { path, .. } => path,
        }
    }

    pub fn move_to(&self) -> Option<&str> {
        match self {
            Self::Update { move_to, .. } => move_to.as_deref(),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaderKind {
    Add,
    Delete,
    Update,
}

#[derive(Debug)]
struct FileHeader {
    kind: HeaderKind,
    path: String,
}

fn fail_parse(message: impl AsRef<str>, line_number: Option<usize>) -> PatchError {
    let suffix = line_number.map_or_else(String::new, |line| format!(" on line {line}"));
    PatchError::new(format!("Invalid patch{suffix}: {}", message.as_ref()))
}

fn get_header(line: &str) -> Result<Option<FileHeader>, PatchError> {
    let trimmed = line.trim();
    for (marker, kind) in [
        ("*** Add File:", HeaderKind::Add),
        ("*** Delete File:", HeaderKind::Delete),
        ("*** Update File:", HeaderKind::Update),
    ] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            let path = rest.trim();
            if path.is_empty() {
                return Err(fail_parse(
                    format!("{marker} must be followed by a path"),
                    None,
                ));
            }
            return Ok(Some(FileHeader {
                kind,
                path: path.to_owned(),
            }));
        }
    }
    Ok(None)
}

fn starts_with_whitespace(line: &str) -> bool {
    line.chars().next().is_some_and(char::is_whitespace)
}

fn is_update_boundary(line: &str) -> Result<bool, PatchError> {
    Ok(!starts_with_whitespace(line) && get_header(line)?.is_some())
}

fn is_valid_heredoc_identifier(identifier: &str) -> bool {
    let mut characters = identifier.chars();
    matches!(characters.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn heredoc_identifier(line: &str) -> Option<&str> {
    let value = line.trim().strip_prefix("<<")?;
    let unquoted = if value.len() >= 2 {
        let first = value.as_bytes()[0] as char;
        let last = value.as_bytes()[value.len() - 1] as char;
        if (first == '\'' || first == '"') && first == last {
            &value[1..value.len() - 1]
        } else {
            value
        }
    } else {
        value
    };
    is_valid_heredoc_identifier(unquoted).then_some(unquoted)
}

fn patch_lines(patch: &str) -> Vec<String> {
    let normalized = patch.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = normalized.trim().split('\n').map(str::to_owned).collect();
    if lines.len() >= 4
        && heredoc_identifier(&lines[0])
            .is_some_and(|identifier| lines.last().is_some_and(|line| line.trim() == identifier))
    {
        lines.remove(0);
        lines.pop();
    }
    lines
}

fn finish_chunk(
    current: &mut Option<UpdateChunk>,
    chunks: &mut Vec<UpdateChunk>,
    line_number: usize,
) -> Result<(), PatchError> {
    let Some(chunk) = current.take() else {
        return Ok(());
    };
    if chunk.old_lines.is_empty() && chunk.new_lines.is_empty() {
        return Err(fail_parse(
            "an update hunk must contain at least one change line",
            Some(line_number),
        ));
    }
    chunks.push(chunk);
    Ok(())
}

pub fn parse_patch(patch: &str) -> Result<Vec<PatchOperation>, PatchError> {
    let lines = patch_lines(patch);
    if lines.len() < 2 || lines[0].trim() != "*** Begin Patch" {
        return Err(fail_parse("the first line must be '*** Begin Patch'", None));
    }
    if lines
        .last()
        .is_none_or(|line| line.trim() != "*** End Patch")
    {
        return Err(fail_parse("the last line must be '*** End Patch'", None));
    }

    let mut operations = Vec::new();
    let mut environment_id_seen = false;
    let mut index = 1;
    let end = lines.len() - 1;

    while index < end {
        let line = &lines[index];
        let trimmed = line.trim();

        if let Some(environment_id) = trimmed.strip_prefix("*** Environment ID:") {
            if environment_id_seen || !operations.is_empty() {
                return Err(fail_parse(
                    "the environment ID can only appear once before file hunks",
                    Some(index + 1),
                ));
            }
            if environment_id.trim().is_empty() {
                return Err(fail_parse(
                    "the environment ID cannot be empty",
                    Some(index + 1),
                ));
            }
            environment_id_seen = true;
            index += 1;
            continue;
        }

        let header = get_header(line)?.ok_or_else(|| {
            fail_parse(
                format!("'{trimmed}' is not a valid file hunk header"),
                Some(index + 1),
            )
        })?;

        match header.kind {
            HeaderKind::Add => {
                index += 1;
                let mut added_lines = Vec::new();
                while index < end && get_header(&lines[index])?.is_none() {
                    let add_line = &lines[index];
                    let Some(content) = add_line.strip_prefix('+') else {
                        return Err(fail_parse(
                            "every added line must start with '+'",
                            Some(index + 1),
                        ));
                    };
                    added_lines.push(content.to_owned());
                    index += 1;
                }
                operations.push(PatchOperation::Add {
                    path: header.path,
                    lines: added_lines,
                });
            }
            HeaderKind::Delete => {
                operations.push(PatchOperation::Delete { path: header.path });
                index += 1;
            }
            HeaderKind::Update => {
                index += 1;
                let mut move_to = None;
                if index < end
                    && !starts_with_whitespace(&lines[index])
                    && let Some(destination) = lines[index].trim().strip_prefix("*** Move to:")
                {
                    let destination = destination.trim();
                    if destination.is_empty() {
                        return Err(fail_parse(
                            "*** Move to: must be followed by a path",
                            Some(index + 1),
                        ));
                    }
                    move_to = Some(destination.to_owned());
                    index += 1;
                }

                let mut chunks = Vec::new();
                let mut current: Option<UpdateChunk> = None;

                while index < end {
                    let update_line = &lines[index];
                    let without_trailing_whitespace = update_line.trim_end();

                    if is_update_boundary(update_line)? {
                        finish_chunk(&mut current, &mut chunks, index + 1)?;
                        break;
                    }

                    if current.as_ref().is_some_and(|chunk| chunk.end_of_file) {
                        if without_trailing_whitespace.is_empty() {
                            index += 1;
                            continue;
                        }
                        if without_trailing_whitespace != "@@"
                            && !without_trailing_whitespace.starts_with("@@ ")
                        {
                            return Err(fail_parse(
                                "only a blank line or a new @@ context may follow *** End of File",
                                Some(index + 1),
                            ));
                        }
                    }

                    if without_trailing_whitespace == "*** End of File" {
                        let Some(chunk) = current.as_mut() else {
                            return Err(fail_parse(
                                "*** End of File must follow at least one change line",
                                Some(index + 1),
                            ));
                        };
                        if chunk.old_lines.is_empty() && chunk.new_lines.is_empty() {
                            return Err(fail_parse(
                                "*** End of File must follow at least one change line",
                                Some(index + 1),
                            ));
                        }
                        chunk.end_of_file = true;
                        index += 1;
                        continue;
                    }

                    if without_trailing_whitespace == "@@"
                        || without_trailing_whitespace.starts_with("@@ ")
                    {
                        finish_chunk(&mut current, &mut chunks, index + 1)?;
                        current = Some(UpdateChunk {
                            context: (without_trailing_whitespace != "@@")
                                .then(|| without_trailing_whitespace[3..].to_owned()),
                            old_lines: Vec::new(),
                            new_lines: Vec::new(),
                            end_of_file: false,
                        });
                        index += 1;
                        continue;
                    }

                    let chunk = current.get_or_insert_with(|| UpdateChunk {
                        context: None,
                        old_lines: Vec::new(),
                        new_lines: Vec::new(),
                        end_of_file: false,
                    });

                    if update_line.is_empty() {
                        chunk.old_lines.push(String::new());
                        chunk.new_lines.push(String::new());
                    } else if let Some(context_line) = update_line.strip_prefix(' ') {
                        chunk.old_lines.push(context_line.to_owned());
                        chunk.new_lines.push(context_line.to_owned());
                    } else if let Some(added_line) = update_line.strip_prefix('+') {
                        chunk.new_lines.push(added_line.to_owned());
                    } else if let Some(removed_line) = update_line.strip_prefix('-') {
                        chunk.old_lines.push(removed_line.to_owned());
                    } else {
                        return Err(fail_parse(
                            format!(
                                "unexpected line '{update_line}'; update lines must start with ' ', '+', or '-'"
                            ),
                            Some(index + 1),
                        ));
                    }
                    index += 1;
                }

                finish_chunk(&mut current, &mut chunks, index + 1)?;
                if chunks.is_empty() {
                    return Err(fail_parse(
                        format!("update hunk for '{}' is empty", header.path),
                        Some(index + 1),
                    ));
                }
                if let Some(eof_index) = chunks.iter().position(|chunk| chunk.end_of_file)
                    && eof_index != chunks.len() - 1
                {
                    return Err(fail_parse(
                        "*** End of File must mark the final update chunk",
                        Some(index + 1),
                    ));
                }
                operations.push(PatchOperation::Update {
                    path: header.path,
                    move_to,
                    chunks,
                });
            }
        }
    }

    Ok(operations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heredoc_and_environment_id() {
        let operations = parse_patch(
            "<<'PATCH'\n*** Begin Patch\n*** Environment ID: env-1\n*** Add File: file.txt\n+hello\n*** End Patch\nPATCH",
        )
        .unwrap();
        assert_eq!(
            operations,
            vec![PatchOperation::Add {
                path: "file.txt".to_owned(),
                lines: vec!["hello".to_owned()],
            }]
        );
    }

    #[test]
    fn rejects_empty_update() {
        let error =
            parse_patch("*** Begin Patch\n*** Update File: file.txt\n*** End Patch").unwrap_err();
        assert!(error.to_string().contains("is empty"));
    }
}
