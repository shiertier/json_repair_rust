# json-repair-rust

Rust/Python utilities for **deterministic JSON cleanup** and **schema-guided extraction** from messy LLM/log output. Exposed via [PyO3](https://github.com/PyO3/pyo3) + [maturin](https://github.com/PyO3/maturin) as the module `llm_json_utils` (PyPI package name `json-repair-rust`).

> 简体中文文档请见：[README.zh-CN.md](README.zh-CN.md)

## APIs in this crate

- `repair_json(text: str) -> Any` - strict, minimal JSON repair.
- `JsonExtractor(schema)` - finds a schema-shaped object inside noisy bytes/strings and returns Python values.

## `repair_json`: deterministic structural patcher

- Auto-closes truncated objects/arrays at EOF and tolerates trailing commas.
- Ignores `//` / `#` line comments, `/*...*/` block comments, and fenced ``` ``` code blocks so you can feed Markdown directly.
- Parses numbers like Python: ints -> `int`, floats -> `float`, huge ints -> Python `int` (arbitrary precision).
- Preserves unknown escapes and broken `\u` sequences instead of dropping data.
- Raises `ValueError` on real structural errors (missing `:`, mismatched delimiters, etc.) rather than guessing user intent.

## `JsonExtractor`: schema-guided extraction for LLM/log text

- Accepts a minimal JSON-Schema-like dict (`type`, `properties`, `items`, optional `required`), builds Aho-Corasick anchors for field names, then hunts for the first object that matches the schema.
- Robust to the typical noise around LLM replies: missing/extra commas, truncated containers, stray `%`/units after numbers, unescaped quotes, single/full-width quotes, and thousand separators in numbers.
- Works on bytes to avoid encoding surprises; will scan for `{` automatically and stops once a schema-shaped object is parsed.
- Enforces safety valves: recursion depth capped at 128 and strings capped at 1 MB; missing `required` fields surface as `ValueError`.
- Will not synthesize fields or coerce unknown literals; it only extracts what the schema anchors allow.

## Design principles

- **Deterministic fixes only** - patch small, well-defined structural glitches; fail loudly on ambiguous input.
- **Schema as the guardrail** - extraction is anchored by known field names so we avoid "hallucinating" structure from arbitrary prose.
- **Fast and small** - hand-rolled recursive descent with zero per-character allocations on the hot path.

## Python usage

Strict repair:

```python
from llm_json_utils import repair_json

obj = repair_json('{"a": 1, "b": [1,2,],} // trailing comma is fine')
assert obj == {"a": 1, "b": [1, 2]}
```

Schema-guided extraction:

```python
from llm_json_utils import JsonExtractor

schema = {
    "type": "object",
    "properties": {
        "summary": {"type": "string"},
        "score": {"type": "number"},
    },
    "required": ["summary"],
}

extractor = JsonExtractor(schema)
blob = b"Thoughts... {'summary': 'Done', 'score': 95.5 %} Thanks!"
data = extractor.extract(blob)
assert data["summary"] == "Done"
assert data["score"] == 95.5
```

## Build locally

```bash
pip install maturin
maturin develop
python - <<'PY'
from llm_json_utils import repair_json
print(repair_json('{"x": 1,}'))
PY
```
