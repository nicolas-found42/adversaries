# Adversarial Council

A command-line tool that answers a decision question by running a structured adversarial debate across four AI providers (Mistral, OpenRouter, Ollama Cloud, OpenCode Go), then synthesising the result into an executive decision brief.

## Language

### The debate

**Decision question**:
The choice under deliberation, supplied as an argument, from `--file`, or at the prompt. It is framed with a deadline and limits, not as an open-ended topic.
_Avoid_: prompt, query, ask

**Background material**:
Reference documents the executive supplies with `--context` (txt, md, csv, and optionally pdf/docx/xlsx) so the models argue from real numbers instead of assumptions.
_Avoid_: context, attachments, inputs

**Turn**:
One model's output within the debate, carrying its model label, role, text, and elapsed seconds.
_Avoid_: message, response, answer

**Role**:
The job a turn performs: `Opening`, `Red team`, `Rebuttal`, or `Brief`.
_Avoid_: stage, step

**Opening position**:
The first turn, where the Lead Analyst frames the decision, lists at least three options including delay, and recommends one.
_Avoid_: proposal, first answer

**Red team**:
A turn that attacks the most recent output, targeting its numbered claims rather than summarising it.
_Avoid_: critique, review, feedback

**Rebuttal**:
The Lead Analyst's response to the red-team chain, marking each attack CONCEDE / PARTIALLY CONCEDE / REJECT and stating a revised recommendation.
_Avoid_: defence, reply

**Round**:
One OpenRouter → Ollama Cloud → OpenCode Go → Mistral rebuttal loop after the opening. `--rounds N` runs N of them before the brief.
_Avoid_: iteration, pass, cycle

**Chain**:
The fixed sequence of models and roles a council run follows: opening, N rounds, then the brief.
_Avoid_: pipeline, workflow

**Claim**:
A single numbered assertion (C1, C2… from the Lead Analyst; R1, R2… from red teams) that later turns cite when attacking or conceding.
_Avoid_: point, argument

**Transcript**:
The decision question, background material, and every turn so far, rendered as text and handed to the next model.
_Avoid_: history, log, conversation

### Output

**Brief**:
The Closing turn: a one-page executive decision brief with recommendation, confidence, options table, surviving and defeated arguments, unresolved disagreements, risks, and next steps.
_Avoid_: summary, synthesis, conclusion

**Report**:
The artefacts written to the output folder — a Markdown file plus an HTML copy when `markdown` is installed.
_Avoid_: brief (the brief is the content; the report is the file)

### Runtime

**Caller**:
A per-vendor adapter (`call_mistral`, `call_openrouter`, `call_ollama_cloud`, `call_opencode_go`, or a fake) invoked by model label for one turn.
_Avoid_: client (the vendor SDK object), provider

**Model label**:
The vendor key a turn is filed under: `Mistral`, `OpenRouter`, `Ollama-Cloud`, or `OpenCode-Go`. It selects the model ID and the caller.

**Dry run**:
A council run with fake callers that exercises the whole chain without API calls or spend.
_Avoid_: mock, stub, test mode
