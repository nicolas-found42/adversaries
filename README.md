# Adversarial Council

Ask a decision question once. It runs a red-team chain across three AI vendors and gives you a one-page executive decision brief.

```
Claude (opening position)
   → Gemini   attacks Claude's arguments
   → OpenAI   attacks Gemini's critique
   → Claude   concedes / rebuts, revises its recommendation
   → Claude   writes the Decision Brief (with the full debate attached as an appendix)
```

Each model gets the whole debate so far, but its job is to attack the **most recent** output. Every claim is numbered (C1, R1…) so later models can target specific points.

## Setup (one time, ~5 minutes)

1. Install Python 3.10+.
2. In this folder, run:
   ```
   pip install -r requirements.txt
   ```
3. Copy `.env.example` to `.env` and paste your three API keys:
   - Anthropic: https://console.anthropic.com
   - OpenAI: https://platform.openai.com/api-keys
   - Gemini: https://aistudio.google.com/apikey
4. Check which models your keys can use, and set the ones you want in `.env`:
   ```
   python council.py --list-models
   ```

## Use

```
python council.py "Should we open a Nairobi office in Q2 2027 or serve East Africa remotely?"
```

Give the models your numbers and documents (text, Markdown, or CSV):

```
python council.py --file decision.md --context pnl_2026.csv --context market_notes.md
```

Run more attack loops for a high-stakes call:

```
python council.py --rounds 2 "..."
```

Test it without spending anything on API calls:

```
python council.py --dry-run "test"
```

The report is saved to `./reports/<date>_<question>.md`. The brief comes first, then the full debate.

## What's in the brief

Recommendation and confidence level, the options compared, the arguments that held up against attack, the ones that didn't, where the models still disagree, risks with early-warning signals, what to find out before you decide, and next steps for the first 30 days.

## Getting better results

- **Write the question as a decision**, with a deadline and limits: "Hire 3 engineers in Lagos or outsource to an agency, budget ₦X, decide by Dec 1".
- **Give it your real data** with `--context`. If you don't, the models fall back on generic assumptions.
- **Pay most attention to "Unresolved disagreements".** When three different models still disagree after the debate, that is usually where your own judgement or more research is needed.
- **Cost:** one round is 5 model calls, usually a few cents to about $1 depending on the models and how much context you include.
- **Change the order or roles:** edit `run_council()` in `council.py`. The prompts are plain text at the top of the file.

*This is decision support, not a decision. Check facts and figures before acting.*
