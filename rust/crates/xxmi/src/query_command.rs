//! `xxmi query`: read-only lookups; kinds, output and exit codes.

use std::env;
use std::path::{Path, PathBuf};

use serde::Serialize;
use xxmi_core::defaults::{bundled_lookup, bundled_snapshots};
use xxmi_core::paths::{PathResolver, resolve_path};
use xxmi_core::query::library::{
    LibraryInfo, LoadedPackage, describe_snapshot_symbol, describe_symbol, list_libraries,
};
use xxmi_core::query::render::{render_spec_entry, render_symbol};
use xxmi_core::query::spec::{
    SpecEntry, SpecListItem, list_keys, list_sections, lookup_command_like, lookup_operators,
};
use xxmi_core::workspace::Workspace;
use xxmi_core::workspace::package::find_package_root;
use xxmi_core::workspace::snapshot::{SnapshotSource, SymbolSnapshot};

use crate::args::{self, OptionSpec};
use crate::error::CliError;

const OPTIONS: [OptionSpec; 3] = [
    args::flag("json"),
    args::string("package"),
    args::flag("help"),
];

const HELP: &str = concat!(
    "xxmi query <kind> [name] [--package DIR] [--json]\n",
    "\n",
    "Read-only lookups. Kinds:\n",
    "  section-kinds          every section kind the DLL knows\n",
    "  keys <section>         keys a section reads, e.g. Resource\n",
    "  command <name>         a command, function or ->member: docs, args, DLL source\n",
    "  operator <op>          an operator token with its precedence\n",
    "  symbol <qualifiedName> a library symbol, e.g. CommandList\\ZZMI\\SetTextures\n",
    "  library [name]         libraries (namespaces) of the package, with symbol counts\n",
    "\n",
    "symbol and library read the package around --package (default: the current folder), else the\n",
    "bundled ZZMI snapshot, which has locations but no doc comments. Not implemented yet: refs, outline."
);

/// Library data for `symbol` and `library`: the package around `--package`/cwd, else the bundle.
enum Library {
    Package {
        workspace: Box<Workspace>,
        root: PathBuf,
        snapshot: SymbolSnapshot,
    },
    Bundled(&'static SymbolSnapshot),
}

impl Library {
    fn load(dir: Option<&str>) -> Result<Self, CliError> {
        let start = match dir {
            Some(dir) => resolve_path(Path::new(dir)),
            None => env::current_dir().map_err(CliError::from)?,
        };

        if let Some(root) = find_package_root(&start, &PathResolver::default()) {
            let mut workspace = Workspace::new(bundled_lookup(), &[]);
            workspace.load_package(&root)?;
            let source = SnapshotSource {
                repository: String::new(),
                commit: String::new(),
            };
            let snapshot = SymbolSnapshot::build(&workspace, &root, "package".to_string(), source);
            return Ok(Self::Package {
                workspace: Box::new(workspace),
                root,
                snapshot,
            });
        }

        if let Some(dir) = dir {
            return Err(CliError::Usage(format!(
                "no d3dx.ini found at or above {dir}"
            )));
        }
        bundled_snapshots()
            .first()
            .map(Self::Bundled)
            .ok_or_else(|| {
                CliError::Usage("no package found and no bundled library snapshot".to_string())
            })
    }

    fn snapshot(&self) -> &SymbolSnapshot {
        match self {
            Self::Package { snapshot, .. } => snapshot,
            Self::Bundled(snapshot) => snapshot,
        }
    }

    fn package(&self) -> Option<LoadedPackage<'_>> {
        match self {
            Self::Package {
                workspace, root, ..
            } => Some(LoadedPackage { workspace, root }),
            Self::Bundled(_) => None,
        }
    }
}

