//! `params.parquet`: the resolved register of a run, one row per parameter
//! (plan/04-data-outputs-analytics.md §2).

use arrow::array::{ArrayRef, BooleanArray, Float64Array, Int32Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use marg_params::{ParamValue, ResolvedSpec};
use std::sync::Arc;

/// Schema version of `params.parquet`.
pub const PARAMS_SCHEMA_VERSION: u32 = 1;

fn json(v: &Option<ParamValue>) -> Option<String> {
    v.as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
}

/// Build the `params` table for a resolved spec.
pub fn params_batch(spec: &ResolvedSpec) -> Result<RecordBatch, arrow::error::ArrowError> {
    let ps = &spec.params;
    let col_str = |f: &dyn Fn(&marg_params::ResolvedParam) -> String| -> ArrayRef {
        Arc::new(StringArray::from_iter_values(ps.values().map(f)))
    };
    let schema = Schema::new(vec![
        Field::new("path", DataType::Utf8, false),
        Field::new("value", DataType::Utf8, true),
        Field::new("unit", DataType::Utf8, false),
        Field::new("value_si", DataType::Utf8, true),
        Field::new("si_unit", DataType::Utf8, false),
        Field::new("mean_si", DataType::Float64, true),
        Field::new("grade", DataType::Utf8, false),
        Field::new("role", DataType::Utf8, false),
        Field::new("stage", DataType::Int32, true),
        Field::new("source", DataType::Utf8, false),
        Field::new("guide", DataType::Utf8, false),
        Field::new("set_by", DataType::Utf8, false),
        Field::new("fix_overridden", DataType::Boolean, false),
    ]);
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from_iter_values(ps.keys())),
        Arc::new(StringArray::from(
            ps.values().map(|p| json(&p.value)).collect::<Vec<_>>(),
        )),
        col_str(&|p| p.unit.label().to_string()),
        Arc::new(StringArray::from(
            ps.values().map(|p| json(&p.si)).collect::<Vec<_>>(),
        )),
        col_str(&|p| p.unit.si_label().to_string()),
        Arc::new(Float64Array::from(
            ps.values()
                .map(|p| p.si.as_ref().map(ParamValue::mean))
                .collect::<Vec<_>>(),
        )),
        col_str(&|p| p.grade.label().to_string()),
        col_str(&|p| p.role.label().to_string()),
        Arc::new(Int32Array::from(
            ps.values()
                .map(|p| p.stage.map(i32::from))
                .collect::<Vec<_>>(),
        )),
        col_str(&|p| p.source.clone()),
        col_str(&|p| p.guide.clone()),
        col_str(&|p| p.set_by.label().to_string()),
        Arc::new(BooleanArray::from(
            ps.values().map(|p| p.fix_overridden).collect::<Vec<_>>(),
        )),
    ];
    RecordBatch::try_new(Arc::new(schema), columns)
}
