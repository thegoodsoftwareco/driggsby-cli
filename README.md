# Driggsby

[Driggsby](https://driggsby.com) is an MCP server for personal finance. You
link accounts through Plaid, and your AI client gets
read-only MCP tools for transactions, balances, investments, and debts.

## Setup

```bash
npx driggsby@latest mcp setup
```

This prompts you to choose a setup path. For Claude Code and Codex, it runs the
native MCP setup command. Or specify one:

```bash
npx driggsby@latest mcp setup claude-code
npx driggsby@latest mcp setup codex
npx driggsby@latest mcp setup other
```

After setup, authenticate Driggsby through your client — run `/mcp` in
Claude Code, or sign in through the browser window Codex opens.

For another MCP client, choose Other. Driggsby currently supports only
OAuth-based remote MCP at:

```text
https://app.driggsby.com/mcp
```

## Tools

Includes tools like:

| Tool | Description |
|---|---|
| `get_overview` | Balances, accounts, net worth |
| `search_cash_transactions` | Search and filter across all accounts |
| `query_cash_sql` | SQL over transaction data |
| `list_recurring_transactions` | Subscriptions and recurring payments |
| `list_investment_holdings` | Portfolio positions |
| `list_outstanding_debts` | Balances, rates, minimums |
| `query_investment_sql` | SQL over investment data |

## Examples

- "What's my net worth across all accounts?"
- "How much did I spend on dining out last month?"
- "List every recurring subscription."
- "Find every Amazon charge over $100 this year."
- "Show my top 10 merchants by total spend this year."

## Options

```bash
# Claude Code scope (user by default, local for project-only)
npx driggsby@latest mcp setup claude-code -s local

# Print the native command without running it
npx driggsby@latest mcp setup codex --print
```

## Security

Read-only via Plaid. Cannot move money, initiate transfers, or make trades.
See [driggsby.com](https://driggsby.com) for details.

## License

Apache-2.0 — see [LICENSE](LICENSE).
