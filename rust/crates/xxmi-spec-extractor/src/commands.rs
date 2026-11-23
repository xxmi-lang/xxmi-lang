use std::sync::LazyLock;

use regex::Regex;

use crate::cpp::{
    AnchorNotFound, CppFile, cached_regex, find_calls, if_blocks, split_top_level, string_literal,
};
use crate::dll::Extraction;
use crate::model::{MatchKind, SpecCommand, SpecCommandValue, SpecFunction, SpecResourceMember};
use crate::operators::{OperatorPatterns, group_patterns, is_function_name};
use crate::text::integer_value;

const CL: &str = "CommandList.cpp";
const INI: &str = "IniHandler.cpp";

struct Match {
    name: String,
    kind: MatchKind,
}

const KEY: &str = "key";
const VALUE: &str = r"val->c_str\(\)";
const LINE: &str = r"line->c_str\(\)";
const WORD: &str = r"(?-u:\w)+";

/// Literal comparisons against `subject` in `code`: `!wcscmp(subject, L"x")` is exact,
/// `!wcsncmp(subject, L"x", n)` and `subject.compare(0, n, L"x")` are prefix matches.
fn comparisons(code: &str, subject: &str) -> Vec<Match> {
    let pattern = cached_regex(&format!(
        r#"wcs(n)?cmp(?-u:\s)*\((?-u:\s)*{subject}(?-u:\s)*,(?-u:\s)*L"([^"]+)"|{subject}(?-u:\s)*(?:\.|->)(?-u:\s)*compare(?-u:\s)*\((?-u:\s)*0(?-u:\s)*,(?-u:\s)*[0-9]+(?-u:\s)*,(?-u:\s)*L"([^"]+)""#
    ));
    pattern
        .captures_iter(code)
        .filter_map(|m| {
            if let Some(name) = m.get(2) {
                let kind = if m.get(1).is_some() {
                    MatchKind::Prefix
                } else {
                    MatchKind::Exact
                };
                Some(Match {
                    name: name.as_str().to_string(),
                    kind,
                })
            } else {
                m.get(3).map(|name| Match {
                    name: name.as_str().to_string(),
                    kind: MatchKind::Prefix,
                })
            }
        })
        .collect()
}

fn command(m: &Match, kind: &str, source: &str, values: Vec<SpecCommandValue>) -> SpecCommand {
    SpecCommand {
        name: crate::text::trim(&m.name).to_string(),
        kind: kind.to_string(),
        match_kind: m.kind,
        values,
        source: source.to_string(),
        xxmi: false,
        arg_count: None,
        indirect: None,
        form: None,
        docs: Default::default(),
    }
}

fn values(code: &str, source: &str) -> Vec<SpecCommandValue> {
    let mut out: Vec<SpecCommandValue> = Vec::new();
    for v in comparisons(code, VALUE) {
        if !out.iter().any(|o| o.name == v.name) {
            out.push(SpecCommandValue {
                name: v.name,
                match_kind: v.kind,
                source: source.to_string(),
                xxmi: false,
            });
        }
    }
    out
}

/// Blocks that test only `val` (`snapshotinitialtextures`):
/// the keyword is the command, whatever the key is, so on a bare line it is the whole line.
fn value_commands(condition: &str, source: &str) -> Vec<SpecCommand> {
    comparisons(condition, VALUE)
        .iter()
        .map(|m| SpecCommand {
            form: Some("value"),
            ..command(m, "general", source, Vec::new())
        })
        .collect()
}

/// `ParseCommandListGeneralCommands`: one if-block per command keyword.
fn general_commands(file: &CppFile) -> Result<Vec<SpecCommand>, AnchorNotFound> {
    let function = "ParseCommandListGeneralCommands";
    let source = format!("{}:{function}", file.name);
    let mut commands: Vec<SpecCommand> = Vec::new();
    for block in if_blocks(file.function_body(function)?) {
        let arg_count = reader_arg_count(file, block.body);
        let keys = comparisons(block.condition, KEY);
        if keys.is_empty() {
            commands.extend(value_commands(block.condition, &source));
            continue;
        }

        for key in keys {
            let existing = commands.iter().position(|c| c.name == key.name);
            let vals = values(block.body, &source);
            match existing {
                Some(at) => merge_values(&mut commands[at], vals),
                None => {
                    let mut cmd = command(&key, "general", &source, vals);
                    cmd.arg_count = arg_count;
                    commands.push(cmd);
                }
            }
        }
    }
    if commands.is_empty() {
        return Err(AnchorNotFound::new(
            &file.name,
            function,
            "no key comparisons",
        ));
    }
    Ok(commands)
}

