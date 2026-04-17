# Extract Complex Logic out of the Justfile

Growing shell logic inside `just/plugins.just` (and likely
other `just/*.just` files going forward) is becoming a smell.
The `stage-bundled-plugins` recipe now parses TOML, applies
comment filtering, dedupes ids, validates existence, and
stages archives — and the accompanying `test-stage-bundled-plugins`
recipe runs parse-logic tests defined inline in another
Justfile recipe. That test strategy works but pins the
logic in a place that is hard to unit-test cleanly and easy
to break silently when sed/grep/bash behaviour shifts
(already hit once with macOS BSD sed not understanding
`\s` and another time with `grep -o` and `pipefail`).

**Status:** needs discussion

**Problem areas:**

- TOML parsing in sed/awk/grep is fragile (BSD vs GNU sed,
  `\s` vs `[[:space:]]*`, comment detection, multi-line
  arrays, arrays with trailing commas, nested tables).
- Test coverage inside a Justfile is awkward — the "test"
  recipe has to re-implement parts of the recipe-under-test
  to exercise a function, and there is no easy way to mock
  filesystem state.
- Error messages are shell-printed strings rather than
  structured errors; CI log grepping is the only way to
  assert behaviour.
- The Justfile-as-orchestrator pattern breaks down as soon
  as logic gets non-trivial; a recipe that does "more than
  one thing" tends to pull in helper functions that
  themselves want tests.

**Proposed direction (to discuss):**

- Move non-trivial logic out of Justfile recipes into
  small, self-contained scripts under a `tools/` or `just/`
  directory. Candidates:
  - A Rust binary crate under `tools/` (compiled on demand
    via `cargo run -p …`). Pros: real types, real tests,
    easy macOS/Linux/Windows parity. Cons: compile overhead
    on first run.
  - A Bun script (TypeScript) if the logic is primarily
    file/IO oriented. Pros: frontend devs already have Bun,
    fast startup. Cons: adds another language to the build
    surface.
  - A plain bash script in `tools/` with a matching
    `tools/<script>.bats` or shell-level test file. Pros:
    stays in the shell family. Cons: does not solve the
    fragility problem, only moves it.
- Leave the Justfile as a thin orchestrator: each recipe
  calls out to a script and does little else beyond
  argument passing and documentation. This also makes the
  Justfile itself more readable.
- Keep truly trivial recipes (`cargo fmt`, `cargo test`,
  `bun run build`) inline — the test is whether a reader
  would comfortably grok the recipe at a glance.

**Concrete starting point:**

- `stage-bundled-plugins` in `just/plugins.just` — its TOML
  parsing, dedup, and existence check logic is the largest
  single piece that deserves extraction.
- `test-stage-bundled-plugins` in the same file should be
  retired in favour of real unit tests wherever the logic
  lands.

**Open questions:**

- Pick a language for extracted tools (Rust / Bun / bash).
- Is there appetite for a `tools/` workspace crate now, or
  should the first extracted script pick a language and we
  commit to a convention from there?
- How do we enforce that future Justfile growth follows the
  "thin orchestrator" principle without a reviewer
  constantly policing it?
