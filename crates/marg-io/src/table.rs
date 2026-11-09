//! Writing Arrow record batches as zstd-compressed Parquet.

use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use std::fs::File;
use std::path::Path;

/// Errors writing a table.
#[derive(Debug, thiserror::Error)]
pub enum TableError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("arrow: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("parquet: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
}

/// Writer properties shared by every output table.
pub fn writer_properties() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::ZSTD(
            ZstdLevel::try_new(3).expect("valid zstd level"),
        ))
        .set_created_by(format!("marg {}", marg_core::VERSION))
        .build()
}

/// Write one batch to `path`.
pub fn write_batch(path: &Path, batch: &RecordBatch) -> Result<(), TableError> {
    let file = File::create(path)?;
    let mut w = ArrowWriter::try_new(file, batch.schema(), Some(writer_properties()))?;
    w.write(batch)?;
    w.close()?;
    Ok(())
}
