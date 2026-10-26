//! `import.penVariables`: a pen.dev file's variables (design tokens) as FigCraft variables. pen.dev
//! keeps the `.pen` file encrypted, so this reads what its MCP `execute` → `GetVariables()` hands out:
//! `{variables: {name: {type, value}}, themes: {axis: [values]}}`, where a themed value is a list of
//! `{value, theme: {axis: value}}`. Each theme axis becomes a collection with a mode per value;
//! variables without a theme go to a "Tokens" collection with one mode. `$name` values stay
//! aliases. The result is written as the local variables JSON and read by `variable.import`, so
//! collections and variables of the same names are updated rather than doubled.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::cmd::CommandSpec;
use crate::params::get;
use crate::{Result, Session, bad, spec};

const CMD: &str = "import.penVariables";
/// The collection of variables without a theme.
const UNTHEMED: &str = "Tokens";

pub fn specs() -> Vec<CommandSpec> {
    vec![spec!(
        CMD,
        "Import variables from pen.dev",
        "{variables (pen.dev's GetVariables() output: {variables: {name: {type, value}}, themes?}, as an object or JSON text)}  each theme axis a collection with a mode per value, unthemed variables in \"Tokens\", `$name` values as aliases  → variable.import's {collections, variables, updated, warnings}",
        false,
        import
    )]
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let raw = get(p, "variables").ok_or_else(|| bad(CMD, "`variables` is required: pen.dev's GetVariables() output (read_guide pen)"))?;
    let parsed;
    let data = match raw {
        Value::String(text) => {
            parsed = serde_json::from_str::<Value>(text).map_err(|e| bad(CMD, format!("`variables` is not JSON: {e}")))?;
            &parsed
        }
        other => other,
    };
    let (rest, mut warnings) = to_local_variables(data).map_err(|e| bad(CMD, e))?;
    let mut out = s.execute("variable.import", &json!({"variables": rest}))?;
    if let Some(Value::Array(w)) = out.get("warnings") {
        warnings.extend(w.iter().filter_map(Value::as_str).map(str::to_string));
    }
    out["warnings"] = json!(warnings);
    Ok(out)
}

