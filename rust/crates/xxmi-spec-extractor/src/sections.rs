use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::cpp::{
    AnchorNotFound, CppFile, cached_regex, find_calls, if_blocks, string_literal, string_literals,
};
use crate::dll::Extraction;
use crate::model::{SpecBuiltinSection, SpecDynamicKeys, SpecKey, SpecSection};
use crate::text::{prefix_units, trim};

const INI: &str = "IniHandler.cpp";

/// Where a section's keys are read, besides literal-section reads found by scanning.
#[derive(Clone, Copy)]
struct KeySource {
    file: &'static str,
    /// Functions whose `GetIni*(section, L"key", …)` calls read keys of this section.
    /// More than one names alternatives across DLL versions:
    /// every one that exists is read, and it's an error only if none does.
    functions: &'static [&'static str],
    /// `wchar_t *X[]` list of keys a command-list section parses outside the command list.
    whitelist: Option<&'static str>,
    /// Function that compares keys by hand: `wcscmp(key->c_str(), L"include")`.
    compares: Option<&'static str>,
}

const NONE: KeySource = KeySource {
    file: INI,
    functions: &[],
    whitelist: None,
    compares: None,
};

const fn whitelist_of(name: &'static str) -> KeySource {
    KeySource {
        whitelist: Some(name),
        ..NONE
    }
}

