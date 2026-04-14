# AGENTS.md

This repository contains the public, open-source Driggsby CLI. The CLI is the
local MCP bridge that lets users connect AI clients such as Codex, Claude, and
other MCP clients to Driggsby.

Driggsby is a personal financial MCP server that provides secure access to users'
financial data (such as balances, transactions, and investments) to their AI client
or agent of choice. As such, security is non-negotiable and is your top priority.

## Project Scope

- This repo owns the Rust CLI, npm wrapper package, GitHub Release artifacts, and
  npm publishing workflow for the public `driggsby` package.
- Main install path:
  - Interactive commands: `npx driggsby@latest login`, `npx driggsby@latest status`,
    and `npx driggsby@latest logout`.
  - Non-interactive MCP launchers: `npx -y driggsby@latest mcp-server`.
- The npm package must not include platform binaries. It should contain only the
  JavaScript installer/shim, package metadata, license/readme files, and checksum
  metadata. Platform binaries are hosted as GitHub Release artifacts.
- This is a public repo. Do not add private Driggsby service code, private
  infrastructure details, customer data, credentials, internal repo names,
  non-public runbooks, or private operational debugging instructions.

## Security

- Treat this as consumer financial software. The CLI stores local auth/session
  material and forwards user financial MCP requests, so security is non-negotiable.
- Never expose secrets, token values, local key material, private paths, or internal
  service diagnostics in public CLI output or public MCP responses.
- Public remote MCP/OAuth validation errors may be surfaced when they are already
  part of the public remote contract. Local filesystem, keychain, socket, JSON
  parse, serde, HTTP implementation, or process errors must be mapped to
  consumer-safe messages.
- Suggested terminal commands in CLI output must be copy-paste safe. Do not wrap
  terminal command suggestions in shell backticks. Markdown docs may use backticks.
- GitHub Actions must stay pinned to immutable SHAs unless there is a deliberate
  reviewed reason to update them.
- Never publish binaries, npm packages, or release artifacts from an unreviewed
  branch or from a tag that is not current `origin/main`.

## Conversation And Autonomy

- Be concise and direct with maintainers. Give the upshot first.
- Do not ask maintainers to run commands that you can run yourself.
- Be proactive: when a likely next step is obvious, do it.
- Ask clarification only when a reasonable assumption would be risky.
- Keep the maintainer informed with short progress updates during long work.

## Planning Process

When asked to prepare a plan:

1. Clarify and research first.
   - Ask questions only when ambiguity could cause downstream mistakes.
   - Review recent git history and open PRs.
   - Review current repo structure, `README.md`, `Justfile`, workflows, and relevant
     source before planning.
   - Follow existing repo patterns unless there is a strong reason not to.
   - Do not duplicate existing structure.

2. Write a tactical implementation plan.
   - Prefer a checklist with concrete files, checks, and expected behavior.
   - Prioritize ease of use, security, small scope, and first-shot agent success.
   - Call out release, npm, signing, or platform-support consequences explicitly.

3. Keep scope tight.
   - If the plan crosses multiple systems, split it.
   - Avoid turning a CLI fix into release-infra churn unless the release path is
     directly implicated.

4. Review the plan with the maintainer.
   - Present the plan plus any remaining decision points.
   - Revise based on feedback before implementing if the user asked for a plan.

## Development Process

When implementing a feature, fix, or release change:

1. Set up the work correctly.
   - If the current branch is `main`, create a feature branch first.
   - Do not treat local commits on `main` as complete.
   - Unless explicitly told otherwise, work is not complete until the branch is
     pushed and a GitHub PR exists.
   - Do not revert unrelated local changes.
   - Install the repo hooks in local clones with
     `git config core.hooksPath .githooks`.

2. Research before editing.
   - Inspect relevant source, tests, workflows, npm tooling, and release metadata.
   - Check edited source file lengths before changes; keep source files under 500
     lines. The 500-line source gate is enforced by
     `scripts/check_source_line_lengths.sh`.
   - For release behavior, inspect `.github/workflows/release.yml`,
     `.github/workflows/pr-security.yml`, `dist-workspace.toml`,
     `npm/driggsby/package.json`, and `scripts/release/*`.

