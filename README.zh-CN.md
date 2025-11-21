# llm_json_repair（中文说明）

Rust 实现的 **确定性 JSON 轻量修复 + 基于 Schema 的提取** 工具，通过 [PyO3](https://github.com/PyO3/pyo3) 和 [maturin](https://github.com/PyO3/maturin) 暴露为 Python 模块 `llm_json_utils`（PyPI 包名 `llm-json-utils`，仓库名 `llm_json_repair`）。

## 提供的 API

- `repair_json(text: str) -> Any` —— 严格、最小化的 JSON 修复。
- `JsonExtractor(schema)` —— 按给定 Schema，在含噪声的文本/字节流里寻找并提取 JSON。

## `repair_json`：确定性结构修复

- EOF 时自动闭合对象/数组，接受尾逗号。
- 忽略 `//` / `#` 行注释、`/*...*/` 块注释，以及 Markdown fenced code block，Markdown 可直接喂给它。
- 数字行为与 Python 一致：整数 -> `int`，浮点 -> `float`，超大整数交给 Python `int()`，不丢精度。
- 保留未知转义和损坏的 `\u` 序列，不会吞字符。
- 真正的结构错误（缺少冒号、分隔符错等）直接抛出 `ValueError`，绝不瞎猜。

## `JsonExtractor`：Schema 驱动的 LLM/日志提取器

- 接受简化版 JSON Schema（`type` / `properties` / `items` / 可选 `required`），内部用 Aho-Corasick 锚点定位字段，找到第一个符合 Schema 的对象。
- 能容忍常见噪声：缺/多逗号、截断的容器、数字后跟单位或 `%`、未转义的引号、单/全角引号、带千分位的数字等。
- 直接处理 `bytes` 以避免编码问题，会自动从第一个 `{` 开始扫描，匹配成功即返回。
- 安全阈值：递归深度上限 128，字符串最长 1MB；缺少 `required` 字段时抛出 `ValueError`。
- 不会凭空生成字段，也不会强行把未知字面量塞进结果。

## 设计理念

- **确定性优先**：只修复明确且安全的小问题，模糊输入直接报错，避免掩盖上游 bug。
- **Schema 当护栏**：提取依赖字段锚点，不尝试把任意长文本“硬解释成 JSON”。
- **小而快**：手写递归下降解析器，热路径零多余分配。

## Python 使用示例

严格修复：

```python
from llm_json_utils import repair_json

data = repair_json('{"a": 1, "b": [1,2,],} // 尾逗号无压力')
assert data == {"a": 1, "b": [1, 2]}
```

基于 Schema 的提取：

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
blob = b"思考中... {'summary': 'Done', 'score': 95.5 %} 谢谢！"
data = extractor.extract(blob)
assert data["summary"] == "Done"
assert data["score"] == 95.5
```

大整数示例（修复器）：

```python
from llm_json_utils import repair_json

data = repair_json('{"id": 123456789012345678901234567890}')
assert isinstance(data["id"], int)
```

## 本地构建（开发者）

```bash
pip install maturin
maturin develop

python - <<'PY'
from llm_json_utils import repair_json
print(repair_json('{"x": 1,}'))
PY
```

## 适用 / 不适用

适合：

- LLM 生成的 JSON 偶尔截断、缺/多逗号；
- 需要从长对话、日志或 Markdown 里抽取结构化字段；
- 想在**不更改语义、不掩盖错误**前提下做有限修复。

不适合：

- 期望“无论输入多烂都能给一个 JSON”；
- 想靠它掩盖上游系统/Prompt 的结构性问题或把任意文本 JSON 化。
