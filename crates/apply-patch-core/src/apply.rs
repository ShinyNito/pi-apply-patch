use std::fs;
use std::path::{Path, PathBuf};

use crate::matcher::{IndexedText, Replacement, compute_replacements};
use crate::{PatchError, PatchOperation, UpdateChunk};

#[derive(Clone, Debug)]
pub struct PreparedOperation {
    pub operation: PatchOperation,
    pub absolute_path: PathBuf,
    pub destination_path: Option<PathBuf>,
    pub contents: Option<String>,
    pub diff: Option<String>,
}

fn line_ending_for(contents: &str) -> &'static str {
    if contents.contains("\r\n") {
        "\r\n"
    } else if contents.contains('\r') {
        "\r"
    } else {
        "\n"
    }
}

fn apply_replacements(lines: &mut Vec<String>, replacements: &[Replacement]) {
    for replacement in replacements.iter().rev() {
        lines.splice(
            replacement.start..replacement.start + replacement.old_length,
            replacement.new_lines.clone(),
        );
    }
}

fn build_updated_contents(
    original: &str,
    chunks: &[UpdateChunk],
    path: &str,
) -> Result<String, PatchError> {
    let indexed = IndexedText::new(original);
    let replacements = compute_replacements(&indexed, path, chunks)?;
    let mut lines = indexed.to_lines();
    apply_replacements(&mut lines, &replacements);
    if lines.last().is_none_or(|line| !line.is_empty()) {
        lines.push(String::new());
    }
    Ok(lines.join("\n").replace('\n', line_ending_for(original)))
}

fn split_diff_lines(contents: &str) -> Vec<String> {
    let normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = normalized.split('\n').map(str::to_owned).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

fn diff_lines(old_lines: &[String], new_lines: &[String]) -> Vec<String> {
    let mut table = vec![vec![0; new_lines.len() + 1]; old_lines.len() + 1];
    for old_index in (0..old_lines.len()).rev() {
        for new_index in (0..new_lines.len()).rev() {
            table[old_index][new_index] = if old_lines[old_index] == new_lines[new_index] {
                table[old_index + 1][new_index + 1] + 1
            } else {
                table[old_index + 1][new_index].max(table[old_index][new_index + 1])
            };
        }
    }

    let mut output = Vec::new();
    let mut old_index = 0;
    let mut new_index = 0;
    while old_index < old_lines.len() || new_index < new_lines.len() {
        if old_index < old_lines.len()
            && new_index < new_lines.len()
            && old_lines[old_index] == new_lines[new_index]
        {
            output.push(format!(" {}", old_lines[old_index]));
            old_index += 1;
            new_index += 1;
        } else if new_index < new_lines.len()
            && (old_index == old_lines.len()
                || table[old_index][new_index + 1] > table[old_index + 1][new_index])
        {
            output.push(format!("+{}", new_lines[new_index]));
            new_index += 1;
        } else {
            output.push(format!("-{}", old_lines[old_index]));
            old_index += 1;
        }
    }
    output
}

fn diff_for_operation(operation: &PatchOperation, original: Option<&str>) -> String {
    match operation {
        PatchOperation::Add { lines, .. } => lines
            .iter()
            .map(|line| format!("+{line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        PatchOperation::Delete { .. } => split_diff_lines(
            original.expect("delete operations always include their original contents"),
        )
        .iter()
        .map(|line| format!("-{line}"))
        .collect::<Vec<_>>()
        .join("\n"),
        PatchOperation::Update { chunks, .. } => chunks
            .iter()
            .flat_map(|chunk| {
                let mut output = vec![format!(
                    "@@{}",
                    chunk
                        .context
                        .as_ref()
                        .map_or_else(String::new, |context| format!(" {context}"))
                )];
                output.extend(diff_lines(&chunk.old_lines, &chunk.new_lines));
                output
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn resolve_path(cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

pub fn prepare_operation(
    operation: PatchOperation,
    cwd: &Path,
    include_diff: bool,
) -> Result<PreparedOperation, PatchError> {
    let absolute_path = resolve_path(cwd, operation.path());
    match &operation {
        PatchOperation::Add { lines, .. } => {
            let contents = if lines.is_empty() {
                String::new()
            } else {
                format!("{}\n", lines.join("\n"))
            };
            let diff = include_diff.then(|| diff_for_operation(&operation, None));
            Ok(PreparedOperation {
                operation,
                absolute_path,
                destination_path: None,
                contents: Some(contents),
                diff,
            })
        }
        PatchOperation::Delete { .. } => {
            let original = fs::read_to_string(&absolute_path)?;
            let diff = include_diff.then(|| diff_for_operation(&operation, Some(&original)));
            Ok(PreparedOperation {
                operation,
                absolute_path,
                destination_path: None,
                contents: None,
                diff,
            })
        }
        PatchOperation::Update {
            path,
            move_to,
            chunks,
        } => {
            let original = fs::read_to_string(&absolute_path)?;
            let contents = build_updated_contents(&original, chunks, path)?;
            let destination_path = move_to.as_ref().map(|path| resolve_path(cwd, path));
            let diff = include_diff.then(|| diff_for_operation(&operation, Some(&original)));
            Ok(PreparedOperation {
                operation,
                absolute_path,
                destination_path,
                contents: Some(contents),
                diff,
            })
        }
    }
}

pub fn apply_prepared_operation(prepared: &PreparedOperation) -> Result<(), PatchError> {
    match &prepared.operation {
        PatchOperation::Add { .. } => {
            if let Some(parent) = prepared.absolute_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                &prepared.absolute_path,
                prepared
                    .contents
                    .as_deref()
                    .expect("prepared add operations always include contents"),
            )?;
        }
        PatchOperation::Delete { path } => {
            if fs::metadata(&prepared.absolute_path)?.is_dir() {
                return Err(PatchError::new(format!("Cannot delete directory '{path}'")));
            }
            fs::remove_file(&prepared.absolute_path)?;
        }
        PatchOperation::Update { .. } => {
            let target = prepared
                .destination_path
                .as_ref()
                .unwrap_or(&prepared.absolute_path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                target,
                prepared
                    .contents
                    .as_deref()
                    .expect("prepared update operations always include contents"),
            )?;
            if prepared.destination_path.is_some() {
                fs::remove_file(&prepared.absolute_path)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        let name = format!(
            "pi-apply-patch-rust-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn prepares_and_applies_exact_update_with_crlf() {
        let cwd = temporary_directory();
        let path = cwd.join("file.txt");
        fs::write(&path, "\"hello\"\r\n").unwrap();
        let operation = PatchOperation::Update {
            path: "file.txt".to_owned(),
            move_to: None,
            chunks: vec![UpdateChunk {
                context: None,
                old_lines: vec!["\"hello\"".to_owned()],
                new_lines: vec!["\"world\"".to_owned()],
                end_of_file: true,
            }],
        };
        let prepared = prepare_operation(operation, &cwd, true).unwrap();
        apply_prepared_operation(&prepared).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "\"world\"\r\n");
        assert_eq!(prepared.diff.as_deref(), Some("@@\n-\"hello\"\n+\"world\""));
        fs::remove_dir_all(cwd).unwrap();
    }
}
