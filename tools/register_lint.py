#!/usr/bin/env python3
"""Register-lint gate for DEUS VULT prose surfaces (docs/ROADMAP.md gap G11).

Scans the game's prose surfaces for violations of the register canon and
exits non-zero on any finding, so it can run as a CI / pre-commit gate.
The same output doubles as a one-off audit report (--report adds scan
stats and a per-rule breakdown). The tool never mutates files and never
auto-fixes; every waiver must be written by a human, per rule.

Surfaces scanned (textual fields, not Rust syntax):
  * crusade-core/src/data/*.rs  — string literals AND comments (names,
    lore, rite names, register notes in the data tables)
  * crusade-app/src/{view,conversion,client,persist}.rs — string
    literals that face the player (keys / identifiers / paths skipped)
  * docs/HISTORICAL-FOES.md, docs/CONVERSION.md, docs/OVERWORLD.md —
    prose lines (fenced code blocks and HTML comments skipped)

BANNED-REGISTER RULES (case-insensitive, word-boundary matching).
Sources:
  HF6  = docs/HISTORICAL-FOES.md §6 "THE REGISTER SECTION" (2026-10-16):
         chronicle-firm, attested citations, never caricature
  CV8  = docs/CONVERSION.md §8 "Banned register for all conversion text"
  DS22 = docs/DOCTRINAL-SINS.md §2.2 vocabulary table (the G11 grounding
         volume per docs/ROADMAP.md gaps G1/G11)

  rule id         pattern (regex body)                                source        exception
  --------        -------------------                                ------        ---------
  slur            raghead|towelhead|camel jockey|muzzie|mooselim|     HF6.3         never
                  musloid|sand monkey
  mock-arabic     mahound|maho(u)met|mohamet|muhammet|ali baba|       HF6.3,        never
                  open sesame|abulbul|achmed the                      DS22
                  (mock-spellings of the Prophet's name and
                  Orientalist-fantasy name fragments)
  anachronism     turkey shoot|terrorist|jihadi(sts)|suicide           HF6 head         never
                  bomber/vest/attack|roadside bomb|shock and
                  awe|collateral damage|friendly fire|headshot
                  (modern war/gamer jargon breaks the chronicle voice)
  gore-thrill     splatter|gibber(ish/ing/ed/s)|giblet|pulped|        CV8, HF6.1    never
                  fountains/sprays of blood|gore-drenched
                  (gore-thrills, esp. at refusal)
  horde-framing   horde|grovel(ling)|swarthy|sweaty|bestial|           HF6.3,        never
                  bloodthirsty|treachery/treacherous by birth         DS22
                  ("heathen horde" framing, treachery-by-birth,
                  physical mockery: banned without exception)
  exonym          saracen|infidel|paynim|heathen|pagan                HF6.2,        ATTRIBUTION
                  (period exonyms in narrator voice)                  DS22
  coercion        forced convert|converted him|forced baptism|        CV8           never
                  baptized him by force|compelled his baptism
  game-damnation  damned to hell|soul is/was damned|soul               CV8           never
                  condemned|burn/roast in hell
                  (the GAME must not pronounce damnation;
                  "his soul unto God" is the exact frame)

ATTRIBUTION-CONTEXT EXCEPTION (exonym only). HF6.2: period exonyms live
only inside quotation marks, attributed to a named character or source.
A finding is waived when BOTH hold within the same scanned text unit
(string literal, comment, or markdown line):
  1. the term sits inside quotation marks — detected heuristically:
     at least one quote character (", \", or typographic) occurs before
     the match, at least one after, and the count before is odd;
  2. an attribution marker matches — ATTRIBUTION_RE below, e.g.
     "says the chronicler", "as the age called", "they called him",
     "according to", "in his chronicle" (possessive only — "in the
     chronicle" is generic and does NOT waive), "quoted", "attributed", or a
     named source (William of Tyre, Peter the Venerable, Bernard of
     Clairvaux, Robert of Ketton).
Precision first: unmarked quoted speech still flags; suppress it with a
reglint:allow once a human confirms the attribution.

SUPPRESSION. On the same physical line as the finding's match:
  Rust:     // reglint:allow <rule>[, <rule> ...]     (a "# reglint:allow"
            form is honored too — the marker text is what matters)
  Markdown: <!-- reglint:allow <rule>[, <rule> ...] -->
There is deliberately NO allow-all form: every suppression must name the
rule(s) it waives. Markers naming unknown rules (including "all") are
reported as warnings and do not suppress anything.

Usage:
    python3 tools/register_lint.py              # gate: exit 1 on findings
    python3 tools/register_lint.py --report     # audit report + stats
    python3 tools/register_lint.py --self-test  # embedded corpus check
"""
from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

