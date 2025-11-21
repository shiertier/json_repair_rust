import pytest

from llm_json_utils import JsonExtractor, repair_json


def test_repair_json_trailing_comma_and_comments():
    payload = """
    {
        "a": 1,
        "b": [1, 2,], // comment
    }
    """
    data = repair_json(payload)
    assert data == {"a": 1, "b": [1, 2]}


def test_repair_json_preserves_unknown_escapes():
    data = repair_json(r'{"path": "C:\\\\Windows", "weird": "\\\\u123z"}')
    assert data["path"].endswith("Windows")
    assert data["path"].count("\\") >= 2  # backslashes preserved
    assert data["weird"].startswith("\\")
    assert data["weird"].endswith("123z")


def test_schema_extractor_handles_noisy_bytes():
    schema = {
        "type": "object",
        "properties": {
            "summary": {"type": "string"},
            "score": {"type": "number"},
        },
        "required": ["summary"],
    }
    extractor = JsonExtractor(schema)
    blob = b'Thoughts... {"summary": "Done", "score": 95.5 %} Thanks!'
    obj = extractor.extract(blob)
    assert obj["summary"] == "Done"
    assert obj["score"] == 95.5


def test_schema_extractor_missing_required_field_raises():
    schema = {
        "type": "object",
        "properties": {
            "summary": {"type": "string"},
            "score": {"type": "number"},
        },
        "required": ["summary"],
    }
    extractor = JsonExtractor(schema)
    with pytest.raises(ValueError):
        extractor.extract(b"{'score': 10}")
