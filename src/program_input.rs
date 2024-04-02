use std::collections::HashMap;

use cairo_vm::Felt252;
use serde::de::Error;
use serde_json::{Result as JsonResult, Value as JsonValue};

pub type Value = Felt252;

#[derive(Debug, Clone)]
pub struct ProgramInput {
    input_values: HashMap<String, Value>,
}

impl ProgramInput {
    pub fn new(input_values: HashMap<String, Value>) -> Self {
        ProgramInput { input_values }
    }

    pub fn from_json(input: &str) -> JsonResult<Self> {
        match serde_json::from_str(input)? {
            JsonValue::Object(obj) => {
                let mut res = HashMap::new();
                for (k, v) in obj {
                    res.insert(k, serde_json::from_value::<Value>(v)?);
                }
                Ok(ProgramInput::new(res))
            }
            _ => Err(Error::custom("invalid program input")),
        }
    }

    pub fn get(&self, var: &str) -> Value {
        self.input_values[var]
    }
}
