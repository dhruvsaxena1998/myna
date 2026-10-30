# 10: AI provider, Summary and Chat

**Status:** draft
**Spec stories:** 40–49
**Depends on:** 05

One user-supplied provider powers a Summary tab and a per-meeting chat, with clear off-device badges and real cost.

## Stories
- As a user, I want to set one provider (OpenAI, Anthropic, OpenRouter, local) with my key in the Keychain.
- As a user, I want a Summary tab with summary, decisions and action items, and custom templates.
- As a user, I want a chat per meeting with tokens and cost shown.
- As a user, I want price and context window from the provider or a maintained list, never typed by me.

## Done when
- [ ] Key in Keychain; "sends text off this Mac" pill wherever data leaves
- [ ] Summary and chat use only included transcript lines (tested)
- [ ] Cost from provider/price list; "cost unknown" otherwise; local is free
- [ ] Checked with a real key

## Open questions for you
- Which provider do you test with first?
