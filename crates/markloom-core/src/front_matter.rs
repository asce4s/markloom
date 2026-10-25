//! YAML front matter to [`FrontMatter`](crate::FrontMatter).

use crate::ast::{FrontMatter, FrontMatterError};

/// Parse a YAML mapping into front matter.
///
/// `title` and `author` are top-level strings. `markloom.preset` is a string
/// and `markloom.style` is copied into a TOML table.
pub fn parse_front_matter(yaml: &str) -> Result<FrontMatter, FrontMatterError> {
    let value: serde_json::Value =
        serde_saphyr::from_str(yaml).map_err(|error| FrontMatterError::new(error.to_string()))?;
    let toml::Value::Table(table) = json_to_toml(value)? else {
        return Err(FrontMatterError::new("front matter must be a mapping"));
    };
    Ok(from_table(&table))
}

/// Inner YAML of a comrak front-matter blob, which includes the `---` fences.
pub(crate) fn yaml_body(raw: &str) -> String {
    let mut lines = raw.lines();
    if lines.next().is_none_or(|line| line.trim() != "---") {
        return raw.trim().to_string();
    }
    let mut body = String::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(line);
    }
    body
}

fn from_table(table: &toml::Table) -> FrontMatter {
    let title = string_field(table, "title");
    let author = string_field(table, "author");
    let markloom = table.get("markloom").and_then(toml::Value::as_table);
    let preset = markloom.and_then(|table| string_field(table, "preset"));
    let style = markloom.and_then(|table| match table.get("style") {
        Some(toml::Value::Table(style)) => Some(style.clone()),
        _ => None,
    });
    FrontMatter {
        title,
        author,
        preset,
        style,
    }
}

fn string_field(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(ToString::to_string)
}

fn json_to_toml(value: serde_json::Value) -> Result<toml::Value, FrontMatterError> {
    match value {
        serde_json::Value::Null => Err(FrontMatterError::new("null is not a TOML value")),
        serde_json::Value::Bool(flag) => Ok(toml::Value::Boolean(flag)),
        serde_json::Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                Ok(toml::Value::Integer(integer))
            } else if let Some(float) = number.as_f64() {
                Ok(toml::Value::Float(float))
            } else {
                Err(FrontMatterError::new("number does not fit TOML"))
            }
        }
        serde_json::Value::String(text) => Ok(toml::Value::String(text)),
        serde_json::Value::Array(items) => {
            let mut array = Vec::with_capacity(items.len());
            for item in items {
                array.push(json_to_toml(item)?);
            }
            Ok(toml::Value::Array(array))
        }
        serde_json::Value::Object(map) => {
            let mut table = toml::Table::new();
            for (key, item) in map {
                if item.is_null() {
                    continue;
                }
                table.insert(key, json_to_toml(item)?);
            }
            Ok(toml::Value::Table(table))
        }
    }
}