3. Implement carefully.
   - Use `apply_patch` for manual edits.
   - Keep Rust boring and explicit.
   - Preserve public CLI/MCP output quality.
   - Avoid new dependencies unless they are clearly justified.
   - If adding a dependency, verify the current crate/package version before pinning.

4. Test the behavior, not just compilation.
   - Add or update focused tests for real regressions.
   - Avoid test bloat and near-duplicates.
   - For CLI-output changes, run live output smokes.
   - For broker/MCP changes, include concurrency or invalid-input coverage when
     relevant.

5. Review after tests pass.
   - For non-trivial Rust, release, installer, npm, security, or MCP changes, run:
     - one simplification/scope review,
     - two standard code reviews,
     - two security/privacy reviews.
   - Reviewer prompts must start with:

```text
You are a READ-ONLY reviewer. Do NOT edit files, do NOT create pull requests, do NOT perform the work of a developer. You are a CODE REVIEWER only.
```

   - Reviewer subagents must be spawned with `fork_context=false`, must be read-only, and must not create branches, commits, pushes,
     pull requests, PR comments, issue comments, labels, or reactions.
   - Fix valid `medium+` findings.
   - Documentation-only edits may skip the full reviewer pass when they do not
     change product behavior, release behavior, security posture, or public
     contracts.

6. Verify and commit.
   - Run the repo's required checks before committing.
   - Smoke test real CLI output when public behavior changed.
   - Commit messages must be descriptive and end with this footer as the final line:

```text
Authored by: Codex
```

7. Sync and open the PR.
   - Sync the feature branch with `origin/main`.
   - Resolve conflicts and re-run relevant checks.
   - Push the branch.
   - Open a PR with a clear title and body covering summary, why, testing, and risks.

## Rust Rules

- No `unsafe` in normal development.
- No `unwrap()` or `expect()` in non-test code.
- No `panic!`, `todo!`, `unimplemented!`, or `unreachable!` in non-test code.
- Use `Result` and `?` for recoverable errors.
- If `unsafe` is truly required, stop and get explicit maintainer approval first.
- Keep modules and functions small enough that the next agent can understand them
  quickly.
- Prefer explicit types and straightforward control flow over cleverness.

## TypeScript And Npm Tooling

- New Node-side release, installer, or validation tooling should be TypeScript, not
  plain JavaScript, unless editing an existing runtime JavaScript file.
- Keep TypeScript strict and type-clean. Do not use `any` or unsafe shortcuts.
- The npm package is generated from `npm/driggsby` and validated by the release
  surface checks. Do not hand-edit generated tarball contents.
- Do not add platform binaries to npm package `files`.
- The npm installer should download GitHub Release artifacts, verify checksums, and
  install only the selected platform binary.

## Checks

Use the repo's `Justfile` recipes:

- `just required-check`: runs npm install/check/build, Rust formatting, and strict
  clippy gates for libraries, binaries, examples, and tests. It also runs the
  500-line source file gate.
- `just verify`: runs `just required-check`, `cargo test --all-features --locked`,
  and `cargo build --locked`.

Useful targeted checks:

- `cargo test -p driggsby broker:: -- --nocapture`
- `cargo test --all-features`
- `npm run check`
- `npm run build`
- `bash scripts/check_source_line_lengths.sh`
- `npm run pack:npm`
- `node dist/scripts/release/check-npm-publish-surface.js target/distrib/driggsby-X.Y.Z.tgz`

For CLI-output changes, smoke test the actual terminal output:

```bash
cargo run -p driggsby -- --help
cargo run -p driggsby -- status
```

For release workflow changes, also run or inspect:

- `bash scripts/check_github_action_pins.sh`
- `bash scripts/release/install-cargo-dist.sh`
- `dist plan --tag=driggsby-vX.Y.Z --output-format=json`
- `actionlint .github/workflows/*.yml` when available

## CLI Output Rules

- CLI output should be calm, explicit, and easy for humans and agents to act on.
- Prefer consistent sections such as:

```text
Next:
  npx driggsby@latest login
```

- Terminal command suggestions must be raw, copy-pasteable commands. Do not use
  shell backticks in terminal output.
