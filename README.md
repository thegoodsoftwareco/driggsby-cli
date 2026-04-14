# Driggsby CLI

`driggsby` is the local command-line bridge for connecting AI clients to
Driggsby over MCP.

## Quick Start

Set up Driggsby for an MCP client:

```bash
npx driggsby@latest mcp setup
```

Run `mcp setup` once for each MCP client you want to use. Driggsby opens
browser sign-in only when the saved Driggsby CLI session is missing or older
than 8 hours.

You can also choose a supported client directly:

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

Check readiness:

```bash
npx driggsby@latest status
```

## Release Model

This repository owns the public CLI source, GitHub Release artifacts, and npm
publishing workflow for the `driggsby` package.

Create release tags from `main` using this format:

```text
driggsby-vX.Y.Z
```

The tag-triggered release workflow builds macOS and Linux artifacts with
`cargo-dist`, uploads them to this public repository's GitHub Release, scans the
generated npm package, and publishes `driggsby` to npm using trusted publishing.
Release artifacts currently cover macOS arm64, macOS x64, Linux arm64 glibc,
and Linux x64 glibc.

## License

Licensed under the Apache License, Version 2.0. See `LICENSE`.
