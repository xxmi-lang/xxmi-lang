/**
 * xxmi-lsp (docs/03-lsp.md). Everything language-related comes from @xxmi-lang/core; this file
 * only keeps editor state, schedules work and converts results to LSP types.
 */
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import {
  defaultSpec,
  bundledSnapshots,
  collectIniFiles,
  completions,
  ConfigLoader,
  definition,
  hover,
  lintFile,
  outline,
  references,
  rename,
  SpecLookup,
  targetAt,
  Workspace,
  type CompletionKind,
  type Diagnostic as CoreDiagnostic,
  type Fix,
  type OutlineSymbol,
  type RuleSetting,
  type Spec,
  type SymbolSnapshot,
} from '@xxmi-lang/core';
import {
  CodeActionKind,
  CompletionItemKind,
  DiagnosticSeverity,
  ErrorCodes,
  FileChangeType,
  ResponseError,
  SymbolKind,
  TextDocuments,
  TextDocumentSyncKind,
  type CodeAction,
  type Connection,
  type Diagnostic,
  type DocumentSymbol,
  type InitializeResult,
  type Location,
  type TextEdit,
  type WorkspaceEdit,
} from 'vscode-languageserver';
import { TextDocument } from 'vscode-languageserver-textdocument';

export interface ServerOptions {
  version: string;
  /** Embedded data for single-file binaries; defaults come from @xxmi-lang/core otherwise. */
  spec?: Spec;
  snapshots?: SymbolSnapshot[];
}

const DEBOUNCE_MS = 150;
/** docs/03-lsp.md "Robustness": bigger files only get syntax/structure rules while typing. */
const LARGE_FILE = 5 * 1024 * 1024;
/** Background indexing stops here (opening a whole install with thousands of mods). */
const MAX_INDEXED_FILES = 5000;

const SEVERITY: Record<CoreDiagnostic['severity'], DiagnosticSeverity> = {
  error: DiagnosticSeverity.Error,
  warning: DiagnosticSeverity.Warning,
  info: DiagnosticSeverity.Information,
  hint: DiagnosticSeverity.Hint,
};

const COMPLETION_KIND: Record<CompletionKind, CompletionItemKind> = {
  section: CompletionItemKind.Class,
  key: CompletionItemKind.Property,
  command: CompletionItemKind.Function,
  keyword: CompletionItemKind.Keyword,
  value: CompletionItemKind.EnumMember,
  variable: CompletionItemKind.Variable,
  reference: CompletionItemKind.Reference,
  file: CompletionItemKind.File,
};

const SECTION_SYMBOL: Record<string, SymbolKind> = {
  CommandList: SymbolKind.Function,
  BuiltInCommandList: SymbolKind.Function,
  CustomShader: SymbolKind.Function,
  BuiltInCustomShader: SymbolKind.Function,
  Resource: SymbolKind.Struct,
  Pool: SymbolKind.Array,
  TextureOverride: SymbolKind.Event,
  ShaderOverride: SymbolKind.Event,
  ShaderRegex: SymbolKind.Event,
  Key: SymbolKind.Key,
  Preset: SymbolKind.Key,
  Constants: SymbolKind.Constant,
  Include: SymbolKind.File,
};

