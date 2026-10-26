//! Variable bindings for `import.pen`: pen.dev node JSON read *without* `resolveVariables` keeps
//! `$name` where a property uses a design token. Before the layers are converted, each `$name` is
//! replaced by the value of this document's variable of that name (in the modes the node's `theme`
//! chooses, inherited down the tree), and the property is bound to that variable, so the imported
//! design follows the tokens (`import.penVariables` brings them in first). The bindings and modes go
//! on the node as `_bind` (field → variable id) and `_modes` (collection → mode), which `pen`
//! turns into `boundVariables` and `variable.setMode`.
//!
//! Properties FigCraft binds with another unit or type (a font weight held as text, a line height
//! held as a ratio, an opacity in 0-1) take the value without the binding.

use std::collections::BTreeMap;

use figcraft_doc::{CollectionId, ModeId, VariableType, VariableValue, Variables};
use serde_json::{Map, Value, json};

/// Resolve and bind the `$name` values of `nodes` (one node or an array) against `vars`.
pub fn bind(nodes: &Value, vars: &Variables, warnings: &mut Vec<String>) -> Value {
    let mut cx = Cx { vars, missing: Vec::new() };
    let out = match nodes {
        Value::Array(a) => Value::Array(a.iter().map(|n| cx.node(n, &BTreeMap::new(), 0)).collect()),
        one => cx.node(one, &BTreeMap::new(), 0),
    };
    cx.missing.sort();
    cx.missing.dedup();
    if !cx.missing.is_empty() {
        let shown: Vec<&str> = cx.missing.iter().take(8).map(String::as_str).collect();
        let more = if cx.missing.len() > shown.len() { format!(" and {} more", cx.missing.len() - shown.len()) } else { String::new() };
        warnings.push(format!(
            "no variable named {}{more} in this document: import the tokens with import.penVariables first (those values are left out)",
            shown.join(", ")
        ));
    }
    out
}

struct Cx<'a> {
    vars: &'a Variables,
    missing: Vec<String>,
}

/// What a bound field takes.
#[derive(Clone, Copy, PartialEq)]
enum Want {
    Color,
    Number,
    Text,
    /// The value only: FigCraft binds this field with another unit or type.
    ValueOnly,
}