/// pen.dev variables as the local variables JSON (`meta.variableCollections`, `meta.variables`).
pub fn to_local_variables(data: &Value) -> std::result::Result<(Value, Vec<String>), String> {
    // GetVariables() output, or just its `variables` object.
    let vars = match data.get("variables") {
        Some(Value::Object(v)) => v,
        _ => data.as_object().ok_or("expected pen.dev's GetVariables() output")?,
    };
    let mut warnings = Vec::new();
    // Axis → its values in order: from `themes`, then whatever the values use.
    let mut axes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(Value::Object(t)) = data.get("themes") {
        for (axis, values) in t {
            let list: Vec<String> = values.as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
            axes.insert(axis.clone(), list);
        }
    }
    // Each variable's collection (its axis, or none) and values by mode name.
    struct Var {
        kind: &'static str,
        axis: Option<String>,
        values: Vec<(Option<String>, Value)>,
    }
    let mut list: Vec<(String, Var)> = Vec::new();
    for (name, def) in vars {
        let kind = match def.get("type").and_then(Value::as_str) {
            Some("color") => "COLOR",
            Some("number") => "FLOAT",
            Some("string") => "STRING",
            Some("boolean") => "BOOLEAN",
            other => {
                warnings.push(format!("`{name}`: type {} is not imported", other.unwrap_or("(none)")));
                continue;
            }
        };
        let entries: Vec<(Map<String, Value>, Value)> = match def.get("value") {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|e| Some((e.get("theme").and_then(Value::as_object).cloned().unwrap_or_default(), e.get("value")?.clone())))
                .collect(),
            Some(v) => vec![(Map::new(), v.clone())],
            None => Vec::new(),
        };
        let used: Vec<String> = {
            let mut u: Vec<String> = entries.iter().flat_map(|(t, _)| t.keys().cloned()).collect();
            u.sort();
            u.dedup();
            u
        };
        if used.len() > 1 {
            warnings.push(format!("`{name}` is themed on {}: only `{}` is kept", used.join(" and "), used.first().map_or("", String::as_str)));
        }
        let axis = used.first().cloned();
        let mut values = Vec::new();
        for (theme, v) in entries {
            let mode = axis.as_ref().and_then(|a| theme.get(a)).and_then(Value::as_str).map(str::to_string);
            if let (Some(a), Some(m)) = (&axis, &mode) {
                let modes = axes.entry(a.clone()).or_default();
                if !modes.contains(m) {
                    modes.push(m.clone());
                }
            }
            values.push((mode, v));
        }
        list.push((name.clone(), Var { kind, axis, values }));
    }
    if list.is_empty() {
        return Err("no variables found".into());
    }
    let coll_id = |axis: &Option<String>| format!("VariableCollectionId:pen:{}", axis.as_deref().unwrap_or(UNTHEMED));
    let var_id = |i: usize| format!("VariableID:pen:{i}");
    let ids: BTreeMap<&str, usize> = list.iter().enumerate().map(|(i, (n, _))| (n.as_str(), i)).collect();
    // Collections: the axes in use, then the untheme one when needed.
    let mut collections = Map::new();
    let mut members: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (i, (_, v)) in list.iter().enumerate() {
        members.entry(coll_id(&v.axis)).or_default().push(var_id(i));
    }
    let mode_id = |cid: &str, k: usize| format!("{cid}:{k}");
    let mut modes_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (axis, modes) in &axes {
        let cid = coll_id(&Some(axis.clone()));
        let Some(vids) = members.get(&cid) else { continue };
        let ms: Vec<Value> = modes.iter().enumerate().map(|(k, m)| json!({"modeId": mode_id(&cid, k), "name": m})).collect();
        collections.insert(cid.clone(), json!({"id": cid, "name": axis, "modes": ms, "defaultModeId": mode_id(&cid, 0), "variableIds": vids}));
        modes_of.insert(cid, modes.clone());
    }
    let plain = coll_id(&None);
    if let Some(vids) = members.get(&plain) {
        collections.insert(
            plain.clone(),
            json!({"id": plain, "name": UNTHEMED, "modes": [{"modeId": mode_id(&plain, 0), "name": "Default"}], "defaultModeId": mode_id(&plain, 0), "variableIds": vids}),
        );
        modes_of.insert(plain.clone(), vec!["Default".into()]);
    }
    let mut variables = Map::new();
    for (i, (name, v)) in list.iter().enumerate() {
        let cid = coll_id(&v.axis);
        let modes = modes_of.get(&cid).cloned().unwrap_or_default();
        let mut by_mode = Map::new();
        for (mode, raw) in &v.values {
            let Some(value) = value_of(v.kind, raw, &ids, name, &mut warnings) else { continue };
            match mode {
                // A value without a theme stands for every mode the themed ones leave out.
                None => {
                    for k in 0..modes.len() {
                        by_mode.entry(mode_id(&cid, k)).or_insert_with(|| value.clone());
                    }
                }
                Some(m) => {
                    if let Some(k) = modes.iter().position(|x| x == m) {
                        by_mode.insert(mode_id(&cid, k), value);
                    }
                }
            }
        }
        variables.insert(
            var_id(i),
            json!({"id": var_id(i), "name": name, "variableCollectionId": cid, "resolvedType": v.kind, "valuesByMode": by_mode, "scopes": ["ALL_SCOPES"]}),
        );
    }
    Ok((json!({"meta": {"variableCollections": collections, "variables": variables}}), warnings))
}

