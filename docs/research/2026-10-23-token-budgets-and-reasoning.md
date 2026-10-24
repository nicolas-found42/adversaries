# Output limits, context overflow, and reasoning in multi-model runs

**Date:** 2026-10-23
**Scope:** How comparable open-source LLM applications handle output length and context overflow; implications for Adversarial Council's independent role/model choices. Includes provider controls and an audit of the current Python wrapper.
**Method:** Official documentation and first-party source code only. Source snapshots are pinned where possible. No paid model calls were made for this research. Recommendations are design judgments, not claims that a particular number has been validated by a benchmark.

## Implemented decision after user review

The final user requirement supersedes the earlier suggestion to disable optional thinking:
**always enable reasoning when supported, at its lowest positive advertised level**.
Council now uses provider/model metadata for output allowance and context preflight,
records response diagnostics, preserves incomplete final output, and sends no automatic
retry/continuation/summarization calls. Unknown output maxima retain provider defaults;
unknown reasoning capability/controls stop before inference. See the
[implementation and diagnostic audit](../agents/diagnostics.md).

Authenticated metadata-only checks found Mistral Small/latest reasoning support and a
262,144-token context, OpenRouter output/context/effort metadata, and Ollama Cloud
`/api/show` thinking values `[false, "low", "high", "max"]` for the configured Flash
model. Go's model endpoint supplied IDs only. No paid completions were made in this
implementation/research phase; offline HTTP fixtures do not establish real cost savings.

