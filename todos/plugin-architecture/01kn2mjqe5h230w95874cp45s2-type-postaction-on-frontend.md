# Type PostAction on Frontend

`PostAction` arrives as a raw string from Rust's serde serialization. The
frontend matches it via bare string equality (`"Dismiss"`, `"ShowCustomUI"`)
with no TypeScript union type.

`"KeepOpen"` is silently unhandled — it accidentally does the right thing
(nothing), but this is not type-checked.

Define a TypeScript union type:
```typescript
type PostAction = "Nothing" | "Dismiss" | "KeepOpen" | "ShowCustomUI";
```

Use it as the return type of the `search_execute` invoke call. Add exhaustive
handling or an explicit default case.
