# ADR-0006: AI-assisted contribution guidelines and authorship policy

- Status: Accepted
- Date: 2026-10-25

## Context

rfvp is a clean-room Rust reimplementation of the proprietary FAVORITE View Point (FVP) visual novel
engine. The codebase balances reverse-engineered bytecode VM semantics, strict syscall compatibility,
subtle type tag invariants, `no_std` embedded core constraints, and low-level graphics backends across
multiple platforms.

AI coding assistants and Large Language Models (LLMs) are widely used to assist with software
development, refactoring, and documentation. However, low-level engine emulation and reverse-engineering
are notoriously sensitive to subtle bugs:
- Subtle behavioral discrepancies in VM syscalls or type coercions can break titles in ways that
  surface hours into a playthrough.
- Hallucinated assumptions about edge cases, memory layouts, or endianness can compromise engine
  stability, particularly on constrained console platforms.
- Lower-tier language models frequently struggle to maintain the exact invariants required by
  rfvp's architecture.

Additionally, maintainability, legal clarity, and technical accountability require a well-defined
attribution policy for contributions in the repository's git history.

## Decision

1. **Permitted AI Usage and Recommended Model Tier:**
   - AI-assisted contributions are welcome, provided they meet the project's rigorous quality bar.
   - It is strongly recommended that any AI-assisted code generation or complex architectural refactoring
     use frontier-class reasoning models at or exceeding the capability tier of **Claude Opus 5.5** or
     **GPT-6 Astra**.
   
2. **Mandatory Human Review for Sub-Tier Models:**
   - If code is generated or assisted by models below this capability threshold—or if automated tooling
     is applied to non-trivial logic—rigorous manual human inspection and verification are mandatory
     prior to submission.
   - The submitting author must personally audit every line of code, verify behavioral accuracy
     against real game scripts or reverse-engineering findings, and ensure full test coverage.

3. **Authorship and Attribution Policy:**
   - AI models, tools, or bots must **never** be attributed as git authors, commit authors, or co-authors
     (e.g., using `Co-authored-by:` git trailers for AI agents).
   - Git commits and pull requests must reflect solely human authorship. The human contributor who
     submits the change assumes full technical, legal, and maintenance responsibility for the code.

## Consequences

- The repository maintains high code quality and fidelity to original engine quirks without being
  diluted by unverified or hallucinated automated code generation.
- Human contributors remain accountable for the changes they commit.
- Clear expectations are set for anyone leveraging AI tooling when contributing to rfvp.