RUST_DATA_GLOB = "crusade-core/src/data/*.rs"
RUST_APP_FILES = (
    "crusade-app/src/view.rs",
    "crusade-app/src/conversion.rs",
    "crusade-app/src/client.rs",
    "crusade-app/src/persist.rs",
)
DOC_FILES = (
    "docs/HISTORICAL-FOES.md",
    "docs/CONVERSION.md",
    "docs/OVERWORLD.md",
)


@dataclass(frozen=True)
class Rule:
    rule_id: str
    pattern: str
    source: str
    note: str
    exception: str | None  # None = no exception; "attribution" = HF6.2 waiver


RULES: list[Rule] = [
    Rule(
        "slur",
        r"ragheads?|towelheads?|camel\s?jockeys?|muzzies?|mooselims?|musloids?|sand\s?monkeys?",
        "HF6.3",
        "ethnic mockery / modern slur class (incl. 'mooselim' variants)",
        None,
    ),
    Rule(
        "mock-arabic",
        r"mahounds?|maho(u)?mets?|mohamets?|muhammets?|ali\s?babas?|open\s?sesame|abulbuls?|achmed\s+the",
        "HF6.3, DS22",
        "mock-Arabic fantasy name fragments and mock-spellings of the "
        "Prophet's name (use 'the Prophet' / 'their prophet')",
        None,
    ),
    Rule(
        "anachronism",
        r"turkey\s+shoot|terrorists?|jihadis?ts?|suicide\s+(?:bombers?|vests?|attacks?)"
        r"|roadside\s+bombs?|shock\s+and\s+awe|collateral\s+damage|friendly\s+fire|headshots?",
        "HF6 head (chronicle-firm voice)",
        "modern war/gamer jargon breaks the 12th-c. chronicle voice",
        None,
    ),
    Rule(
        "gore-thrill",
        r"splatters?|splattered|gibber(?:ish|ed|ing|s)?|giblets?|pulped"
        r"|fountains?\s+of\s+blood|sprays?\s+of\s+blood|gore-?drenched",
        "CV8, HF6.1",
        "gore-thrills (esp. at refusal); the chronicle's verbs for the "
        "fallen foe are 'yield, stand, hold' — never 'gibber'",
        None,
    ),
    Rule(
        "horde-framing",
        r"hordes?|grovell?(?:ed|ing|s)?|swarthy|sweaty|bestial|bloodthirsty"
        r"|treacherous\s+by\s+birth|treachery\s+by\s+birth",
        "HF6.3, DS22",
        "'heathen horde' framing, groveling, treachery-by-birth, "
        "physical mockery — banned without exception",
        None,
    ),
    Rule(
        "exonym",
        r"saracens?|infidels?|paynims?|heathens?|pagans?",
        "HF6.2, DS22",
        "period exonyms in narrator voice; allowed only inside attributed "
        "quotation (narrator uses the men's own names)",
        "attribution",
    ),
    Rule(
        "coercion",
        r"forced\s+converts?|converted\s+(?:him|her|them)|forced\s+baptism"
        r"|baptized\s+(?:him|her|them)\s+by\s+force|compelled\s+(?:his|her|their)\s+baptism",
        "CV8",
        "coercion verbs — baptism is written as joy, not conquest candy",
        None,
    ),
    Rule(
        "game-damnation",
        r"damned\s+to\s+hell|soul\s+is\s+damned|soul\s+was\s+damned|soul\s+condemned"
        r"|burns?\s+in\s+hell|burned\s+in\s+hell|roasts?\s+in\s+hell",
        "CV8",
        "the game must not pronounce damnation; 'his soul unto God' is "
        "the exact frame — judgment belongs to the Judge",
        None,
    ),
]

