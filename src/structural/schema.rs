use ahash::{AHashMap, AHashSet};
use smallvec::SmallVec;
use std::sync::Arc;

/// 阈值：字段数少于 16 时，线性扫描通常比 Hash 计算快，且省内存
pub const SMALL_MAP_THRESHOLD: usize = 16;

#[derive(Debug, Clone)]
pub enum FieldLookup {
    /// 极速路径：CPU 缓存友好的线性存储
    Small(SmallVec<[(Vec<u8>, Arc<SchemaNode>); SMALL_MAP_THRESHOLD]>),
    /// 慢速路径：巨型对象回退
    Large(AHashMap<Vec<u8>, Arc<SchemaNode>>),
}

impl FieldLookup {
    #[inline(always)]
    pub fn get(&self, key: &[u8]) -> Option<&Arc<SchemaNode>> {
        match self {
            FieldLookup::Small(vec) => {
                for (k, node) in vec {
                    if k.as_slice() == key {
                        return Some(node);
                    }
                }
                None
            }
            FieldLookup::Large(map) => map.get(key),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SchemaNode {
    PrimitiveString,
    PrimitiveNumber,
    PrimitiveBool,
    Array(Arc<SchemaNode>),
    Object {
        fields: FieldLookup,
        required: AHashSet<Vec<u8>>,
        /// Aho-Corasick 自动机，用于快速查找 Key
        ac: Arc<aho_corasick::AhoCorasick>,
    },
    Any, // 对应 Schema 中的 {}，放弃 Schema 驱动，退化为通用解析
}
