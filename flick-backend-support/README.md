# Shared backend support

These helpers have concrete shared consumers; target representations remain
with their backend. The crate depends on portable `flick-ir` and JSON string
encoding, not on the checker or a project driver.

| Module | Consumers |
| --- | --- |
| `Known`, `names` | JS/Go emission; frontend TS declarations and constructor identities |
| `tail`, `loops`, `order` | JS and Go emission |
| `reach` | JS lexical placement; Go selection; frontend build validation |
| `exports` | JS and frontend TS re-export syntax |
| `effects`, `EffectBridge` | JS emission and frontend runtime/declaration policy |
| generated binding tables | JS/Go emission and frontend runtime policy, from package-owned metadata |

`native-project` enables the existing native reachability/Go table surface;
minimal/browser consumers do not need it. Portable IR and the WIT backend
world remain the versioned contracts. These internal Rust helpers do not
establish another public plugin ABI.
