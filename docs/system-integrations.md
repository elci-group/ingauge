# System integration inventory

`ingauge discover --json` performs a secret-safe inventory. It reports only whether a credential variable, executable, or ViCo manifest exists; it never reads credential values or arbitrary harness settings.

The built-in catalog covers the provider mappings installed with ViCo/Hermes: Anthropic, Arcee, AWS Bedrock, Gemini/Google, GLM/ZAI, Groq, Grok/xAI, Hugging Face, Kimi/Moonshot, MiniMax, Novita, Ollama, OpenCode Go/Zen, OpenAI, OpenRouter, and Xiaomi/MiMo. Router discovery covers GIA Proxy, Hermes, Ollama, ViCo, and ViCo Vee. Harness discovery is dynamic: every JSON manifest in `$VICO_HARNESS_DIR` or `~/.vico/harnesses` is included, including newly added harnesses.

Capacity transport is intentionally uniform. Configure one `[providers.<name>]` table per target with an HTTPS endpoint (or loopback HTTP), optional bearer credential environment variable, and optional `usage_path`. This makes providers, routers, and harnesses first-class simultaneous sources while retaining response byte, record, timeout, retry, identifier, and credential-name bounds.

Ingauge does not issue paid model requests as a health probe. Where a vendor lacks a stable read-only usage or quota API, place its existing router or harness behind the canonical endpoint documented in the README.
