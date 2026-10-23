#!/usr/bin/env python3
"""
Adversarial Council — executive decision support via a cross-model red-team chain.

Flow (one round):
    Claude (proposal) -> Gemini (attacks Claude) -> OpenAI (attacks Gemini) -> Claude (rebuts OpenAI)
Then Claude writes a one-page executive decision brief from the whole debate.

With --rounds N > 1, the chain continues Gemini -> OpenAI -> Claude for extra loops before the brief.

Usage:
    python council.py "Should we expand into Kenya in Q2 2027?"
    python council.py --file decision.md --context financials.csv --rounds 2
    python council.py --dry-run "Test question"        # no API calls, checks the pipeline
    python council.py --list-models                     # show model IDs your keys can use
"""

from __future__ import annotations

import argparse
import datetime as dt
import html
import importlib
import os
import re
import sys
import time
from dataclasses import dataclass
from pathlib import Path

for _s in (sys.stdout, sys.stderr):  # Windows consoles default to a legacy codepage
    _reconfigure = getattr(_s, "reconfigure", None)
    if _reconfigure is not None:
        _reconfigure(encoding="utf-8", errors="replace")

try:
    from dotenv import load_dotenv

    load_dotenv(Path(__file__).with_name(".env"))
except ImportError:
    pass

# ---------------------------------------------------------------- config ---

CLAUDE_MODEL = os.getenv("CLAUDE_MODEL", "claude-sonnet-4-5")
GEMINI_MODEL = os.getenv("GEMINI_MODEL", "gemini-2.5-pro")
OPENAI_MODEL = os.getenv("OPENAI_MODEL", "gpt-5")
MAX_TOKENS = int(os.getenv("COUNCIL_MAX_TOKENS", "4000"))

# ---------------------------------------------------------------- prompts --

SHARED_RULES = """You are part of an executive decision-support council made up of different AI models.
The goal is a better DECISION, not winning. Rules:
- Number every claim you make (e.g. C1, C2...) so later reviewers can target them.
- Separate facts, assumptions, and judgement. Flag any assumption the decision hinges on.
- Give rough probabilities or confidence (low/med/high) for key claims.
- Be concrete: numbers, timeframes, owners, triggers. No filler, no flattery.
- If you don't know something, say what information would resolve it.
- Keep it under ~900 words."""

PROPOSER = (
    SHARED_RULES
    + """

ROLE: Lead Analyst (opening position).
Produce:
1. Decision framing — what exactly is being decided, by when, and what success looks like.
2. Options — at least 3, including "do nothing / delay".
3. Recommendation — one option, with numbered supporting arguments (C1, C2...).
4. Key assumptions and the evidence for each.
5. Top risks and mitigations.
6. What would change your mind."""
)

ATTACKER = (
    SHARED_RULES
    + """

ROLE: Adversarial Red Team. You are reviewing the output of the PREVIOUS model in the chain.
Your job is to break its argument. Do NOT summarise it politely.
Produce:
1. Strongest attacks — target specific numbered claims; for each, explain the flaw
   (weak evidence, hidden assumption, logical gap, survivorship/optimism bias, missing
   second-order effect, wrong base rate, incentive blindness, reversibility ignored).
2. Missing options or stakeholders the previous model ignored.
3. A pre-mortem: it is 18 months later and this decision failed — the most likely story why.
4. Your counter-recommendation (it may agree with the original if the attacks fail —
   but say so explicitly and explain why the argument survives).
5. Concessions — the points from the previous output that you could NOT break.
Number your own claims (R1, R2...)."""
)

REBUTTAL = (
    SHARED_RULES
    + """

ROLE: Lead Analyst (rebuttal). You opened this debate; the red-team chain has now attacked it.
Respond to the MOST RECENT critique, but use the full debate.
Produce:
1. Point-by-point verdict on the strongest attacks: CONCEDE / PARTIALLY CONCEDE / REJECT, with reasons.
   Do not defend a point just because it was yours.
2. Counter-attacks on weaknesses in the critique itself.
3. Your REVISED recommendation and what changed from your opening position.
4. Remaining genuine uncertainties."""
)

SYNTHESIS = """You are the Chief of Staff writing for a busy executive. You have the full transcript of an
adversarial debate between three AI models about a decision. Write a decision brief in Markdown:

# Decision Brief: <short title>
**Recommendation:** one sentence. **Confidence:** Low / Medium / High (and why, in one line).

## The decision
2-3 sentences: what is being decided, deadline, what is at stake.

## Options considered
A table: Option | Upside | Downside | Reversibility | Verdict

## Arguments that survived attack
Bullet list — only claims that withstood red-teaming, with who challenged them.

## Arguments that were defeated or weakened
Bullet list — what fell apart and why.

## Unresolved disagreements
Where the models still disagree, and what evidence would settle it.

## Key risks & tripwires
Table: Risk | Likelihood | Impact | Early-warning signal | Pre-agreed response

## Before you decide — information to get
Numbered list of the highest-value facts to verify, with owner suggestions.

## Next steps (first 30 days)
Numbered, concrete.

Be honest about uncertainty. If the debate shows the right answer is "wait / get more information",
say so. Do not introduce new arguments that weren't in the debate except to flag gaps. Max ~800 words."""


