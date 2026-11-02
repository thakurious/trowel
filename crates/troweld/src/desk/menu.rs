//! Omarchy's menu (D35): the same JSONC tree Omarchy's own menu reads
//! (`$OMARCHY_PATH/default/omarchy/omarchy-menu.jsonc`, then
//! `~/.config/omarchy/extensions/omarchy-menu.jsonc`), with its `when:` / `checked:` guards and
//! the fonts / power-profiles providers. Ported from `shell/plugins/menu/MenuModel.js` and
//! `Menu.qml` (Omarchy 4.0.6); `scripts/menu-golden.mjs` writes the goldens the tests compare
//! against from Omarchy's own MenuModel.js.
//!
//! The panel only ever sends an item id; the action string stays in the daemon.

use std::collections::HashMap;
use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Menu,
    Action,
    Link,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Menu => "menu",
            Kind::Action => "action",
            Kind::Link => "link",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub parent: String,
    pub kind: Kind,
    pub icon: String,
    pub label: String,
    pub title: String,
    pub target: String,
    pub description: String,
    pub action: String,
    pub provider: String,
    pub aliases: Vec<String>,
    pub when: String,
    pub checked: String,
    pub order: usize,
    /// The submenu a provider row came from (`swapProviderRows`).
    pub provider_menu: String,
}

/// The merged tree: items by id, and the order they were declared in.
#[derive(Debug, Clone, Default)]
pub struct Menu {
    pub items: HashMap<String, Item>,
    pub order: Vec<String>,
}

/// `when:` and `checked:` answers, by id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Guards {
    pub when: HashMap<String, bool>,
    pub checked: HashMap<String, bool>,
}

/// A JSON object with its keys in document order: item ids are object keys and their order is
/// the menu's order. A key given twice keeps its first place and its last value, as
/// `JSON.parse` does.
struct Ordered(Vec<(String, Node)>);

enum Node {
    Object(Ordered),
    Other(Value),
}

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut out: Vec<(String, Node)> = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, Node>()? {
                    match out.iter_mut().find(|(key, _)| *key == k) {
                        Some(slot) => slot.1 = v,
                        None => out.push((k, v)),
                    }
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(V)
    }
}

