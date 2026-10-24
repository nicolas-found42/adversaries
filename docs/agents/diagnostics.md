# Diagnosing council runs

## Audit on 2026-10-23

Before this change, nine saved JSON sessions contained argument text, stage order,
elapsed seconds, and (in newer files) role/model selections and resumable configuration.
Reports and checkpoints established **what was saved**, but not the full sequence of
what happened. There were no persistent request/event logs. Both transports discarded
stderr, and backend events and UI errors disappeared when the process/window closed.
No remote telemetry collector was found.

Consequently the earlier Ollama failure cannot be attributed retrospectively to output
exhaustion, reasoning, timeout, or a provider error. The successful six-call Mistral run
establishes UI/pipeline behavior, but its old records cannot establish token spend or
reasoning overhead. New instrumentation cannot reconstruct those missing measurements.

## Files available now

| Evidence | Location | What it establishes |
|---|---|---|
| Session checkpoint | `reports/<session>.json` | Completed turns, separate failed/incomplete turn, partial final output, response diagnostics, run config, event snapshot |
| Durable event journal | `reports/<session>.events.jsonl` | UTC times, sequence, run/call IDs, stage/request start, response/failure and terminal run state |
| Private request snapshot | `reports/<run_id>/<call_id>.request.json` | Actual system/user messages after context assembly, including supplied material and truncation markers |
| Transport lifecycle | `reports/transport.events.jsonl` | Spawn PID, stop/cancel/disconnect/replacement reason, process exit; linked by PID/time on history reload |
| Capability cache | `.council-cache/<hash>.json` | Provider/model limits, reasoning support/controls, provenance, pricing and warnings; refreshed after one hour or a model-catalog refresh |

The Python event journal flushes and fsyncs each record. Checkpoints use an atomic
replacement. These JSON/journal files are mode 0600, request directories are created
mode 0700 on POSIX, and reports/cache are gitignored. Existing directory permissions
are not changed. Native Windows uses the user's filesystem ACLs.

Engine run-start records include the Python/platform versions, source fingerprints,
configuration without question/context contents, question/context hashes, and PID.
Request records include selected and returned model IDs, policy provenance, conservative
input estimate, output allowance, reasoning policy, response/request IDs, finish reason,
reported usage (including reasoning/cache details when supplied), latency, and cost.
Errors retain type, HTTP status, safe message, request ID, and stack frame locations
without local variables or source lines. SDK and application completion retries are off.

Provider-reported cost is labeled separately from catalog-derived estimates. Missing
usage or cost stays unknown, never zero. Estimates do not account reliably for every
cache discount, tier, subscription, or future pricing change.

## UI and export

The shared desktop/browser **Diagnostics** tab (shortcut **6**) shows per-stage
policy, usage, finish reason and cost. Expand **Request details** or **Event history**
for IDs, provenance, warnings and the event sequence. **Copy diagnostics** copies a
JSON bundle of run/stage metadata; it excludes argument text and request-snapshot
contents. Private prompt snapshots remain local files for an authorized investigation.
Known environment credentials and bearer values are redacted; no API headers or raw
reasoning traces are deliberately stored. Private request snapshots contain the user's
question/background and must be treated as sensitive project data. Retention is local
and manual: removing a session's artifacts and its run-ID directory removes its logs
and snapshots; the shared transport file and capability cache are separate.

The session loader refreshes event history from the journal, tolerating a partial last
line after a crash, rather than relying only on the last checkpoint's event snapshot.
Old sessions say that historical request events were not recorded. Incomplete output
remains readable, clearly labeled, and excluded from subsequent prompts and the claim
ledger. A deliberate stage retry preserves diagnostics on resumed completed turns;
it creates a new run/journal rather than overwriting the failed attempt.

## Automatic request policy

There is no universal 4,000-token ceiling and no token slider. The old
`COUNCIL_MAX_TOKENS` override is no longer read.

1. Discover capabilities for the exact selected provider/model. Metadata reads are
   not inference requests; SDK retries are disabled for inference.
2. If both output maximum and context window are known, allow the smaller of the
   output maximum and conservatively estimated context room. If only output maximum
   is known, use it. If output maximum is undisclosed, omit the output override and
   label it provider-default/unknown; this does **not** promise unlimited output.
