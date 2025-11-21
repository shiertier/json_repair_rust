#![allow(non_local_definitions)]
use crate::structural::schema::SchemaNode;
use crate::utils::cursor::Cursor;
use pyo3::prelude::*;
use std::sync::Arc;

mod repair;
pub mod structural;
pub mod utils;

/// 严格修复 JSON 字符串
#[pyfunction]
pub fn repair_json(py: Python, text: &str) -> PyResult<PyObject> {
    repair::repair_json(py, text)
}

/// 基于 Schema 的 JSON 提取器
#[pyclass]
struct JsonExtractor {
    root: Arc<SchemaNode>,
}

#[pymethods]
impl JsonExtractor {
    #[new]
    fn new(schema_obj: &PyAny) -> PyResult<Self> {
        let root = structural::compiler::compile(schema_obj).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid schema: {:?}", e))
        })?;
        Ok(JsonExtractor {
            root: Arc::new(root),
        })
    }

    fn extract(&self, py: Python, text: &[u8]) -> PyResult<PyObject> {
        // 1. 大海捞针：寻找 JSON 起始
        let mut start_pos = 0;
        while let Some(idx) = memchr::memchr(b'{', &text[start_pos..]) {
            let abs_idx = start_pos + idx;

            // 简单探测
            let mut cursor = Cursor::new(&text[abs_idx..]);

            // 2. 执行解析
            match structural::parser::parse_node(&mut cursor, &self.root, py, 0) {
                Ok(obj) => return Ok(obj),
                Err(_) => {
                    // 解析失败，继续找下一个
                    start_pos = abs_idx + 1;
                    continue;
                }
            }
        }

        Err(pyo3::exceptions::PyValueError::new_err(
            "No matching JSON found",
        ))
    }
}

#[pymodule]
fn llm_json_utils(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(repair_json, m)?)?;
    m.add_class::<JsonExtractor>()?;
    Ok(())
}