/// Returns the process exit code.
pub fn run(raw_args: &[String]) -> Result<i32, CliError> {
    let parsed = args::parse(raw_args, &OPTIONS)?;
    if parsed.flag("help") {
        println!("{HELP}");
        return Ok(0);
    }

    let json = parsed.flag("json");
    let kind = parsed.positionals.first().map(String::as_str);
    let name = parsed.positionals.get(1).map(String::as_str);

    match kind {
        Some("section-kinds") => list(json, list_sections(bundled_lookup().spec())),
        Some("keys") => keys(json, name),
        Some(kind @ ("command" | "operator")) => spec_lookup(json, kind, name),
        Some("symbol") => symbol(json, name, parsed.string("package")),
        Some("library") => library(json, name, parsed.string("package")),
        Some("refs" | "outline") => Err(CliError::Usage(format!(
            "xxmi query {} is not implemented yet",
            kind.unwrap_or_default()
        ))),
        other => Err(CliError::Usage(format!(
            "unknown query kind {}; see xxmi query --help",
            other.unwrap_or("(none)")
        ))),
    }
}

fn print_data(json: bool, data: &impl Serialize, text: String) -> Result<i32, CliError> {
    if json {
        let pretty =
            serde_json::to_string_pretty(data).map_err(|e| CliError::Internal(e.to_string()))?;
        println!("{pretty}");
    } else {
        println!("{text}");
    }
    Ok(0)
}

fn list(json: bool, items: Vec<SpecListItem>) -> Result<i32, CliError> {
    let text = items
        .iter()
        .map(|i| {
            let xxmi = if i.xxmi { " [XXMI-only]" } else { "" };
            let summary = if i.summary.is_empty() {
                String::new()
            } else {
                format!(" - {}", i.summary)
            };
            format!("{}{xxmi}{summary}", i.name)
        })
        .collect::<Vec<_>>()
        .join("\n");
    print_data(json, &items, text)
}

fn keys(json: bool, section: Option<&str>) -> Result<i32, CliError> {
    let section = section
        .ok_or_else(|| CliError::Usage("xxmi query keys needs a section name".to_string()))?;
    let items = list_keys(bundled_lookup(), section).ok_or_else(|| {
        CliError::Internal(
            "Error: listing keys needs a known section, e.g. \"Resource\"".to_string(),
        )
    })?;
    list(json, items)
}

fn spec_lookup(json: bool, kind: &str, name: Option<&str>) -> Result<i32, CliError> {
    let name = name.ok_or_else(|| CliError::Usage(format!("xxmi query {kind} needs a name")))?;
    let lookup = bundled_lookup();
    let entries: Vec<SpecEntry> = if kind == "command" {
        lookup_command_like(lookup.spec(), lookup, name)
    } else {
        lookup_operators(lookup.spec(), name)
    };

    if entries.is_empty() {
        eprintln!("xxmi: nothing found for {kind} {name}");
        return Ok(1);
    }
    let text = entries
        .iter()
        .map(render_spec_entry)
        .collect::<Vec<_>>()
        .join("\n\n");
    print_data(json, &entries, text)
}

fn symbol(json: bool, name: Option<&str>, package: Option<&str>) -> Result<i32, CliError> {
    let name = name
        .ok_or_else(|| CliError::Usage("xxmi query symbol needs a qualified name".to_string()))?;
    let library = Library::load(package)?;

    let info = match library.package() {
        Some(pkg) => describe_symbol(&pkg, name),
        None => describe_snapshot_symbol(library.snapshot(), name),
    };
    let Some(info) = info else {
        eprintln!("xxmi: no symbol {name}");
        return Ok(1);
    };
    print_data(json, &info, render_symbol(&info))
}

fn library(json: bool, name: Option<&str>, package: Option<&str>) -> Result<i32, CliError> {
    let library = Library::load(package)?;
    let mut libraries = list_libraries(
        library.snapshot(),
        bundled_lookup(),
        library.package().as_ref(),
    );
    if let Some(name) = name {
        libraries.retain(|l| l.namespace.to_lowercase() == name.to_lowercase());
        if libraries.is_empty() {
            eprintln!("xxmi: no library {name}");
            return Ok(1);
        }
    }

    let text = libraries
        .iter()
        .map(render_library)
        .collect::<Vec<_>>()
        .join("\n");
    print_data(json, &libraries, text)
}

fn render_library(library: &LibraryInfo) -> String {
    let counts = library
        .counts
        .0
        .iter()
        .map(|(kind, count)| format!("{count} {}", kind.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let doc = library.module_doc.as_ref().map_or_else(String::new, |doc| {
        format!("\n  {}", doc.text.split('\n').next().unwrap_or_default())
    });
    format!("{}  ({counts}){doc}", library.namespace)
}