3. Reject a conservative context overflow before inference. The estimator is UTF-8
   bytes plus message/framing allowance, **not an exact model tokenizer**; it can stop
   early. No additional evidence is silently trimmed or summarized by this policy.
   The existing document reader's 60,000-character truncation remains marked in the
   assembled request snapshot.
4. Always request positive reasoning for supported models, choosing the least
   advertised effort or minimum positive budget. OpenRouter requests supporting
   endpoints with `require_parameters`; boolean-only Ollama thinking is enabled;
   native fixed reasoning is identified. Unknown reasoning capability or a known
   thinking model with unknown positive controls stops before inference.
5. Preserve the final-answer channel only. Ignore separate thinking chunks/fields;
   strip a leading `<think>…</think>` wrapper when present. Length exhaustion,
   missing final text, tool/filter finishes, or absent finish confirmation are explicit
   incomplete stages, even if partial text exists.
6. Never automatically retry, continue, summarize, switch models or fall back after a
   failed/incomplete call. `--max-attempts` accepts only 1, including legacy CLI runs.
   Retrying from the UI requires a user action and can incur another call.

Mistral Small/Medium's documented positive level is `high`; native Magistral has no
adjustable level. Ollama's `/api/show` supplies actual thinking values. Go's IDs-only
catalog cannot establish output caps; the exact supported DeepSeek V4/V4.1 Flash/Pro
IDs use a documented `low` profile with provenance/warning, not an invented output
maximum. Unsupported arbitrary future IDs stop on unknown policy. See the
[research and sources](../research/2026-10-23-token-budgets-and-reasoning.md).

## Investigation workflow

1. Find the session by question/date in history or `reports/` and open its diagnostics.
2. Check terminal events. `request_started` without `request_finished`/`request_failed`
   means no response was observed. Missing terminal events imply interruption or a
   crash, not proof of provider failure. Correlate the engine PID with transport events.
3. Check policy/metadata source, finish reason, usage, actual model and request ID.
   For context failures, compare the estimate and limits with the private request
   snapshot; for incomplete output, keep the partial answer outside the debate.
4. Compare code fingerprints and configuration before reproducing. Use the HTTP mocks
   in `tests/test_inference.py` and replay tests in Rust first. These exercise the SDK
   payload and one-request invariant without network inference. A later live test
   requires an explicitly bounded call/spend allowance and should use the same prompt.
5. Run the repository quality gates and inspect both UI surfaces after changes.

## Remaining visibility limits

- Providers may omit reasoning/cache tokens, exact routed backend, finish details,
  output limits or charged cost. Requested reasoning controls are not evidence of a
  measured savings or a provider honoring every field.
- Positive reasoning inherently costs time/tokens. A large output allowance and a
  low effort request do not constitute a hard cost or wall-clock cap. Cancellation
  or the 90-second local SDK timeout does not prove remote generation/billing stopped.
- A hard kill, disk failure or failure before Python run initialization can leave no
  terminal engine record. Transport PID/exit data narrows the failure; discarded raw
  stderr and provider-internal traces remain unavailable. Logs are local, not a
  crash-proof remote collector. Native user cancellation and OS termination may
  produce a missing terminal engine event; transport events distinguish observed stops.
- Raw reasoning is deliberately unavailable; only supplied usage and presence metadata
  can explain reasoning expense. Older sessions retain their original blind spots.

## Verification recorded for this change

- Authenticated **metadata-only** reads normalized all four configured defaults:
  Mistral Small/latest (262,144 context, positive `high`, output maximum undisclosed),
  OpenRouter Llama 3.3 70B (131,072 context, 16,384 output, no reasoning),
  Ollama Cloud DeepSeek V4.1 Flash (1,048,576 context, `low`, output maximum undisclosed),
  and Go DeepSeek V4.1 Flash (`low` profile, context/output maxima undisclosed).
  These checks built parameters but sent no completion request.
- SDK HTTP mocks verify payloads, one inference attempt, rate-limit failures,
  reasoning-only/truncated/chunked responses, metadata discovery, preflight, and
  checkpoint/request-ID preservation. Rust tests verify failed-turn replay and ledger exclusion.
- A labeled offline truncated-response fixture was opened through native computer use
  and the Jev-controlled browser. Both surfaces showed incomplete output and diagnostics.
  A native process dry run completed all six stages and persisted its journal with zero
  `request_started` inference events.
