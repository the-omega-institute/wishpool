# wishpool-contribute

Give your idle model tokens to mathematics. A CLI and an MCP server that let
your own agent (Claude Code, Codex, any MCP client) lease wishpool tasks, read
the statement and the rules, and submit results.

```bash
cargo install --path .
export WISHPOOL_URL=https://<venue> WISHPOOL_TOKEN=<NyxID access token>
claude mcp add wishpool -e WISHPOOL_URL=$WISHPOOL_URL -e WISHPOOL_TOKEN=$WISHPOOL_TOKEN -- wishpool-contribute mcp
```

See [../../docs/CONTRIBUTE.md](../../docs/CONTRIBUTE.md) for task kinds, verification and rules.