# ---------------------------------------------------------------- clients --


@dataclass
class Turn:
    model: str  # "Claude" | "Gemini" | "OpenAI"
    role: str  # "Opening" | "Red team" | "Rebuttal" | "Brief"
    text: str
    seconds: float


def _retry(fn, attempts=4) -> str:
    for i in range(attempts):
        try:
            return fn()
        except Exception as e:
            if i == attempts - 1:
                raise
            wait = 2 ** (i + 1)
            print(f"   ! {type(e).__name__}: {str(e)[:160]} — retrying in {wait}s", file=sys.stderr)
            time.sleep(wait)
    raise RuntimeError(f"_retry gave up after {attempts} attempt(s)")


def call_claude(system: str, user: str) -> str:
    import anthropic
    from anthropic.types import TextBlock

    client = anthropic.Anthropic()  # ANTHROPIC_API_KEY

    def go():
        msg = client.messages.create(
            model=CLAUDE_MODEL,
            max_tokens=MAX_TOKENS,
            system=system,
            messages=[{"role": "user", "content": user}],
        )
        return "".join(block.text for block in msg.content if isinstance(block, TextBlock))

    return _retry(go)


def call_gemini(system: str, user: str) -> str:
    from google import genai
    from google.genai import types

    client = genai.Client()  # GEMINI_API_KEY or GOOGLE_API_KEY

    def go():
        r = client.models.generate_content(
            model=GEMINI_MODEL,
            contents=user,
            config=types.GenerateContentConfig(system_instruction=system, max_output_tokens=MAX_TOKENS * 2),
        )
        if not r.text:
            raise RuntimeError(
                f"Gemini returned no text (finish reason: {r.candidates[0].finish_reason if r.candidates else 'none'})"
            )
        return r.text

    return _retry(go)


def call_openai(system: str, user: str) -> str:
    from openai import OpenAI

    client = OpenAI()  # OPENAI_API_KEY

    def go():
        r = client.chat.completions.create(
            model=OPENAI_MODEL,
            messages=[{"role": "system", "content": system}, {"role": "user", "content": user}],
            max_completion_tokens=MAX_TOKENS * 3,  # reasoning models spend tokens thinking
        )
        text = r.choices[0].message.content
        if not text:
            raise RuntimeError("OpenAI returned no text (try raising COUNCIL_MAX_TOKENS)")
        return text

    return _retry(go)


def call_fake(name):
    def f(system: str, user: str) -> str:
        role = system.split("ROLE:")[1].split("\n")[0].strip() if "ROLE:" in system else "Brief"
        return f"[{name} — dry run — {role}] C1: placeholder claim. ({len(user)} chars of context received)"

    return f


CALLERS = {"Claude": call_claude, "Gemini": call_gemini, "OpenAI": call_openai}


# --------------------------------------------------------------- pipeline --


def transcript_text(question: str, context: str, turns: list[Turn]) -> str:
    parts = [f"## DECISION QUESTION\n{question}"]
    if context:
        parts.append(f"## BACKGROUND MATERIAL (from the executive)\n{context}")
    for i, t in enumerate(turns, 1):
        parts.append(f"## TURN {i} — {t.model} ({t.role})\n{t.text}")
    return "\n\n".join(parts)


def run_turn(name: str, role: str, system: str, user: str, turns: list[Turn], callers) -> Turn:
    label = {"Claude": CLAUDE_MODEL, "Gemini": GEMINI_MODEL, "OpenAI": OPENAI_MODEL}[name]
    print(f"\n▶ {name} [{label}] — {role} ...", flush=True)
    t0 = time.time()
    text = callers[name](system, user)
    turn = Turn(name, role, text.strip(), time.time() - t0)
    turns.append(turn)
    print(f"  ✓ {len(text.split())} words in {turn.seconds:.0f}s")
    preview = re.sub(r"\s+", " ", text)[:300]
    print(f"  {preview}…")
    return turn


def attack_prompt(question, context, turns):
    prev = turns[-1]
    return (
        transcript_text(question, context, turns[:-1])
        + f"\n\n=====\n## OUTPUT TO ATTACK — {prev.model} ({prev.role})\n{prev.text}\n=====\n\n"
        "Attack the output above. You may draw on earlier turns, but your target is the most recent one."
    )