const fn reads(file: &'static str, functions: &'static [&'static str]) -> KeySource {
    KeySource {
        file,
        functions,
        ..NONE
    }
}

const OVERRIDE: &str = "Override.cpp";

/// The map from section name (as in the DLL's section tables) to the code that reads its keys.
/// This is the one piece of hand-kept knowledge in the extractor: when the DLL moves a parser,
/// extraction fails with the missing anchor named, and this map gets updated.
fn key_sources(section: &str) -> Vec<KeySource> {
    match section {
        "TextureOverride" => vec![
            whitelist_of("TextureOverrideIniKeys"),
            whitelist_of("TextureOverrideFuzzyMatchesIniKeys"),
            reads(INI, &["ParseTextureOverrideSections"]),
            reads(INI, &["parse_texture_override_common"]),
            reads(INI, &["parse_texture_override_fuzzy_match"]),
        ],
        "ShaderOverride" => vec![
            whitelist_of("ShaderOverrideIniKeys"),
            reads(INI, &["ParseShaderOverrideSections"]),
        ],
        "ShaderRegex" => vec![
            whitelist_of("ShaderRegexIniKeys"),
            reads(INI, &["parse_shader_regex_section_main"]),
        ],
        "CustomShader" => vec![
            whitelist_of("CustomShaderIniKeys"),
            reads(INI, &["ParseCustomShaderSections"]),
            reads(INI, &["ParseBlendState"]),
            reads(INI, &["ParseDepthStencilState"]),
            reads(INI, &["ParseRSState"]),
            reads(INI, &["ParseTopology"]),
            reads(INI, &["ParseSamplerState"]),
        ],
        // XXMI split the per-section parser (ParseResourceSection) out of the loop
        // that vanilla 3DMigoto still reads keys in (ParseResourceSections).
        "Resource" => vec![
            reads(INI, &["ParseResourceSection", "ParseResourceSections"]),
            reads(INI, &["ParseResourceInitialData"]),
        ],
        "Filter" => vec![reads(INI, &["ParseFilterSection"])],
        // A pool's resource template is parsed with ParseResourceSection(section_name, L"template").
        "Pool" => vec![
            reads(INI, &["ParseResourcePoolSection"]),
            reads(INI, &["ParseResourceSection"]),
            reads(INI, &["ParseResourceInitialData"]),
        ],
        "Key" => vec![
            reads(INI, &["RegisterPresetKeyBindings"]),
            reads(OVERRIDE, &["Override::ParseIniSection"]),
            reads(OVERRIDE, &["KeyOverrideCycle::ParseIniSection"]),
        ],
        "Preset" => vec![
            reads(INI, &["ParsePresetOverrideSections"]),
            reads(OVERRIDE, &["Override::ParseIniSection"]),
        ],
        "Include" => vec![KeySource {
            compares: Some("ParseIncludedIniFiles"),
            ..NONE
        }],
        _ => vec![],
    }
}

struct Getter {
    name: &'static str,
    value_type: &'static str,
    repeatable: bool,
}

const fn getter(name: &'static str, value_type: &'static str) -> Getter {
    Getter {
        name,
        value_type,
        repeatable: false,
    }
}

/// Functions that read ini values, and the value type each implies.
const GETTERS: &[Getter] = &[
    getter("GetIniInt", "int"),
    getter("GetIniFloat", "float"),
    getter("GetIniBool", "bool"),
    getter("GetIniString", "string"),
    getter("GetIniStringAndLog", "string"),
    getter("GetIniWstring", "string"),
    getter("GetIniHexString", "hex"),
    getter("GetIniBoolOrInt", "bool-or-int"),
    getter("GetIniBoolIntOrEnum", "bool-or-int"),
    getter("GetIniHash", "hash"),
    getter("GetIniEnum", "enum"),
    getter("GetIniEnumClass", "enum"),
    Getter {
        repeatable: true,
        ..getter("GetIniStringMultipleKeys", "string")
    },
    getter("GetPrivateProfileInt", "int"),
    getter("GetPrivateProfileString", "string"),
    getter("RegisterIniKeyBinding", "key-binding"),
    getter("IniHasKey", "unknown"),
];

struct KeyRead {
    /// The section argument as written: a literal (`L"Logging"`) or a variable.
    section: String,
    key: SpecKey,
}

fn is_names_table(arg: &str) -> bool {
    arg.strip_suffix("Names")
        .is_some_and(|stem| !stem.is_empty() && stem.bytes().all(crate::text::is_word_byte))
}

/// How far after a getter call a `ParseFormatString` still belongs to its value.
const WINDOW_UNITS: usize = 400;

/// A string handed straight to ParseFormatString is a DXGI format name (util.h DXGIFormats).
fn feeds_format_parser(code: &str, call_index: usize) -> bool {
    static NEXT_READ: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?-u:\b)GetIni(?-u:\w)*(?-u:\s)*\(").unwrap());
    static FORMAT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?-u:\b)ParseFormatString(?-u:\s)*\(").unwrap());
    let after = prefix_units(&code[call_index..], WINDOW_UNITS);
    let window = match NEXT_READ.find(&after[1..]) {
        Some(next) => &after[..next.start() + 1],
        None => after,
    };
    FORMAT.is_match(window)
}

fn key_type(getter: &Getter, args: &[&str]) -> String {
    let mut value_type = getter.value_type.to_string();
    if (value_type == "enum" || getter.name == "GetIniBoolIntOrEnum")
        && let Some(table) = args.iter().find(|a| is_names_table(a))
    {
        let prefix = if getter.name == "GetIniBoolIntOrEnum" {
            "bool-or-int|"
        } else {
            ""
        };
        value_type = format!("{prefix}enum:{table}");
    }
    value_type
}

fn new_key(name: &str, value_type: String, source: String) -> SpecKey {
    SpecKey {
        name: name.to_string(),
        value_type,
        source,
        xxmi: false,
        gimi: true,
        repeatable: None,
        docs: Default::default(),
    }
}

/// Every getter call in `code` whose key argument is a literal.
fn key_reads(code: &str, file: &str, symbol: &str) -> Vec<KeyRead> {
    let mut reads: Vec<(usize, KeyRead)> = Vec::new();
    for getter in GETTERS {
        for call in find_calls(code, getter.name) {
            let Some(key) = string_literal(call.arg(1)) else {
                continue;
            };
            let mut value_type = key_type(getter, &call.args);
            if value_type == "string" && feeds_format_parser(code, call.index) {
                value_type = "enum:DXGIFormats".to_string();
            }
            let mut spec = new_key(&key, value_type, format!("{file}:{symbol}"));
            if getter.repeatable {
                spec.repeatable = Some(true);
            }
            reads.push((
                call.index,
                KeyRead {
                    section: call.arg(0).to_string(),
                    key: spec,
                },
            ));
        }
    }
    in_source_order(reads)
}

fn in_source_order(mut reads: Vec<(usize, KeyRead)>) -> Vec<KeyRead> {
    reads.sort_by_key(|(index, _)| *index);
    reads.into_iter().map(|(_, read)| read).collect()
}

#[derive(Clone)]
struct SectionEntry {
    name: String,
    prefix: bool,
}

fn section_table(file: &CppFile, name: &str) -> Result<Vec<SectionEntry>, AnchorNotFound> {
    static ENTRY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"\{(?-u:\s)*L"([^"]+)"(?-u:\s)*,(?-u:\s)*(true|false)(?-u:\s)*\}"#).unwrap()
    });
    let entries: Vec<SectionEntry> = ENTRY
        .captures_iter(file.array_initializer(name)?)
        .map(|m| SectionEntry {
            name: m[1].to_string(),
            prefix: &m[2] == "true",
        })
        .collect();
    if entries.is_empty() {
        return Err(AnchorNotFound::new(
            &file.name,
            name,
            "no {L\"…\", bool} entries",
        ));
    }
    Ok(entries)
}

