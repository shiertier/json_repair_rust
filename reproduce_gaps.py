import llm_json_utils
import json

print("--- Test 1: Python Literals ---")
try:
    res = llm_json_utils.repair_json('{"a": True, "b": None, "c": False}')
    print(f"Result: {res}")
except Exception as e:
    print(f"Failed: {e}")

print("\n--- Test 2: Prefix Extraction ---")
try:
    res = llm_json_utils.repair_json('Here is the json: {"key": "value"}')
    print(f"Result: {res}")
except Exception as e:
    print(f"Failed: {e}")