COMPILED = [(r, re.compile(r"\b(?:" + r.pattern + r")\b", re.IGNORECASE)) for r in RULES]
RULE_IDS = {r.rule_id for r in RULES}

# Attribution markers for the exonym exception (HF6.2): "period exonyms
# live only inside quotation marks, attributed to a named character or
# source". Matched against the whole scanned text unit.
ATTRIBUTION_RE = re.compile(
    r"\b(?:says?|writes?|records?|notes?)\s+the\s+"
    r"(?:chronicler|bishop|clerk|chaplain|emir|sultan|captain|herald)\b"
    r"|\bthe\s+chronicler\s+(?:writes|records|notes|says)\b"
    r"|\baccording\s+to\b"
    r"|\bas\s+the\s+(?:age|chronicle|annals?|records?)\s+"
    r"(?:said|wrote|say|writes|called|named|put\s+it)\b"
    r"|\bthe\s+age\s+(?:called|named|said|wrote)\b"
    r"|\b(?:they|he|she)\s+called\s+(?:him|them|it|the)\b"
    r"|\bso\s+(?:says|wrote|records)\s+the\b"
    r"|\bin\s+(?:his|her|their)\s+"
    r"(?:chronicle|letter|history|annals?|account|report)\b"
    r"|\bquoted\b|\battributed\b"
    r"|\bWilliam\s+of\s+Tyre\b|\bPeter\s+the\s+Venerable\b"
    r"|\bBernard\s+of\s+Clairvaux\b|\bRobert\s+of\s+Ketton\b",
    re.IGNORECASE,
)

# Suppression markers: "reglint:allow <rule>[, <rule>...]". No allow-all.
SUPPRESS_HEAD_RE = re.compile(r"reglint:allow\b", re.IGNORECASE)
SUPPRESS_LIST_RE = re.compile(r"\s*([a-z][a-z0-9-]*(?:\s*,\s*[a-z0-9-]+)*)", re.IGNORECASE)

# Quote tokens for the in-quotation heuristic ("..." / \"...\" / typographic).
QUOTE_TOKEN_RE = re.compile(r'\\["\']|["\u201c\u201d\u2018\u2019]')

# Literal shapes we skip in app files per the G11 brief: keys,
# identifiers, paths, format fragments without prose.
KEYLIKE_SEPARATOR_RE = re.compile(r"[./:_\-{}#<>=]")


@dataclass
class Unit:
    """One scanned text unit: a string literal, comment, or prose line."""

    text: str
    line: int  # 1-based line where the unit starts
    kind: str  # 'literal' | 'comment' | 'prose'
    nl_marks: list = field(default_factory=list)  # text offsets of newlines

    def line_of(self, pos: int) -> int:
        return self.line + sum(1 for m in self.nl_marks if m <= pos)


@dataclass(frozen=True)
class Finding:
    rel: str
    line: int
    rule: str
    excerpt: str


# --------------------------------------------------------------------------
# extraction
# --------------------------------------------------------------------------

