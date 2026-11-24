//! Extractor tests:
//! the synthetic DLL pair in fixtures/spec-extractor, source-drift failures,
//! and the real DLL when its checkouts are given.

use std::fs;
use std::path::{Path, PathBuf};

use xxmi_spec_extractor::{
    DllSource, ExtractOptions, GitInfo, Spec, SpecBody, extract_body, extract_spec, serialize_spec,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/spec-extractor")
}

fn git(name: &str) -> GitInfo {
    GitInfo {
        repository: format!("https://example.invalid/{name}"),
        commit: "0".repeat(40),
        commit_date: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn extract_fixture() -> (Spec, Vec<String>) {
    let fixtures = fixtures();
    let result = extract_spec(ExtractOptions {
        dll_root: &fixtures.join("xxmi"),
        baseline_root: &fixtures.join("vanilla"),
        dll_git: Some(git("xxmi")),
        baseline_git: Some(git("vanilla")),
        gimi_root: None,
        gimi_git: None,
    })
    .unwrap();
    (result.spec, result.baseline_warnings)
}

trait Flagged {
    fn identity(&self) -> &str;
    fn is_xxmi(&self) -> bool;
    fn is_gimi(&self) -> bool;
}

macro_rules! flagged {
    ($type:ty, $field:ident) => {
        impl Flagged for $type {
            fn identity(&self) -> &str {
                &self.$field
            }
            fn is_xxmi(&self) -> bool {
                self.xxmi
            }
            fn is_gimi(&self) -> bool {
                self.gimi
            }
        }
    };
}

flagged!(xxmi_spec_extractor::SpecSection, name);
flagged!(xxmi_spec_extractor::SpecKey, name);
flagged!(xxmi_spec_extractor::SpecCommand, name);
flagged!(xxmi_spec_extractor::SpecFunction, name);
flagged!(xxmi_spec_extractor::SpecEnum, name);
flagged!(xxmi_spec_extractor::SpecOperatorToken, token);

fn flag<T: Flagged>(entries: &[T], name: &str) -> Option<bool> {
    entries
        .iter()
        .find(|e| e.identity().to_lowercase() == name.to_lowercase())
        .map(Flagged::is_xxmi)
}

fn gimi_flag<T: Flagged>(entries: &[T], name: &str) -> Option<bool> {
    entries
        .iter()
        .find(|e| e.identity().to_lowercase() == name.to_lowercase())
        .map(Flagged::is_gimi)
}

fn section<'a>(spec: &'a Spec, name: &str) -> &'a xxmi_spec_extractor::SpecSection {
    spec.body.sections.iter().find(|s| s.name == name).unwrap()
}

fn keys(spec: &Spec, name: &str) -> Vec<String> {
    section(spec, name)
        .keys
        .iter()
        .map(|k| format!("{}:{}", k.name, k.value_type))
        .collect()
}

fn command<'a>(spec: &'a Spec, name: &str) -> &'a xxmi_spec_extractor::SpecCommand {
    spec.body.commands.iter().find(|c| c.name == name).unwrap()
}

#[test]
fn matches_the_golden_output() {
    let (spec, _) = extract_fixture();
    let expected = fixtures().join("expected");
    for (name, text) in serialize_spec(&spec) {
        let golden = fs::read_to_string(expected.join(name)).unwrap();
        assert_eq!(text, golden, "{name}");
    }
}

#[test]
fn is_byte_identical_across_runs() {
    let (first, _) = extract_fixture();
    let (second, _) = extract_fixture();
    assert_eq!(serialize_spec(&first), serialize_spec(&second));
}