fn is_macro_name(item: &str) -> bool {
    let mut bytes = item.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_uppercase() || b == b'_')
        && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// Keys of a `wchar_t *X[] = { L"a", MACRO, NULL }` list, expanding same-file macros.
fn whitelist(file: &CppFile, name: &str) -> Result<Vec<String>, AnchorNotFound> {
    let mut keys = Vec::new();
    for item in file.array_initializer(name)?.split(',').map(trim) {
        if let Some(literal) = string_literal(item) {
            keys.push(literal);
        } else if is_macro_name(item) && item != "NULL" {
            let Some(macro_text) = file.macro_text(item) else {
                return Err(AnchorNotFound::new(
                    &file.name,
                    &format!("#define {item}"),
                    "",
                ));
            };
            keys.extend(string_literals(&macro_text));
        }
    }
    Ok(keys)
}

const LITE_GETTERS: [(&str, &str); 3] = [
    ("find_ini_setting_lite", "string"),
    ("find_ini_bool_lite", "bool"),
    ("find_ini_int_lite", "int"),
];

/// The early-startup readers (`ini_parser_lite.cpp`, used by DLLMainHook.cpp and the Injector):
/// `s = find_ini_section_lite(buf, "loader")`, then `find_ini_setting_lite(s, "target", …)`.
fn lite_reads(file: &CppFile) -> Vec<KeyRead> {
    static SECTION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"((?-u:\w)+)(?-u:\s)*=(?-u:\s)*find_ini_section_lite(?-u:\s)*\((?-u:\s)*(?-u:\w)+(?-u:\s)*,(?-u:\s)*"([^"]+)""#,
        )
        .unwrap()
    });
    let sections: HashMap<&str, &str> = SECTION
        .captures_iter(&file.code)
        .map(|m| (m.get(1).unwrap().as_str(), m.get(2).unwrap().as_str()))
        .collect();

    let mut reads = Vec::new();
    for (getter, value_type) in LITE_GETTERS {
        for call in find_calls(&file.code, getter) {
            let (Some(section), Some(key)) =
                (sections.get(call.arg(0)), string_literal(call.arg(1)))
            else {
                continue;
            };
            reads.push((
                call.index,
                KeyRead {
                    section: section.to_string(),
                    key: new_key(
                        &key,
                        value_type.to_string(),
                        format!("{}:literal", file.name),
                    ),
                },
            ));
        }
    }
    in_source_order(reads)
}

struct Duplicates {
    sections: Vec<SectionEntry>,
    keys: HashMap<String, Vec<String>>,
}

