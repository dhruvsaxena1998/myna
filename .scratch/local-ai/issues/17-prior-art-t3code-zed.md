# How T3 Code and Zed use Claude and Codex subscriptions

Type: research
Status: resolved
Blocked by: 

## Question

How do the open-source apps T3 Code and Zed let users work through their Claude and Codex subscriptions (and OpenCode and Pi, if supported)? For each app and each agent: the integration path (ACP adapter, Claude Agent SDK, spawning the `claude`/`codex` CLI, `codex app-server`, other), how the user signs in and whose plan is billed, how tools and permissions are controlled, and anything the app or the vendor says about whether that use of a subscription is allowed. Myna will support only Claude, Codex, OpenCode and Pi.

## Answer

- T3 Code drives Claude through the Agent SDK pointed at the user's own `claude` binary, and runs one-shot text jobs as `claude -p --tools "" --permission-mode dontAsk`. The user signs in with `claude auth login`, and T3 stores no Claude credentials. Its FAQ says it is within Claude Code's terms, but no Anthropic text confirms that.
- T3 Code drives Codex through `codex app-server`, with `codex exec -s read-only` for text jobs. It also offers a sign-in that follows OpenAI's documented ChatGPT plan-usage flow. T3 drives OpenCode through `opencode serve` and Pi through `pi --mode rpc`.
- Zed hosts Claude, Codex, OpenCode and Pi through ACP registry adapters, and says billing and terms are "between you and the agent provider". It names no Anthropic or OpenAI approval.
- Zed's built-in ChatGPT Subscription provider signs in with the Codex CLI's own OAuth client ID and calls `chatgpt.com/backend-api/codex`. That is not OpenAI's documented third-party route.
- Anthropic's support page (updated 2026-10-07) says the Agent SDK and `claude -p` still draw from plan limits. The legal page still bars third-party plan sign-in, and the text does not reconcile the two.

Full findings: [research/17-prior-art-t3code-zed.md](../research/17-prior-art-t3code-zed.md)