/// One value in the local variables JSON's form: `$name` an alias, colours as RGBA in 0-1.
fn value_of(kind: &str, raw: &Value, ids: &BTreeMap<&str, usize>, name: &str, warnings: &mut Vec<String>) -> Option<Value> {
    if let Some(target) = raw.as_str().and_then(|s| s.strip_prefix('$')) {
        return match ids.get(target) {
            Some(i) => Some(json!({"type": "VARIABLE_ALIAS", "id": format!("VariableID:pen:{i}")})),
            None => {
                warnings.push(format!("`{name}`: alias to `{target}`, which isn't among the variables, left out"));
                None
            }
        };
    }
    let value = match kind {
        "COLOR" => crate::params::color_of(raw).map(|c| json!({"r": c.r, "g": c.g, "b": c.b, "a": c.a})),
        "FLOAT" => raw.as_f64().filter(|f| f.is_finite()).map(|f| json!(f)),
        "STRING" => raw.as_str().map(|s| json!(s)),
        _ => raw.as_bool().map(|b| json!(b)),
    };
    if value.is_none() {
        warnings.push(format!("`{name}`: {raw} is not a {} value, left out", kind.to_lowercase()));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> Value {
        json!({"themes": {"mode": ["light", "dark"]}, "variables": {
            "background": {"type": "color", "value": [{"value": "#F9FAFB", "theme": {"mode": "light"}}, {"value": "#0F1420", "theme": {"mode": "dark"}}]},
            "sidebar/background": {"type": "color", "value": "$background"},
            "radius/md": {"type": "number", "value": 8},
            "font/body": {"type": "string", "value": "Inter"},
            "font/heading": {"type": "string", "value": "$font/body"},
            "dense": {"type": "boolean", "value": false},
            "half": {"type": "color", "value": [{"value": "#000000"}, {"value": "#FFFFFF", "theme": {"mode": "dark"}}]},
        }})
    }

    #[test]
    fn axes_become_collections_and_aliases_stay_aliases() {
        let mut s = Session::new();
        let r = s.execute(CMD, &json!({"variables": tokens()})).unwrap();
        assert_eq!((r["collections"].as_u64(), r["variables"].as_u64()), (Some(2), Some(7)), "{r}");
        assert_eq!(r["warnings"], json!([]), "{r}");
        let vars = &s.doc().unwrap().variables;
        let coll = |n: &str| vars.collections.iter().find(|c| c.name == n).unwrap();
        let mode = coll("mode");
        assert_eq!(mode.modes.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["light", "dark"]);
        assert_eq!(coll(UNTHEMED).modes.len(), 1);
        let var = |n: &str| vars.variables.iter().find(|v| v.name == n).unwrap();
        let bg = var("background");
        assert_eq!(bg.collection, mode.id);
        let dark = &mode.modes[1].id;
        assert!(matches!(bg.values.get(dark), Some(figcraft_doc::VariableValue::Color(c)) if c.to_hex().starts_with("0F1420")));
        assert!(matches!(var("sidebar/background").values.values().next(), Some(figcraft_doc::VariableValue::Alias(a)) if *a == bg.id));
        assert!(matches!(var("font/heading").values.values().next(), Some(figcraft_doc::VariableValue::Alias(_))));
        // A value without a theme fills the modes the themed ones leave out.
        let half = var("half");
        assert!(matches!(half.values.get(&mode.modes[0].id), Some(figcraft_doc::VariableValue::Color(c)) if c.to_hex().starts_with("000000")));
        assert!(matches!(half.values.get(dark), Some(figcraft_doc::VariableValue::Color(c)) if c.to_hex().starts_with("FFFFFF")));
    }

    #[test]
    fn importing_again_updates_rather_than_doubles() {
        let mut s = Session::new();
        s.execute(CMD, &json!({"variables": tokens()})).unwrap();
        let r = s.execute(CMD, &json!({"variables": tokens().to_string()})).unwrap();
        assert_eq!((r["collections"].as_u64(), r["variables"].as_u64(), r["updated"].as_u64()), (Some(0), Some(0), Some(7)));
        assert_eq!(s.doc().unwrap().variables.variables.len(), 7);
    }

    #[test]
    fn odd_input_is_named() {
        let mut s = Session::new();
        let r = s
            .execute(CMD, &json!({"variables": {"variables": {"a": {"type": "gradient", "value": 1}, "b": {"type": "number", "value": "$nope"}, "c": {"type": "color", "value": "nope"}}}}))
            .unwrap();
        let w = r["warnings"].to_string();
        assert!(w.contains("type gradient") && w.contains("`nope`") && w.contains("not a color"), "{w}");
        for bad in [json!({}), json!({"variables": "{"}), json!({"variables": {"variables": {}}}), json!({"variables": 3})] {
            assert!(s.execute(CMD, &bad).is_err(), "{bad}");
        }
    }
}
