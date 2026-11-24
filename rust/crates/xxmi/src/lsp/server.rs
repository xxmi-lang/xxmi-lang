//! Editor state: the workspace, the open documents, the debounce timers and diagnostics.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};
use xxmi_core::lint::{
    ConfigLoader, LintOptions, RuleSetting, collect_ini_files, is_collected, lint_file,
    skipped_for_package,
};
use xxmi_core::paths::resolve_path;
use xxmi_core::profile::{self, Profile};
use xxmi_core::trace;
use xxmi_core::workspace::{FileId, Workspace};

use super::convert::publish;
use super::documents::Documents;
use super::guard::guarded;
use super::uri::{path_to_uri, uri_to_path};

const DEBOUNCE: Duration = Duration::from_millis(50);
/// Bigger files only get syntax and structure rules while typing (docs/03-lsp.md "Robustness").
const LARGE_FILE: usize = 5 * 1024 * 1024;
/// Background indexing stops here (opening a whole install with thousands of mods).
const MAX_INDEXED_FILES: usize = 5000;

const ERROR: u32 = 1;
const WARNING: u32 = 2;

pub struct Server<'a> {
    connection: &'a Connection,
    pub workspace: Workspace,
    pub documents: Documents,
    configs: ConfigLoader,
    options: LintOptions,
    /// URIs as the client sent them: drive-letter case and escaping must round-trip.
    uri_by_path: HashMap<PathBuf, String>,
    /// Documents with a change waiting out the debounce, and when it is due.
    pending: HashMap<String, Instant>,
    last_change: HashMap<String, Instant>,
    roots: Vec<PathBuf>,
    supports_configuration: bool,
    configuration_request: Option<RequestId>,
    next_request: i32,
    /// Set by `XXMI_LSP_INJECT_PANIC`, so the tests can make a handler panic.
    pub inject_panics: bool,
}

impl<'a> Server<'a> {
    pub fn new(
        connection: &'a Connection,
        roots: Vec<PathBuf>,
        supports_configuration: bool,
    ) -> Self {
        let options = LintOptions::bundled();
        let configs = ConfigLoader::lenient();
        let mut workspace = Workspace::new(options.lookup, &options.snapshots);
        workspace.set_configs(configs.clone());

        Self {
            connection,
            workspace,
            documents: Documents::default(),
            configs,
            options,
            uri_by_path: HashMap::new(),
            pending: HashMap::new(),
            last_change: HashMap::new(),
            roots,
            supports_configuration,
            configuration_request: None,
            next_request: 0,
            inject_panics: std::env::var_os("XXMI_LSP_INJECT_PANIC").is_some(),
        }
    }

    pub fn send(&self, notification: Notification) {
        // A closed channel means the client is gone; the loop ends on its own.
        let _ = self
            .connection
            .sender
            .send(Message::Notification(notification));
    }

    fn log(&self, level: u32, message: String) {
        let params = json!({ "type": level, "message": message });
        self.send(Notification::new("window/logMessage".to_string(), params));
    }

    pub fn to_uri(&self, path: &Path) -> String {
        self.uri_by_path
            .get(path)
            .cloned()
            .unwrap_or_else(|| path_to_uri(path))
    }

    /// The loaded file of an open or indexed document, after any change still being debounced.
    pub fn file_id(&mut self, uri: &str) -> Option<FileId> {
        if self.pending.contains_key(uri) {
            self.update(uri);
        }
        self.workspace.file_id(&uri_to_path(uri))
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.pending.values().min().copied()
    }