- Markdown docs, PR bodies, and comments should still use Markdown backticks around
  commands.
- Do not expose local paths, keychain internals, socket paths, raw serde errors, or
  private implementation details in consumer-facing output unless explicitly needed
  for a local diagnostic command.

## Public MCP Rules

- The public MCP surface must never expose private Driggsby internals.
- Preserve public remote MCP/OAuth validation errors when they are already part of
  the remote public contract. This lets CLI MCP users see the same useful errors an
  OAuth MCP client would see for bad dates, invalid page tokens, unsupported SQL,
  and other input issues.
- Sanitize local CLI implementation failures before they reach MCP clients.
- If a local broker request fails internally, return a consumer-safe error with a
  clear next step rather than raw implementation detail.
- Optimize for first-shot success by a zero-context agent. Tool descriptions,
  schemas, errors, and next steps should be plain English and hard to misread.

## Local Broker And Daemon Behavior

- `mcp-server` is a stdio shim. It ensures the local `cli-daemon` is running, then
  forwards MCP tool discovery and tool calls through local IPC.
- `status` is read-only and should not restart or upgrade the daemon.
- `mcp-server` may start or replace the daemon when needed.
- A running daemon must not be reused blindly across CLI upgrades. Versioned ping
  should ensure the daemon version matches the current binary.
- Concurrent `npx -y driggsby@latest mcp-server` launches must serialize daemon
  start/restart through the startup lock to avoid stampedes.
- Local IPC timeout layers should remain distinct:
  - short timeout for connect,
  - short timeout for write,
  - short timeout for control requests,
  - longer timeout for real MCP tool responses.

## Release Process

Releases are tag-triggered. The release workflow runs only for tags matching:

```text
driggsby-vX.Y.Z
```

Before creating a release tag:

1. Update all versioned metadata to `X.Y.Z`.
2. Confirm these files agree:
   - `Cargo.toml`
   - `Cargo.lock`
   - `package.json`
   - `package-lock.json`
   - `npm/driggsby/package.json`
3. Confirm `npm/driggsby/package.json` points at:

```text
https://github.com/thegoodsoftwareco/driggsby-cli/releases/download/driggsby-vX.Y.Z
```

4. Run `just verify`.
5. Run npm package surface validation for the generated package.
6. Merge the PR to `main`.
7. Sync local `main` with `origin/main`.
8. Create and push the tag from the current `origin/main` commit:

```bash
git tag driggsby-vX.Y.Z origin/main
git push origin driggsby-vX.Y.Z
```

The release workflow rejects tags that are not on current `origin/main`.

Current release artifact targets are:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-gnu`

Windows is not currently part of the release artifact matrix. Do not claim Windows
release support until the workflow, installer metadata, signing story, and tests
actually support it.

The tag-triggered release workflow:

- validates the tag format and tagged commit,
- runs Rust and npm verification,
- runs `cargo audit`,
- plans the release with `cargo-dist`,
- builds platform artifacts,
- creates the GitHub Release,
- scans the generated npm package,
- publishes `driggsby` to npm through trusted publishing.

The npm trusted publisher must match the public repository, release workflow file,
and `npm-publish` environment. If npm publish fails, inspect trusted publishing and
environment protection before changing package code.

If a release fails after a tag push:

- Do not overwrite public release artifacts.
- Fix the workflow or code in a new PR.
- Bump to a new version if npm or GitHub already observed the failed version in a
  way that cannot be safely retried.
- Merge to `main`, then create a new tag from current `origin/main`.

## Platform Support

- Supported today:
  - macOS Apple Silicon: `aarch64-apple-darwin`
  - macOS Intel: `x86_64-apple-darwin`
  - Linux x64 glibc: `x86_64-unknown-linux-gnu`
- Not supported today:
  - Windows release artifacts,
  - Linux musl/static binaries,
  - Linux arm64 release artifacts.
- Do not imply signing/notarization exists until the workflows and credentials are
  actually implemented.

## Public Documentation

- Keep public docs useful but not revealing.
- It is okay to document install commands, supported platforms, release process,
  troubleshooting, and public MCP behavior.
- Do not document non-public deployment details, private architecture, private
  tools, customer data handling internals, or credential locations.
