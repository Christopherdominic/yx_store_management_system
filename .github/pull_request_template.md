## Summary

<!-- What does this change do and why? -->

## SRS requirements

<!-- List the requirement IDs this PR implements or affects, e.g. FR-11, NFR-09 -->

-

## Testing

<!-- SRS Part C: link each requirement to a test and record the expected and actual results -->

- [ ] Tests added or updated for each requirement above
- [ ] `cargo fmt`, `cargo clippy` and `cargo test` pass locally (backend)
- [ ] `pnpm lint` and `pnpm build` pass locally (frontend)

## Checklist

- [ ] Money uses integer minor units (kobo), never floats (NFR-30)
- [ ] Store and business scope comes from the server, not from IDs the client sends (FR-06)
- [ ] Stock changes go through the stock movement service (FR-11)
- [ ] No secrets, passwords or tokens in code or logs (NFR-15)
- [ ] Needs product-owner sign-off (affects financial accuracy, inventory integrity, security or access control)
