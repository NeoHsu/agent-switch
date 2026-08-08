# Development Guide

Agent Switch keeps its local quality gates close to the CI commands. The
repository pins development tools in [`mise.toml`](../mise.toml); install them
before running the full gate:

```bash
mise install
mise run check:fast
```

## Stable tasks

| Task | Purpose |
| --- | --- |
| `mise run check:fast` | Fast locked CLI package check |
| `mise run fmt` | Format Rust sources |
| `mise run lint` | rustfmt check and Clippy with warnings denied |
| `mise run test` | Workspace tests and doctests |
| `mise run test:nextest` | Parallel tests with failure reporting plus doctests |
| `mise run msrv` | Check Rust 1.85 compatibility |
| `mise run coverage` | Enforce 80% line and function coverage floors |
| `mise run deps:check` | Detect unused Cargo dependencies |
| `mise run security` | Secret, license, source, ban, and advisory checks |
| `mise run workflow:check` | Validate workflow syntax and immutable pins |
| `mise run workflow:security` | Audit workflows with zizmor |
| `mise run check:pr` | Run the complete deterministic PR gate |

The portable source of task behavior is `mise.toml`. CI runs the same individual
Cargo and script gates directly so failures remain easy to diagnose.

## Verification before finishing

Use the smallest relevant check while iterating, then broaden before committing:

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
scripts/check-workflows.sh
```

For a release-quality local check:

```bash
mise run check:pr
```

The workflow checker enforces immutable GitHub Action commit pins, disabled
checkout credentials, job timeouts, cancellation behavior, and the required
quality/security commands. `deny.toml` rejects unknown registries and Git
sources, wildcard dependencies, and unapproved licenses.

## Coverage and security

Coverage is measured with `cargo-llvm-cov`. The current floor is intentionally
set at 80% for both lines and functions; new behavior should include focused
unit or integration tests rather than lowering the floor.

Security gates include:

- `gitleaks` for source-tree secret scanning;
- `cargo deny` for licenses, sources, bans, and advisories;
- `cargo audit` for RustSec advisories; and
- `zizmor` plus `actionlint` for GitHub Actions workflows.
