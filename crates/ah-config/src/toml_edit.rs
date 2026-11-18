use serde_json::Value;
use toml_edit::{DocumentMut, Item, Table, Value as TomlValue};

/// Merge a JSON object patch into a TOML document while preserving comments and layout.
/// Existing tables merge recursively; scalar and array values are replaced.
pub fn merge_toml(document: &str, patch: &Value) -> anyhow::Result<String> {
    let patch = patch
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("TOML patch must be a JSON object"))?;
    let mut parsed: DocumentMut = document
        .parse()
        .map_err(|e| anyhow::anyhow!("parse TOML: {e}"))?;
    merge_table(parsed.as_table_mut(), patch)?;
    Ok(parsed.to_string())
}

fn merge_table(table: &mut Table, patch: &serde_json::Map<String, Value>) -> anyhow::Result<()> {
    for (key, value) in patch {
        if let Some(existing) = table.get_mut(key) {
            if let (Some(nested), Some(incoming)) = (existing.as_table_mut(), value.as_object()) {
                merge_table(nested, incoming)?;
                continue;
            }
            // Replace in place: the key (and the comments before it) stays, and a replaced
            // value keeps its surrounding whitespace and trailing comment.
            match (existing.as_value_mut(), to_item(value)?) {
                (Some(old), Item::Value(mut new)) => {
                    *new.decor_mut() = old.decor().clone();
                    *old = new;
                }
                (_, new) => *existing = new,
            }
            continue;
        }
        table.insert(key, to_item(value)?);
    }
    Ok(())
}

fn to_item(value: &Value) -> anyhow::Result<Item> {
    Ok(match value {
        Value::Object(values) => {
            let mut table = Table::new();
            merge_table(&mut table, values)?;
            Item::Table(table)
        }
        Value::Array(values) => Item::Value(TomlValue::Array(toml_edit::Array::from_iter(
            values
                .iter()
                .map(to_toml_value)
                .collect::<anyhow::Result<Vec<_>>>()?,
        ))),
        scalar => Item::Value(to_toml_value(scalar)?),
    })
}

fn to_toml_value(value: &Value) -> anyhow::Result<TomlValue> {
    Ok(match value {
        Value::Null => anyhow::bail!("TOML does not support null values"),
        Value::Bool(value) => TomlValue::from(*value),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                TomlValue::from(value)
            } else if let Some(value) = value.as_u64() {
                let value = i64::try_from(value)
                    .map_err(|_| anyhow::anyhow!("unsigned integer exceeds TOML range"))?;
                TomlValue::from(value)
            } else {
                TomlValue::from(
                    value
                        .as_f64()
                        .ok_or_else(|| anyhow::anyhow!("invalid number"))?,
                )
            }
        }
        Value::String(value) => TomlValue::from(value.as_str()),
        Value::Array(values) => TomlValue::Array(toml_edit::Array::from_iter(
            values
                .iter()
                .map(to_toml_value)
                .collect::<anyhow::Result<Vec<_>>>()?,
        )),
        Value::Object(_) => anyhow::bail!("nested objects are TOML tables, not values"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_preserves_comments_and_existing_values() {
        let original = "# keep this note\n[team]\nprofile = \"core\"\n";
        let result = merge_toml(
            original,
            &json!({"team": {"profile": "developer", "enabled": true}}),
        )
        .unwrap();
        assert!(result.contains("# keep this note"));
        assert!(result.contains("profile = \"developer\""));
        assert!(result.contains("enabled = true"));
    }

    #[test]
    fn merge_is_idempotent_and_preserves_unmanaged_fields() {
        let original = "[team]\nkeep = 1\nmanaged = false\n";
        let patch = json!({"team": {"managed": true, "new": "yes"}});
        let once = merge_toml(original, &patch).unwrap();
        let twice = merge_toml(&once, &patch).unwrap();
        assert_eq!(once, twice);
        let parsed: toml_edit::DocumentMut = once.parse().unwrap();
        assert_eq!(parsed["team"]["keep"].as_integer(), Some(1));
        assert_eq!(parsed["team"]["managed"].as_bool(), Some(true));
    }

    #[test]
    fn rejects_invalid_document_patch_and_null() {
        assert!(merge_toml("[", &json!({"x": 1})).is_err());
        assert!(merge_toml("", &json!([1])).is_err());
        assert!(merge_toml("", &json!({"x": null})).is_err());
    }
}