ROUND_STEPS = (
    ("Gemini", "Red team", ATTACKER),
    ("OpenAI", "Red team", ATTACKER),
    ("Claude", "Rebuttal", REBUTTAL),
)


def run_council(question: str, context: str, rounds: int, callers) -> list[Turn]:
    turns: list[Turn] = []
    run_turn("Claude", "Opening", PROPOSER, transcript_text(question, context, []), turns, callers)
    for _ in range(rounds):
        _run_round(question, context, turns, callers)
    _run_brief(question, context, turns, callers)
    return turns


def _run_round(question: str, context: str, turns: list[Turn], callers) -> None:
    for name, role, system in ROUND_STEPS:
        run_turn(name, role, system, attack_prompt(question, context, turns), turns, callers)


def _run_brief(question: str, context: str, turns: list[Turn], callers) -> None:
    prompt = transcript_text(question, context, turns) + "\n\nWrite the decision brief now."
    run_turn("Claude", "Brief", SYNTHESIS, prompt, turns, callers)


def write_report(question: str, turns: list[Turn], outdir: Path, rounds: int) -> Path:
    path = _report_path(question, outdir)
    md_text = "\n".join(_report_lines(question, turns, rounds))
    path.write_text(md_text, encoding="utf-8")
    return write_html_copy(path, md_text, question)


def _report_path(question: str, outdir: Path) -> Path:
    outdir.mkdir(parents=True, exist_ok=True)
    stamp = dt.datetime.now().strftime("%Y-%m-%d_%H%M")
    slug = re.sub(r"[^a-z0-9]+", "-", question.lower())[:50].strip("-") or "decision"
    return outdir / f"{stamp}_{slug}.md"


def _report_lines(question: str, turns: list[Turn], rounds: int) -> list[str]:
    header = [
        turns[-1].text,
        "\n---\n",
        "# Appendix: Full adversarial debate\n",
        _generation_note(rounds),
        f"**Question:** {question}\n",
    ]
    body = [f"\n## Turn {i} — {t.model}: {t.role}\n\n{t.text}\n" for i, t in enumerate(turns[:-1], 1)]
    footer = "\n---\n*AI-generated decision support. Verify facts and figures before acting.*\n"
    return [*header, *body, footer]


def _generation_note(rounds: int) -> str:
    return (
        f"*Generated {dt.datetime.now():%d %b %Y %H:%M} · {rounds} round(s) · "
        f"Claude={CLAUDE_MODEL}, Gemini={GEMINI_MODEL}, OpenAI={OPENAI_MODEL}*\n"
    )


def write_html_copy(path: Path, md_text: str, question: str) -> Path:
    try:
        document = _html_document(md_text, question)
    except ImportError:
        return path
    html_path = path.with_suffix(".html")
    html_path.write_text(document, encoding="utf-8")
    return html_path


def _html_document(md_text: str, question: str) -> str:
    body = importlib.import_module("markdown").markdown(md_text, extensions=["tables", "sane_lists"])
    title = html.escape(question[:80])
    return f"<!doctype html><meta charset='utf-8'><title>{title}</title><style>{_HTML_CSS}</style>{body}"


_HTML_CSS = (
    "body{font:16px/1.6 Segoe UI,system-ui,sans-serif;max-width:860px;margin:40px auto;padding:0 20px;color:#1f2328}"
    "h1{border-bottom:2px solid #d0d7de;padding-bottom:8px}h2{margin-top:32px}"
    "table{border-collapse:collapse;width:100%;margin:12px 0}th,td{border:1px solid #d0d7de;padding:6px 10px;text-align:left;vertical-align:top}"
    "th{background:#f6f8fa}hr{margin:40px 0}"
)


def read_document(p: Path, limit: int = 60_000) -> str:
    """Turn txt/md/csv/pdf/docx/xlsx into plain text for the models."""
    try:
        text = _read_text_for(p)
    except Exception as e:
        print(f"  ! Could not read {p.name}: {e}", file=sys.stderr)
        return "(file could not be read)"
    if len(text) > limit:
        text = text[:limit] + f"\n...[truncated — {len(text):,} characters total]"
    return text


def _read_text_for(p: Path) -> str:
    ext = p.suffix.lower()
    if ext == ".pdf":
        return _read_pdf(p)
    if ext == ".docx":
        return _read_docx(p)
    if ext in (".xlsx", ".xlsm"):
        return _read_xlsx(p)
    return p.read_text(encoding="utf-8", errors="replace")


def _read_pdf(p: Path) -> str:
    reader = importlib.import_module("pypdf").PdfReader
    return "\n".join(page.extract_text() or "" for page in reader(str(p)).pages)


