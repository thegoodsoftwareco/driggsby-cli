# driggsby

`driggsby` is the local CLI for connecting AI clients to Driggsby over MCP.

## Quick Start

```bash
npx driggsby@latest mcp setup
npx driggsby@latest status
npx driggsby@latest mcp list
```

Run `mcp setup` once for each MCP client you want to use. Driggsby opens
browser sign-in only when the saved Driggsby CLI session is missing or older
than 8 hours.

For supported clients, you can set up directly:

```bash
npx driggsby@latest mcp setup claude-code
npx driggsby@latest mcp setup claude-desktop
npx driggsby@latest mcp setup codex
```

Claude Desktop setup is macOS-only in this release.

Claude Code MCP scope can be set explicitly. Driggsby defaults Claude Code
setup to user scope.

```bash
npx driggsby@latest mcp setup claude-code --mcp-scope user
npx driggsby@latest mcp setup claude-code --mcp-scope local
```

Supported native artifacts currently cover macOS arm64, macOS x64, Linux arm64
glibc, and Linux x64 glibc.

## License

Apache-2.0