def extract_rust(text: str, scan_comments: bool) -> list[Unit]:
    """Extract string literals (always) and comments (optionally) from
    Rust source. Handles escapes, `\\<newline>` line continuations
    (stripped, as Rust does), raw strings, char literals, and lifetimes.
    """
    units: list[Unit] = []
    i, n = 0, len(text)
    line = 1

    def ident_char(c: str) -> bool:
        return c.isalnum() or c == "_"

    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
        elif text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            if scan_comments:
                units.append(Unit(text[i + 2 : j], line, "comment"))
            i = j
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            start_line, marks = line, []
            while j < n and depth > 0:
                if text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    if text[j] == "\n":
                        line += 1
                        marks.append(j - i - 2)
                    j += 1
            if scan_comments:
                body = text[i + 2 : j - 2]
                u = Unit(body, start_line, "comment", marks)
                units.append(u)
            i = j
        elif c == "'":
            # char literal 'x' / '\n' vs lifetime 'a
            m = re.match(r"'(\\.|[^\\'])'", text[i:])
            i += m.end() if m else 1
        elif (
            c == "r"
            and (i == 0 or not ident_char(text[i - 1]))
            and i + 1 < n
            and not ident_char(text[i + 1])
        ):
            m = re.match(r'r(#*)"', text[i:])
            if m:
                term = '"' + m.group(1)
                j = i + m.end()
                start_line, marks = line, []
                while j < n and not text.startswith(term, j):
                    if text[j] == "\n":
                        line += 1
                        marks.append(j - (i + m.end()))
                    j += 1
                units.append(Unit(text[i + m.end() : j], start_line, "literal", marks))
                i = j + len(term)
            else:
                i += 1
        elif c == '"':
            j = i + 1
            start_line, buf, marks = line, [], []
            while j < n and text[j] != '"':
                ch = text[j]
                if ch == "\\" and j + 1 < n:
                    nxt = text[j + 1]
                    if nxt == "\n":  # line continuation: \ NL indent -> ""
                        line += 1
                        marks.append(len(buf))
                        j += 2
                        while j < n and text[j] in " \t":
                            j += 1
                        continue
                    buf.append(ch)
                    buf.append(nxt)
                    j += 2
                    continue
                if ch == "\n":
                    line += 1
                    marks.append(len(buf))
                buf.append(ch)
                j += 1
            units.append(Unit("".join(buf), start_line, "literal", marks))
            i = j + 1
        else:
            i += 1
    return units


def extract_markdown(text: str) -> list[Unit]:
    """Extract prose lines from markdown, skipping fenced code blocks and
    HTML comments (suppression markers live there)."""
    units: list[Unit] = []
    in_fence = False
    in_comment = False
    for i, ln in enumerate(text.splitlines(), 1):
        if ln.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence or in_comment:
            if in_comment and "-->" in ln:
                in_comment = False
            continue
        if "<!--" in ln:
            head, _, tail = ln.partition("<!--")
            if "-->" in tail:
                ln = head + tail.partition("-->")[2]
            else:
                in_comment = True
                ln = head
        if ln.strip():
            units.append(Unit(ln, i, "prose"))
    return units


def is_keylike(lit: str) -> bool:
    """True for literals that are keys / identifiers / paths / symbol
    fragments rather than prose (skipped per the G11 brief)."""
    t = lit.strip()
    if len(t) < 2 or not re.search(r"[A-Za-z]", t):
        return True
    if re.search(r"\s", t):
        return False  # prose has whitespace
    return bool(KEYLIKE_SEPARATOR_RE.search(t))


# --------------------------------------------------------------------------
# scanning
# --------------------------------------------------------------------------

def parse_suppressions(raw_line: str) -> tuple[set, list]:
    """Return (honored rule ids, invalid marker tokens) from a raw
    physical line. No allow-all: 'all', '*', or a bare marker suppress
    nothing and are reported."""
    allowed: set = set()
    invalid: list = []
    for m in SUPPRESS_HEAD_RE.finditer(raw_line):
        lm = SUPPRESS_LIST_RE.match(raw_line[m.end() :])
        if not lm:
            invalid.append("<bare marker — allow-all is not supported>")
            continue
        for tok in re.split(r"[,\s]+", lm.group(1)):
            if not tok:
                continue
            if tok.lower() in RULE_IDS:
                allowed.add(tok.lower())
            else:
                invalid.append(tok)
    return allowed, invalid


def in_quoted_span(text: str, start: int, end: int) -> bool:
    """Heuristic: the span sits between an opening and closing quote
    inside the same unit (see module docstring)."""
    before = len(QUOTE_TOKEN_RE.findall(text[:start]))
    after = len(QUOTE_TOKEN_RE.findall(text[end:]))
    return before >= 1 and after >= 1 and before % 2 == 1


def excerpt(text: str, match: re.Match, width: int = 76) -> str:
    s = max(0, match.start() - 28)
    e = min(len(text), match.end() + 28)
    frag = text[s:e].replace("\n", " ").replace("\\\n", " ")
    return "{}{}{}".format("..." if s > 0 else "", frag, "..." if e < len(text) else "")


