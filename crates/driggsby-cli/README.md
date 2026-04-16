# driggsby

`driggsby` helps configure AI clients to connect directly to Driggsby over MCP.

## Quick Start

```bash
npx driggsby@latest mcp setup
```

Run `mcp setup` once for each MCP client you want to use. Your AI client handles
OAuth with Driggsby when it connects to:

```text
https://app.driggsby.com/mcp
```

Choose a supported client directly:

```bash
npx driggsby@latest mcp setup claude-code
npx driggsby@latest mcp setup codex
npx driggsby@latest mcp setup other
```

Choose `other` for another OAuth-capable MCP client. Driggsby currently supports
only OAuth-based remote MCP. Configure the MCP URL above with server name
`driggsby`.

Claude Code MCP scope can be set explicitly with `-s`. Driggsby defaults
Claude Code setup to user scope.

```bash
npx driggsby@latest mcp setup claude-code -s user
npx driggsby@latest mcp setup claude-code -s local
```

Print the native client command without running it:

```bash
npx driggsby@latest mcp setup codex --print
```

## Setup Options

- Claude Code
- Codex
- Other OAuth-capable MCP clients

## License

Licensed under the Apache License, Version 2.0. See the repository root
`LICENSE` file.
