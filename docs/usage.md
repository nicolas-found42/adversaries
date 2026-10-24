# Using Council

[Documentation](README.md) · [Quick start](../README.md#try-it-without-an-api-key)

## Frame a decision

Click **New decision** and write the alternatives, deadline, and constraints. Attach relevant documents using **Attach context**. Text, Markdown, and CSV work with the base backend; PDF, DOCX, and XLSX readers are included by `uv sync --all-extras`.

Choose a provider and model for every role in **Your council**. Each role has a searchable model dropdown and an editable model ID. Catalogs are loaded from your configured provider accounts; a listed model still needs to support that provider's chat-completions API and Council's request policy. Refresh the catalog after changing credentials or to fetch newer model lists.

You can mix providers, mix models from one provider, or reuse one model for every role. A single provider account is enough when every role uses it.

## Understand the debate

One round follows this order:

1. **Opening analyst** proposes a recommendation and numbered claims.
2. **First critique** challenges the opening.
3. **Second critique** challenges the preceding critique.
4. **Third critique** challenges the preceding critique.
5. **Rebuttal** revisits the recommendation in light of the challenges.
6. **Decision brief** synthesizes the debate.

Each stage receives the debate so far, with critiques directed at the most recent argument. The roles are separate prompts; their outputs are not independent factual verification, especially when reusing the same model.

The UI supports one to five rounds. Each extra round adds three critiques and one rebuttal: one round is six calls, two rounds are ten. The composer shows the total before launch. More rounds add cost and waiting time; they do not guarantee a better answer.

## Follow progress and read the result

The debate panel shows completed stages and elapsed time for the current stage. Some models return output only when they finish; a thinking indicator can remain visible while they work. Turn off **Follow live**, or select an earlier stage, to read at your own pace. A completed brief is announced without taking you away from your current view.

The brief covers the recommendation and confidence, alternatives, tradeoffs, unresolved disagreements, risks, information still needed, and next actions. Model formatting and length can vary. **Copy brief** copies the result; individual arguments have a copy action too.

Collapse **Debate panel** when you want more reading space. The decision question remains visible above the views.

## Inspect claims and assumptions

Open **Claims & assumptions** to inspect the model's labeled claims and use **Read source argument** to return to the stage that introduced one.

Prompts distinguish supplied material, model assumptions, and judgement. A supplied claim needs an exact excerpt from the question or attached context; unmatched claims are downgraded to assumptions. A quote match establishes only that the excerpt exists in your input. It does not prove the input or the model's conclusion is true. Formatting differences and model responses can leave supplied material labeled source-unconfirmed.

**UNCHALLENGED** means that a claim has not been challenged in the recorded debate. It does not mean the claim is verified. Check important facts and disagreements yourself.

## Stop, recover, or reopen

**Stop** cancels the local run. Providers may already have processed or billed a request; cancellation is not a refund guarantee.

Errors and incomplete responses stop the spinner and preserve completed arguments. A retry continues at the failed stage with the original role assignments; a restart repeats the entire council. Both are deliberate user actions that can create new paid calls. There are no automatic retries, continuation calls, or model fallbacks.

Saved decisions appear in the sidebar. **All decisions** opens history; select a session and press Enter or double-click to replay it. Stage checkpoints also preserve interrupted runs. History replay reads saved content and does not rerun inference.

For failures or unexpected usage, open **Diagnostics**. See [the investigation guide](agents/diagnostics.md) for the evidence recorded and its limits.

## Keyboard shortcuts

All primary actions also have buttons. Text input takes priority while editing.

| Key | Action |
| --- | --- |
| `n` | New decision |
| `1` | Debate |
| `2` | Decision brief |
| `3` | Claims & assumptions |
| `4` | History |
| `5` | Help |
| `6` | Diagnostics |
| `Tab` | Cycle views when no text field is focused |
| `j` / `k`, arrow keys | Navigate turns, claims, or sessions |
| `Enter` in history | Open the selected session |
| `c` | Stop the current run |
| `r` | Rerun the current question |
| `Esc` | Close the composer |
| `q` | Quit the native app |
