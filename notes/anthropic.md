# Anthropic findings (P0.7), 2026-10-30

- Key check: `GET /v1/models` (free, no tokens) → 200; `claude-haiku-5-5` available
- Request: `fixtures/claude-intent-request.json` → response `fixtures/claude-intent-order.json`
- Headers: `x-api-key`, `anthropic-version: 2023-06-01` (the P0.7 call also sent `fallbacks` + its beta header; dropped from the design, see 05 §4); body has `output_config: {effort: "low", format: {type: "json_schema", schema}}`: all accepted
- `"recharge jio 9876543210 for 199"` → `{"kind":"order","operator":"jio","phone":"9876543210","amount_inr":199,...}`, `stop_reason: end_turn`, **497 in / 80 out tokens**, 0 thinking tokens
- Gotcha: nullable enum as `type: ["string","null"]` + `enum` → 400 `Invalid schema`. Use `anyOf`.
- The text block holds the JSON as a string → parse `content[0].text`

## Token budget (low balance)
- Tests use these fixtures, never the live API. Live calls only in `/smoke` with an explicit flag
- Most of the 497 input tokens are the schema; keep the system prompt short
