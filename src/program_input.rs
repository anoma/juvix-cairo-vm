use std::collections::HashMap;

use cairo_vm::Felt252;
use indexmap::IndexMap;
use serde::de::Error;
use serde_json::{Result as JsonResult, Value as JsonValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    ValueFelt(Felt252),
    ValueBool(bool),
    ValueRecord(IndexMap<String, Value>),
    ValueList(Vec<Value>),
}

fn value_from_json(val: JsonValue) -> JsonResult<Value> {
    match val {
        JsonValue::Number(num) => Felt252::from_dec_str(num.as_str())
            .map_err(|_| Error::custom("invalid field element"))
            .map(|x| Value::ValueFelt(x)),
        JsonValue::String(_) => serde_json::from_value::<Felt252>(val)
            .map_err(|_| Error::custom("invalid field element"))
            .map(|x| Value::ValueFelt(x)),
        JsonValue::Bool(_) => serde_json::from_value::<bool>(val)
            .map_err(|_| Error::custom("invalid boolean"))
            .map(|x| Value::ValueBool(x)),
        JsonValue::Object(obj) => {
            let mres: JsonResult<IndexMap<String, Value>> = obj
                .into_iter()
                .map(|(k, v)| value_from_json(v).map(|x| (k, x)))
                .collect();
            Ok(Value::ValueRecord(mres?))
        }
        JsonValue::Array(arr) => {
            let mres: JsonResult<Vec<Value>> =
                arr.into_iter().map(|x| value_from_json(x)).collect();
            Ok(Value::ValueList(mres?))
        }
        _ => Err(Error::custom("invalid value")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
                    res.insert(k, value_from_json(v)?);
                }
                Ok(ProgramInput::new(res))
            }
            _ => Err(Error::custom("invalid program input")),
        }
    }

    pub fn get(&self, var: &str) -> &Value {
        &self.input_values[var]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case((r#"{"X": 123}"#,
        ProgramInput::new(HashMap::from([
            (String::from("X"), Value::ValueFelt(Felt252::from(123)))
        ]))
    ))]
    #[case((r#"{"X": "0xAFF"}"#,
        ProgramInput::new(HashMap::from([
            (String::from("X"), Value::ValueFelt(Felt252::from(0xAFF)))
        ]))
    ))]
    #[case((r#"{"X": true}"#,
        ProgramInput::new(HashMap::from([
            (String::from("X"), Value::ValueBool(true))
        ]))
    ))]
    #[case((r#"{"X": false}"#,
        ProgramInput::new(HashMap::from([
            (String::from("X"), Value::ValueBool(false))
        ]))
    ))]
    #[case((r#"{"X": {"X": 123, "Y": true}}"#,
        ProgramInput::new(HashMap::from([
            (String::from("X"),
                Value::ValueRecord(IndexMap::from([
                    (String::from("X"), Value::ValueFelt(Felt252::from(123))),
                    (String::from("Y"), Value::ValueBool(true))
                ]))
            )
        ]))
    ))]
    #[case((r#"{"X": [1, 2, 3]}"#,
        ProgramInput::new(HashMap::from([
            (String::from("X"),
                Value::ValueList(Vec::from([
                    Value::ValueFelt(Felt252::from(1)),
                    Value::ValueFelt(Felt252::from(2)),
                    Value::ValueFelt(Felt252::from(3))
                ]))
            )
        ]))
    ))]
    #[case((r#"{"X": {"X": 123, "Y": true, "Z": {"A": [1, 2, 3], "B": 17}}}"#,
        ProgramInput::new(HashMap::from([
            (String::from("X"),
                Value::ValueRecord(IndexMap::from([
                    (String::from("X"), Value::ValueFelt(Felt252::from(123))),
                    (String::from("Y"), Value::ValueBool(true)),
                    (String::from("Z"), Value::ValueRecord(IndexMap::from([
                        (String::from("A"), Value::ValueList(Vec::from([
                            Value::ValueFelt(Felt252::from(1)),
                            Value::ValueFelt(Felt252::from(2)),
                            Value::ValueFelt(Felt252::from(3))
                        ]))),
                        (String::from("B"), Value::ValueFelt(Felt252::from(17)))
                    ])))
                ]))
            )
        ]))
    ))]
    fn tests_program_input_from_json(#[case] arg: (&str, ProgramInput)) {
        assert_eq!(ProgramInput::from_json(arg.0).unwrap(), arg.1)
    }
}
