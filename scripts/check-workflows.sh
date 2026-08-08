#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ACTIONLINT_BIN="${ACTIONLINT_BIN:-}"
if [[ -z "$ACTIONLINT_BIN" ]]; then
  if command -v actionlint >/dev/null 2>&1; then
    ACTIONLINT_BIN="$(command -v actionlint)"
  else
    echo "actionlint not found. Install pinned tools with: mise install" >&2
    exit 127
  fi
fi

"$ACTIONLINT_BIN" .github/workflows/*.yml

unpinned="$({
  grep -HnE '^[[:space:]]*(- )?uses:' .github/workflows/*.yml || true
} | grep -Ev '@[0-9a-f]{40}([[:space:]]|$)' || true)"
if [[ -n "$unpinned" ]]; then
  echo "GitHub Actions must use immutable 40-character commit SHAs:" >&2
  echo "$unpinned" >&2
  exit 1
fi

checkout_count="$({ grep -hE '^[[:space:]]*(- )?uses: actions/checkout@' .github/workflows/*.yml || true; } | wc -l | tr -d ' ')"
persist_count="$({ grep -hF 'persist-credentials: false' .github/workflows/*.yml || true; } | wc -l | tr -d ' ')"
if [[ "$checkout_count" != "$persist_count" ]]; then
  echo "Every checkout action must disable persisted credentials" >&2
  exit 1
fi

for workflow in .github/workflows/ci.yml .github/workflows/release.yml; do
  if ! grep -Fq "timeout-minutes:" "$workflow"; then
    echo "$workflow must define job timeouts" >&2
    exit 1
  fi
  if ! grep -Fq "concurrency:" "$workflow"; then
    echo "$workflow must define concurrency behavior" >&2
    exit 1
  fi
done

for required in \
  "cargo fmt --all --check" \
  "cargo clippy --workspace --all-targets --locked -- -D warnings" \
  "cargo check --workspace --all-targets --locked" \
  "cargo nextest run --workspace --locked" \
  "cargo test --doc --workspace --locked" \
  "python3 scripts/check-skill-version.py" \
  "python3 scripts/verify-release-artifacts.py --self-test" \
  "cargo machete" \
  "cargo deny check" \
  "cargo audit --deny warnings" \
  "gitleaks dir --redact --no-banner ."; do
  if ! grep -Fq "$required" .github/workflows/ci.yml; then
    echo ".github/workflows/ci.yml must contain: $required" >&2
    exit 1
  fi
done

for required in \
  "sha256sum" \
  "cargo audit --deny warnings" \
  "python3 scripts/verify-release-artifacts.py --version-from-cargo --execute-native" \
  "actions/attest-build-provenance@" \
  "actions/upload-artifact@"; do
  if ! grep -Fq "$required" .github/workflows/release.yml; then
    echo ".github/workflows/release.yml must contain: $required" >&2
    exit 1
  fi
done

printf 'OK: workflow syntax, immutable pins, credential hygiene, timeouts, and quality gates are valid.\n'