fn merge_values(existing: &mut SpecCommand, vals: Vec<SpecCommandValue>) {
    for v in vals {
        if !existing.values.iter().any(|e| e.name == v.name) {
            existing.values.push(v);
        }
    }
}

static HANDLER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"return(?-u:\s)+(Parse(?-u:\w)*)(?-u:\s)*\(").unwrap());
static READER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u:\b)CommandArgumentReader(?-u:\b)").unwrap());
static FINISHED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.Finished(?-u:\s)*\((?-u:\s)*\)").unwrap());
static COMMA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"ConsumeSeparator(?-u:\s)*\((?-u:\s)*SeparatorMode::Comma(?-u:\s)*\)").unwrap()
});

/// Argument count of a command whose parser
/// (the `return ParseX(…)` in `body`) reads its value with `CommandArgumentReader`:
/// one more than the comma separators it consumes,
/// provided it insists on consuming all input (`args.Finished()`).
fn reader_arg_count(file: &CppFile, body: &str) -> Option<i64> {
    let handler = HANDLER.captures(body)?.get(1)?.as_str();
    let code = file.try_function_body(handler)?;
    if !READER.is_match(code) || !FINISHED.is_match(code) {
        return None;
    }
    Some(COMMA.find_iter(code).count() as i64 + 1)
}

/// `ParseDrawCommand`: keyword, special values, and `ParseDrawCommandArgs(…, indirect, nargs, …)`.
fn draw_commands(file: &CppFile) -> Result<Vec<SpecCommand>, AnchorNotFound> {
    let function = "ParseDrawCommand";
    let source = format!("{}:{function}", file.name);
    let mut commands = Vec::new();
    for block in if_blocks(file.function_body(function)?) {
        let Some(key) = comparisons(block.condition, KEY).into_iter().next() else {
            continue;
        };
        let mut cmd = command(&key, "draw", &source, values(block.body, &source));
        if let Some(call) = find_calls(block.body, "ParseDrawCommandArgs").first() {
            if let Some(count) = call.args.get(4).and_then(|a| integer_value(a)) {
                cmd.arg_count = Some(count);
            }
            if call.args.get(3) == Some(&"true") {
                cmd.indirect = Some(true);
            }
        }
        commands.push(cmd);
    }
    if commands.is_empty() {
        return Err(AnchorNotFound::new(
            &file.name,
            function,
            "no draw keywords",
        ));
    }
    Ok(commands)
}

/// `ParseCommandListFlowControl`: `if `, `elif `, `else if `, `else`, `endif`.
fn flow_control(file: &CppFile) -> Result<Vec<SpecCommand>, AnchorNotFound> {
    let function = "ParseCommandListFlowControl";
    let found = comparisons(file.function_body(function)?, LINE);
    if found.is_empty() {
        return Err(AnchorNotFound::new(
            &file.name,
            function,
            "no line comparisons",
        ));
    }
    let source = format!("{}:{function}", file.name);
    Ok(found
        .iter()
        .map(|m| command(m, "flow", &source, Vec::new()))
        .collect())
}

