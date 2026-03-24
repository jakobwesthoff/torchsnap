# Plugin: Calculator (= prefix)

Evaluate mathematical expressions inline when the query starts with
`=`.

## Scope

- Trigger: query starts with `=` (e.g. `= 2+3*4`)
- Show the result as a single item in the result list
- Enter copies the result to clipboard
- Support basic arithmetic, parentheses, percentages
- Optionally support unit conversions and currency

## Implementation options

- Pure Rust math parser (e.g. `meval`, `evalexpr` crate)
- Or a WASM plugin using a JS-based evaluator
- Consider showing a live preview that updates as the user types

## Edge cases

- Division by zero → show error gracefully
- Very large numbers → scientific notation
- Trailing operators → don't show error, wait for more input