/// Objects keep their key order (`Value` would sort them); anything else is a plain `Value`.
impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Node;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Node, A::Error> {
                Ordered::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(Node::Object)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> Result<Node, A::Error> {
                Value::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))
                    .map(Node::Other)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Node, E> {
                Ok(Node::Other(v.into()))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Node, E> {
                Ok(Node::Other(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Node, E> {
                Ok(Node::Other(v.into()))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Node, E> {
                Ok(Node::Other(v.into()))
            }
            fn visit_str<E>(self, v: &str) -> Result<Node, E> {
                Ok(Node::Other(v.into()))
            }
            fn visit_unit<E>(self) -> Result<Node, E> {
                Ok(Node::Other(Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}

impl Node {
    fn into_value(self) -> Value {
        match self {
            Node::Other(v) => v,
            Node::Object(o) => {
                Value::Object(o.0.into_iter().map(|(k, n)| (k, n.into_value())).collect())
            }
        }
    }
}

/// `stripJsonc`: whole-line `//` comments and trailing commas go. Like Omarchy's, it doesn't
/// look inside strings.
pub fn strip_jsonc(raw: &str) -> String {
    let mut kept = String::with_capacity(raw.len());
    for line in raw.split_inclusive('\n') {
        if !line.trim_start().starts_with("//") {
            kept.push_str(line);
        }
    }
    // `,(\s*[}\]])` → `$1`.
    let chars: Vec<char> = kept.chars().collect();
    let mut out = String::with_capacity(kept.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// `normalizeItem`.
pub fn normalize_item(id: &str, value: &Value) -> Item {
    let aliases = match value.get("aliases") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().filter(|s| !s.is_empty()).map(String::from))
            .collect(),
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    };
    let parent = if id == "root" {
        String::new()
    } else if let Some(p) = value.get("parent").and_then(Value::as_str) {
        p.to_string()
    } else if let Some((head, _)) = id.rsplit_once('.') {
        head.to_string()
    } else {
        "root".into()
    };
    let action = text(value, "action");
    let target = text(value, "target");
    let kind = if !action.is_empty() {
        Kind::Action
    } else if !target.is_empty() {
        Kind::Link
    } else {
        Kind::Menu
    };
    let label = text(value, "label");
    Item {
        id: id.into(),
        parent,
        kind,
        icon: text(value, "icon"),
        label: if label.is_empty() { id.into() } else { label },
        title: text(value, "title"),
        target,
        description: text(value, "description"),
        action,
        provider: text(value, "provider"),
        aliases,
        when: text(value, "when"),
        checked: text(value, "checked"),
        order: 0,
        provider_menu: String::new(),
    }
}

/// `parseMenuJsonc`: empty on anything that isn't an object of objects.
pub fn parse(raw: &str) -> Vec<Item> {
    let stripped = strip_jsonc(raw);
    if stripped.trim().is_empty() {
        return Vec::new();
    }
    let Ok(Ordered(top)) = serde_json::from_str::<Ordered>(&stripped) else {
        return Vec::new();
    };
    // `{ "items": { … } }` is accepted too.
    let mut source = top;
    if let Some(pos) = source
        .iter()
        .position(|(k, n)| k == "items" && matches!(n, Node::Object(_)))
    {
        if let Node::Object(Ordered(inner)) = source.swap_remove(pos).1 {
            source = inner;
        }
    }
    source
        .into_iter()
        .filter_map(|(id, node)| match node {
            Node::Object(_) => Some(normalize_item(&id, &node.into_value())),
            Node::Other(_) => None,
        })
        .collect()
}

/// `mergeMenuSources`: a later source's row replaces the earlier one (normalized rows carry every
/// field) and keeps its place; a root is added when none is declared.
pub fn merge(sources: &[Vec<Item>]) -> Menu {
    let mut menu = Menu::default();
    for src in sources {
        for entry in src {
            if entry.id.is_empty() {
                continue;
            }
            if !menu.items.contains_key(&entry.id) {
                menu.order.push(entry.id.clone());
            }
            menu.items.insert(entry.id.clone(), entry.clone());
        }
    }
    if !menu.items.contains_key("root") {
        let mut root = normalize_item("root", &json!({}));
        root.label = "Go".into();
        menu.items.insert("root".into(), root);
        menu.order.insert(0, "root".into());
    }
    for (i, id) in menu.order.iter().enumerate() {
        if let Some(e) = menu.items.get_mut(id) {
            e.order = i;
        }
    }
    menu
}

impl Menu {
    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.get(id)
    }

    /// `swapProviderRows`: replaces the rows `menu_id`'s provider gave last time.
    pub fn swap_provider_rows(&mut self, menu_id: &str, rows: Vec<Item>) {
        let items = &mut self.items;
        self.order.retain(|id| match items.get(id) {
            Some(e) if e.provider_menu == menu_id => {
                items.remove(id);
                false
            }
            Some(_) => true,
            None => false,
        });
        for mut row in rows {
            if row.id.is_empty() || self.items.contains_key(&row.id) {
                continue;
            }
            row.provider_menu = menu_id.into();
            row.order = self.order.len();
            self.order.push(row.id.clone());
            self.items.insert(row.id.clone(), row);
        }
    }

    /// `pathFor`: labels from the top, joined with ` › `.
    pub fn path_for(&self, id: &str) -> String {
        let mut labels = Vec::new();
        let mut current = self.get(id);
        let mut guard = 0;
        while let Some(e) = current {
            if e.id == "root" || guard >= 32 {
                break;
            }
            labels.insert(0, e.label.clone());
            current = self.get(&e.parent);
            guard += 1;
        }
        labels.join(" › ")
    }

    /// `depthFor`.
    pub fn depth_for(&self, id: &str) -> usize {
        let mut depth = 0;
        let mut current = self.get(id);
        let mut guard = 0;
        while let Some(e) = current {
            if e.parent.is_empty() || e.parent == "root" || guard >= 32 {
                break;
            }
            depth += 1;
            current = self.get(&e.parent);
            guard += 1;
        }
        depth
    }

    /// `isVisible`: a `when:` hides only on an explicit false; a submenu shows when one of its
    /// children does (provider menus always, their rows load later).
    pub fn is_visible(&self, guards: &Guards, entry: &Item, depth: usize) -> bool {
        if !entry.when.is_empty() && guards.when.get(&entry.id) == Some(&false) {
            return false;
        }
        if entry.kind == Kind::Action {
            return true;
        }
        if !entry.provider.is_empty() {
            return true;
        }
        if depth >= 32 {
            return false;
        }
        let target = if entry.kind == Kind::Link {
            &entry.target
        } else {
            &entry.id
        };
        self.order.iter().any(|id| {
            self.get(id).is_some_and(|child| {
                child.parent == *target && self.is_visible(guards, child, depth + 1)
            })
        })
    }

    /// `labelFor`: ` ✓` after a row whose `checked:` held.
    pub fn label_for(&self, guards: &Guards, entry: &Item) -> String {
        if !entry.checked.is_empty() && guards.checked.get(&entry.id) == Some(&true) {
            format!("{} ✓", entry.label)
        } else {
            entry.label.clone()
        }
    }
}

/// Readers a `checked:` compares against, captured once per batch (`GUARD_READERS`).
pub const GUARD_READERS: &[&str] = &[
    "omarchy-channel-current",
    "omarchy-default-agent",
    "omarchy-default-browser",
    "omarchy-default-editor",
    "omarchy-default-terminal",
    "omarchy-dns",
];

/// `guardHelpers`: package and command presence answered inside the guard process.
fn guard_helpers() -> String {
    concat!(
        "declare -A __omarchy_pkgs=()\n",
        "mapfile -t __omarchy_pkg_names < <({ pacman -Qq; LC_ALL=C pacman -Qi",
        " | awk '/^[A-Za-z]/ { provides = ($0 ~ /^Provides/); sub(/^[^:]*: /, \"\") }",
        " provides && $0 != \"None\" { n = split($0, p, \" \");",
        " for (i = 1; i <= n; i++) { sub(/[<>=].*/, \"\", p[i]); print p[i] } }'; } 2>/dev/null)\n",
        "for __omarchy_pkg in \"${__omarchy_pkg_names[@]}\"; do __omarchy_pkgs[$__omarchy_pkg]=1; done\n",
        "__omarchy_pkg_has() { [[ -n ${__omarchy_pkgs[$1]-} ]] && return 0; ",
        "[[ $1 == *[\\<\\>=]* ]] && { pacman -Q \"$1\" &>/dev/null; return; }; return 1; }\n",
        "omarchy-pkg-present() { local p; for p in \"$@\"; do __omarchy_pkg_has \"$p\" || return 1; done; return 0; }\n",
        "omarchy-pkg-missing() { local p; for p in \"$@\"; do __omarchy_pkg_has \"$p\" || return 0; done; return 1; }\n",
        "omarchy-cmd-present() { local c; for c in \"$@\"; do command -v \"$c\" &>/dev/null || return 1; done; return 0; }\n",
        "omarchy-cmd-missing() { local c; for c in \"$@\"; do command -v \"$c\" &>/dev/null || return 0; done; return 1; }\n",
    )
    .to_string()
}

fn guard_reader_slot(index: usize) -> String {
    format!("${{__omarchy_read_{index}}}")
}

fn substitute_guard_readers(expression: &str) -> String {
    let mut out = expression.to_string();
    for (i, reader) in GUARD_READERS.iter().enumerate() {
        out = out.replace(&format!("$({reader})"), &guard_reader_slot(i));
    }
    out
}

fn guard_line(id: &str, tag: char, expression: &str) -> String {
    format!(
        "if {{ {}; }} >/dev/null 2>&1; then echo {id}:{tag}:1; else echo {id}:{tag}:0; fi\n",
        substitute_guard_readers(expression)
    )
}

/// `guardScript`: one bash script for every guard, printing `<id>:<w|c>:<0|1>` per line.
pub fn guard_script(menu: &Menu) -> String {
    let mut guards = String::new();
    for id in &menu.order {
        let Some(e) = menu.get(id) else { continue };
        if !e.when.is_empty() {
            guards += &guard_line(id, 'w', &e.when);
        }
        if !e.checked.is_empty() {
            guards += &guard_line(id, 'c', &e.checked);
        }
    }
    if guards.is_empty() {
        return guards;
    }
    let mut prelude = guard_helpers();
    for (i, reader) in GUARD_READERS.iter().enumerate() {
        if guards.contains(&guard_reader_slot(i)) {
            prelude += &format!("__omarchy_read_{i}=$({reader} 2>/dev/null) || :\n");
        }
    }
    prelude + &guards
}

/// The guard script's output, read as `guardProc.onExited` does.
pub fn parse_guards(out: &str) -> Guards {
    let mut g = Guards::default();
    for line in out.lines() {
        let line = line.trim();
        let Some((rest, value)) = line.rsplit_once(':') else {
            continue;
        };
        let Some((id, tag)) = rest.rsplit_once(':') else {
            continue;
        };
        let value = value == "1";
        match tag {
            "w" => {
                g.when.insert(id.into(), value);
            }
            "c" => {
                g.checked.insert(id.into(), value);
            }
            _ => {}
        }
    }
    g
}

/// `slugify`.
pub fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in value.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c);
        } else {
            dash = true;
        }
    }
    if out.is_empty() {
        "item".into()
    } else {
        out
    }
}

/// `Util.shellQuote`.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// A bash provider from `Menu.qml`'s `providers`: rows are `label\tvalue\tcurrent`. The `apps`
/// provider is not here: the main list is Trowel's app launcher.
pub struct Provider {
    pub key: &'static str,
    pub script: &'static str,
    pub icon: &'static str,
    action: fn(&str) -> String,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        key: "fonts",
        script: "current=$(omarchy-font-current 2>/dev/null); omarchy-font-list 2>/dev/null | while read -r f; do [[ -z $f ]] && continue; printf '%s\\t%s\\t%s\\n' \"$f\" \"$f\" \"$current\"; done",
        icon: "\u{f031}",
        action: |v| format!("omarchy-font-set {}", shell_quote(v)),
    },
    Provider {
        key: "power-profiles",
        script: "current=$(powerprofilesctl get 2>/dev/null); omarchy-powerprofiles-list 2>/dev/null | while read -r p; do [[ -z $p ]] && continue; printf '%s\\t%s\\t%s\\n' \"$p\" \"$p\" \"$current\"; done",
        icon: "\u{f040b}",
        action: |v| format!("omarchy-powerprofiles-set autodetect {}", shell_quote(v)),
    },
];

pub fn provider(key: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.key == key)
}

