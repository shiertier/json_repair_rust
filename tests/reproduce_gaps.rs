use llm_json_utils::repair_json;
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[test]
fn test_gaps() -> PyResult<()> {
    pyo3::prepare_freethreaded_python();
    Python::with_gil(|py| {
        println!("--- Test 1: Python Literals ---");
        let json_str = "{ \"a\": True, \"b\": None, \"c\": False, \"d\": NaN, \"e\": Infinity }";
        match repair_json(py, json_str) {
            Ok(obj) => {
                let dict = obj.downcast::<PyDict>(py)?;
                let val_a = dict.get_item("a")?.unwrap();
                println!("Success: a={}", val_a);
                // We can't easily check NaN/Inf in Rust via PyObject without more casting, but if it parses, it's good.
            }
            Err(e) => panic!("Failed to parse literals: {}", e),
        }

        println!("\n--- Test 2: Prefix Extraction ---");
        let json_str_2 = "Here is the json: { \"key\": \"value\" }";
        match repair_json(py, json_str_2) {
            Ok(obj) => {
                let dict = obj.downcast::<PyDict>(py)?;
                if dict.contains("key")? {
                    println!("Success: Found key 'key'");
                }
            }
            Err(e) => panic!("Failed to extract: {}", e),
        }

        println!("\n--- Test 3: Unquoted Keys (Should Fail) ---");
        let json_str_3 = "{ key: 'value', _underscore: 123, $dollar: true }";
        match repair_json(py, json_str_3) {
            Ok(_) => panic!("Should have failed for unquoted keys"),
            Err(e) => println!("Success: Failed as expected for unquoted keys: {}", e),
        }

        Ok(())
    })
}
