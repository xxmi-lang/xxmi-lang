//! Which entries exist only in the XXMI DLL: identity is case-insensitive like the DLL.

use std::collections::HashSet;

use crate::model::SpecBody;

fn id(parts: &[&str]) -> String {
    parts.join("\0").to_lowercase()
}

/// Calls `visit` with the identity and the `xxmi` flag of every flaggable entry.
fn visit_identities(body: &mut SpecBody, visit: &mut impl FnMut(String, &mut bool)) {
    for b in &mut body.builtin_sections {
        visit(id(&["builtin", &b.name]), &mut b.xxmi);
    }
    for s in &mut body.sections {
        visit(id(&["section", &s.name]), &mut s.xxmi);
        for k in &mut s.keys {
            visit(id(&["key", &s.name, &k.name]), &mut k.xxmi);
        }
        for d in &mut s.dynamic_keys {
            visit(id(&["dynamic", &s.name, &d.kind]), &mut d.xxmi);
        }
    }
    for c in &mut body.commands {
        visit(id(&["command", &c.kind, &c.name]), &mut c.xxmi);
        for v in &mut c.values {
            visit(id(&["value", &c.kind, &c.name, &v.name]), &mut v.xxmi);
        }
    }
    for m in &mut body.resource_members {
        visit(id(&["member", &m.name]), &mut m.xxmi);
    }
    for f in &mut body.functions {
        visit(id(&["function", &f.name]), &mut f.xxmi);
    }
    for t in &mut body.operators.tokens {
        visit(id(&["token", &t.token]), &mut t.xxmi);
    }
    for p in &mut body.operators.precedence {
        visit(id(&["precedence", &p.group]), &mut p.xxmi);
    }
    for e in &mut body.enums {
        visit(id(&["enum", &e.name]), &mut e.xxmi);
        for v in &mut e.values {
            visit(id(&["enum-value", &e.name, &v.name]), &mut v.xxmi);
        }
    }
}

/// Sets `xxmi: true` on everything in `body` that the vanilla `baseline` doesn't have.
pub fn flag_xxmi(body: &mut SpecBody, baseline: &mut SpecBody) {
    let mut known = HashSet::new();
    visit_identities(baseline, &mut |key, _| {
        known.insert(key);
    });
    visit_identities(body, &mut |key, xxmi| *xxmi = !known.contains(&key));
}