/// `mergeProviderRows`: one action row per line, `✓` as the icon of the current value; ids that
/// slugify alike get a `-` until they're unique.
pub fn provider_rows(spec: &Provider, menu_id: &str, out: &str) -> Vec<Item> {
    let mut rows = Vec::new();
    let mut taken = std::collections::HashSet::new();
    for line in out.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        let label = parts.first().copied().unwrap_or("");
        if label.is_empty() {
            continue;
        }
        let value = parts
            .get(1)
            .copied()
            .filter(|v| !v.is_empty())
            .unwrap_or(label);
        let current = parts.get(2).copied().unwrap_or("");
        let mut id = format!("{menu_id}.{}", slugify(value));
        while !taken.insert(id.clone()) {
            id.push('-');
        }
        let mut row = normalize_item(&id, &json!({}));
        row.parent = menu_id.into();
        row.kind = Kind::Action;
        row.icon = if value == current {
            "✓".into()
        } else {
            spec.icon.into()
        };
        row.label = label.into();
        row.action = (spec.action)(value);
        rows.push(row);
    }
    rows
}

/// What the panel gets: visible rows in menu order, no actions. `label` is the raw label (search
/// matches it, as `nameSearchText` does) and `checked` says whether `labelFor` would add ` ✓`.
/// The `apps` item is left out (Trowel's main list is the app launcher).
pub fn rows(menu: &Menu, guards: &Guards, dup_of: impl Fn(&str) -> Option<String>) -> Vec<Value> {
    menu.order
        .iter()
        .filter_map(|id| menu.get(id))
        .filter(|e| e.id != "root" && e.provider != "apps")
        .filter(|e| menu.is_visible(guards, e, 0))
        .map(|e| {
            let mut row = json!({
                "id": e.id,
                "parent": e.parent,
                "kind": e.kind.as_str(),
                "label": e.label,
                "icon": e.icon,
                "path": menu.path_for(&e.id),
                "depth": menu.depth_for(&e.id),
                "order": e.order,
            });
            let o = row.as_object_mut().expect("object");
            if menu.label_for(guards, e) != e.label {
                o.insert("checked".into(), true.into());
            }
            if !e.title.is_empty() {
                o.insert("title".into(), e.title.clone().into());
            }
            if !e.description.is_empty() {
                o.insert("description".into(), e.description.clone().into());
            }
            if !e.aliases.is_empty() {
                o.insert("aliases".into(), json!(e.aliases));
            }
            if e.kind == Kind::Link {
                o.insert("target".into(), e.target.clone().into());
            }
            if let Some(dup) = (e.kind == Kind::Action)
                .then(|| dup_of(&e.action))
                .flatten()
            {
                o.insert("dup_of".into(), dup.into());
            }
            row
        })
        .collect()
}

