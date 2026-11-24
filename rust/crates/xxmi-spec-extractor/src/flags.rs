//! Which entries exist only in the XXMI DLL, and which the GIMI DLL lacks:
//! identity is case-insensitive like the DLL.

use std::collections::HashSet;

use crate::model::SpecBody;

fn id(parts: &[&str]) -> String {
    parts.join("\0").to_lowercase()
}

/// Calls `visit` with the identity and the `xxmi` and `gimi` flags of every flaggable entry.
fn visit_identities(body: &mut SpecBody, visit: &mut impl FnMut(String, &mut bool, &mut bool)) {
    for b in &mut body.builtin_sections {
        visit(id(&["builtin", &b.name]), &mut b.xxmi, &mut b.gimi);
    }

    for s in &mut body.sections {
        visit(id(&["section", &s.name]), &mut s.xxmi, &mut s.gimi);
        for k in &mut s.keys {
            visit(id(&["key", &s.name, &k.name]), &mut k.xxmi, &mut k.gimi);
        }
        for d in &mut s.dynamic_keys {
            visit(id(&["dynamic", &s.name, &d.kind]), &mut d.xxmi, &mut d.gimi);
        }
    }

    for c in &mut body.commands {
        visit(id(&["command", &c.kind, &c.name]), &mut c.xxmi, &mut c.gimi);
        for v in &mut c.values {
            visit(
                id(&["value", &c.kind, &c.name, &v.name]),
                &mut v.xxmi,
                &mut v.gimi,
            );
        }
    }

    for m in &mut body.resource_members {
        visit(id(&["member", &m.name]), &mut m.xxmi, &mut m.gimi);
    }

    for f in &mut body.functions {
        visit(id(&["function", &f.name]), &mut f.xxmi, &mut f.gimi);
    }

    for t in &mut body.operators.tokens {
        visit(id(&["token", &t.token]), &mut t.xxmi, &mut t.gimi);
    }
    for p in &mut body.operators.precedence {
        visit(id(&["precedence", &p.group]), &mut p.xxmi, &mut p.gimi);
    }

    for e in &mut body.enums {
        visit(id(&["enum", &e.name]), &mut e.xxmi, &mut e.gimi);
        for v in &mut e.values {
            visit(
                id(&["enum-value", &e.name, &v.name]),
                &mut v.xxmi,
                &mut v.gimi,
            );
        }
    }
}

fn identities(body: &mut SpecBody) -> HashSet<String> {
    let mut known = HashSet::new();
    visit_identities(body, &mut |key, _, _| {
        known.insert(key);
    });
    known
}

/// Sets `xxmi: true` on everything in `body` that the vanilla `baseline` doesn't have.
pub fn flag_xxmi(body: &mut SpecBody, baseline: &mut SpecBody) {
    let known = identities(baseline);
    visit_identities(body, &mut |key, xxmi, _| *xxmi = !known.contains(&key));
}

/// Sets `gimi: false` on everything in `body` that the GIMI DLL `baseline` doesn't have.
/// Returns what only the baseline has (entries removed since), as readable identities.
pub fn flag_gimi(body: &mut SpecBody, baseline: &mut SpecBody) -> Vec<String> {
    let known = identities(baseline);
    visit_identities(body, &mut |key, _, gimi| *gimi = known.contains(&key));

    let current = identities(body);
    let mut removed: Vec<String> = known
        .difference(&current)
        .map(|key| key.replace('\0', " "))
        .collect();
    removed.sort();
    removed
}