    pub fn run_due(&mut self) {
        let now = Instant::now();
        let due: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(uri, _)| uri.clone())
            .collect();
        for uri in due {
            if guarded(|| self.update(&uri)).is_none() {
                eprintln!("xxmi-lsp: panic while linting {uri}");
                self.recover(&uri);
            }
        }
    }

    /// After a panic the document's cached state can't be trusted: drop it and rebuild it from
    /// the last text the client sent. A document that panics again stays unloaded until its
    /// next change.
    pub fn recover(&mut self, uri: &str) {
        self.pending.remove(uri);
        let path = uri_to_path(uri);

        let resynced = guarded(|| {
            self.workspace.close_document(&path);
            self.update(uri);
        });
        if resynced.is_none() {
            eprintln!("xxmi-lsp: {uri} panics again; leaving it unloaded");
            let _ = guarded(|| self.workspace.close_document(&path));
            self.send(publish(uri, &[]));
        }
    }

    fn fallback_root(&self, path: &Path) -> PathBuf {
        let text = path.to_string_lossy();
        let nearest = self
            .roots
            .iter()
            .filter(|root| text.starts_with(root.to_string_lossy().as_ref()))
            .max_by_key(|root| root.as_os_str().len());
        nearest
            .cloned()
            .unwrap_or_else(|| path.parent().unwrap_or(path).to_path_buf())
    }

    /// Re-reads an open document into the workspace.
    fn refresh(&mut self, uri: &str) -> bool {
        self.pending.remove(uri);
        let Some(text) = self.documents.get(uri).map(|d| d.text.clone()) else {
            return false;
        };

        let path = uri_to_path(uri);
        self.uri_by_path.insert(path.clone(), uri.to_string());
        let root = self.fallback_root(&path);
        let opened = trace::span("index", || self.workspace.open_document(&path, text, &root));
        match opened {
            Ok(_) => true,
            Err(error) => {
                self.log(ERROR, format!("xxmi-lsp: {error}"));
                false
            }
        }
    }

    fn update(&mut self, uri: &str) {
        if self.refresh(uri) {
            self.publish(false);
        }
    }

    pub fn did_open(&mut self, uri: String, text: String) {
        self.documents.open(uri.clone(), text);
        self.last_change.insert(uri.clone(), Instant::now());
        self.update(&uri);
    }

    /// A change after a quiet period is linted at once (an agent's single edit gets diagnostics
    /// while it's still looking); a burst of keystrokes is linted at the end of the debounce.
    pub fn did_change(&mut self, uri: &str, changes: Vec<super::documents::Change>) {
        if !trace::span("apply_change", || self.documents.change(uri, changes)) {
            return;
        }

        let now = Instant::now();
        let last = self.last_change.insert(uri.to_string(), now);
        let quiet = last.is_none_or(|at| now.duration_since(at) > DEBOUNCE);
        if quiet && !self.pending.contains_key(uri) {
            self.update(uri);
        } else {
            self.pending.insert(uri.to_string(), now + DEBOUNCE);
        }
    }

    pub fn did_save(&mut self, uri: &str) {
        if self.refresh(uri) {
            self.publish(true);
        }
    }

    pub fn did_close(&mut self, uri: &str) {
        self.pending.remove(uri);
        self.documents.close(uri);
        self.last_change.remove(uri);
        self.send(publish(uri, &[]));

        self.workspace.close_document(&uri_to_path(uri));
        self.publish(false);
    }

    /// One notification is one batch with a single refresh: unpacking an archive reports
    /// hundreds of created files at once.
    pub fn files_changed(&mut self, changed: Vec<(String, bool)>) {
        let mut seen = HashSet::new();
        for (uri, created) in changed {
            // The editor buffer wins; a repeated uri is read from disk only once.
            if self.documents.get(&uri).is_some() || !seen.insert(uri.clone()) {
                continue;
            }

            let path = uri_to_path(&uri);
            if self.workspace.reload(&path).is_none() && created {
                self.index_created(&path);
            }
        }
        self.publish(false);
    }

    /// Loads a file the startup index would have loaded.
    fn index_created(&mut self, path: &Path) {
        let full = resolve_path(path);
        let root = self
            .roots
            .iter()
            .map(|root| resolve_path(root))
            .filter(|root| full.starts_with(root))
            .max_by_key(|root| root.as_os_str().len());

        if let Some(root) = root
            && is_collected(&root, &full)
            && full.is_file()
        {
            self.index_file(&full, &root);
        }
    }

    pub fn publish(&mut self, full: bool) {
        let uris: Vec<(String, usize)> = self
            .documents
            .all()
            .iter()
            .map(|d| (d.uri.clone(), d.text.len()))
            .collect();

        for (uri, bytes) in uris {
            let path = uri_to_path(&uri);
            let Some(id) = self.workspace.file_id(&path) else {
                continue;
            };
            let package_root = self.workspace.package_root(&self.workspace.files()[id]);
            if skipped_for_package(&self.options, package_root) {
                self.send(publish(&uri, &[]));
                continue;
            }
            let large = !full && bytes > LARGE_FILE && self.utf16_len(&uri) > LARGE_FILE;
            self.options.rule_prefixes = large.then(|| vec!["XM0", "XM1"]);

            let linted = trace::span("lint", || {
                lint_file(id, &self.workspace, &self.options, &self.configs)
            });
            let diagnostics = match linted {
                Ok(diagnostics) => diagnostics,
                Err(error) => {
                    self.log(
                        ERROR,
                        format!("xxmi-lsp: lint failed for {}: {error}", path.display()),
                    );
                    Vec::new()
                }
            };
            trace::span("publish", || self.send(publish(&uri, &diagnostics)));
        }
        self.report_config_problems();
    }

    /// A broken xxmi.toml is read once, so this reports it once per session.
    fn report_config_problems(&self) {
        for problem in self.configs.take_problems() {
            let message = format!("xxmi-lsp: {problem}; linting with the defaults instead");
            self.log(WARNING, message);
        }
    }

    fn utf16_len(&self, uri: &str) -> usize {
        self.documents
            .get(uri)
            .map_or(0, |d| d.text.encode_utf16().count())
    }

    /// Asks the client for the `xxmi` settings; the answer arrives as a response.
    pub fn request_settings(&mut self) {
        if !self.supports_configuration {
            return;
        }
        self.next_request += 1;
        let id = RequestId::from(self.next_request);
        self.configuration_request = Some(id.clone());

        let params = json!({ "items": [{ "section": "xxmi" }] });
        let request = Request::new(id, "workspace/configuration".to_string(), params);
        let _ = self.connection.sender.send(Message::Request(request));
    }

    pub fn settings_response(&mut self, id: &RequestId, result: Option<Value>) {
        if self.configuration_request.as_ref() != Some(id) {
            return;
        }
        self.configuration_request = None;
        self.options.overrides = rule_overrides(result.as_ref());
        self.options.require_package = require_package(result.as_ref());
        self.workspace
            .set_profile_override(mi_package(result.as_ref()));
        self.publish(false);
    }

    /// Opens the rest of the workspace folders, so references and rename see files that aren't open.
    pub fn index_roots(&mut self) {
        let mut indexed = 0;
        for root in self.roots.clone() {
            let Ok(paths) = collect_ini_files(&root) else {
                continue;
            };
            for path in paths {
                if indexed >= MAX_INDEXED_FILES {
                    let message = format!(
                        "xxmi-lsp: indexed the first {MAX_INDEXED_FILES} .ini files; open a smaller folder for full coverage."
                    );
                    self.log(WARNING, message);
                    return;
                }
                if self.workspace.file_id(&path).is_some() {
                    continue;
                }
                indexed += usize::from(self.index_file(&path, &root));
            }
        }
        self.publish(false);
    }

    /// Whether the file was added; DISABLED folders and the like are skipped.
    fn index_file(&mut self, path: &Path, root: &Path) -> bool {
        let added = match self.workspace.excluded(path, Some(root)) {
            Ok(true) => return false,
            Ok(false) => self.workspace.add_target(path, root).map(|_| ()),
            Err(error) => Err(error),
        };
        if let Err(error) = &added {
            self.log(
                WARNING,
                format!("xxmi-lsp: skipped {}: {error}", path.display()),
            );
        }
        added.is_ok()
    }
}

