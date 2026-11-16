use anyhow::{Context, Result};
use jsonc_parser::{cst::CstRootNode, ParseOptions};
use serde_json::Value;

/// Merge a JSON object into a JSONC document, preserving comments and untouched formatting.
/// Existing scalar and array values are replaced; object values are merged recursively.
pub fn merge_jsonc(document: &str, patch: &Value) -> Result<String> {
    let patch = patch.as_object().context("JSONC patch must be an object")?;
    let root =
        CstRootNode::parse(document, &ParseOptions::default()).context("parse JSONC document")?;
    let object = root
        .object_value()
        .context("JSONC document root must be an object")?;
    merge_object(&object, patch)?;
    Ok(root.to_string())
}

fn merge_object(
    target: &jsonc_parser::cst::CstObject,
    patch: &serde_json::Map<String, Value>,
) -> Result<()> {
    for (key, value) in patch {
        if let (Some(existing), Some(incoming)) = (target.get(key), value.as_object()) {
            if let Some(node) = existing.value() {
                if let Some(nested) = node.as_object() {
                    merge_object(&nested, incoming)?;
                    continue;
                }
            }
        }

        let cst_value = to_cst_value(value)?;
        if let Some(existing) = target.get(key) {
            existing.set_value(cst_value);
        } else {
            target.append(key, cst_value);
        }
    }
    Ok(())
}

fn to_cst_value(value: &Value) -> Result<jsonc_parser::cst::CstInputValue> {
    use jsonc_parser::cst::CstInputValue as Input;
    match value {
        Value::Null => Ok(Input::Null),
        Value::Bool(value) => Ok(Input::Bool(*value)),
        Value::Number(value) => Ok(Input::Number(value.to_string())),
        Value::String(value) => Ok(Input::String(value.clone())),
        Value::Array(values) => Ok(Input::Array(
            values
                .iter()
                .map(to_cst_value)
                .collect::<Result<Vec<_>>>()?,
        )),
        Value::Object(values) => Ok(Input::Object(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), to_cst_value(value)?)))
                .collect::<Result<Vec<_>>>()?,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_preserves_comments_and_existing_values() {
        let original = "{\n  // keep this note\n  \"existing\": 1,\n}\n";
        let result = merge_jsonc(original, &json!({"added": true})).unwrap();
        assert!(result.contains("// keep this note"));
        assert!(result.contains("\"existing\": 1"));
        assert!(result.contains("\"added\": true"));
    }

    #[test]
    fn merge_recursively_updates_and_is_idempotent() {
        let original = "{\"nested\": {\"keep\": 1, \"managed\": false}}";
        let patch = json!({"nested": {"managed": true, "new": "yes"}});
        let once = merge_jsonc(original, &patch).unwrap();
        let twice = merge_jsonc(&once, &patch).unwrap();
        assert_eq!(once, twice);
        assert!(once.contains("\"keep\": 1"));
        assert!(once.contains("\"managed\": true"));
        assert!(once.contains("\"new\": \"yes\""));
    }

    #[test]
    fn rejects_non_object_root_and_invalid_patch() {
        assert!(merge_jsonc("[]", &json!({"x": 1})).is_err());
        assert!(merge_jsonc("{", &json!({"x": 1})).is_err());
        assert!(merge_jsonc("{}", &json!([1])).is_err());
    }
}