def _read_docx(p: Path) -> str:
    document = importlib.import_module("docx").Document(str(p))
    text = "\n".join(par.text for par in document.paragraphs)
    return text + _docx_tables(document)


def _docx_tables(d) -> str:
    return "".join("\n" + "\n".join(" | ".join(c.text for c in row.cells) for row in t.rows) for t in d.tables)


def _read_xlsx(p: Path) -> str:
    openpyxl = importlib.import_module("openpyxl")
    workbook = openpyxl.load_workbook(str(p), data_only=True, read_only=True)
    return "".join(_sheet_text(ws) for ws in workbook.worksheets)


def _sheet_text(ws) -> str:
    rows = [
        " | ".join("" if v is None else str(v) for v in row)
        for row in ws.iter_rows(values_only=True)
        if any(v is not None for v in row)
    ]
    return f"\n[Sheet: {ws.title}]\n" + "".join(f"{row}\n" for row in rows)


def list_models():
    for name, fn in _model_sources():
        try:
            print(f"\n{name}:\n  " + "\n  ".join(fn()))
        except Exception as e:
            print(f"\n{name}: could not list ({e})")


def _model_sources():
    return [("Anthropic", _anthropic_models), ("OpenAI", _openai_models), ("Gemini", _gemini_models)]


def _anthropic_models() -> list[str]:
    return [m.id for m in __import__("anthropic").Anthropic().models.list(limit=50)]


def _openai_models() -> list[str]:
    return sorted(m.id for m in __import__("openai").OpenAI().models.list() if m.id.startswith(("gpt", "o")))


def _gemini_models() -> list[str]:
    client = __import__("google.genai", fromlist=["Client"]).Client()
    return [
        m.name.removeprefix("models/") for m in client.models.list() if "generateContent" in (m.supported_actions or [])
    ]


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(description="Claude → Gemini → OpenAI → Claude adversarial decision council")
    ap.add_argument("question", nargs="?", help="The decision question (or use --file)")
    ap.add_argument("--file", "-f", help="Read the decision question/brief from a text/markdown file")
    ap.add_argument(
        "--context", "-c", action="append", default=[], help="Background file(s) to include (txt, md, csv). Repeatable."
    )
    ap.add_argument(
        "--rounds", "-r", type=int, default=1, help="Full Gemini→OpenAI→Claude loops before the brief (default 1)"
    )
    ap.add_argument("--out", "-o", default="reports", help="Output folder (default ./reports)")
    ap.add_argument("--dry-run", action="store_true", help="Run the pipeline with fake responses (no API calls)")
    ap.add_argument("--list-models", action="store_true", help="List model IDs available to your API keys")
    ap.add_argument("--open", action="store_true", help="Open the finished brief in your browser")
    return ap


def main():
    a = build_parser().parse_args()
    if a.list_models:
        return list_models()
    question = _resolve_question(a)
    _run_and_report(a, question, _build_context(a), resolve_callers(a.dry_run))


def _resolve_question(a) -> str:
    question = Path(a.file).read_text(encoding="utf-8") if a.file else a.question
    if question:
        return question
    question = input("Decision question: ").strip()
    if not question:
        build_parser().error("No question given.")
    return question


def _build_context(a) -> str:
    context = "\n\n".join(f"### {Path(p).name}\n{read_document(Path(p))}" for p in a.context)
    if a.context:
        print(f"Included {len(a.context)} background file(s): {', '.join(Path(p).name for p in a.context)}")
    return context


def _run_and_report(a, question: str, context: str, callers) -> None:
    print(f"Adversarial Council — {a.rounds} round(s), {1 + 3 * a.rounds + 1} model calls")
    try:
        turns = run_council(question.strip(), context, a.rounds, callers)
    except KeyboardInterrupt:
        sys.exit("\nStopped.")
    path = write_report(question.strip().splitlines()[0], turns, Path(a.out), a.rounds)
    print(f"\n✅ Decision brief saved to: {path.resolve()}")
    if a.open:
        _open_report(path)


def _open_report(path: Path) -> None:
    import webbrowser

    webbrowser.open(path.resolve().as_uri())


def resolve_callers(dry_run: bool):
    if dry_run:
        return {k: call_fake(k) for k in CALLERS}
    missing = [
        k
        for k, v in {
            "ANTHROPIC_API_KEY": os.getenv("ANTHROPIC_API_KEY"),
            "OPENAI_API_KEY": os.getenv("OPENAI_API_KEY"),
            "GEMINI_API_KEY": os.getenv("GEMINI_API_KEY") or os.getenv("GOOGLE_API_KEY"),
        }.items()
        if not v
    ]
    if missing:
        sys.exit(f"Missing API keys: {', '.join(missing)}. Copy .env.example to .env and fill them in.")
    return CALLERS


if __name__ == "__main__":
    main()
