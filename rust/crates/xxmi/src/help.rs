//! Usage and help texts, verbatim from the TypeScript CLI (packages/cli/src/cli.ts).

use xxmi_core::rules::RULES;

pub const USAGE: &str = concat!(
    "Usage:\n",
    "  xxmi lint [paths…] [--format text|json] [--rules XM201,XM202] [--max-warnings N] [--require-package]\n",
    "  xxmi lint --stdin --stdin-path <path> [--format text|json] [--rules …] [--max-warnings N]\n",
    "  xxmi query <kind> [name] [--package DIR] [--json]\n",
    "  xxmi index <package dir> --out FILE [--name NAME]\n",
    "  xxmi lsp [--stdio]\n",
    "  xxmi capabilities [--json]\n",
    "\n",
    "Run `xxmi <command> --help` for details."
);

pub fn lint_help() -> String {
    let rules: Vec<String> = RULES
        .iter()
        .map(|r| format!("  {}  {:<7}  {}", r.id, r.severity.as_str(), r.description))
        .collect();

    format!(
        concat!(
            "xxmi lint [paths…]  Lint .ini files and folders (default: .)\n",
            "xxmi lint --stdin --stdin-path <path>  Lint the text on stdin as if it were the file <path>\n",
            "\n",
            "  --format text|json   output format (default text)\n",
            "  --rules XM201,XM202  run only these rules\n",
            "  --max-warnings N     exit 1 if there are more than N warnings\n",
            "  --require-package    skip files that are not inside a package (no d3dx.ini in any parent folder)\n",
            "  --stdin              read one file's text from stdin instead of from disk\n",
            "  --stdin-path <path>  with --stdin: the file the text stands for. Its namespace and package come\n",
            "                       from this path, which doesn't need to exist; paths in the output are this one\n",
            "  --spec <path>        use this spec.json instead of the one built in\n",
            "\n",
            "Rules:\n",
            "{}"
        ),
        rules.join("\n")
    )
}