/// `xxmi.requirePackage` of the settings answer; off unless it is `true`.
fn require_package(result: Option<&Value>) -> bool {
    result
        .and_then(|settings| settings.get(0))
        .and_then(|section| section.get("requirePackage"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// `xxmi.miPackage` of the settings answer: a profile id, or `auto` (and anything unknown) to
/// detect the profile of each package.
fn mi_package(result: Option<&Value>) -> Option<&'static Profile> {
    result
        .and_then(|settings| settings.get(0))
        .and_then(|section| section.get("miPackage"))
        .and_then(Value::as_str)
        .and_then(profile::by_id)
}

/// `xxmi.rules` of the settings answer: rule id to setting name.
fn rule_overrides(result: Option<&Value>) -> HashMap<String, RuleSetting> {
    let rules = result
        .and_then(|settings| settings.get(0))
        .and_then(|section| section.get("rules"))
        .and_then(Value::as_object);

    rules
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| Some((id.clone(), RuleSetting::parse(value.as_str()?)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_package_is_read_from_the_first_settings_section() {
        assert!(require_package(Some(&json!([{ "requirePackage": true }]))));
        assert!(!require_package(Some(
            &json!([{ "requirePackage": false }])
        )));
        assert!(!require_package(Some(
            &json!([{ "requirePackage": "yes" }])
        )));
        assert!(!require_package(Some(&json!([{}]))));
        assert!(!require_package(None));
    }

    #[test]
    fn mi_package_is_a_profile_id_or_auto() {
        let setting = |value| mi_package(Some(&json!([{ "miPackage": value }]))).map(|p| p.id);

        assert_eq!(setting("wwmi"), Some("wwmi"));
        assert_eq!(setting("auto"), None);
        assert_eq!(setting("nope"), None);
        assert_eq!(mi_package(Some(&json!([{}]))), None);
        assert_eq!(mi_package(None), None);
    }

    #[test]
    fn only_files_outside_a_package_are_skipped() {
        let strict = LintOptions {
            require_package: true,
            ..LintOptions::bundled()
        };
        let lenient = LintOptions::bundled();

        assert!(skipped_for_package(&strict, None));
        assert!(!skipped_for_package(&strict, Some(Path::new("/pkg"))));
        assert!(!skipped_for_package(&lenient, None));
    }
}