#[test]
fn gives_every_entry_a_source() {
    let (spec, _) = extract_fixture();
    let body = &spec.body;
    let mut sources: Vec<&str> = Vec::new();

    for s in &body.sections {
        sources.push(&s.source);
        sources.extend(s.keys.iter().map(|k| k.source.as_str()));
    }

    sources.extend(body.commands.iter().map(|c| c.source.as_str()));
    sources.extend(body.operators.tokens.iter().map(|t| t.source.as_str()));
    sources.extend(body.operators.precedence.iter().map(|p| p.source.as_str()));
    sources.extend(body.enums.iter().map(|e| e.source.as_str()));

    let well_formed = |s: &str| {
        let Some((file, symbol)) = s.split_once(':') else {
            return false;
        };
        !file.is_empty()
            && file
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_./".contains(&b))
            && symbol.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
            && symbol
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_:".contains(&b))
    };

    assert!(sources.iter().all(|s| well_formed(s)), "{sources:?}");
}

#[test]
fn flags_only_what_the_vanilla_baseline_lacks() {
    let (spec, _) = extract_fixture();
    let body = &spec.body;

    assert_eq!(flag(&body.sections, "Pool"), Some(true));
    assert_eq!(flag(&body.sections, "Resource"), Some(false));

    let resource = section(&spec, "Resource");
    assert_eq!(flag(&resource.keys, "color_space"), Some(true));
    assert_eq!(flag(&resource.keys, "type"), Some(false));

    for (name, expected) in [
        ("store", true),
        ("commandlist", true),
        ("locked", true),
        ("run", false),
        ("global", false),
        // Vanilla writes name.compare(0, 6, L"local "), XXMI line.compare(0, 5, L"local").
        ("local", false),
    ] {
        assert_eq!(flag(&body.commands, name), Some(expected), "{name}");
    }

    for (token, expected) in [("<<", true), ("&", true), ("//", false)] {
        assert_eq!(
            flag(&body.operators.tokens, token),
            Some(expected),
            "{token}"
        );
    }

    let shift = body
        .operators
        .precedence
        .iter()
        .find(|p| p.group == "shift_operators");
    assert_eq!(shift.map(|p| p.xxmi), Some(true));

    assert!(body.resource_members.iter().all(|m| m.xxmi));
    assert_eq!(flag(&body.functions, "sin"), Some(true));
    assert_eq!(flag(&body.enums, "PoolIndexTypeNames"), Some(true));
}

#[test]
fn reports_the_anchors_the_baseline_lacks() {
    assert!(extract_fixture().1.is_empty());
}

#[test]
fn extracts_resource_and_shader_target_members_with_their_argument_kinds() {
    let (spec, _) = extract_fixture();

    let member = |name: &str| {
        spec.body
            .resource_members
            .iter()
            .find(|m| m.name == name)
            .unwrap()
    };

    let match_textures = member("matchtextures");
    assert_eq!(match_textures.args, ["resource"]);
    assert_eq!(
        match_textures.source,
        "CommandList.cpp:ShaderTarget::ParseTargetMember"
    );
    assert!(match_textures.xxmi);

    assert!(member("cb_mask").args.is_empty());
    assert_eq!(match_textures.target, "shader");
    assert_eq!(member("cb_mask").target, "shader");
    assert_eq!(member("region").target, "resource");

    assert_eq!(
        member("region").source,
        "CommandList.cpp:ResourceCopyTarget::ParseTargetMember"
    );
}

#[test]
fn extracts_keywords_that_only_test_the_value() {
    let (spec, _) = extract_fixture();
    let snapshot = command(&spec, "snapshotoriginaltextures");

    assert_eq!(snapshot.kind, "general");
    assert_eq!(snapshot.match_kind, xxmi_spec_extractor::MatchKind::Exact);
    assert!(snapshot.xxmi);
    assert_eq!(snapshot.form.as_deref(), Some("value"));

    assert_eq!(
        command(&spec, "restoreoriginaltextures").form.as_deref(),
        Some("value")
    );

    assert_eq!(command(&spec, "store").form, None);
}

