# discord_social_sdk_sys

Raw FFI bindings to the [Discord Social SDK](https://discord.com/developers/docs/discord-social-sdk/overview),
generated with `bindgen` from `cdiscord.h`. Covers all 513 exported functions.

**You probably want [`discord_social_sdk`](https://crates.io/crates/discord_social_sdk)
instead** — the safe wrapper over this crate. Reach for this one only for surface
the wrapper does not cover.

Everything here is `unsafe`, and the C API's conventions are easy to get wrong:

- **Handles** are `struct { void* opaque; }` with `_Init`/`_Drop`/`_Clone`, and
  must be dropped exactly once.
- **Strings** are `Discord_String` (`ptr` + `size`, *not* NUL-terminated). Ones
  the SDK returns are yours to release with `Discord_Free`.
- **Optionals** are `bool Get(self, T* out)` — `false` means absent and leaves
  `out` untouched.
- **Everything delivered to a callback is owned**, including the `ClientResult`.
  Failing to release it leaks on every event.

## Supplying the SDK

This crate binds a prebuilt SDK that Discord distributes separately. It is not
vendored here and cannot be fetched automatically. Download it from the
[Discord Developer Portal](https://discord.com/developers/applications), then
either place it at `<workspace>/discord_social_sdk` or point
`DISCORD_SOCIAL_SDK_DIR` at it:

```bash
export DISCORD_SOCIAL_SDK_DIR=/absolute/path/to/discord_social_sdk
```

The directory search only works inside your own workspace. When this crate is
built from the registry — which includes the verification step of
`cargo publish` — it sits elsewhere, so the environment variable is required and
must be absolute.

The build script links the right library per target (Windows `.lib`/`.dll`, Linux
`.so`, macOS `.dylib` or the `.xcframework` slice) and copies the shared library
next to the built binary so `cargo run` and `cargo test` work unaided. The
`krisp` feature links Discord's Krisp noise-cancellation build and stages its
`.kef` model files.

Requires `libclang` for `bindgen`.

## Documentation

Hosted on [GitHub Pages](https://safeshows.github.io/discord_social_sdk/), not
docs.rs — docs.rs has no network access and no copy of the SDK, so it has nothing
to build against.

## License

The bindings are MIT licensed. The Discord Social SDK binaries they link against
are covered by Discord's own terms.