For Mistral Small/Medium, `high` is the lowest documented positive option. Deprecated
Magistral models reason natively without an effort parameter.
[Adjustable reasoning](https://docs.mistral.ai/studio/conversations/reasoning),
[native Magistral reasoning](https://docs.mistral.ai/resources/deprecated/native-reasoning).

Go Flash's `reasoning_effort="low"` has first-party maintainer evidence: an accepted
catalog correction based on measurements through Go. DeepSeek's current upstream
reasoning guide lists positive efforts, but upstream limits do not guarantee Go's
output caps. This is a documented model profile, not a claim that Council has measured
current V4.1 endpoint behavior.
[Accepted correction](https://github.com/anomalyco/models.dev/pull/3963),
[Go measurements](https://github.com/anomalyco/models.dev/issues/3962),
[DeepSeek thinking API](https://api-docs.deepseek.com/guides/thinking_mode/).

The sections below retain the original comparison and pre-change audit as historical
research evidence; code references now point at the updated implementation.

## Finding

A single 4,000-token ceiling does not solve all three problems: getting a complete answer, fitting a growing transcript into the next model's context, and preventing expensive reasoning. Comparable projects separate these concerns: model-aware output allowances, context management, explicit handling of incomplete responses, and provider-specific thinking controls. OpenCode supplies a particularly close example because it derives output allowance from model metadata and remaining context; Aider supplies useful response truncation and history management patterns. Their application-level behavior does not automatically apply when Council calls a provider endpoint directly. [OpenCode request construction](https://github.com/anomalyco/opencode/blob/df6fb9a4602b8984426ac04477cc9426086c6df2/packages/core/src/session/model-request.ts#L47-L90), [Aider response handling](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/coders/base_coder.py#L1489-L1505), [Aider history management](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/history.py#L35-L101)

## Comparable implementations

| Project | Output allowance and incomplete output | Context overflow | What to borrow |
|---|---|---|---|
| OpenCode | Default allowance derives from the model catalog and remaining context. Detects output-limit responses and bounds automatic continuation. | Reserves headroom, compacts history, and can shrink rejected compaction inputs. | Model-aware internal policy; explicit incomplete state; bounded recovery. |
| Aider | Basic completion construction has no universal output cap; per-model extra parameters can supply one. Detects the provider's `length` finish reason. | Summarizes older history and retains recent exchanges where possible. | Distinguish a complete answer from partial text; preserve useful recent context. |
| LiteLLM | Provides model/token/cost metadata and accounting helpers rather than one mandatory answer limit. | Explicit trimming helper and optional context-window preflight/fallback routing. | Model capability data and request preflight; carefully chosen failure policy. |

The table summarizes the source evidence below; it does not imply that Council needs any of these libraries as a dependency.

### OpenCode: model-derived output and bounded recovery

At inspected commit `df6fb9a4602b8984426ac04477cc9426086c6df2`, `outputLimit` uses the model's catalog output maximum, falling back to 32,000 when unknown and capping at 256,000. If the context window and input size are known, it further limits output to the estimated room left, adding a 15% margin on estimated input tokens. The 1,024-token floor is a fallback for a nearly full context with automatic compaction disabled; it is not evidence that such a request will fit. Primary and compaction requests receive this calculated allowance; title/generate requests retain provider defaults. This is an internal policy, not a required end-user token slider. [Request construction and constants](https://github.com/anomalyco/opencode/blob/df6fb9a4602b8984426ac04477cc9426086c6df2/packages/core/src/session/model-request.ts#L47-L90), [request kinds](https://github.com/anomalyco/opencode/blob/df6fb9a4602b8984426ac04477cc9426086c6df2/packages/core/src/session/model-request.ts#L244-L257)

Compaction reserves at least 16,000 tokens or 15% of the window by default. When a provider rejects the compaction input as too long, predefined shrinking steps target 85%, 70%, 50%, then 35% of the originally rejected request. It supports background summaries and accounts for their usage, including failed and interrupted attempts. **Implication:** summarization can reduce later input size, but has its own token cost and potential latency. Do not call it a free optimization. [Compaction settings](https://github.com/anomalyco/opencode/blob/df6fb9a4602b8984426ac04477cc9426086c6df2/packages/core/src/session/compaction.ts#L42-L49), [reserve and shrinking policy](https://github.com/anomalyco/opencode/blob/df6fb9a4602b8984426ac04477cc9426086c6df2/packages/core/src/session/compaction.ts#L118-L133), [cost accounting](https://github.com/anomalyco/opencode/blob/df6fb9a4602b8984426ac04477cc9426086c6df2/packages/core/src/session/compaction.ts#L838-L855)

The runner treats output exhaustion separately from context overflow. After two output-limit continuations, a third consecutive `length` completion produces an explicit output-limit failure. **Implication:** a partial response must not silently count as success; continuation needs a finite attempt budget. Council need not adopt OpenCode's exact continuation count, especially because debate outputs are structured arguments rather than coding actions. [Runner continuation handling](https://github.com/anomalyco/opencode/blob/df6fb9a4602b8984426ac04477cc9426086c6df2/packages/core/src/session/runner/llm.ts#L292-L322)

**Important boundary:** these findings concern the OpenCode application harness source. They do not establish that the OpenCode Go service applies the harness's output allowance, compaction, continuation, or reasoning defaults to Council's raw chat-completion requests.

### Aider: detect truncation and retain recent context

At commit `5dc9490bb35f9729ef2c95d00a19ccd30c26339c`, basic completion construction sets model/stream/temperature, then merges model-specific `extra_params`; it does not universally set `max_tokens` in that construction path. For Ollama, when `num_ctx` is absent it calculates input tokens × 1.25 + 8,192 as the context allocation. That is a context setting, not the answer's output limit. Per-model configuration can still supply output limits. [Completion construction](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/models.py#L988-L1036)

Aider checks `finish_reason == "length"` for streamed and non-streamed responses and raises a distinct exception. Its main loop continues a truncated response only if the model metadata says assistant prefill is supported; otherwise it preserves partial content and reports exhaustion. Blindly sending “continue” to every provider is therefore not its universal policy. [Finish reason detection](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/coders/base_coder.py#L1894-L1911), [continuation decision](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/coders/base_coder.py#L1489-L1505), [exhaustion reporting](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/coders/base_coder.py#L1527-L1547)

Its history summarizer splits an older head from a recent tail, summarizes the head with another model call, and recurses if needed. It also constrains what the summarizer itself receives. **Implication:** compression needs its own overflow handling and cannot preserve every detail. Council should preserve its original question, attached source facts, claim identifiers, and unresolved objections explicitly when designing any compressed debate representation. The latter is a recommendation specific to Council. [History algorithm](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/history.py#L35-L101), [summarization calls](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/history.py#L103-L134)

Aider translates thinking controls differently for OpenRouter and direct providers: effort becomes OpenRouter's `reasoning.effort`, while direct-style requests get `reasoning_effort`; positive thinking budgets become OpenRouter's `reasoning.max_tokens` or `thinking.budget_tokens`. Setting a zero thinking budget deletes those fields. **Caveat:** deleting an override lets provider defaults apply; source inspection alone does not establish that reasoning is disabled. Council needs verified provider/model behavior rather than interpreting an absent parameter as “off.” [Thinking and effort translation](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/models.py#L791-L864)

### LiteLLM: separate preflight, trimming, and fallback

LiteLLM's explicit `trim_messages` helper limits **input messages** using a model or manually supplied input budget; its documented trim ratio defaults to 0.75. Calling the helper is an application choice. This is different from the completion API's output limit, despite both interfaces using the name `max_tokens`. For Council, silent trimming is risky because it can remove an objection or source qualification. [Message trimming documentation](https://docs.litellm.ai/docs/completion/message_trimming)

The router provides a distinct `context_window_fallbacks` mechanism. Its docs require `enable_pre_call_checks` for preflight context enforcement, rather than assuming all calls are checked automatically. A larger-window fallback is useful infrastructure, but in Council it would change the user's chosen role/model combination and possibly price. Recommended policy: preserve the selected model and report or manage context explicitly instead of silently substituting another model. [Context fallback and preflight documentation](https://docs.litellm.ai/docs/proxy/reliability#context-window-fallbacks-pre-call-checks--fallbacks)

Token accounting helpers use model-specific tokenizers when available and otherwise fall back to tiktoken; the model price/context list is community maintained. These are useful estimates and metadata, not provider guarantees. Cost/usage accounting should preserve provider-reported usage where available, and display unknown cost when reliable pricing is unavailable. [Usage, tokenization, and pricing documentation](https://docs.litellm.ai/docs/completion/token_usage)

## Recommended direction for Council

These are recommendations derived from the comparisons, not implemented behavior:

1. **Remove the universal 4,000-token policy and avoid adding a token-limit control to the normal UI.** Resolve an internal output allowance from the chosen model's actual output capacity and remaining context. A larger allowance permits a complete response; keep requested prose concise through role-specific instructions. Do not interpret a high ceiling as a target length.
2. **Control reasoning separately.** Following user review, always request the smallest positive supported effort or budget. For a model with mandatory or unknown reasoning behavior, identify the limitation beside the selected model; avoid promising zero reasoning cost. Do not silently replace the user's model choice.
3. **Treat input overflow as a different failure.** Preflight the prompt for each selected role/model. At first, preserve the full evidence and stop with a clear explanation if it cannot fit; add explicit structured compression only after validating that it preserves objections and provenance. Retain original transcripts regardless of what is sent next.
4. **Persist response metadata.** At minimum: finish reason, model/provider, visible output, reported input/output/reasoning tokens, latency, and cost when trustworthy. A nonempty answer with `length` is still incomplete. Empty visible output after paid reasoning must be shown as a failed/incomplete stage, not a blank successful turn.
5. **Bound recovery.** Preserve partial responses. Allow deliberate retry/continuation with finite attempts; do not auto-loop or silently re-run the whole debate. Include retries, failed calls, and any future compression calls in session usage.
6. **Do not confuse controls with guarantees.** A high output allowance does not prevent reasoning expense; a low allowance can exhaust the generation budget before an answer. An elapsed timeout does not, by itself, prove that a remote provider stopped generation or billing. A call count limits attempts, not dollars. A strict dollar cap requires reliable pricing, a pre-reserved worst-case request budget including reasoning, and compatible provider behavior; otherwise disclose that it is an estimate.

No universal replacement number is recommended here: arbitrary models and provider defaults make a second global magic number fragile. Model capability data and explicit incomplete states should come first.

## Provider-specific findings and local implementation audit

### What the providers actually expose

| Provider / route | Relevant evidence | Consequence for Council |
|---|---|---|
| Mistral direct | Current adjustable Mistral families support `reasoning_effort="none"` or `"high"`. Mistral describes `none` as minimal thinking, not a universal proof of zero internal reasoning. High effort can return mixed thinking/text chunks. Some hosted families instead require nonzero effort. [Reasoning documentation](https://docs.mistral.ai/studio/conversations/reasoning) | Select the smallest valid mode for the actual model. Extract final text separately from thinking; do not assume every response is a string or every Mistral-hosted model accepts `none`. |
| OpenRouter | Thinking is billed output and commonly shares the completion allowance. A reasoning-only `length` response can have no final answer. `exclude:true` only hides it. The catalog can advertise permitted efforts and mandatory thinking. [Reasoning guide](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens) | Resolve controls from capabilities, choose the minimum positive valid effort and identify mandatory thinking. This can be automatic without a token slider. |
| Ollama OpenAI-compatible route | Supports `reasoning_effort` and `reasoning.effort`. Named effort handling is model-specific; unsupported levels can revert to defaults. [Compatibility reference](https://docs.ollama.com/api/openai-compatibility) | Sending an arbitrary `low` is not sufficient evidence of reduced thinking. Validate the selected model's accepted values. |
| Ollama native discovery | `/api/show` documents a `thinking` object with supported values/default. `think:false` requests no thinking where permitted; absent metadata is inconclusive. [Thinking guide](https://docs.ollama.com/capabilities/thinking) | Discovery is useful when available. Verify that the direct cloud route supplies the same metadata before relying on local-server examples. Keep unknown behavior explicit. |
| OpenCode Go | Go is a gateway/service, while the comparison above is the OpenCode harness. Current Console inference docs distinguish API families and publish model endpoints. They do not specify one universal Go thinking-disable control. [Go documentation](https://opencode.ai/v2/docs/console/go/), [Inference API](https://opencode.ai/v2/docs/console/api/inference/) | Verify the selected Go model's actual endpoint and supported request fields. Do not assume that OpenCode CLI settings or its 32k fallback apply to our direct Go requests. |

For output/context discovery, OpenRouter's catalog includes context length, top-provider completion limits, supported parameters, and pricing. Mistral's model cards include `max_context_length` and capabilities. Our catalog currently retains only model IDs, so this information is discarded before policy selection. The catalog data can guide an internal allowance; missing or stale fields need an explicit fallback policy rather than invented limits. [OpenRouter model properties](https://openrouter.ai/docs/api/api-reference/models/list-all-models-and-their-properties), [Mistral model cards](https://docs.mistral.ai/api/endpoint/models), [local catalog implementation](../../council.py)

OpenRouter defaults to allowing endpoints that ignore unsupported request parameters. If a reasoning control is essential, `provider.require_parameters:true` restricts routing to supporting endpoints; model-level effort validation is still needed. [Provider routing reference](https://openrouter.ai/docs/guides/routing/provider-selection)

Cancellation is also not a universal cost backstop. OpenRouter's first-party guidance says aborted streams stop billing only on providers supporting cancellation; unsupported providers and non-streamed requests can continue generating. Treat a local timeout as a responsiveness mechanism, not proof that the remote bill stopped. [OpenRouter cancellation guidance](https://openrouter.ai/blog/tutorials/sillytavern-openrouter/)

### Problems verified in this repository

This is a source audit, not evidence that every failure happened in the earlier live test. [Current engine source](../../council.py), [event types](../../src/model.rs)

1. `MAX_TOKENS` defaults to 4,000 and `_chat_completion()` passes it to every model. It has no per-model output/context policy.
2. The same function sends no explicit thinking/effort control, so behavior depends on provider/model defaults.
3. `msg.content or msg.reasoning_content` can promote a reasoning trace to an answer when final content is empty. Where a provider returns thinking/text chunks instead of a string, downstream string operations are also unsafe.
4. `finish_reason` and provider `usage` are ignored. Partial text may therefore count as a completed stage, and reasoning cost cannot be distinguished from visible output.
5. `_retry()` retries all exceptions. In the CLI's default multi-attempt path, empty visible content can cause another paid request without first diagnosing reasoning exhaustion. The UI's existing single-attempt path avoids this particular repetition; it does not solve truncation.
6. `_assigned_caller()` uses a 90-second SDK timeout and disables SDK retries. It does not track a separate reasoning phase or enforce a provider-independent wall-clock/spend budget.
7. `transcript_text()` repeatedly includes the entire debate and background. Per-document character truncation in `read_document()` is not token-aware preflight for each model's context window.
8. The earlier live test saved argument text and elapsed seconds, but not token usage or reasoning settings. Those records cannot establish whether reasoning inflated time or cost. No new paid calls were made to manufacture that evidence.

### Implementation order suggested by the evidence

This was the proposed sequence before implementation. The implemented policy is described above.

1. Fix response classification first: separate final content from reasoning, persist usage and finish reasons, and mark length-limited or reasoning-only results incomplete. Never feed them to the next debater as complete arguments.
2. Add a provider/model capability adapter. Preserve output/context limits and supported reasoning modes in catalog data; use verified provider controls rather than model-name guesses. Keep the six provider/model dropdowns and omit a token-limit control.
3. Replace the 4,000 override with an automatic, generous allowance constrained by the selected model's output maximum and remaining input room. Do not blindly adopt OpenCode's 32k fallback, set every model to the largest possible value, or assume omission means unlimited output. Exact fallback policy needs model coverage tests.
4. Apply minimum positive reasoning where supported, following the final user decision. For mandatory reasoning, show a plain-language model label and use its lowest supported mode; do not silently swap models or promise zero overhead. If strict zero extra reasoning is required, mandatory-reasoning models cannot satisfy it.
5. Add input preflight before each role call. Keep the question, evidence, last attacked output, claim IDs, and objections intact. Do not silently trim old attacks or add paid summarizer calls in this first step.
6. Validate with offline provider fixtures for `stop`, `length`, empty final output, hidden reasoning, chunked Mistral content, mandatory/unsupported reasoning, missing metadata, and context exhaustion. Any later live comparison should record reasoning tokens, visible tokens, finish reason, total latency, and charged usage for the same fixed questions; do not claim a quality or savings result from configuration alone.
