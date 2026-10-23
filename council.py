#!/usr/bin/env python3
"""
Adversarial Council — executive decision support via a cross-model red-team chain.

Flow (one round):
    Mistral (proposal) -> OpenRouter (attacks Mistral) -> Ollama Cloud (attacks OpenRouter) -> OpenCode Go (attacks Ollama Cloud) -> Mistral (rebuts)
Then Mistral writes a one-page executive decision brief from the whole debate.

With --rounds N > 1, the chain repeats the attack and rebuttal loop for extra rounds before the brief.

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
from dataclasses import dataclass, field
from pathlib import Path

for _s in (sys.stdout, sys.stderr):  # Windows consoles default to a legacy codepage
    _reconfigure = getattr(_s, "reconfigure", None)
    if _reconfigure is not None:
        _reconfigure(encoding="utf-8", errors="replace")


def _load_env_files():
    try:
        from dotenv import load_dotenv

        load_dotenv(Path(__file__).with_name(".env"))
    except ImportError:
        pass
    _load_zshenv()


def _load_zshenv():
    zshenv = Path.home() / ".zshenv"
    if not zshenv.is_file():
        return
    for line in zshenv.read_text(encoding="utf-8", errors="replace").splitlines():
        _parse_export_line(line.strip())


def _parse_export_line(line: str):
    if not line.startswith("export "):
        return
    k, _, v = line.removeprefix("export ").partition("=")
    k, v = k.strip(), v.strip("'\"")
    if k and v and k not in os.environ:
        os.environ[k] = v


_load_env_files()

# ---------------------------------------------------------------- config ---

MISTRAL_MODEL = os.getenv("MISTRAL_MODEL", "mistral-small-latest")
OPENROUTER_MODEL = os.getenv("OPENROUTER_MODEL", "meta-llama/llama-3.3-70b-instruct")
OLLAMA_CLOUD_MODEL = os.getenv("OLLAMA_CLOUD_MODEL", "deepseek-v4.1-flash")
OPENCODE_GO_MODEL = os.getenv("OPENCODE_GO_MODEL", os.getenv("OPENCODE_MODEL", "deepseek-v4.1-flash"))
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
    model: str  # "Mistral" | "OpenRouter" | "Ollama-Cloud" | "OpenCode-Go"
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


def _chat_completion(client, model: str, system: str, user: str) -> str:
    def go():
        r = client.chat.completions.create(
            model=model,
            messages=[{"role": "system", "content": system}, {"role": "user", "content": user}],
            max_tokens=MAX_TOKENS,
        )
        msg = r.choices[0].message
        text = msg.content or getattr(msg, "reasoning_content", "") or ""
        if not text:
            raise RuntimeError(f"{model} returned no text")
        return text

    return _retry(go)


def call_mistral(system: str, user: str) -> str:
    from openai import OpenAI

    client = OpenAI(base_url="https://api.mistral.ai/v1", api_key=os.environ.get("MISTRAL_API_KEY", ""))
    return _chat_completion(client, MODEL_LABELS["Mistral"], system, user)


def call_openrouter(system: str, user: str) -> str:
    from openai import OpenAI

    client = OpenAI(base_url="https://openrouter.ai/api/v1", api_key=os.environ.get("OPENROUTER_API_KEY", ""))
    return _chat_completion(client, MODEL_LABELS["OpenRouter"], system, user)


def call_ollama_cloud(system: str, user: str) -> str:
    from openai import OpenAI

    key = os.getenv("OLLAMA_CLOUD_API_KEY") or os.getenv("OLLAMA_API_KEY", "")
    client = OpenAI(base_url="https://ollama.com/v1", api_key=key)
    return _chat_completion(client, MODEL_LABELS["Ollama-Cloud"], system, user)


def call_opencode_go(system: str, user: str) -> str:
    from openai import OpenAI

    key = os.getenv("OPENCODE_API_KEY") or os.getenv("OPENCODE_GO_API_KEY", "")
    headers = {"x-opencode-session": "adversarial-council"}
    client = OpenAI(base_url="https://opencode.ai/zen/go/v1", api_key=key, default_headers=headers)
    return _chat_completion(client, MODEL_LABELS["OpenCode-Go"], system, user)


def call_fake(name):
    def f(system: str, user: str) -> str:
        role = system.split("ROLE:")[1].split("\n")[0].strip() if "ROLE:" in system else "Brief"
        return f"[{name} — dry run — {role}] C1: placeholder claim. ({len(user)} chars of context received)"

    return f


CALLERS = {
    "Mistral": call_mistral,
    "OpenRouter": call_openrouter,
    "Ollama-Cloud": call_ollama_cloud,
    "OpenCode-Go": call_opencode_go,
}
MODEL_LABELS = {
    "Mistral": MISTRAL_MODEL,
    "OpenRouter": OPENROUTER_MODEL,
    "Ollama-Cloud": OLLAMA_CLOUD_MODEL,
    "OpenCode-Go": OPENCODE_GO_MODEL,
}
PROVIDER_ORDER = ("Mistral", "OpenRouter", "Ollama-Cloud", "OpenCode-Go")


# --------------------------------------------------------------- pipeline --


@dataclass
class Session:
    question: str
    context: str
    callers: dict
    rounds: int = 1
    turns: list[Turn] = field(default_factory=list)
    providers: tuple[str, ...] = PROVIDER_ORDER


def transcript_text(session: Session, turns: list[Turn] | None = None) -> str:
    turns = session.turns if turns is None else turns
    parts = [f"## DECISION QUESTION\n{session.question}"]
    if session.context:
        parts.append(f"## BACKGROUND MATERIAL (from the executive)\n{session.context}")
    for i, t in enumerate(turns, 1):
        parts.append(f"## TURN {i} — {t.model} ({t.role})\n{t.text}")
    return "\n\n".join(parts)


def run_turn(session: Session, name: str, role: str, system: str, user: str) -> Turn:
    label = MODEL_LABELS[name]
    print(f"\n▶ {name} [{label}] — {role} ...", flush=True)
    t0 = time.time()
    text = session.callers[name](system, user)
    turn = Turn(name, role, text.strip(), time.time() - t0)
    session.turns.append(turn)
    print(f"  ✓ {len(text.split())} words in {turn.seconds:.0f}s")
    preview = re.sub(r"\s+", " ", text)[:300]
    print(f"  {preview}…")
    return turn


def attack_prompt(session: Session) -> str:
    prev = session.turns[-1]
    return (
        transcript_text(session, session.turns[:-1])
        + f"\n\n=====\n## OUTPUT TO ATTACK — {prev.model} ({prev.role})\n{prev.text}\n=====\n\n"
        "Attack the output above. You may draw on earlier turns, but your target is the most recent one."
    )


def _round_steps(session: Session) -> list[tuple[str, str, str]]:
    lead = session.providers[0]
    attackers = [p for p in session.providers if p != lead]
    steps = [(p, "Red team", ATTACKER) for p in attackers]
    steps.append((lead, "Rebuttal", REBUTTAL))
    return steps


def _default_round_steps() -> tuple[tuple[str, str, str], ...]:
    attackers = [p for p in PROVIDER_ORDER if p != "Mistral"]
    steps = [(p, "Red team", ATTACKER) for p in attackers]
    steps.append(("Mistral", "Rebuttal", REBUTTAL))
    return tuple(steps)


ROUND_STEPS = _default_round_steps()


def run_council(session: Session) -> list[Turn]:
    lead = session.providers[0]
    run_turn(session, lead, "Opening", PROPOSER, transcript_text(session, []))
    for _ in range(session.rounds):
        _run_round(session)
    _run_brief(session)
    return session.turns


def _run_round(session: Session) -> None:
    for name, role, system in _round_steps(session):
        run_turn(session, name, role, system, attack_prompt(session))


def _run_brief(session: Session) -> None:
    lead = session.providers[0]
    prompt = transcript_text(session) + "\n\nWrite the decision brief now."
    run_turn(session, lead, "Brief", SYNTHESIS, prompt)


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
    labels = ", ".join(f"{name}={MODEL_LABELS[name]}" for name in PROVIDER_ORDER)
    return f"*Generated {dt.datetime.now():%d %b %Y %H:%M} · {rounds} round(s) · {labels}*\n"


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
    return [
        ("Mistral", _mistral_models),
        ("OpenRouter", _openrouter_models),
        ("Ollama-Cloud", _ollama_cloud_models),
        ("OpenCode-Go", _opencode_go_models),
    ]


def _fetch_models(base_url: str, key: str | None) -> list[str]:
    if not key:
        return ["(no API key set)"]
    from openai import OpenAI

    client = OpenAI(base_url=base_url, api_key=key)
    return sorted(m.id for m in client.models.list().data)


def _mistral_models() -> list[str]:
    return _fetch_models("https://api.mistral.ai/v1", os.getenv("MISTRAL_API_KEY"))


def _openrouter_models() -> list[str]:
    return _fetch_models("https://openrouter.ai/api/v1", os.getenv("OPENROUTER_API_KEY"))


def _ollama_cloud_models() -> list[str]:
    key = os.getenv("OLLAMA_CLOUD_API_KEY") or os.getenv("OLLAMA_API_KEY")
    return _fetch_models("https://ollama.com/v1", key)


def _opencode_go_models() -> list[str]:
    key = os.getenv("OPENCODE_API_KEY") or os.getenv("OPENCODE_GO_API_KEY")
    return _fetch_models("https://opencode.ai/zen/go/v1", key)


def build_parser() -> argparse.ArgumentParser:
    desc = "Mistral → OpenRouter → Ollama Cloud → OpenCode Go adversarial decision council"
    ap = argparse.ArgumentParser(description=desc)
    _add_parser_args(ap)
    return ap


def _add_parser_args(ap: argparse.ArgumentParser) -> None:
    ap.add_argument("question", nargs="?", help="The decision question (or use --file)")
    ap.add_argument("--file", "-f", help="Read the decision question/brief from a text/markdown file")
    ap.add_argument(
        "--context", "-c", action="append", default=[], help="Background file(s) to include (txt, md, csv). Repeatable."
    )
    ap.add_argument("--rounds", "-r", type=int, default=1, help="Full attack loops before the brief (default 1)")
    ap.add_argument("--out", "-o", default="reports", help="Output folder (default ./reports)")
    ap.add_argument("--dry-run", action="store_true", help="Run the pipeline with fake responses (no API calls)")
    ap.add_argument("--list-models", action="store_true", help="List model IDs available to your API keys")
    ap.add_argument("--open", action="store_true", help="Open the finished brief in your browser")
    ap.add_argument("--providers", default=None, help="Comma-separated providers (e.g. mistral,openrouter)")
    ap.add_argument("--skip", action="append", default=[], help="Provider to skip (repeatable)")


def _normalize_provider(name: str) -> str:
    aliases = {
        "mistral": "Mistral",
        "openrouter": "OpenRouter",
        "ollama-cloud": "Ollama-Cloud",
        "ollamacloud": "Ollama-Cloud",
        "opencode-go": "OpenCode-Go",
        "opencodego": "OpenCode-Go",
        "opencode": "OpenCode-Go",
    }
    canonical = aliases.get(name.lower().strip())
    if not canonical:
        sys.exit(f"Unknown provider '{name}'. Valid providers: {', '.join(PROVIDER_ORDER)}")
    return canonical


def _resolve_providers(a) -> tuple[str, ...]:
    raw = [p.strip() for p in a.providers.split(",") if p.strip()] if a.providers else list(PROVIDER_ORDER)
    selected = [_normalize_provider(p) for p in raw]
    to_skip = {_normalize_provider(p) for p in a.skip}
    result = tuple(p for p in selected if p not in to_skip)
    if not result:
        sys.exit("Error: No providers selected after filtering.")
    return result


def main():
    a = build_parser().parse_args()
    if a.list_models:
        return list_models()
    _validate_rounds(a)
    providers = _resolve_providers(a)
    question = _resolve_question(a)
    callers = resolve_callers(a.dry_run, providers)
    _run_and_report(a, question, _build_context(a), callers, providers)


def _validate_rounds(a) -> None:
    if a.rounds < 1:
        build_parser().error("--rounds must be at least 1.")


def _resolve_question(a) -> str:
    question = _read_question_file(a.file) if a.file else a.question or _prompt_for_question()
    if not question.strip():
        build_parser().error("No question given.")
    return question


def _read_question_file(path_str: str) -> str:
    try:
        return Path(path_str).read_text(encoding="utf-8")
    except FileNotFoundError:
        build_parser().error(f"Question file not found: {path_str}")
    except OSError as e:
        build_parser().error(f"Cannot read question file: {e}")


def _prompt_for_question() -> str:
    try:
        return input("Decision question: ").strip()
    except EOFError:
        build_parser().error("No question given.")


def _build_context(a) -> str:
    context = "\n\n".join(f"### {Path(p).name}\n{read_document(Path(p))}" for p in a.context)
    if a.context:
        dest = sys.stderr if getattr(a, "json_stream", False) else sys.stdout
        names = ", ".join(Path(p).name for p in a.context)
        print(f"Included {len(a.context)} background file(s): {names}", file=dest)
    return context


def _run_and_report(a, question: str, context: str, callers, providers: tuple[str, ...] = PROVIDER_ORDER) -> None:
    total_calls = 1 + len(providers) * a.rounds + 1
    print(f"Adversarial Council — {a.rounds} round(s), {total_calls} model calls")
    session = Session(question.strip(), context, callers, a.rounds, providers=providers)
    try:
        turns = run_council(session)
    except KeyboardInterrupt:
        sys.exit("\nStopped.")
    path = write_report(question.strip().splitlines()[0], turns, Path(a.out), a.rounds)
    print(f"\n✅ Decision brief saved to: {path.resolve()}")
    if a.open:
        _open_report(path)


def _open_report(path: Path) -> None:
    import webbrowser

    webbrowser.open(path.resolve().as_uri())


def _missing_keys(providers: tuple[str, ...]) -> list[str]:
    key_map = {
        "Mistral": ("MISTRAL_API_KEY", os.getenv("MISTRAL_API_KEY")),
        "OpenRouter": ("OPENROUTER_API_KEY", os.getenv("OPENROUTER_API_KEY")),
        "Ollama-Cloud": ("OLLAMA_CLOUD_API_KEY", os.getenv("OLLAMA_CLOUD_API_KEY") or os.getenv("OLLAMA_API_KEY")),
        "OpenCode-Go": ("OPENCODE_API_KEY", os.getenv("OPENCODE_API_KEY") or os.getenv("OPENCODE_GO_API_KEY")),
    }
    return [key_map[p][0] for p in providers if not key_map[p][1]]


def resolve_callers(dry_run: bool, providers: tuple[str, ...] = PROVIDER_ORDER):
    if dry_run:
        return {k: call_fake(k) for k in providers}
    missing = _missing_keys(providers)
    if missing:
        sys.exit(f"Missing API keys: {', '.join(missing)}. Set them in ~/.zshenv or .env.")
    return {k: CALLERS[k] for k in providers}


if __name__ == "__main__":
    main()
