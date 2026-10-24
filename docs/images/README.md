# README visuals

These assets show the real Council UI with a fictional product launch question and context. They were extracted from the existing 2026-10-23 customer demo recording, without additional inference calls or fabricated model output.

- `decision-workspace.png`: the saved recommendation and completed-stage panel; cropped to the upper workspace and resized to 1600 px wide.
- `choose-your-council.gif`: provider/model selection in the composer; cropped to the dialog, resized to 760 px wide, and accelerated 2.8×. This shows configuration, not real-time inference.

The recorded live run used OpenCode Go `deepseek-v4.1-flash` for all six roles. The UI supports different selections for every role; the animation includes the configuration changes before that run. No credentials, private decisions, or filesystem picker contents are included.

These are documentation assets, not part of the runtime. See [demo production research](../research/2026-10-23-customer-demo-production.md) for tooling and verification context.