def scan_unit(unit: Unit, rel: str, raw_lines: list) -> tuple[list, list]:
    """Scan one text unit; returns (findings, invalid-marker warnings)."""
    findings: list = []
    warnings: list = []
    for rule, rx in COMPILED:
        for m in rx.finditer(unit.text):
            line = unit.line_of(m.start())
            raw = raw_lines[line - 1] if 0 < line <= len(raw_lines) else unit.text
            allowed, invalid = parse_suppressions(raw)
            warnings.extend((rel, line, tok) for tok in invalid)
            if rule.rule_id in allowed:
                continue
            if rule.exception == "attribution" and in_quoted_span(
                unit.text, m.start(), m.end()
            ) and ATTRIBUTION_RE.search(unit.text):
                continue
            findings.append(Finding(rel, line, rule.rule_id, excerpt(unit.text, m)))
    return findings, warnings


def scan_text(rel: str, text: str, mode: str) -> tuple[list, list, dict]:
    """Scan one file's text. mode: 'rust-data' (literals + comments),
    'rust-app' (literals only), 'markdown' (prose lines)."""
    raw_lines = text.splitlines()
    if mode == "markdown":
        units = extract_markdown(text)
    elif mode == "rust-data":
        units = extract_rust(text, scan_comments=True)
    else:
        units = extract_rust(text, scan_comments=False)
    findings: list = []
    warnings: list = []
    stats = {"units": 0, "skipped": 0}
    for u in units:
        if u.kind == "literal" and is_keylike(u.text):
            stats["skipped"] += 1
            continue
        stats["units"] += 1
        f, w = scan_unit(u, rel, raw_lines)
        findings.extend(f)
        warnings.extend(w)
    return findings, warnings, stats


def scan_file(path: Path, mode: str) -> tuple[list, list, dict]:
    return scan_text(path.relative_to(ROOT).as_posix(), path.read_text(encoding="utf-8"), mode)


# --------------------------------------------------------------------------
# self-test
# --------------------------------------------------------------------------

def self_test() -> bool:
    """Embedded corpus: hit, miss, attribution exception, suppression,
    allow-all rejection, key-skip, multi-line line tracking."""
    cases = [
        (
            "hit: un-attributed exonym in lore",
            "rust-data",
            '        lore: "The Saracen lines held the road.",\n',
            {("exonym", 1)},
        ),
        (
            "miss: the men's own names are the register",
            "rust-data",
            '        lore: "The Sultanate of Rum held the road.",\n',
            set(),
        ),
        (
            "attribution exception: quoted + marked",
            "rust-data",
            '        lore: "He wrote of \\"the paynim host,\\" says the chronicler of Tyre.",\n',
            set(),
        ),
        (
            "attribution exception needs quotes, not just a marker",
            "rust-data",
            '        lore: "The chronicler says the paynim host broke.",\n',
            {("exonym", 1)},
        ),
        (
            "suppression by rule",
            "markdown",
            'The age wrote of "Saracen" lines. <!-- reglint:allow exonym -->\n',
            set(),
        ),
        (
            "suppression is rule-specific",
            "markdown",
            'The age wrote of "Saracen" lines and a horde. <!-- reglint:allow exonym -->\n',
            {("horde-framing", 1)},
        ),
        (
            "bare marker does not suppress",
            "markdown",
            "A horde came. <!-- reglint:allow -->\n",
            {("horde-framing", 1)},
        ),
        (
            "allow-all keyword is rejected",
            "markdown",
            "A horde came. <!-- reglint:allow all -->\n",
            {("horde-framing", 1)},
        ),
        (
            "slur is never excepted, even quoted + attributed",
            "rust-data",
            '        lore: "\\"Ragheads,\\" says the chronicler of Tyre.",\n',
            {("slur", 1)},
        ),
        (
            "mock-arabic hit",
            "rust-data",
            '        name: "Achmed the Sapper",\n',
            {("mock-arabic", 1)},
        ),
        (
            "anachronism hit",
            "markdown",
            "The field was a turkey shoot.\n",
            {("anachronism", 1)},
        ),
        (
            "coercion hit in player-facing literal",
            "rust-app",
            '    notice("He became a forced convert.").to_string()\n',
            {("coercion", 1)},
        ),
        (
            "keys / identifiers / paths are skipped",
            "rust-app",
            '    const KEY: &str = "campaign.converts_baptized";\n',
            set(),
        ),
        (
            "multi-line literal: line numbers tracked",
            "rust-app",
            '    "first line \\\n     and a turkey shoot \\\n     last",\n',
            {("anachronism", 2)},
        ),
        (
            "plural of listed term still hits",
            "markdown",
            "The screed spoke of jihadists.\n",
            {("anachronism", 1)},
        ),
        (
            "generic chronicle reference does not waive",
            "rust-data",
            '        lore: "In the chronicle, the \\"paynim\\" fell.",\n',
            {("exonym", 1)},
        ),
        (
            "coercion: them-form hits",
            "rust-app",
            '    notice("They converted them at spear-point.").to_string()\n',
            {("coercion", 1)},
        ),
    ]
    ok = True
    for label, mode, text, expected in cases:
        findings, _, _ = scan_text("self-test.rs" if mode != "markdown" else "self-test.md", text, mode)
        got = {(f.rule, f.line) for f in findings}
        status = "PASS" if got == expected else "FAIL"
        if got != expected:
            ok = False
        print(f"  [{status}] {label}: expected {sorted(expected)}, got {sorted(got)}")
    print(f"self-test: {'PASS' if ok else 'FAIL'} ({len(cases)} cases)")
    return ok


