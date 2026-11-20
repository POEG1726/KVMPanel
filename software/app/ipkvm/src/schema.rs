use crate::ApiError;
use jsonschema::{error::ValidationErrorKind, Validator};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

static SCHEMAS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/schemas.json"))).unwrap()
});
static VALIDATORS: LazyLock<Mutex<HashMap<String, Arc<Validator>>>> =
    LazyLock::new(Default::default);

pub fn default_config() -> Value {
    let mut cfg = json!({});
    for (section, schema) in SCHEMAS["Config"]["properties"].as_object().unwrap() {
        for (name, field) in schema["properties"].as_object().unwrap() {
            cfg[section][name] = field["default"].clone();
        }
    }
    cfg
}

pub fn validate(name: &str, value: &Value) -> Result<(), ApiError> {
    let validator = {
        let mut cache = VALIDATORS.lock().unwrap();
        cache
            .entry(name.to_owned())
            .or_insert_with(|| {
                let schema = SCHEMAS
                    .get(name)
                    .expect("known contract operation or message");
                Arc::new(
                    jsonschema::options()
                        .should_validate_formats(true)
                        .build(schema)
                        .expect("valid contract schema"),
                )
            })
            .clone()
    };
    validator.validate(value).map_err(|error| {
        let mut path = error.instance_path.to_string();
        let field = match &error.kind {
            ValidationErrorKind::AdditionalProperties { unexpected } => {
                unexpected.first().map(String::as_str)
            }
            ValidationErrorKind::Required { property } => property.as_str(),
            _ => None,
        };
        if let Some(key) = field {
            path.push('/');
            path.push_str(&key.replace('~', "~0").replace('/', "~1"));
        }
        ApiError::new(
            400,
            if name == "Config" {
                "invalid_config"
            } else {
                "invalid_body"
            },
        )
        .with_detail(json!({"path":path}))
    })
}