#[test]
fn extracts_keys_from_whitelists_getters_comparisons_and_literal_sections() {
    let (spec, _) = extract_fixture();

    assert_eq!(
        keys(&spec, "TextureOverride"),
        [
            "hash:hash",
            "match_priority:int",
            "override_byte_stride:int",
            "match_type:enum:ResourceDimensionNames",
            "match_format:enum:DXGIFormats",
        ]
    );

    assert_eq!(
        keys(&spec, "Resource"),
        [
            "max_copies_per_frame:int",
            "filename:string",
            "type:enum:CustomResourceTypeNames",
            "format:enum:DXGIFormats",
            "color_space:enum:CustomColorSpaceNames",
            "width:int",
            "data:string",
        ]
    );

    assert_eq!(
        keys(&spec, "Include"),
        [
            "include:string",
            "include_recursive:string",
            "user_config:string"
        ]
    );

    assert_eq!(
        keys(&spec, "Logging"),
        ["log_level:enum:LogVerbosityNames", "debug:bool"]
    );

    assert_eq!(
        keys(&spec, "Loader"),
        [
            "loader:string",
            "check_version:bool",
            "module:string",
            "require_admin:bool",
            "delay:int",
        ]
    );

    assert_eq!(keys(&spec, "Rendering"), ["storage_directory:string"]);
}

#[test]
fn extracts_section_flags_removed_keys_and_dynamic_keys() {
    let (spec, _) = extract_fixture();
    let removed: Vec<&str> = section(&spec, "Resource")
        .removed_keys
        .iter()
        .map(|k| k.name.as_str())
        .collect();
    assert_eq!(removed, ["mode"]);
    assert!(section(&spec, "Logging").removed_keys.is_empty());

    let key_section = section(&spec, "Key");
    let repeatable: Vec<&str> = key_section
        .keys
        .iter()
        .filter(|k| k.repeatable == Some(true))
        .map(|k| k.name.as_str())
        .collect();
    assert_eq!(repeatable, ["Key", "Back"]);
    let dynamic: Vec<&str> = key_section
        .dynamic_keys
        .iter()
        .map(|d| d.kind.as_str())
        .collect();
    assert_eq!(dynamic, ["ini_param", "variable"]);

    assert!(section(&spec, "Include").allows_duplicate_keys);
    let command_list = section(&spec, "CommandList");
    assert!(command_list.allows_bare_lines);
    assert!(command_list.allows_duplicate_keys);
    assert!(command_list.keys.is_empty());
    let resource = section(&spec, "Resource");
    assert!(!resource.allows_bare_lines);
    assert!(!resource.allows_duplicate_keys);
}

