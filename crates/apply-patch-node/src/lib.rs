use std::path::PathBuf;

use apply_patch_core::{
    PatchOperation, PreparedOperation, UpdateChunk, apply_prepared_operation, parse_patch,
    prepare_operation,
};
use napi::bindgen_prelude::{AsyncTask, Env, Task};
use napi_derive::napi;

#[napi(object)]
pub struct NativeUpdateChunk {
    pub context: Option<String>,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
    pub end_of_file: bool,
}

#[napi(object)]
pub struct NativePatchOperation {
    pub kind: String,
    pub path: String,
    pub move_to: Option<String>,
    pub lines: Option<Vec<String>>,
    pub chunks: Option<Vec<NativeUpdateChunk>>,
}

#[napi(object)]
pub struct NativePreparedOperation {
    pub operation: NativePatchOperation,
    pub absolute_path: String,
    pub destination_path: Option<String>,
    pub contents: Option<String>,
    pub diff: Option<String>,
}

fn napi_error(error: impl ToString) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

fn chunk_to_native(chunk: UpdateChunk) -> NativeUpdateChunk {
    NativeUpdateChunk {
        context: chunk.context,
        old_lines: chunk.old_lines,
        new_lines: chunk.new_lines,
        end_of_file: chunk.end_of_file,
    }
}

fn chunk_from_native(chunk: NativeUpdateChunk) -> UpdateChunk {
    UpdateChunk {
        context: chunk.context,
        old_lines: chunk.old_lines,
        new_lines: chunk.new_lines,
        end_of_file: chunk.end_of_file,
    }
}

fn operation_to_native(operation: PatchOperation) -> NativePatchOperation {
    match operation {
        PatchOperation::Add { path, lines } => NativePatchOperation {
            kind: "add".to_owned(),
            path,
            move_to: None,
            lines: Some(lines),
            chunks: None,
        },
        PatchOperation::Delete { path } => NativePatchOperation {
            kind: "delete".to_owned(),
            path,
            move_to: None,
            lines: None,
            chunks: None,
        },
        PatchOperation::Update {
            path,
            move_to,
            chunks,
        } => NativePatchOperation {
            kind: "update".to_owned(),
            path,
            move_to,
            lines: None,
            chunks: Some(chunks.into_iter().map(chunk_to_native).collect()),
        },
    }
}

fn operation_from_native(operation: NativePatchOperation) -> napi::Result<PatchOperation> {
    match operation.kind.as_str() {
        "add" => Ok(PatchOperation::Add {
            path: operation.path,
            lines: operation
                .lines
                .ok_or_else(|| napi_error("Add operation is missing lines"))?,
        }),
        "delete" => Ok(PatchOperation::Delete {
            path: operation.path,
        }),
        "update" => Ok(PatchOperation::Update {
            path: operation.path,
            move_to: operation.move_to,
            chunks: operation
                .chunks
                .ok_or_else(|| napi_error("Update operation is missing chunks"))?
                .into_iter()
                .map(chunk_from_native)
                .collect(),
        }),
        kind => Err(napi_error(format!("Unknown patch operation kind '{kind}'"))),
    }
}

fn prepared_to_native(prepared: PreparedOperation) -> NativePreparedOperation {
    NativePreparedOperation {
        operation: operation_to_native(prepared.operation),
        absolute_path: prepared.absolute_path.to_string_lossy().into_owned(),
        destination_path: prepared
            .destination_path
            .map(|path| path.to_string_lossy().into_owned()),
        contents: prepared.contents,
        diff: prepared.diff,
    }
}

fn prepared_from_native(prepared: NativePreparedOperation) -> napi::Result<PreparedOperation> {
    let operation = operation_from_native(prepared.operation)?;
    if matches!(
        operation,
        PatchOperation::Add { .. } | PatchOperation::Update { .. }
    ) && prepared.contents.is_none()
    {
        return Err(napi_error("Prepared write operation is missing contents"));
    }
    Ok(PreparedOperation {
        operation,
        absolute_path: PathBuf::from(prepared.absolute_path),
        destination_path: prepared.destination_path.map(PathBuf::from),
        contents: prepared.contents,
        diff: prepared.diff,
    })
}

#[napi(js_name = "parsePatchNative")]
pub fn parse_patch_native(patch: String) -> napi::Result<Vec<NativePatchOperation>> {
    parse_patch(&patch)
        .map(|operations| operations.into_iter().map(operation_to_native).collect())
        .map_err(napi_error)
}

pub struct PrepareOperationTask {
    operation: Option<NativePatchOperation>,
    cwd: String,
    include_diff: bool,
}

impl Task for PrepareOperationTask {
    type Output = PreparedOperation;
    type JsValue = NativePreparedOperation;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let operation = self
            .operation
            .take()
            .ok_or_else(|| napi_error("Patch operation was already consumed"))?;
        prepare_operation(
            operation_from_native(operation)?,
            PathBuf::from(&self.cwd).as_path(),
            self.include_diff,
        )
        .map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(prepared_to_native(output))
    }
}

#[napi(js_name = "prepareOperationNative")]
pub fn prepare_operation_native(
    operation: NativePatchOperation,
    cwd: String,
    include_diff: bool,
) -> AsyncTask<PrepareOperationTask> {
    AsyncTask::new(PrepareOperationTask {
        operation: Some(operation),
        cwd,
        include_diff,
    })
}

pub struct ApplyOperationTask {
    prepared: Option<NativePreparedOperation>,
}

impl Task for ApplyOperationTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let prepared = self
            .prepared
            .take()
            .ok_or_else(|| napi_error("Prepared operation was already consumed"))?;
        apply_prepared_operation(&prepared_from_native(prepared)?).map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, _output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(())
    }
}

#[napi(js_name = "applyPreparedOperationNative")]
pub fn apply_prepared_operation_native(
    prepared: NativePreparedOperation,
) -> AsyncTask<ApplyOperationTask> {
    AsyncTask::new(ApplyOperationTask {
        prepared: Some(prepared),
    })
}
