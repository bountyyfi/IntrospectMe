# IntrospectMe

> GraphQL introspection is disabled. Your schema is not.

GraphQL engines return field suggestion errors even with introspection fully disabled.

```
Cannot query field 'usr' on type 'Query'. Did you mean 'user'?
Cannot query field 'emal' on type 'User'. Did you mean 'email'?
Cannot query field 'passwrd' on type 'User'. Did you mean 'password'?
```

IntrospectMe listens to those whispers. Sends intentionally wrong queries, collects the hints, walks every type recursively until the full schema is sitting on your screen. No introspection queries. Ever. Zero detection.

**PoC result: 27/34 fields recovered against a test schema with introspection disabled.**

The 7 missed fields were short IDs. Everything sensitive was found.

-----

## How It Works

1. Seeds a wordlist of common GraphQL field names against the target
1. Parses `Did you mean...?` suggestions from every error response
1. For each discovered field, probes subfields recursively
1. Brute-forces short field names (`id`, `pk`, `key`, `me`) that are too brief for suggestions
1. Assembles a complete schema SDL from collected suggestions
1. Output is identical to what a full introspection query would return

Requests look like normal failed queries in your logs. Because they are.

-----

## Usage

```bash
introspectme --url https://target.com/graphql --output schema.graphql
```

### Flags

| Flag | Description |
|---|---|
| `--url` | Target GraphQL endpoint |
| `--output` | Output file path for SDL schema (default: schema.graphql) |
| `--json-output` | Output file path for raw JSON discovery data (default: discovered.json) |
| `--delay` | Delay between requests in ms (default: 100) |
| `--user-agent` | Custom User-Agent header |
| `--depth` | Max recursion depth for type walking (default: 10) |
| `--auth` | Authorization header value (e.g., "Bearer token123") |
| `--poc` | Run against local test server to verify technique |

### PoC Mode

Spins up a local GraphQL server with introspection disabled, reconstructs the schema, outputs a side-by-side comparison.

```bash
introspectme --poc
```

### Environment Variables

- `INTROSPECTME_DEBUG=1` — Print all GraphQL error messages to stderr for debugging

-----

## Architecture

```
src/
├── main.rs       — CLI entry point, orchestration
├── cli.rs        — Argument parsing (clap)
├── wordlist.rs   — 800+ base words, typo mutation engine
├── client.rs     — Async HTTP client, error response parser
├── walker.rs     — Recursive type walker + short field brute-forcer
├── schema.rs     — Schema model, SDL/JSON output generation
└── poc.rs        — PoC mode: local GraphQL server (actix-web + async-graphql)
```

### Wordlist & Mutation Engine

800+ base words covering identity, auth, e-commerce, CMS, social, finance, SaaS, healthcare, DevOps, gaming, blockchain, HR, legal, IoT, and more. Each word is mutated into ~8 typo variants:

- Prefix: `xuser`
- Drop last char: `use`
- Swap chars: `suer`
- Suffixes: `users`, `userId`, `userBy`, `userList`
- Case flip: `User`

Short fields (`id`, `pk`, `key`, `me`, `uid`, etc.) are brute-forced directly since they are too brief to trigger suggestion errors.

-----

## Affected Libraries

Tested and confirmed vulnerable by default:

- Apollo Server
- Hasura
- GraphQL Yoga
- Strawberry (Python)

If your GraphQL library returns `Did you mean...?` suggestions -- and most do -- you are affected. This is not a misconfiguration. This is GraphQL.

-----

## The Fix

Disable field suggestions in your GraphQL library configuration.

**Apollo Server:**

```js
new ApolloServer({
  introspection: false,
  formatError: (error) => {
    if (error.message.includes('Did you mean')) {
      return { message: 'Validation error' };
    }
    return error;
  }
})
```

Stripping suggestion text from error responses is the only reliable mitigation.

-----

## Built With

Rust. Because speed matters when you're walking an entire type tree.

-----

## Research

Published by [Bountyy Oy](https://bountyy.fi) -- Finnish cybersecurity consultancy specializing in penetration testing and vulnerability research.

Lonkero -- our Rust-based web vulnerability scanner -- detects this automatically.

[lonkero.bountyy.fi](https://lonkero.bountyy.fi)

-----

## Disclaimer

For authorized security testing and research only. Use responsibly.