/// `whitelisted_duplicate_key`: sections whose keys may all repeat, and per-section keys.
fn duplicate_whitelist(file: &CppFile) -> Result<Duplicates, AnchorNotFound> {
    let section_cmp =
        cached_regex(r#"_wcsn?icmp(?-u:\s)*\((?-u:\s)*section(?-u:\s)*,(?-u:\s)*L"([^"]+)""#);
    let key_cmp = cached_regex(r#"_wcsicmp(?-u:\s)*\((?-u:\s)*key(?-u:\s)*,(?-u:\s)*L"([^"]+)""#);
    let mut duplicates = Duplicates {
        sections: Vec::new(),
        keys: HashMap::new(),
    };
    for block in if_blocks(file.function_body("whitelisted_duplicate_key")?) {
        let Some(m) = section_cmp.captures(block.condition) else {
            continue;
        };
        let section = m[1].to_lowercase();
        let prefix = block.condition.contains("_wcsnicmp");
        let key_names: Vec<String> = key_cmp
            .captures_iter(block.body)
            .map(|k| k[1].to_string())
            .collect();
        if key_names.is_empty() {
            duplicates.sections.push(SectionEntry {
                name: section,
                prefix,
            });
        } else {
            duplicates.keys.insert(section, key_names);
        }
    }
    Ok(duplicates)
}

fn matches_section(section: &str, entry: &SectionEntry) -> bool {
    let s = section.to_lowercase();
    let e = entry.name.to_lowercase();
    if entry.prefix {
        s.starts_with(&e)
    } else {
        s == e
    }
}

fn add_key(keys: &mut Vec<SpecKey>, key: SpecKey) {
    let Some(existing) = keys
        .iter_mut()
        .find(|k| k.name.to_lowercase() == key.name.to_lowercase())
    else {
        keys.push(key);
        return;
    };
    if existing.value_type == "unknown" && key.value_type != "unknown" {
        existing.value_type = key.value_type;
        existing.source = key.source;
    }
    if key.repeatable == Some(true) {
        existing.repeatable = Some(true);
    }
}

fn add_dynamic(list: &mut Vec<SpecDynamicKeys>, kind: &str, source: &str) {
    if !list.iter().any(|d| d.kind == kind) {
        list.push(SpecDynamicKeys {
            kind: kind.to_string(),
            source: source.to_string(),
            xxmi: false,
            gimi: true,
        });
    }
}

/// Bodies of the named function(s) that exist; an anchor error if none of them does.
fn function_bodies<'a>(
    x: &Extraction,
    file: &'a CppFile,
    functions: &[&'static str],
) -> Result<Vec<(&'static str, &'a str)>, AnchorNotFound> {
    if functions.is_empty() {
        return Ok(Vec::new());
    }
    x.step(Vec::new(), || {
        let found: Vec<(&'static str, &'a str)> = functions
            .iter()
            .filter_map(|name| file.try_function_body(name).map(|body| (*name, body)))
            .collect();
        if found.is_empty() {
            return Err(AnchorNotFound::new(
                &file.name,
                &format!("function {}", functions.join(" or ")),
                "",
            ));
        }
        Ok(found)
    })
}

/// Reads with a literal section name (`GetIniBool(L"Logging", L"calls", …)`), in any file.
fn literal_reads(x: &Extraction) -> Vec<KeyRead> {
    let mut literal = Vec::new();
    for file in x.dll.all().filter(|f| f.name.ends_with(".cpp")) {
        for read in key_reads(&file.code, &file.name, "literal") {
            if let Some(section) = string_literal(&read.section) {
                literal.push(KeyRead {
                    section,
                    key: read.key,
                });
            }
        }
        literal.extend(lite_reads(file));
    }
    literal
}

/// Everything `extract_sections` learned once, before it walks the section tables.
struct Tables {
    bare_lines: Vec<SectionEntry>,
    bare_lines_for_command_lists: bool,
    no_duplicate_warnings_for_command_lists: bool,
    duplicates: Duplicates,
    literal_reads: Vec<KeyRead>,
}

fn read_tables(x: &Extraction, ini: &CppFile) -> Result<Tables, AnchorNotFound> {
    static COMMAND_LIST_CALL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"IsCommandListSection(?-u:\s)*\(").unwrap());
    Ok(Tables {
        bare_lines: x.step(Vec::new(), || section_table(ini, "AllowLinesWithoutEquals"))?,
        bare_lines_for_command_lists: x.step(false, || {
            Ok(ini
                .function_body("DoesSectionAllowLinesWithoutEquals")?
                .contains("IsCommandListSection("))
        })?,
        no_duplicate_warnings_for_command_lists: x.step(false, || {
            Ok(COMMAND_LIST_CALL.is_match(ini.function_body("ParseIniSectionLine")?))
        })?,
        duplicates: x.step(
            Duplicates {
                sections: Vec::new(),
                keys: HashMap::new(),
            },
            || duplicate_whitelist(ini),
        )?,
        literal_reads: literal_reads(x),
    })
}

/// Keys and dynamic-key markers one section gets from its `KeySource`s.
struct SectionKeys {
    keys: Vec<SpecKey>,
    dynamic_keys: Vec<SpecDynamicKeys>,
}

fn read_section_keys(
    x: &Extraction,
    entry: &SectionEntry,
    is_command_list: bool,
) -> Result<SectionKeys, AnchorNotFound> {
    static INI_PARAM: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?-u:\b)ParseIniParamName(?-u:\s)*\(").unwrap());
    static VARIABLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\[0\](?-u:\s)*==(?-u:\s)*L'\$'").unwrap());
    let mut found = SectionKeys {
        keys: Vec::new(),
        dynamic_keys: Vec::new(),
    };
    let mut whitelisted: Vec<String> = Vec::new();

    for source in key_sources(&entry.name) {
        let Some(file) = x.step(None, || Ok(Some(x.dll.file(source.file)?)))? else {
            continue;
        };
        if let Some(list) = source.whitelist {
            for name in x.step(Vec::new(), || whitelist(file, list))? {
                whitelisted.push(name.to_lowercase());
                add_key(
                    &mut found.keys,
                    new_key(
                        &name,
                        "unknown".to_string(),
                        format!("{}:{list}", file.name),
                    ),
                );
            }
        }

        for (function, body) in function_bodies(x, file, source.functions)? {
            for read in key_reads(body, &file.name, function) {
                // Literal reads are handled with the other literal sections.
                if string_literal(&read.section).is_some() {
                    continue;
                }
                // Command-list sections only own their whitelisted keys; anything else is a command.
                if is_command_list && !whitelisted.contains(&read.key.name.to_lowercase()) {
                    continue;
                }
                add_key(&mut found.keys, read.key);
            }
            let dynamic_source = format!("{}:{function}", file.name);
            if INI_PARAM.is_match(body) {
                add_dynamic(&mut found.dynamic_keys, "ini_param", &dynamic_source);
            }
            if VARIABLE.is_match(body) {
                add_dynamic(&mut found.dynamic_keys, "variable", &dynamic_source);
            }
        }

        if let Some(function) = source.compares {
            read_comparisons(x, file, function, &mut found.keys)?;
        }
    }
    Ok(found)
}