/// `local` in `ParseCommandListVariableAssignment`, plus the `VariableFlagNames` keywords.
fn declarations(x: &Extraction) -> Result<Vec<SpecCommand>, AnchorNotFound> {
    let mut out = x.step(Vec::new(), || {
        let function = "ParseCommandListVariableAssignment";
        // XXMI compares `line.compare(0, 5, L"local")`, vanilla `name.compare(0, 6, L"local ")`.
        let body = x.dll.file(CL)?.function_body(function)?;
        let found: Vec<SpecCommand> = comparisons(body, WORD)
            .iter()
            .filter(|m| crate::text::trim(&m.name) == "local")
            .take(1)
            .map(|m| command(m, "declaration", &format!("{CL}:{function}"), Vec::new()))
            .collect();
        if found.is_empty() {
            return Err(AnchorNotFound::new(CL, function, "no `local` comparison"));
        }
        Ok(found)
    })?;

    out.extend(x.step(Vec::new(), || {
        static FLAG: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"\{(?-u:\s)*L"([^"]+)""#).unwrap());
        let table = x
            .dll
            .file("CommandList.h")?
            .array_initializer("VariableFlagNames")?;
        Ok(FLAG
            .captures_iter(table)
            .map(|m| {
                let flag = Match {
                    name: m[1].to_string(),
                    kind: MatchKind::Exact,
                };
                command(
                    &flag,
                    "declaration",
                    "CommandList.h:VariableFlagNames",
                    Vec::new(),
                )
            })
            .collect())
    })?);
    Ok(out)
}

/// `pre ` / `post ` key prefixes in `ParseCommandList`.
fn prefixes(file: &CppFile) -> Result<Vec<SpecCommand>, AnchorNotFound> {
    let function = "ParseCommandList";
    let found: Vec<Match> = comparisons(file.function_body(function)?, KEY)
        .into_iter()
        .filter(|m| m.name == "pre " || m.name == "post ")
        .collect();
    if found.is_empty() {
        return Err(AnchorNotFound::new(
            &file.name,
            function,
            "no pre/post prefixes",
        ));
    }
    let source = format!("{}:{function}", file.name);
    Ok(found
        .iter()
        .map(|m| command(m, "prefix", &source, Vec::new()))
        .collect())
}

/// The `members[]` table in a `ParseTargetMember` function:
/// `ResourceCopyTarget` for `->Region(…)` etc., `ShaderTarget` for `->cb_first_slot` etc.
/// Argument kinds are the `MemberArg::Type` names
/// (`unsigned`, `float`, `string`, `hash`, `resource`, …).
fn target_members(
    file: &CppFile,
    function: &str,
    target: &str,
) -> Result<Vec<SpecResourceMember>, AnchorNotFound> {
    static ARG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"MemberArg::Type::((?-u:\w)+)").unwrap());

    let body = CppFile::new(
        format!("{}#{function}", file.name),
        file.function_body(function)?,
    );
    let source = format!("{}:{function}", file.name);
    let mut members = Vec::new();

    for entry in split_top_level(body.array_initializer("members")?) {
        let inner = entry.strip_prefix('{').unwrap_or(entry);
        let inner = inner.strip_suffix('}').unwrap_or(inner);
        let Some(name) = string_literal(split_top_level(inner).first().copied().unwrap_or(""))
        else {
            continue;
        };
        members.push(SpecResourceMember {
            name: name.strip_prefix("->").unwrap_or(&name).to_string(),
            target: target.to_string(),
            args: ARG
                .captures_iter(inner)
                .map(|m| m[1].to_ascii_lowercase())
                .collect(),
            source: source.clone(),
            xxmi: false,
            docs: Default::default(),
        });
    }

    if members.is_empty() {
        return Err(AnchorNotFound::new(
            &file.name,
            function,
            "empty members[] table",
        ));
    }

    Ok(members)
}

/// The members of both target kinds; neither function exists in vanilla 3DMigoto.
fn all_members(x: &Extraction) -> Result<Vec<SpecResourceMember>, AnchorNotFound> {
    let cl = x.dll.file(CL)?;
    let mut members = Vec::new();
    for (function, target) in [
        ("ResourceCopyTarget::ParseTargetMember", "resource"),
        ("ShaderTarget::ParseTargetMember", "shader"),
    ] {
        members.extend(x.step_xxmi_only(Vec::new(), || target_members(cl, function, target))?);
    }
    Ok(members)
}

/// Function-call operators (`sin(…)`, `saturate(…)`, …) in the `unary_operators` group.
pub fn functions(
    file: &CppFile,
    patterns: &OperatorPatterns,
) -> Result<Vec<SpecFunction>, AnchorNotFound> {
    let group = "unary_operators";
    Ok(group_patterns(file, group, patterns)?
        .into_iter()
        .filter(|p| is_function_name(p))
        .map(|name| SpecFunction {
            name: name.to_string(),
            source: format!("{}:{group}", file.name),
            xxmi: false,
            docs: Default::default(),
        })
        .collect())
}

pub struct Commands {
    pub commands: Vec<SpecCommand>,
    pub resource_members: Vec<SpecResourceMember>,
}

pub fn extract_commands(x: &Extraction) -> Result<Commands, AnchorNotFound> {
    let cl = x.dll.file(CL)?;
    let ini = x.dll.file(INI)?;
    let general = general_commands(cl)?;
    let draw = draw_commands(cl)?;

    // ParseCommandListGeneralCommands falls through to ParseDrawCommand for anything else,
    // so a draw keyword is never also a general one; keep the general entry if both ever match.
    let fresh_draw: Vec<SpecCommand> = draw
        .into_iter()
        .filter(|d| !general.iter().any(|g| g.name == d.name))
        .collect();
    let mut commands = general;
    commands.extend(fresh_draw);
    commands.extend(flow_control(cl)?);
    commands.extend(declarations(x)?);
    commands.extend(prefixes(ini)?);
    Ok(Commands {
        commands,
        resource_members: all_members(x)?,
    })
}
