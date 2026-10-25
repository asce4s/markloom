//! Recursive TOML merge used by preset inheritance.

/// Keys in `over` replace `base`. Nested tables merge instead of replacing.
pub fn deep_merge(base: &mut toml::Table, over: &toml::Table) {
    for (key, value) in over {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(base_child)), toml::Value::Table(over_child)) => {
                deep_merge(base_child, over_child);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}