# --------------------------------------------------------------------------
# reporting
# --------------------------------------------------------------------------

def collect_targets() -> list:
    targets = []
    for p in sorted(ROOT.glob(RUST_DATA_GLOB)):
        targets.append((p, "rust-data"))
    for rel in RUST_APP_FILES:
        targets.append((ROOT / rel, "rust-app"))
    for rel in DOC_FILES:
        targets.append((ROOT / rel, "markdown"))
    return targets


def run(report: bool) -> int:
    targets = collect_targets()
    all_findings: list = []
    all_warnings: list = []
    per_surface: dict = {}
    for path, mode in targets:
        findings, warnings, stats = scan_file(path, mode)
        all_findings.extend(findings)
        all_warnings.extend(warnings)
        surface = {
            "rust-data": "crusade-core/src/data/*.rs (literals + comments)",
            "rust-app": "crusade-app/src/* (player-facing literals)",
            "markdown": "docs/*.md (prose)",
        }[mode]
        s = per_surface.setdefault(surface, {"files": 0, "units": 0, "skipped": 0})
        s["files"] += 1
        s["units"] += stats["units"]
        s["skipped"] += stats["skipped"]

    total_units = sum(s["units"] for s in per_surface.values())
    print(f"register-lint: scanned {len(targets)} files, {total_units} prose units")
    if report:
        print()
        print("Scanned surfaces:")
        for name, s in per_surface.items():
            skip = f", {s['skipped']} skipped as keys/paths" if s["skipped"] else ""
            print(f"  {name}: {s['files']} files, {s['units']} units{skip}")

    print()
    by_file: dict = {}
    for f in all_findings:
        by_file.setdefault(f.rel, []).append(f)
    for rel in sorted(by_file):
        print(rel)
        for f in sorted(by_file[rel], key=lambda x: (x.line, x.rule)):
            print(f"  {rel}:{f.line}: {f.rule}: {f.excerpt}")
        print()

    # dedupe invalid-marker warnings
    seen = set()
    for rel, line, tok in all_warnings:
        key = (rel, line, tok)
        if key in seen:
            continue
        seen.add(key)
        print(f"warning: {rel}:{line}: reglint marker '{tok}' is not a rule (ignored; allow-all is unsupported)")

    if report and all_findings:
        counts: dict = {}
        for f in all_findings:
            counts[f.rule] = counts.get(f.rule, 0) + 1
        print()
        print("Findings by rule: " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))

    n = len(all_findings)
    print()
    print(f"== register-lint: {n} finding(s) ==")
    return 1 if n else 0


def main(argv: list | None = None) -> int:
    ap = argparse.ArgumentParser(description="G11 register-lint gate (see docs/REGISTER-LINT.md)")
    ap.add_argument("--report", action="store_true", help="audit mode: add scan stats and per-rule counts")
    ap.add_argument("--self-test", action="store_true", help="run the embedded test corpus and exit")
    args = ap.parse_args(argv)
    if args.self_test:
        return 0 if self_test() else 2
    return run(args.report)


if __name__ == "__main__":
    sys.exit(main())