/// Keys a parser compares by hand: `wcscmp(key->c_str(), L"include")`.
fn read_comparisons(
    x: &Extraction,
    file: &CppFile,
    function: &str,
    keys: &mut Vec<SpecKey>,
) -> Result<(), AnchorNotFound> {
    let Some(body) = x.step(None, || Ok(Some(file.function_body(function)?)))? else {
        return Ok(());
    };
    let compare = cached_regex(
        r#"wcs(?:i)?cmp(?-u:\s)*\((?-u:\s)*key->c_str\(\)(?-u:\s)*,(?-u:\s)*L"([^"]+)""#,
    );
    for m in compare.captures_iter(body) {
        add_key(
            keys,
            new_key(
                &m[1],
                "string".to_string(),
                format!("{}:{function}", file.name),
            ),
        );
    }
    Ok(())
}

pub fn extract_sections(x: &Extraction) -> Result<Vec<SpecSection>, AnchorNotFound> {
    let ini = x.dll.file(INI)?;
    let command_list = section_table(ini, "CommandListSections")?;
    let regular = section_table(ini, "RegularSections")?;
    let tables = read_tables(x, ini)?;

    let mut sections = Vec::new();
    for (table, kind, table_name) in [
        (&command_list, "commandlist", "CommandListSections"),
        (&regular, "regular", "RegularSections"),
    ] {
        for entry in table {
            let is_command_list = kind == "commandlist";
            let SectionKeys {
                mut keys,
                dynamic_keys,
            } = read_section_keys(x, entry, is_command_list)?;

            for read in &tables.literal_reads {
                if matches_section(&read.section, entry) {
                    add_key(&mut keys, read.key.clone());
                }
            }

            let lower = entry.name.to_lowercase();
            for name in tables.duplicates.keys.get(&lower).into_iter().flatten() {
                if let Some(key) = keys
                    .iter_mut()
                    .find(|k| k.name.to_lowercase() == name.to_lowercase())
                {
                    key.repeatable = Some(true);
                }
            }

            sections.push(SpecSection {
                name: entry.name.clone(),
                prefix: entry.prefix,
                kind: kind.to_string(),
                allows_bare_lines: (is_command_list && tables.bare_lines_for_command_lists)
                    || tables
                        .bare_lines
                        .iter()
                        .any(|b| b.name.to_lowercase() == lower),
                allows_duplicate_keys: (is_command_list
                    && tables.no_duplicate_warnings_for_command_lists)
                    || tables.duplicates.sections.iter().any(|d| d.name == lower),
                keys,
                removed_keys: Vec::new(),
                dynamic_keys,
                source: format!("{INI}:{table_name}"),
                xxmi: false,
                gimi: true,
                docs: Default::default(),
            });
        }
    }
    Ok(sections)
}

/// Sections the DLL parses from its own ini excerpt in `InsertBuiltInIniSections`.
pub fn extract_builtin_sections(x: &Extraction) -> Result<Vec<SpecBuiltinSection>, AnchorNotFound> {
    static HEADER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[([^\]]+)\]").unwrap());
    x.step(Vec::new(), || {
        let function = "InsertBuiltInIniSections";
        let source = format!("{INI}:{function}");
        let names: Vec<String> = string_literals(x.dll.file(INI)?.function_body(function)?)
            .iter()
            .filter_map(|line| HEADER.captures(trim(line)).map(|m| m[1].to_string()))
            .collect();
        if names.is_empty() {
            return Err(AnchorNotFound::new(INI, function, "no [section] lines"));
        }
        Ok(names
            .into_iter()
            .map(|name| SpecBuiltinSection {
                name,
                source: source.clone(),
                xxmi: false,
                gimi: true,
            })
            .collect())
    })
}
