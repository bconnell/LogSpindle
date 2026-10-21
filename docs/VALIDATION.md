# Local validation contract

Validation must be performed against the exact candidate that will be merged. A successful command on another
branch or commit is not sufficient evidence.

## Required gates

1. Confirm the checkout is the intended LogSpindle feature branch and that unrelated files remain unchanged.
2. Run `python3 scripts/check_text.py` or `python scripts/check_text.py`. Resolve every reported issue.
3. Generate `Cargo.lock` once if it is absent, using `cargo generate-lockfile`. Review and commit it on the
   candidate branch.
4. Run `cargo fmt --all -- --check`.
5. Run `cargo clippy --locked --all-targets --all-features -- -D warnings`.
6. Run `cargo test --locked --all-targets --all-features`. Include the command-line integration tests.
7. Run `cargo build --locked --release`.
8. Run the resulting executable with `--help` and the checked-in sample input.
9. Run `git diff --check` and confirm the final tree is clean.
10. Re-run all checks after any modification. Record the candidate commit SHA and actual results before merge.

On Windows PowerShell 5.1, `powershell -ExecutionPolicy Bypass -File scripts/verify.ps1` runs the automated gates.
On Linux or macOS, `bash scripts/verify.sh` runs them.

The verification scripts require an existing committed lockfile and stop immediately if it is missing. Generating a
new lockfile is a source change and must be reviewed before release. The text gate does not require a Rust
toolchain.

## Text publication gate

The standalone validator checks required files, UTF-8 decoding, control characters, trailing whitespace,
documentation line length, local Markdown references and headings, known placeholder language, credential-like
material, and disclosure of personal machine paths or temporary infrastructure conditions.

The tool is a safeguard, not a substitute for reviewing actual statements. Inspect prose for overclaims, misleading
scope, unverified performance numbers, misleading test results, licensing and author attribution, and references to
nonexistent features. Verify examples against the built program.

## Failure contract

Any failed required gate blocks merge and publication claims. Fix the cause on the feature branch and rerun the
complete sequence. Do not skip a failing gate, treat a planned test as passed, or report a release that was not
built and exercised.

Do not use parallel heavyweight build or test processes. Keep all file operations inside the authorized checkout.
Do not modify system settings, security tools, global Rust installations, other repositories, or personal data.
Stop if resource pressure or unexpected changes appear. Preserve the current base commit and all unrelated staged
and unstaged work.
