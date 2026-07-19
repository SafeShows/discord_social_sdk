//! Empirical leak detector: hammers each allocation path and watches whether the
//! process's committed memory grows.
//!
//! The static audit checks that every `Discord_Free` matches an allocation. This
//! checks the same thing from the other end, by running each path hundreds of
//! thousands of times and measuring. A path that leaks even 8 bytes per call
//! shows up as clear linear growth; a correct one flattens out once the
//! allocator's free lists reach steady state.
//!
//! ```text
//! cargo run --release --example leakcheck
//! ```
//!
//! What it cannot cover: anything that needs a live authenticated connection.
//! The callback-delivered strings, spans and `ClientResult`s — the paths whose
//! ownership rules are the easiest to get wrong — are only reachable with a real
//! Discord session, so they are exercised by the static audit alone.

use discord_social_sdk::enums::{ActivityType, LoggingSeverity};
use discord_social_sdk::{Activity, ActivityButton, ActivityParty, Client, run_callbacks};

/// Bytes of private (committed) memory charged to this process.
///
/// Private commit is used rather than working set because the working set is
/// perturbed by paging decisions the leak has nothing to do with.
#[cfg(windows)]
fn committed_bytes() -> usize {
    #[repr(C)]
    #[derive(Default)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: size_of::<ProcessMemoryCounters>() as u32,
        ..Default::default()
    };
    // SAFETY: `counters` is a correctly sized, fully initialised POD struct, and
    // `cb` reports its size as the API requires.
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok == 0 { 0 } else { counters.pagefile_usage }
}

#[cfg(not(windows))]
fn committed_bytes() -> usize {
    // /proc/self/statm reports resident pages; good enough to spot linear growth.
    // Absent on macOS, where this harness cannot measure and will report every
    // path as 0 B/iter — which reads as "clean" but means "not measured". Run it
    // on Windows or Linux for a meaningful result.
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<usize>().ok())
        .map(|pages| pages * 4096)
        .unwrap_or_else(|| {
            eprintln!("warning: cannot read process memory on this platform; results are meaningless");
            0
        })
}

/// Run `body` `iters` times and report bytes retained per iteration.
///
/// A warmup pass runs first so that one-off allocations — lazily initialised SDK
/// state, allocator arena growth — are not misread as a leak.
fn measure(name: &str, iters: usize, mut body: impl FnMut()) -> f64 {
    let warmup = (iters / 10).max(1_000);
    for _ in 0..warmup {
        body();
    }

    let before = committed_bytes();
    for _ in 0..iters {
        body();
    }
    let after = committed_bytes();

    let delta = after as i64 - before as i64;
    let per_iter = delta as f64 / iters as f64;

    // Allocator noise is a few KB either way; flag only sustained growth.
    let verdict = if per_iter > 1.0 {
        "LEAK?"
    } else if per_iter > 0.05 {
        "watch"
    } else {
        "ok"
    };
    println!("  {name:<44} {delta:>+10} B total  {per_iter:>8.3} B/iter  {verdict}");
    per_iter
}

fn main() {
    let iters: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(200_000);

    println!("leakcheck: {iters} iterations per path\n");
    let mut worst: Vec<(String, f64)> = Vec::new();

    // --- strings out of the SDK (string::out) ---
    worst.push((
        "static scope getters".into(),
        measure("Client::default_presence_scopes()", iters, || {
            std::hint::black_box(Client::default_presence_scopes());
        }),
    ));
    worst.push((
        "version_hash".into(),
        measure("Client::version_hash()", iters, || {
            std::hint::black_box(Client::version_hash());
        }),
    ));

    // --- handle construction and destruction ---
    worst.push((
        "Activity new/drop".into(),
        measure("Activity::new() + drop", iters, || {
            std::hint::black_box(Activity::new());
        }),
    ));

    // --- strings into and back out of a handle ---
    let mut activity = Activity::new();
    worst.push((
        "string set/get".into(),
        measure("Activity set_name/name round trip", iters, || {
            activity.set_name("a moderately long activity name");
            std::hint::black_box(activity.name());
        }),
    ));

    // --- optional getters (string::out_opt), both present and absent ---
    worst.push((
        "optional string present".into(),
        measure("Activity set_state(Some)/state", iters, || {
            activity.set_state(Some("in a match"));
            std::hint::black_box(activity.state());
        }),
    ));
    activity.set_state(None);
    worst.push((
        "optional string absent".into(),
        measure("Activity state() == None", iters, || {
            std::hint::black_box(activity.state());
        }),
    ));

    // --- enums, a pure by-value path used as a control ---
    worst.push((
        "enum round trip (control)".into(),
        measure("Activity activity_type round trip", iters, || {
            activity.set_activity_type(ActivityType::Playing);
            std::hint::black_box(activity.activity_type());
        }),
    ));

    // --- spans (span::out): elements adopted, array freed ---
    let mut spanned = Activity::new();
    spanned.add_button(&ActivityButton::with_label_and_url(
        "Play",
        "https://example.com/1",
    ));
    spanned.add_button(&ActivityButton::with_label_and_url(
        "Watch",
        "https://example.com/2",
    ));
    worst.push((
        "span getter".into(),
        measure("Activity::buttons() -> Vec<ActivityButton>", iters, || {
            std::hint::black_box(spanned.buttons());
        }),
    ));

    // --- clone ---
    worst.push((
        "handle clone".into(),
        measure("Activity::clone() + drop", iters, || {
            std::hint::black_box(spanned.clone());
        }),
    ));

    // --- nested handle in/out (setter copies, getter transfers) ---
    let mut party = ActivityParty::new();
    party.set_id("party-1");
    worst.push((
        "nested handle set/get".into(),
        measure("Activity set_party/party round trip", iters, || {
            activity.set_party(Some(&party));
            std::hint::black_box(activity.party());
        }),
    ));

    // --- client construction ---
    worst.push((
        "Client new/drop".into(),
        measure("Client::new() + drop", iters / 20, || {
            std::hint::black_box(Client::new());
        }),
    ));

    // --- callback userdata: box, hand to SDK, replace, let SDK free ---
    let mut client = Client::new();
    worst.push((
        "callback register/replace".into(),
        measure("on_status_changed + run_callbacks", iters / 20, || {
            client.on_status_changed(|_, _, _| {});
            run_callbacks();
        }),
    ));

    // Log sinks are excluded on purpose: the SDK is documented not to release
    // them, so unbounded growth here is expected behaviour, not a wrapper bug.
    let _ = LoggingSeverity::Info;

    println!("\nsummary");
    worst.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (name, per_iter) in worst.iter().take(3) {
        println!("  highest retention: {name:<32} {per_iter:>8.3} B/iter");
    }
    let suspicious = worst.iter().filter(|(_, v)| *v > 1.0).count();
    println!(
        "\n{} of {} paths retained more than 1 B/iter",
        suspicious,
        worst.len()
    );
}