/// `system_run menu:<id>`: the action of a visible action row.
pub fn action_for<'a>(menu: &'a Menu, guards: &Guards, id: &str) -> Option<&'a str> {
    let e = menu.get(id)?;
    (e.kind == Kind::Action && !e.action.is_empty() && menu.is_visible(guards, e, 0))
        .then_some(e.action.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/menu")
            .join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    fn fixture_menu() -> Menu {
        merge(&[
            parse(&fixture("omarchy-menu.jsonc")),
            parse(&fixture("extension.jsonc")),
        ])
    }

    fn golden() -> Value {
        serde_json::from_str(&fixture("golden.json")).unwrap()
    }

    #[test]
    fn tree_matches_menu_model() {
        let menu = fixture_menu();
        let guards = parse_guards(&fixture("guards.txt"));
        let golden = golden();
        let tree = golden["tree"].as_array().unwrap();
        assert_eq!(menu.order.len(), tree.len());
        for (id, g) in menu.order.iter().zip(tree) {
            let e = menu.get(id).unwrap();
            assert_eq!(g["id"], *id);
            assert_eq!(g["parent"], e.parent, "{id} parent");
            assert_eq!(g["kind"], e.kind.as_str(), "{id} kind");
            assert_eq!(g["label"], menu.label_for(&guards, e), "{id} label");
            assert_eq!(g["path"], menu.path_for(id), "{id} path");
            assert_eq!(g["target"], e.target, "{id} target");
            assert_eq!(g["provider"], e.provider, "{id} provider");
            assert_eq!(g["aliases"], json!(e.aliases), "{id} aliases");
            assert_eq!(g["description"], e.description, "{id} description");
            assert_eq!(g["action"], e.action, "{id} action");
            assert_eq!(g["order"], e.order, "{id} order");
            assert_eq!(g["visible"], menu.is_visible(&guards, e, 0), "{id} visible");
        }
    }

    #[test]
    fn guard_script_is_byte_identical() {
        assert_eq!(guard_script(&fixture_menu()), fixture("guard.sh"));
    }

    #[test]
    fn slugs_match() {
        let got: Vec<String> = ["Fira Code", "Fira-Code", "  JetBrains Mono NF ", "!!!"]
            .iter()
            .map(|s| slugify(s))
            .collect();
        assert_eq!(json!(got), golden()["slugs"]);
    }

    #[test]
    fn extension_overrides_and_adds() {
        let menu = fixture_menu();
        let about = menu.get("about").unwrap();
        assert_eq!(about.label, "About This Machine");
        // It keeps the place the shipped file gave it.
        assert!(
            menu.order.iter().position(|i| i == "about")
                < menu.order.iter().position(|i| i == "system")
        );
        assert_eq!(menu.get("personal.go-system").unwrap().kind, Kind::Link);
    }

    #[test]
    fn jsonc_edges() {
        assert!(parse("").is_empty());
        assert!(parse("not json").is_empty());
        assert!(parse("[1, 2]").is_empty());
        let items = parse(
            "{\n  // c\n  \"items\": {\"b\": {\"label\": \"B\"}, \"a\": {\"action\": \"x\",},},\n}",
        );
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"], "document order, not sorted");
        assert_eq!(items[1].kind, Kind::Action);
        assert_eq!(items[0].parent, "root");
        let dup = parse(r#"{"x": {"label": "one"}, "y": {}, "x": {"label": "two"}}"#);
        assert_eq!(dup.len(), 2);
        assert_eq!((dup[0].id.as_str(), dup[0].label.as_str()), ("x", "two"));
    }

    #[test]
    fn rows_hide_guarded_and_apps_and_carry_no_actions() {
        let menu = fixture_menu();
        let guards = parse_guards(&fixture("guards.txt"));
        let rows = rows(&menu, &guards, |a| {
            (a == "omarchy-system-lock").then(|| "lock-screen".into())
        });
        let ids: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert!(!ids.contains(&"apps"));
        assert!(!ids.contains(&"personal.hidden"));
        assert!(!ids.contains(&"system.hibernate"));
        assert!(ids.contains(&"install.aur"));
        assert!(rows.iter().all(|r| r.get("action").is_none()));
        let lock = rows.iter().find(|r| r["id"] == "system.lock").unwrap();
        assert_eq!(lock["dup_of"], "lock-screen");
        assert_eq!(lock["path"], "System › Lock");
        let stable = rows
            .iter()
            .find(|r| r["id"] == "update.channel.stable")
            .unwrap();
        assert_eq!(stable["checked"], true);
        assert!(rows
            .iter()
            .find(|r| r["id"] == "install.aur")
            .unwrap()
            .get("checked")
            .is_none());
        assert_eq!(
            action_for(&menu, &guards, "install.aur"),
            Some("xdg-terminal-exec --app-id=org.omarchy.terminal omarchy-pkg-aur-install")
        );
        assert_eq!(
            action_for(&menu, &guards, "install"),
            None,
            "a submenu runs nothing"
        );
        assert_eq!(action_for(&menu, &guards, "personal.hidden"), None);
        assert_eq!(action_for(&menu, &guards, "rm -rf ~"), None);
    }

    #[test]
    fn provider_rows_and_swap() {
        let mut menu = fixture_menu();
        let spec = provider("fonts").unwrap();
        let rows = provider_rows(
            spec,
            "style.font",
            "Fira Code\tFira Code\tJetBrains\nFira-Code\tFira-Code\tJetBrains\nJetBrains\tJetBrains\tJetBrains\n\n",
        );
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "style.font.fira-code",
                "style.font.fira-code-",
                "style.font.jetbrains"
            ]
        );
        assert_eq!(rows[2].icon, "✓");
        assert_eq!(rows[0].action, "omarchy-font-set 'Fira Code'");
        menu.swap_provider_rows("style.font", rows);
        assert_eq!(
            menu.path_for("style.font.jetbrains"),
            "Style › Font › JetBrains"
        );
        let again = provider_rows(spec, "style.font", "Only\tOnly\t\n");
        menu.swap_provider_rows("style.font", again);
        assert!(menu.get("style.font.jetbrains").is_none());
        assert!(menu.get("style.font.only").is_some());
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}
