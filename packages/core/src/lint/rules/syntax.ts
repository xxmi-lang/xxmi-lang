import type { LineModel } from '../../model/types.ts';
import type { SyntaxIssue } from '../../parser/index.ts';
import type { Rule, RuleContext } from '../types.ts';

/** Lines whose value the DLL parses as an expression (`CommandListExpression::parse`). */
function isExpressionLine(line: LineModel | undefined): boolean {
  if (!line) return false;
  const content = line.content.text.toLowerCase();
  return (
    /^(if|elif|else if)\s/.test(content) ||
    /^(local\s+)?\$/.test(content) ||
    content.startsWith('local ')
  );
}

function issues(context: RuleContext): { issue: SyntaxIssue; line: LineModel | undefined }[] {
  const { file } = context;
  const out: { issue: SyntaxIssue; line: LineModel | undefined }[] = [];
  const lineAt = (lines: LineModel[], n: number) => lines.find((l) => l.line === n);
  for (const issue of file.preambleIssues)
    out.push({ issue, line: lineAt(file.preamble, issue.range.start.line) });
  for (const section of file.sections) {
    for (const issue of section.syntaxIssues)
      out.push({ issue, line: lineAt(section.lines, issue.range.start.line) });
  }
  return out;
}

export const XM001: Rule = {
  id: 'XM001',
  severity: 'error',
  description: 'Parse error (tree-sitter ERROR/MISSING node)',
  check(context) {
    for (const { issue, line } of issues(context)) {
      if (!isExpressionLine(line)) context.report(issue.range, issue.message);
    }
  },
};

export const XM109: Rule = {
  id: 'XM109',
  severity: 'error',
  description: 'Unknown operator or malformed expression',
  check(context) {
    for (const { issue, line } of issues(context)) {
      if (isExpressionLine(line)) {
        context.report(
          issue.range,
          `Malformed expression: ${issue.message.replace(/^Syntax error: /, '')}`,
        );
      }
    }
  },
};

export const XM002: Rule = {
  id: 'XM002',
  severity: 'warning',
  description: "File isn't valid UTF-8 (likely GBK/Shift-JIS)",
  check(context) {
    if (context.file.encoding !== 'unknown') return;
    context.report(
      { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
      "File isn't valid UTF-8 (likely GBK or Shift-JIS). The DLL reads inis as UTF-8, so non-ASCII text such as names and paths won't read as intended.",
    );
  },
};
