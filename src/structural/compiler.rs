use super::schema::{FieldLookup, SchemaNode, SMALL_MAP_THRESHOLD};
use ahash::{AHashMap, AHashSet};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use smallvec::SmallVec;
use std::sync::Arc;

pub fn compile(schema_obj: &PyAny) -> PyResult<SchemaNode> {
    if let Ok(schema_dict) = schema_obj.downcast::<PyDict>() {
        let type_val = schema_dict.get_item("type")?;

        if let Some(t) = type_val {
            let type_str = t.extract::<String>()?;
            match type_str.as_str() {
                "string" => Ok(SchemaNode::PrimitiveString),
                "integer" | "number" => Ok(SchemaNode::PrimitiveNumber),
                "boolean" => Ok(SchemaNode::PrimitiveBool),
                "array" => {
                    let items = schema_dict.get_item("items")?.ok_or_else(|| {
                        PyErr::new::<pyo3::exceptions::PyValueError, _>(
                            "Array schema missing 'items'",
                        )
                    })?;
                    let inner_node = compile(items)?;
                    Ok(SchemaNode::Array(Arc::new(inner_node)))
                }
                "object" => {
                    let properties = schema_dict.get_item("properties")?;
                    let required_list = schema_dict.get_item("required")?;

                    let mut fields_vec = SmallVec::new();
                    let mut fields_map = AHashMap::new();
                    let mut patterns = Vec::new();
                    let mut required_set = AHashSet::new();

                    if let Some(props) = properties {
                        if let Ok(props_dict) = props.downcast::<PyDict>() {
                            for (k, v) in props_dict {
                                let key_str = k.extract::<String>()?;
                                let key_bytes = key_str.as_bytes().to_vec();
                                let node = Arc::new(compile(v)?);

                                // 构建 Aho-Corasick 模式
                                // 1. 双引号: "key"
                                let mut dq = Vec::with_capacity(key_bytes.len() + 2);
                                dq.push(b'"');
                                dq.extend_from_slice(&key_bytes);
                                dq.push(b'"');
                                patterns.push(dq);

                                // 2. 单引号: 'key'
                                let mut sq = Vec::with_capacity(key_bytes.len() + 2);
                                sq.push(b'\'');
                                sq.extend_from_slice(&key_bytes);
                                sq.push(b'\'');
                                patterns.push(sq);

                                if props_dict.len() < SMALL_MAP_THRESHOLD {
                                    fields_vec.push((key_bytes.clone(), node.clone()));
                                } else {
                                    fields_map.insert(key_bytes.clone(), node.clone());
                                }
                            }
                        }
                    }

                    if let Some(req) = required_list {
                        if let Ok(req_list) = req.downcast::<PyList>() {
                            for item in req_list {
                                let req_str = item.extract::<String>()?;
                                required_set.insert(req_str.as_bytes().to_vec());
                            }
                        }
                    }

                    let fields = if fields_map.is_empty() && !fields_vec.is_empty() {
                        FieldLookup::Small(fields_vec)
                    } else {
                        FieldLookup::Large(fields_map)
                    };

                    // 构建 AC 自动机
                    let ac = aho_corasick::AhoCorasick::new(&patterns).map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                            "Failed to build Aho-Corasick automaton: {}",
                            e
                        ))
                    })?;

                    Ok(SchemaNode::Object {
                        fields,
                        required: required_set,
                        ac: Arc::new(ac),
                    })
                }
                _ => Ok(SchemaNode::Any),
            }
        } else {
            // No type specified, assume Any
            Ok(SchemaNode::Any)
        }
    } else {
        // Not a dict, maybe a string (primitive type shorthand)?
        // For now, just return Any
        Ok(SchemaNode::Any)
    }
}