export function startServer(connection: Connection, options: ServerOptions): void {
  const documents = new TextDocuments(TextDocument);
  const lookup = new SpecLookup(options.spec ?? defaultSpec());
  const workspace = new Workspace({ lookup, snapshots: options.snapshots ?? bundledSnapshots() });
  const configs = new ConfigLoader();
  /** URIs as the client sent them (drive-letter case and escaping must round-trip). */
  const uriByPath = new Map<string, string>();
  const pending = new Map<string, ReturnType<typeof setTimeout>>();
  let roots: string[] = [];
  let overrides: Record<string, RuleSetting> = {};
  let supportsConfiguration = false;
  /** Serializes workspace mutations; requests wait for it so they see a consistent model. */
  let queue: Promise<unknown> = Promise.resolve();

  const enqueue = <T>(work: () => Promise<T>): Promise<T> => {
    const next = queue.then(work, work);
    queue = next.catch((error: unknown) => {
      connection.console.error(
        `xxmi-lsp: ${error instanceof Error ? (error.stack ?? error.message) : String(error)}`,
      );
    });
    return next;
  };

  const toPath = (uri: string): string => resolve(fileURLToPath(uri));
  const toUri = (path: string): string => uriByPath.get(path) ?? pathToFileURL(path).href;
  const fallbackRoot = (path: string): string =>
    roots.filter((r) => path.startsWith(r)).sort((a, b) => b.length - a.length)[0] ?? dirname(path);

  const publish = (full = false): void => {
    for (const document of documents.all()) {
      const path = toPath(document.uri);
      const file = workspace.files.get(path);
      if (!file) continue;
      const large = document.getText().length > LARGE_FILE && !full;
      let diagnostics: Diagnostic[] = [];
      try {
        diagnostics = lintFile(file, workspace, {
          configs,
          overrides,
          ...(large ? { rulePrefixes: ['XM0', 'XM1'] } : {}),
        }).map(toLspDiagnostic);
      } catch (error) {
        connection.console.error(`xxmi-lsp: lint failed for ${path}: ${String(error)}`);
      }
      void connection.sendDiagnostics({ uri: document.uri, diagnostics });
    }
  };

  const update = (uri: string): Promise<void> =>
    enqueue(async () => {
      const timer = pending.get(uri);
      if (timer) clearTimeout(timer);
      pending.delete(uri);
      const document = documents.get(uri);
      if (!document) return;
      const path = toPath(uri);
      uriByPath.set(path, uri);
      await workspace.openDocument(path, document.getText(), fallbackRoot(path));
      publish();
    });

  /**
   * Debounce with a leading edge: a change after a quiet period is linted at once (an agent's
   * single edit gets diagnostics while it's still looking), while a burst of keystrokes is
   * linted 150 ms after the last one.
   */
  const lastChange = new Map<string, number>();
  const schedule = (uri: string): void => {
    const now = Date.now();
    const quiet = now - (lastChange.get(uri) ?? 0) > DEBOUNCE_MS && !pending.has(uri);
    lastChange.set(uri, now);
    if (quiet) {
      void update(uri);
      return;
    }
    const timer = pending.get(uri);
    if (timer) clearTimeout(timer);
    pending.set(
      uri,
      setTimeout(() => void update(uri), DEBOUNCE_MS),
    );
  };

  /** Runs a request against the current model of `uri`; never throws to the client. */
  const withFile = async <T>(
    uri: string,
    fallback: T,
    run: (file: NonNullable<ReturnType<typeof workspace.files.get>>) => T,
  ): Promise<T> => {
    if (pending.has(uri)) await update(uri);
    await queue;
    const file = workspace.files.get(toPath(uri));
    if (!file) return fallback;
    try {
      return run(file);
    } catch (error) {
      if (error instanceof ResponseError) throw error;
      connection.console.error(
        `xxmi-lsp: request failed: ${error instanceof Error ? (error.stack ?? error.message) : String(error)}`,
      );
      return fallback;
    }
  };

  const fetchSettings = async (): Promise<void> => {
    if (!supportsConfiguration) return;
    const settings = (await connection.workspace.getConfiguration('xxmi')) as {
      rules?: Record<string, RuleSetting>;
    } | null;
    overrides = settings?.rules ?? {};
  };

  connection.onInitialize((params): InitializeResult => {
    roots = (params.workspaceFolders ?? []).map((f) => toPath(f.uri));
    // Older clients (e.g. some Kate versions) send only the deprecated rootUri.
    // eslint-disable-next-line @typescript-eslint/no-deprecated
    const rootUri = params.rootUri;
    if (roots.length === 0 && rootUri) roots = [toPath(rootUri)];
    supportsConfiguration = params.capabilities.workspace?.configuration === true;
    return {
      capabilities: {
        textDocumentSync: { openClose: true, change: TextDocumentSyncKind.Incremental, save: true },
        definitionProvider: true,
        referencesProvider: true,
        renameProvider: { prepareProvider: true },
        hoverProvider: true,
        completionProvider: { triggerCharacters: ['[', '$', '\\', '='] },
        documentSymbolProvider: true,
        codeActionProvider: { codeActionKinds: [CodeActionKind.QuickFix] },
      },
      serverInfo: { name: 'xxmi-lsp', version: options.version },
    };
  });

  connection.onInitialized(() => {
    void fetchSettings().then(() => {
      publish();
    });
    // docs/01-language-model.md §4: open files first, then the rest of the opened folders in
    // the background, so references and rename see files that aren't open.
    void enqueue(async () => {
      let indexed = 0;
      for (const root of roots) {
        let paths: string[];
        try {
          paths = collectIniFiles(root);
        } catch {
          continue;
        }
        for (const path of paths) {
          if (indexed >= MAX_INDEXED_FILES) {
            connection.console.warn(
              `xxmi-lsp: indexed the first ${MAX_INDEXED_FILES} .ini files; open a smaller folder for full coverage.`,
            );
            return;
          }
          if (workspace.files.has(path)) continue;
          try {
            await workspace.addTarget(path, root);
            indexed++;
          } catch (error) {
            connection.console.warn(`xxmi-lsp: skipped ${path}: ${String(error)}`);
          }
        }
      }
      publish();
    });
  });

  connection.onDidChangeConfiguration(() => {
    void enqueue(async () => {
      await fetchSettings();
      publish();
    });
  });

  documents.onDidOpen((e) => {
    void update(e.document.uri);
  });
  documents.onDidChangeContent((e) => {
    schedule(e.document.uri);
  });
  documents.onDidSave(
    (e) =>
      void enqueue(async () => {
        await update(e.document.uri);
        publish(true);
      }),
  );
  documents.onDidClose((e) => {
    const timer = pending.get(e.document.uri);
    if (timer) clearTimeout(timer);
    pending.delete(e.document.uri);
    void connection.sendDiagnostics({ uri: e.document.uri, diagnostics: [] });
    void enqueue(async () => {
      await workspace.closeDocument(toPath(e.document.uri));
      publish();
    });
  });

  connection.onDidChangeWatchedFiles((params) => {
    void enqueue(async () => {
      for (const change of params.changes) {
        if (documents.get(change.uri)) continue; // the editor buffer wins
        if (change.type === FileChangeType.Created) continue; // picked up when included or opened
        await workspace.reload(toPath(change.uri));
      }
      publish();
    });
  });

  connection.onDefinition((params) =>
    withFile(params.textDocument.uri, null as Location | null, (file) => {
      const target = targetAt(workspace, file, params.position);
      const location = target ? definition(workspace, target) : undefined;
      return location ? { uri: toUri(location.path), range: location.range } : null;
    }),
  );

  connection.onReferences((params) =>
    withFile(params.textDocument.uri, [] as Location[], (file) => {
      const target = targetAt(workspace, file, params.position);
      if (!target) return [];
      return references(workspace, target, params.context.includeDeclaration).map((l) => ({
        uri: toUri(l.path),
        range: l.range,
      }));
    }),
  );

  connection.onPrepareRename((params) =>
    withFile(params.textDocument.uri, null, (file) => {
      const result = rename(workspace, file, params.position);
      if (!result.ok) throw new ResponseError(ErrorCodes.InvalidRequest, result.message);
      return { range: result.range, placeholder: result.placeholder };
    }),
  );

  connection.onRenameRequest((params) =>
    withFile(params.textDocument.uri, null as WorkspaceEdit | null, (file) => {
      const result = rename(workspace, file, params.position, params.newName);
      if (!result.ok) throw new ResponseError(ErrorCodes.InvalidRequest, result.message);
      const changes: Record<string, TextEdit[]> = {};
      for (const [path, edits] of result.edits) changes[toUri(path)] = edits;
      return { changes };
    }),
  );

  connection.onHover((params) =>
    withFile(params.textDocument.uri, null, (file) => {
      const result = hover(workspace, file, params.position);
      return result
        ? { contents: { kind: 'markdown' as const, value: result.contents }, range: result.range }
        : null;
    }),
  );

  connection.onCompletion((params) =>
    withFile(params.textDocument.uri, [], (file) =>
      completions(workspace, file, params.position).map((item) => ({
        label: item.label,
        kind: COMPLETION_KIND[item.kind],
        ...(item.detail ? { detail: item.detail } : {}),
        ...(item.documentation
          ? { documentation: { kind: 'markdown' as const, value: item.documentation } }
          : {}),
        textEdit: { range: item.range, newText: item.label },
      })),
    ),
  );

  connection.onDocumentSymbol((params) =>
    withFile(params.textDocument.uri, [] as DocumentSymbol[], (file) =>
      outline(file).map(toDocumentSymbol),
    ),
  );

  connection.onCodeAction((params) => {
    const actions: CodeAction[] = [];
    for (const diagnostic of params.context.diagnostics) {
      const fix = diagnostic.data as Fix | undefined;
      if (diagnostic.source !== 'xxmi' || !fix?.edits) continue;
      actions.push({
        title: fix.title,
        kind: CodeActionKind.QuickFix,
        diagnostics: [diagnostic],
        isPreferred: true,
        edit: { changes: { [params.textDocument.uri]: fix.edits } },
      });
    }
    return actions;
  });

  documents.listen(connection);
  connection.listen();
}

function toLspDiagnostic(d: CoreDiagnostic): Diagnostic {
  return {
    range: d.range,
    severity: SEVERITY[d.severity],
    code: d.id,
    source: 'xxmi',
    message: d.message,
    ...(d.fix ? { data: d.fix } : {}),
  };
}

function toDocumentSymbol(symbol: OutlineSymbol): DocumentSymbol {
  return {
    name: symbol.name,
    kind:
      symbol.kind === 'group'
        ? SymbolKind.Namespace
        : (SECTION_SYMBOL[symbol.kind] ?? SymbolKind.Object),
    range: symbol.range,
    selectionRange: symbol.selectionRange,
    children: symbol.children.map(toDocumentSymbol),
  };
}

export { VERSION } from '@xxmi-lang/core';