#[test]
fn extracts_commands_with_values_argument_counts_and_precedence() {
    let (spec, _) = extract_fixture();
    let body = &spec.body;
    let run: Vec<String> = command(&spec, "run")
        .values
        .iter()
        .map(|v| {
            format!(
                "{}/{}",
                v.name,
                serde_json::to_value(v.match_kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
            )
        })
        .collect();
    assert_eq!(run, ["customshader/prefix", "commandlist/prefix"]);

    let draw = command(&spec, "drawindexed");
    assert_eq!((draw.kind.as_str(), draw.arg_count), ("draw", Some(3)));
    let store = command(&spec, "store");
    assert_eq!((store.kind.as_str(), store.arg_count), ("general", Some(3)));
    assert_eq!(command(&spec, "handling").arg_count, None);
    let indirect = command(&spec, "dispatchindirect");
    assert_eq!(
        (indirect.arg_count, indirect.indirect),
        (Some(1), Some(true))
    );
    let flow = command(&spec, "if");
    assert_eq!(flow.kind, "flow");
    assert_eq!(flow.match_kind, xxmi_spec_extractor::MatchKind::Prefix);

    let builtin: Vec<&str> = body
        .builtin_sections
        .iter()
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(builtin, ["BuiltInCommandListUnbindAllRenderTargets"]);
    let prefixes: Vec<&str> = body
        .commands
        .iter()
        .filter(|c| c.kind == "prefix")
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(prefixes, ["post", "pre"]);

    let members: Vec<String> = body
        .resource_members
        .iter()
        .map(|m| format!("{}({})", m.name, m.args.join(",")))
        .collect();
    assert_eq!(
        members,
        [
            "size()",
            "region(unsigned,unsigned)",
            "cb_mask()",
            "matchtextures(resource)"
        ]
    );
    let levels: Vec<String> = body
        .operators
        .precedence
        .iter()
        .map(|p| format!("{}:{}", p.group, p.operators.join(" ")))
        .collect();
    assert_eq!(
        levels,
        [
            "unary_operators:! -",
            "multi_division_operators:* //",
            "add_subtract_operators:+",
            "shift_operators:<< >>",
            "bitwise_and_operators:&",
            "and_operators:&&",
        ]
    );
    assert_eq!(body.operators.precedence[0].associativity, "right");
    assert!(body.operators.precedence[0].unary);
}

fn with_edit(file: &str, edit: impl Fn(String) -> String) -> DllSource {
    DllSource::load_with(&fixtures().join("xxmi"), &|name, text| {
        if name == file { edit(text) } else { text }
    })
    .unwrap()
}

fn extract_error(dll: DllSource, strict: bool) -> String {
    match extract_body(dll, strict) {
        Ok(_) => panic!("extraction should fail"),
        Err(error) => error.to_string(),
    }
}

fn extract_ok(dll: DllSource, strict: bool) -> (SpecBody, Vec<String>) {
    extract_body(dll, strict).unwrap()
}

#[test]
fn fails_loudly_when_a_parser_function_is_renamed() {
    let dll = with_edit("CommandList.cpp", |t| {
        t.replace("ParseCommandListFlowControl(", "ParseFlowControlRenamed(")
    });
    assert!(extract_error(dll, true).contains("ParseCommandListFlowControl"));
}

#[test]
fn fails_loudly_when_a_table_is_renamed() {
    let dll = with_edit("IniHandler.cpp", |t| {
        t.replace("RegularSections[]", "OtherSections[]")
    });
    assert!(extract_error(dll, true).contains("RegularSections"));
}

#[test]
fn fails_loudly_when_a_macro_in_a_key_list_is_missing() {
    let dll = with_edit("IniHandler.cpp", |t| {
        t.replace(
            "#define TEXTURE_OVERRIDE_FUZZY_MATCHES",
            "#define SOMETHING_ELSE",
        )
    });
    assert!(extract_error(dll, true).contains("TEXTURE_OVERRIDE_FUZZY_MATCHES"));
}

#[test]
fn fails_loudly_when_a_component_folder_is_missing() {
    let files: Vec<_> = DllSource::load(&fixtures().join("xxmi"))
        .unwrap()
        .into_files()
        .into_iter()
        .filter(|f| !f.name.starts_with("Injector/"))
        .collect();
    let partial = || DllSource::new(files.clone(), vec!["Injector".to_string()]);
    assert!(extract_error(partial(), true).contains("Injector/: could not find directory"));
    assert_eq!(extract_ok(partial(), false).1.len(), 1);
}

#[test]
fn records_instead_of_failing_in_baseline_mode() {
    let dll = with_edit("CommandList.h", |t| {
        t.replace("VariableFlagNames", "RenamedFlagNames")
    });
    let (body, warnings) = extract_ok(dll, false);
    assert!(!body.commands.iter().any(|c| c.name == "locked"));
    assert_eq!(warnings.len(), 1);
}

#[test]
fn stays_quiet_in_baseline_mode_when_an_xxmi_only_member_table_is_absent() {
    let edit = |t: String| t.replace("ParseTargetMember(const", "RenamedMember(const");
    let (body, warnings) = extract_ok(with_edit("CommandList.cpp", edit), false);
    assert!(body.resource_members.is_empty());
    assert!(warnings.is_empty());
    assert!(extract_error(with_edit("CommandList.cpp", edit), true).contains("ParseTargetMember"));
}

/// Runs only with real checkouts: XXMI_DLL_SRC (XXMI-Libs-Package) and MIGOTO_DLL_SRC (3Dmigoto).
/// CI sets XXMI_REQUIRE_DLL_SRC where they must exist, so a missing checkout fails instead of skipping.
fn real_spec() -> Option<Spec> {
    let (Ok(dll), Ok(baseline)) = (
        std::env::var("XXMI_DLL_SRC"),
        std::env::var("MIGOTO_DLL_SRC"),
    ) else {
        assert!(
            std::env::var_os("XXMI_REQUIRE_DLL_SRC").is_none(),
            "XXMI_DLL_SRC and MIGOTO_DLL_SRC are required here"
        );
        return None;
    };
    Some(
        extract_spec(ExtractOptions {
            dll_root: Path::new(&dll),
            baseline_root: Path::new(&baseline),
            dll_git: Some(git("xxmi")),
            baseline_git: Some(git("vanilla")),
            gimi_root: None,
            gimi_git: None,
        })
        .unwrap()
        .spec,
    )
}

#[test]
fn real_dll_covers_every_section_prefix_and_operator_token_with_a_source() {
    let Some(spec) = real_spec() else { return };
    let body = &spec.body;
    assert!(body.sections.len() >= 25);
    assert!(
        body.sections
            .iter()
            .all(|s| s.source.starts_with("IniHandler.cpp:"))
    );
    assert!(
        body.operators
            .tokens
            .iter()
            .all(|t| t.source == "CommandList.cpp:operator_tokens")
    );
    for token in [
        "===", "<<", ">>", "**", "//", "&&", "||", "&", "|", "^", "~",
    ] {
        assert!(
            body.operators.tokens.iter().any(|t| t.token == token),
            "{token}"
        );
    }
}

#[test]
fn real_dll_flags_xxmi_features() {
    let Some(spec) = real_spec() else { return };
    let body = &spec.body;

    assert_eq!(flag(&body.commands, "store"), Some(true));
    assert_eq!(flag(&body.sections, "Pool"), Some(true));
    assert!(!body.resource_members.is_empty());
    assert!(body.resource_members.iter().all(|m| m.xxmi));

    for name in [
        "cb_first_slot",
        "cb_last_slot",
        "cb_slot_count",
        "srv_first_slot",
        "srv_last_slot",
        "srv_slot_count",
        "cb_size",
        "srv_mask",
        "srv_type",
    ] {
        assert!(
            body.resource_members.iter().any(|m| m.name == name),
            "{name}"
        );
    }

    // Vanilla 3DMigoto already has these; they must not be flagged.
    for name in [
        "run",
        "if",
        "endif",
        "global",
        "persist",
        "local",
        "handling",
        "drawindexed",
    ] {
        assert_eq!(flag(&body.commands, name), Some(false), "{name}");
    }

    let resource = section(&spec, "Resource");
    for key in ["type", "format", "filename", "stride"] {
        assert_eq!(flag(&resource.keys, key), Some(false), "{key}");
    }
}

fn extract_with_gimi(gimi: Option<&Path>) -> Spec {
    let fixtures = fixtures();
    extract_spec(ExtractOptions {
        dll_root: &fixtures.join("xxmi"),
        baseline_root: &fixtures.join("vanilla"),
        dll_git: Some(git("xxmi")),
        baseline_git: Some(git("vanilla")),
        gimi_root: gimi,
        gimi_git: Some(git("gimi")),
    })
    .unwrap()
    .spec
}

#[test]
fn stamps_no_gimi_flag_without_a_gimi_baseline() {
    let spec = extract_with_gimi(None);

    assert!(spec.meta.gimi.is_none());
    assert!(spec.body.sections.iter().all(|s| s.gimi));
    assert!(spec.body.commands.iter().all(|c| c.gimi));
    assert!(!serialize_spec(&spec)[0].1.contains("gimi"));
}

#[test]
fn flags_what_the_gimi_baseline_lacks() {
    // The vanilla fixture stands in for the GIMI baseline, so gimi is the inverse of xxmi.
    let spec = extract_with_gimi(Some(&fixtures().join("vanilla")));
    let body = &spec.body;

    assert_eq!(
        spec.meta.gimi.as_ref().unwrap().repository,
        git("gimi").repository
    );
    assert!(body.commands.iter().any(|c| !c.gimi));
    assert!(body.commands.iter().all(|c| c.gimi != c.xxmi));
    assert!(body.sections.iter().all(|s| s.gimi != s.xxmi));
    assert!(
        body.sections
            .iter()
            .flat_map(|s| &s.keys)
            .all(|k| k.gimi != k.xxmi)
    );
    assert!(serialize_spec(&spec)[1].1.contains("\"gimi\": false"));
}

#[test]
fn omits_gimi_true_from_the_serialized_spec() {
    let spec = extract_with_gimi(Some(&fixtures().join("xxmi")));

    assert!(spec.body.commands.iter().all(|c| c.gimi));
    assert!(
        serialize_spec(&spec)
            .iter()
            .all(|(name, text)| *name == "meta.json" || !text.contains("gimi"))
    );
}

/// Runs only with all three real checkouts; XXMI_DLL_SRC and MIGOTO_DLL_SRC are as for `real_spec`,
/// GIMI_DLL_SRC is XXMI-Libs-Package at v0.1.0 (vanilla GIMI DLL code).
fn real_gimi_spec() -> Option<Spec> {
    let (Ok(dll), Ok(baseline), Ok(gimi)) = (
        std::env::var("XXMI_DLL_SRC"),
        std::env::var("MIGOTO_DLL_SRC"),
        std::env::var("GIMI_DLL_SRC"),
    ) else {
        assert!(
            std::env::var_os("XXMI_REQUIRE_DLL_SRC").is_none(),
            "XXMI_DLL_SRC, MIGOTO_DLL_SRC and GIMI_DLL_SRC are required here"
        );
        return None;
    };
    Some(
        extract_spec(ExtractOptions {
            dll_root: Path::new(&dll),
            baseline_root: Path::new(&baseline),
            dll_git: Some(git("xxmi")),
            baseline_git: Some(git("vanilla")),
            gimi_root: Some(Path::new(&gimi)),
            gimi_git: Some(git("gimi")),
        })
        .unwrap()
        .spec,
    )
}

#[test]
fn real_dll_flags_what_the_gimi_dll_lacks() {
    let Some(spec) = real_gimi_spec() else { return };
    let body = &spec.body;

    for name in ["Pool", "Filter", "Input"] {
        assert_eq!(gimi_flag(&body.sections, name), Some(false), "{name}");
    }
    assert_eq!(gimi_flag(&body.commands, "locked"), Some(false));
    for name in ["sin", "abs", "random", "countbits"] {
        assert_eq!(gimi_flag(&body.functions, name), Some(false), "{name}");
    }
    for token in ["<<", ">>", "&", "|", "^", "~"] {
        assert_eq!(
            gimi_flag(&body.operators.tokens, token),
            Some(false),
            "{token}"
        );
    }
    assert!(body.resource_members.iter().all(|m| !m.gimi));
}

#[test]
fn real_dll_keeps_what_the_gimi_dll_has() {
    let Some(spec) = real_gimi_spec() else { return };
    let body = &spec.body;

    for name in [
        "store", "persist", "local", "global", "elif", "else if", "else", "run",
    ] {
        assert_eq!(gimi_flag(&body.commands, name), Some(true), "{name}");
    }
    for name in ["&&", "||", "==", "+", "**"] {
        assert_eq!(
            gimi_flag(&body.operators.tokens, name),
            Some(true),
            "{name}"
        );
    }

    let texture_override = section(&spec, "TextureOverride");
    for key in [
        "override_byte_stride",
        "override_vertex_count",
        "filter_index",
        "match_priority",
        "match_first_index",
    ] {
        assert_eq!(gimi_flag(&texture_override.keys, key), Some(true), "{key}");
    }
    assert_eq!(
        gimi_flag(&texture_override.keys, "uav_byte_stride"),
        Some(false)
    );

    for name in [
        "TextureOverride",
        "ShaderOverride",
        "Resource",
        "CommandList",
        "Key",
    ] {
        assert_eq!(gimi_flag(&body.sections, name), Some(true), "{name}");
    }
}
