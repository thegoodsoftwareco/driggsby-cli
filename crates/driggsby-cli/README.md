# driggsby

`driggsby` is the local CLI for connecting AI clients to Driggsby over MCP.

## What You Get

- browser-based sign-in to Driggsby
- a local MCP server for tools like Codex and Claude Code
- access to supported Driggsby tools from your AI client

## Run

```bash
npx driggsby@latest login
```

## Install

```bash
npm install -g driggsby
```

If you prefer not to install globally, use `npx driggsby@latest` for
human-invoked commands like `login`, `status`, and `logout`, and use
`npx -y driggsby@latest mcp-server` for non-interactive MCP launcher flows.

On machines without working platform keyring support, such as some headless
Linux servers, Driggsby falls back to an owner-only local file-backed secret
store so the CLI can still complete login and run the broker.

Published npm installs currently include native artifacts for macOS arm64,
macOS x64, Linux arm64 glibc, and Linux x64 glibc.

## Quick Start

1. Sign in:

```bash
npx driggsby@latest login
```

2. Add Driggsby as an MCP server in your client:

```bash
codex mcp add driggsby -- npx -y driggsby@latest mcp-server
```

3. Check broker status any time:

```bash
npx driggsby@latest status
```

## Commands

```bash
npx driggsby@latest login
npx driggsby@latest status
npx -y driggsby@latest mcp-server
npx driggsby@latest logout
```

## License

Licensed under the Apache License, Version 2.0. See the repository root
`LICENSE` file.
