# ctty7 maintenance line

The `ctty7` branch is the stable integration line for
`cloudy-liu/ctty7`. It contains the upstream component code plus reusable UI
behavior that ctty7 needs. The application pins an exact commit from this
branch in `Cargo.toml` and `Cargo.lock`.

Use `main` for the general fork line, `sync/upstream-*` for temporary upstream
imports and conflict resolution, and `feature/*` or `fix/*` for focused work.
Merge a change into `ctty7` only after the component tests and the affected
ctty7 tests pass.

The application should not depend on `main` or on a moving branch. Keep
`gpui-component`, `gpui-component-assets`, and `gpui-component-macros` on the
same component revision. If a change is useful to all GPUI Component users,
send it upstream and remove the duplicate fork change after it lands.
