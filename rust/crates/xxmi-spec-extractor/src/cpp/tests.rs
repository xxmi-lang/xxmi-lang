use super::*;

#[test]
fn strip_comments_keeps_comment_markers_inside_string_literals() {
    let code = "x = L\"//\"; // gone\ny = \"/*\"; /* gone\n too */ z = 1;";
    let stripped = strip_comments(code);
    assert!(stripped.contains("L\"//\""));
    assert!(stripped.contains("\"/*\""));
    assert!(!stripped.contains("gone"));
    assert!(stripped.contains("z = 1"));
    assert_eq!(stripped.len(), code.len());
    assert_eq!(stripped.split('\n').count(), code.split('\n').count());
}

#[test]
fn strip_comments_handles_escaped_quotes_and_char_literals() {
    assert_eq!(
        strip_comments(r#"a = "\"//"; b = '"'; // c"#),
        r#"a = "\"//"; b = '"';     "#
    );
}

fn sample() -> CppFile {
    CppFile::new(
        "x.cpp",
        "static int helper(int a);
static bool Foo(const wchar_t *s,
		int n)
{
	if (helper(1)) { return true; }
	return false;
}
void Bar::Baz() const
{
	call();
}
static int Table[] = { 1, 2, /* 3, */ 4 };
#define LIST \\
	L\"a\", \\
	L\"b\"
#define ONE L\"x\"",
    )
}

#[test]
fn finds_a_definition_not_a_declaration_or_call() {
    let file = sample();
    assert!(file.function_body("Foo").unwrap().contains("return false"));
    // `helper` is only declared and called, never defined.
    assert!(file.try_function_body("helper").is_none());
}

#[test]
fn finds_qualified_member_functions_with_trailing_qualifiers() {
    assert_eq!(
        sample().function_body("Bar::Baz").unwrap().trim(),
        "call();"
    );
}

#[test]
fn names_a_missing_anchor() {
    let error = sample().function_body("Missing").unwrap_err();
    assert_eq!(error.to_string(), "x.cpp: could not find function Missing");
}

#[test]
fn reads_array_initializers_without_comments() {
    let file = sample();
    assert_eq!(
        split_top_level(file.array_initializer("Table").unwrap()),
        ["1", "2", "4"]
    );
}

#[test]
fn reads_multi_line_macros() {
    let file = sample();
    let list = file.macro_text("LIST").unwrap();
    assert_eq!(string_literals(&list), ["a", "b"]);
    assert_eq!(file.macro_text("ONE").unwrap().trim(), "L\"x\"");
    assert!(file.macro_text("NOPE").is_none());
}

#[test]
fn splits_arguments_at_top_level_commas_only() {
    assert_eq!(
        split_top_level("a, f(b, c), L\"d,e\", {1, 2}"),
        ["a", "f(b, c)", "L\"d,e\"", "{1, 2}"]
    );
}

#[test]
fn reads_wide_and_narrow_string_literals() {
    assert_eq!(string_literal("L\"key\"").as_deref(), Some("key"));
    assert_eq!(string_literal("\"a\\\"b\"").as_deref(), Some("a\"b"));
    assert_eq!(string_literal("key"), None);
}

#[test]
fn finds_calls_but_not_definitions_of_other_names() {
    let calls = find_calls(
        "GetIniInt(s, L\"a\", 0); MyGetIniInt(s, L\"b\"); x.GetIniInt(s, L\"c\");",
        "GetIniInt",
    );
    let keys: Vec<&str> = calls.iter().map(|c| c.arg(1)).collect();
    assert_eq!(keys, ["L\"a\""]);
}

#[test]
fn splits_a_flat_if_chain() {
    let blocks = if_blocks(
        "
      if (!wcscmp(key, L\"a\")) return 1;
      if (!wcscmp(key, L\"b\")) { if (x) y(); return 2; }
      else if (!wcscmp(key, L\"c\")) z();",
    );
    let conditions: Vec<&str> = blocks.iter().map(|b| b.condition).collect();
    assert_eq!(
        conditions,
        [
            "!wcscmp(key, L\"a\")",
            "!wcscmp(key, L\"b\")",
            "!wcscmp(key, L\"c\")"
        ]
    );
}
