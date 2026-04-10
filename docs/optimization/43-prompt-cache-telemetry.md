# Prompt Cache Telemetry

## What Yode Now Surfaces

### Per turn

- prompt tokens
- completion tokens
- cache write tokens
- cache read tokens
- cache status: `miss`, `hit`, `miss+write`, `hit+write`

### Session summary

- reported turn count
- cache-hit turn count
- cache-miss turn count
- cache-fill turn count
- cumulative cache write/read tokens

### Related diagnostics

- `/status` shows prompt-cache totals and last-turn state
- `/status` also shows system-prompt segment token estimates, so cache behavior can be interpreted alongside prompt growth

## Provider Notes

- Anthropic: reads cache creation/read token fields directly
- OpenAI: uses `prompt_tokens_details.cached_tokens` when available
- Gemini: uses cached content token count when available

## Why This Matters

- helps explain prompt-token plateaus vs growth
- makes cache-hit/miss behavior visible without opening provider logs
- supports long-session tuning and resume-path diagnostics
