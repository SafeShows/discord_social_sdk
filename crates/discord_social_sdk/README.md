# discord_social_sdk

Safe Rust bindings to the [Discord Social SDK](https://discord.com/developers/docs/discord-social-sdk/overview)
— friends, lobbies, messaging, voice calls, and rich presence.

📖 **[API documentation](https://safeshows.github.io/discord_social_sdk/)**

Handles become RAII types, optional fields become `Option`, fallible calls become
`Result`, and callbacks become closures. 502 of the C API's 513 exported
functions are wrapped; the raw layer lives in
[`discord_social_sdk_sys`](https://crates.io/crates/discord_social_sdk_sys).

## Supplying the SDK

This crate binds a prebuilt SDK that Discord distributes separately. It is not
vendored here and cannot be fetched automatically. Download it from the
[Discord Developer Portal](https://discord.com/developers/applications), then
either place it at `<workspace>/discord_social_sdk` or set
`DISCORD_SOCIAL_SDK_DIR` to its absolute path. Requires `libclang` for `bindgen`.

Windows, Linux, and macOS are supported. The build script links the right library
and stages the shared library next to your binary, so `cargo run` works unaided.

## Usage

The SDK is callback-driven and single-threaded. Nothing happens until
`run_callbacks()` is pumped, and every callback arrives on the thread that calls
it:

```rust,no_run
use discord_social_sdk::{Client, enums::ClientStatus, run_callbacks};

let mut client = Client::new();
client.set_application_id(YOUR_APP_ID);

client.on_status_changed(|status, error, detail| {
    if let Some(error) = error {
        eprintln!("connection error: {error:?} ({detail})");
    }
    if status == ClientStatus::Ready {
        println!("connected");
    }
});

client.connect();

loop {
    run_callbacks();
    std::thread::sleep(std::time::Duration::from_millis(16));
}
```

Authorization is OAuth2 with PKCE — see the `client::auth` module docs for the
full flow.

## How the C API maps to Rust

| C | Rust |
| --- | --- |
| `struct T { void* opaque; }` + `_Init`/`_Drop`/`_Clone` | a struct with `Drop` and `Clone` |
| `Discord_String` out-param | `String` (the SDK's buffer is freed for you) |
| `bool Get(self, T* out)` | `Option<T>` |
| `Set(self, T* value)`, null meaning "clear" | `Option<T>` argument |
| `Discord_XSpan` out-param | `Vec<X>` |
| `Discord_ClientResult` | `Result<T, Error>` |
| `(callback, userDataFree, userData)` | a boxed closure, freed by the SDK |

Two rules the wrapper enforces so you do not have to:

- **Strings passed in are borrowed, strings passed out are owned.** The SDK
  copies its inputs, and the wrapper frees every buffer it hands you.
- **Everything a callback receives is owned, not lent** — handles, strings,
  spans, and the `ClientResult` itself. Each must be released exactly once. This
  is easy to get backwards; treating a callback argument as borrowed leaks on
  every event.

## Threading

Wrapper types are neither `Send` nor `Sync` by design: the SDK expects to be
driven from a single thread. `set_free_threaded()` opts the underlying SDK into
its thread-safe mode, which only matters if you use the `sys` crate directly.

## Panics across FFI

A panic inside one of your callbacks would otherwise abort the process, since
unwinding out of an `extern "C"` function is not allowed. The wrapper catches
panics at the boundary, reports them on stderr, and keeps the SDK running.

## One caveat

`Client::add_log_callback` and `add_voice_log_callback` are the one place the SDK
does not promise to release your closure, and the two SDK builds disagree: the
release library frees it when the client is dropped, the debug library keeps it
for the whole process. Keep log handlers' captures small and free of anything
lifetime-sensitive.

## License

The bindings are MIT licensed. The Discord Social SDK binaries they link against
are covered by Discord's own terms.
