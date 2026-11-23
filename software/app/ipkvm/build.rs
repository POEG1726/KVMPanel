use serde_json::{json, Value};
use std::{env, fs, path::PathBuf};

// Resolve local OpenAPI references at build time. The board needs no YAML files.
fn expand(node: &mut Value, doc: &Value) {
    match node {
        Value::Object(map) => {
            if let Some(reference) = map.remove("$ref") {
                let path = reference.as_str().unwrap().strip_prefix('#').unwrap();
                let mut resolved = doc.pointer(path).unwrap().as_object().unwrap().clone();
                resolved.append(map);
                *map = resolved;
            }
            map.values_mut().for_each(|v| expand(v, doc));
        }
        Value::Array(array) => array.iter_mut().for_each(|v| expand(v, doc)),
        _ => {}
    }
}

fn main() {
    let mjpeg = "src/hardware/mjpeg.c";
    println!("cargo:rerun-if-changed={mjpeg}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        cc::Build::new().file(mjpeg).compile("kvmd_mjpeg");
    }
    println!("cargo:rerun-if-env-changed=KVMD_API_DIR");
    let api = PathBuf::from(env::var_os("KVMD_API_DIR").unwrap_or_else(|| "../api".into()));
    let openapi = api.join("openapi.yaml");
    println!("cargo:rerun-if-changed={}", openapi.display());
    let doc: Value = serde_yaml::from_slice(&fs::read(openapi).unwrap()).unwrap();
    let mut schemas = json!({"Config":doc["components"]["schemas"]["Config"], "Health":doc["components"]["schemas"]["Health"]});
    for item in doc["paths"].as_object().unwrap().values() {
        for op in item.as_object().unwrap().values() {
            if let Some(s) = op.pointer("/requestBody/content/application~1json/schema") {
                schemas[op["operationId"].as_str().unwrap()] = s.clone();
            }
            if let Some(id) = op["operationId"].as_str() {
                for (code, res) in op["responses"].as_object().unwrap() {
                    if let Some(s) = res.pointer("/content/application~1json/schema") {
                        schemas[format!("{id}/{code}")] = s.clone();
                    }
                }
            }
        }
    }
    expand(&mut schemas, &doc);
    for file in ["panel", "ws"] {
        let path = api.join(format!("schema/{file}.schema.json"));
        println!("cargo:rerun-if-changed={}", path.display());
        let schema: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        for (name, definition) in schema["$defs"].as_object().unwrap() {
            let mut entry = definition.clone();
            expand(&mut entry, &schema);
            if matches!(
                name.as_str(),
                "ClientMessage" | "ServerMessage" | "FromPanel" | "ToPanel"
            ) {
                schemas[name] = entry;
            }
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(
        out.join("schemas.json"),
        serde_json::to_vec(&schemas).unwrap(),
    )
    .unwrap();
}
