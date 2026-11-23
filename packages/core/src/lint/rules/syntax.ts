import type { LineModel, SectionModel } from '../../model/types.ts';
import type { SyntaxIssue } from '../../parser/index.ts';
import type { Rule, RuleContext } from '../types.ts';
import { isMalformedHeader } from './header.ts';
import { isHonoured } from './outside-section.ts';
import { isMalformedLine, isMisplacedFlow } from './structure.ts';

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
  for (const issue of file.preambleIssues) {
    const line = lineAt(file.preamble, issue.range.start.line);
    // XM003 owns every preamble line but `namespace` and `condition`, XM111 the flow keywords.
    if (!line || isHonoured(line)) out.push({ issue, line });
  }
  for (const section of file.sections) {
    for (const issue of section.syntaxIssues) {
      const line = lineAt(section.lines, issue.range.start.line);
      if (!isClaimed(section, issue, line)) out.push({ issue, line });
    }
  }
  return out;
}

/** Whether XM005, XM006, XM103, XM110 or XM111 reports the line the issue starts on. */
function isClaimed(
  section: SectionModel,
  issue: SyntaxIssue,
  line: LineModel | undefined,
): boolean {
  if (!line) {
    return issue.range.start.line === section.header.line && isMalformedHeader(section.header);
  }
  // XM110 owns `key =`: it names the cause the syntax error only hints at.
  return (
    line.value?.text === '' || isMalformedLine(section, line) || isMisplacedFlow(section, line)
  );
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