impl Cx<'_> {
    fn node(&mut self, n: &Value, modes: &BTreeMap<CollectionId, ModeId>, depth: usize) -> Value {
        let Some(o) = n.as_object() else { return n.clone() };
        if depth > crate::layer_tree::MAX_DEPTH {
            return n.clone();
        }
        let mut o = o.clone();
        let mut modes = modes.clone();
        let mut chosen = Map::new();
        if let Some(Value::Object(theme)) = o.get("theme") {
            for (axis, mode) in theme {
                let Some(m) = mode.as_str() else { continue };
                if let Some(c) = self.vars.collections.iter().find(|c| c.name == *axis)
                    && let Some(md) = c.modes.iter().find(|x| x.name == m)
                {
                    modes.insert(c.id.clone(), md.id.clone());
                    chosen.insert(c.name.clone(), json!(m));
                }
            }
        }
        let ty = o.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        let mut binds = Map::new();
        let keys: Vec<String> = o.keys().cloned().collect();
        for k in keys {
            if k == "children" || k == "theme" {
                continue;
            }
            let Some(v) = o.get(&k).cloned() else { continue };
            let new = match k.as_str() {
                "fill" if ty != "icon" => self.paints(&v, "fills", &modes, &mut binds),
                "stroke" => self.paints(&v, "strokes", &modes, &mut binds),
                "effect" => self.effects(&v, &modes, &mut binds),
                "cornerRadius" => self.corners(&v, &modes, &mut binds),
                "padding" => self.padding(&v, &modes, &mut binds),
                "strokeWidth" => self.one(&v, Want::Number, Some("strokeWeight"), &modes, &mut binds),
                "gap" | "width" | "height" | "fontSize" => self.one(&v, Want::Number, Some(&k), &modes, &mut binds),
                "fontFamily" => self.one(&v, Want::Text, Some("fontFamily"), &modes, &mut binds),
                "content" if ty == "text" => self.one(&v, Want::Text, Some("characters"), &modes, &mut binds),
                _ => self.one(&v, Want::ValueOnly, None, &modes, &mut binds),
            };
            o.insert(k, new);
        }
        if let Some(Value::Array(children)) = o.get("children") {
            let kids: Vec<Value> = children.iter().map(|c| self.node(c, &modes, depth + 1)).collect();
            o.insert("children".into(), Value::Array(kids));
        }
        if !binds.is_empty() {
            o.insert("_bind".into(), Value::Object(binds));
        }
        if !chosen.is_empty() {
            o.insert("_modes".into(), Value::Object(chosen));
        }
        Value::Object(o)
    }

    /// `v` with `$name` replaced by its value; bound to `field` when the variable fits `want`.
    fn one(&mut self, v: &Value, want: Want, field: Option<&str>, modes: &BTreeMap<CollectionId, ModeId>, binds: &mut Map<String, Value>) -> Value {
        let Some(name) = v.as_str().and_then(|s| s.strip_prefix('$')) else { return v.clone() };
        let Some(var) = self.vars.variables.iter().find(|x| x.name == name) else {
            self.missing.push(name.to_string());
            return v.clone();
        };
        let Some(value) = self.vars.resolve(&var.id, modes) else { return v.clone() };
        let fits = match want {
            Want::Color => var.kind == VariableType::Color,
            Want::Number => var.kind == VariableType::Number,
            Want::Text => var.kind == VariableType::String,
            Want::ValueOnly => false,
        };
        if let (true, Some(f)) = (fits, field) {
            binds.insert(f.to_string(), json!(var.id));
        }
        match value {
            VariableValue::Color(c) => json!(format!("#{}", c.to_hex())),
            VariableValue::Number(n) => json!(n),
            VariableValue::String(s) => json!(s),
            VariableValue::Boolean(b) => json!(b),
            VariableValue::Alias(_) => v.clone(),
        }
    }

    /// A fill or stroke (one or a list): solid colours bound as `<list>/<i>/color`, gradient stops
    /// resolved.
    fn paints(&mut self, v: &Value, list: &str, modes: &BTreeMap<CollectionId, ModeId>, binds: &mut Map<String, Value>) -> Value {
        let mut one = |cx: &mut Self, p: &Value, i: usize| -> Value {
            let field = format!("{list}/{i}/color");
            match p {
                Value::String(_) => cx.one(p, Want::Color, Some(&field), modes, binds),
                Value::Object(o) => {
                    let mut o = o.clone();
                    if let Some(c) = o.get("color").cloned() {
                        o.insert("color".into(), cx.one(&c, Want::Color, Some(&field), modes, binds));
                    }
                    if let Some(Value::Array(stops)) = o.get("colors").cloned() {
                        let stops: Vec<Value> = stops
                            .iter()
                            .map(|s| {
                                let mut s = s.as_object().cloned().unwrap_or_default();
                                for k in ["color", "position"] {
                                    if let Some(x) = s.get(k).cloned() {
                                        s.insert(k.into(), cx.one(&x, Want::ValueOnly, None, modes, binds));
                                    }
                                }
                                Value::Object(s)
                            })
                            .collect();
                        o.insert("colors".into(), Value::Array(stops));
                    }
                    for k in ["opacity", "rotation", "enabled"] {
                        if let Some(x) = o.get(k).cloned() {
                            o.insert(k.into(), cx.one(&x, Want::ValueOnly, None, modes, binds));
                        }
                    }
                    Value::Object(o)
                }
                other => other.clone(),
            }
        };
        match v {
            Value::Array(a) => Value::Array(a.iter().enumerate().map(|(i, p)| one(self, p, i)).collect()),
            p => one(self, p, 0),
        }
    }

    fn effects(&mut self, v: &Value, modes: &BTreeMap<CollectionId, ModeId>, binds: &mut Map<String, Value>) -> Value {
        let mut one = |cx: &mut Self, e: &Value, i: usize| -> Value {
            let Some(o) = e.as_object() else { return e.clone() };
            let mut o = o.clone();
            for (k, want, field) in [("color", Want::Color, "color"), ("blur", Want::Number, "radius"), ("radius", Want::Number, "radius")] {
                if let Some(x) = o.get(k).cloned() {
                    let f = format!("effects/{i}/{field}");
                    o.insert(k.into(), cx.one(&x, want, Some(&f), modes, binds));
                }
            }
            if let Some(Value::Object(off)) = o.get("offset").cloned() {
                let off: Map<String, Value> = off.iter().map(|(k, x)| (k.clone(), cx.one(x, Want::ValueOnly, None, modes, binds))).collect();
                o.insert("offset".into(), Value::Object(off));
            }
            Value::Object(o)
        };
        match v {
            Value::Array(a) => Value::Array(a.iter().enumerate().map(|(i, e)| one(self, e, i)).collect()),
            e => one(self, e, 0),
        }
    }

    fn corners(&mut self, v: &Value, modes: &BTreeMap<CollectionId, ModeId>, binds: &mut Map<String, Value>) -> Value {
        match v {
            Value::Array(a) => {
                let fields = ["topLeftRadius", "topRightRadius", "bottomRightRadius", "bottomLeftRadius"];
                Value::Array(a.iter().zip(fields).map(|(x, f)| self.one(x, Want::Number, Some(f), modes, binds)).collect())
            }
            one => self.one(one, Want::Number, Some("cornerRadius"), modes, binds),
        }
    }

    fn padding(&mut self, v: &Value, modes: &BTreeMap<CollectionId, ModeId>, binds: &mut Map<String, Value>) -> Value {
        let fields: &[&str] = match v.as_array().map(Vec::len) {
            Some(2) => &["paddingVertical", "paddingHorizontal"],
            Some(4) => &["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"],
            Some(_) => return v.clone(),
            None => return self.one(v, Want::Number, Some("padding"), modes, binds),
        };
        let a = v.as_array().cloned().unwrap_or_default();
        Value::Array(a.iter().zip(fields).map(|(x, f)| self.one(x, Want::Number, Some(f), modes, binds)).collect())
    }
}
