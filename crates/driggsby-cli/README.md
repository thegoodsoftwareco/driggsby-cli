# driggsby

`driggsby` is the local CLI for connecting AI clients to Driggsby over MCP.

## What You Get

- browser-based sign-in to Driggsby
- a local MCP server for tools like Codex and Claude Code
- access to supported Driggsby tools from your AI client

## Run

```bash
npx driggsby@latest mcp connect
```

## Install

```bash
npm install -g driggsby
```

If you prefer not to install globally, use `npx driggsby@latest` for
human-invoked commands like `mcp connect`, `mcp clients list`, `mcp clients
disconnect-all`, and `status`. The `mcp connect` command installs the MCP
launcher configuration for supported clients, or prints configuration for other
MCP clients.

On machines without working platform keyring support, such as some headless
Linux servers, Driggsby falls back to an owner-only local file-backed secret
store so the CLI can still connect and run the broker.

Published npm installs currently include native artifacts for macOS arm64,
macOS x64, Linux arm64 glibc, and Linux x64 glibc.

## Quick Start

1. Connect Driggsby to an MCP client:

```bash
npx driggsby@latest mcp connect
```

2. Or choose a supported client directly:

```bash
npx driggsby@latest mcp connect claude-code
npx driggsby@latest mcp connect claude-desktop
npx driggsby@latest mcp connect codex
```

Claude Desktop automatic setup is macOS-only in this release.

Claude Code supports explicit MCP config scope. Driggsby defaults Claude Code
setup to user scope.

```bash
npx driggsby@latest mcp connect claude-code --mcp-scope user
npx driggsby@latest mcp connect claude-code --mcp-scope local
```

To print MCP config without mutating a supported client's config:

```bash
npx driggsby@latest mcp connect codex --no-auto-add-mcp-config
```

3. Check broker status any time:

```bash
npx driggsby@latest status
```

## Commands

```bash
npx driggsby@latest mcp connect
npx driggsby@latest mcp clients list
npx driggsby@latest mcp clients disconnect <client>
npx driggsby@latest mcp clients disconnect-all
npx driggsby@latest status
npx -y driggsby@latest mcp-server
```

## License

Licensed under the Apache License, Version 2.0. See the repository root
`LICENSE` file.
