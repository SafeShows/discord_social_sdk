//! End-to-end checks against the real Discord SDK binary.
//!
//! These exercise everything that does not need a live connection: handle
//! construction and destruction, the string boundary in both directions,
//! optional getters, span ownership, `Clone`, and callback registration and
//! teardown. Anything requiring an authenticated session is out of scope here.
//!
//! Run under a leak checker if one is available — most of what these assert is
//! that ownership is handled correctly, which otherwise fails silently.

use discord_social_sdk::enums::{ActivityType, ClientStatus, LoggingSeverity};
use discord_social_sdk::{Activity, ActivityButton, ActivityParty, Client, run_callbacks};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

/// Serialises tests that touch the SDK's process-global callback queue.
///
/// `run_callbacks` and `reset_callbacks` act on state shared by every `Client` in
/// the process, and the SDK is designed to be driven from a single thread. Rust
/// runs tests on parallel threads, so without this one test's pump drains — and
/// frees — another's callbacks, which fails intermittently.
static SDK: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    // A poisoned lock only means some earlier test panicked; the SDK state is no
    // less usable, so recover rather than cascading failures into every test.
    SDK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn client_reports_its_version() {
    // Reading static version data proves the library is linked and callable.
    assert!(Client::version_major() >= 1, "unexpected major version");
    assert!(!Client::version_hash().is_empty(), "expected a build hash");
}

#[test]
fn default_scopes_are_non_empty() {
    // Exercises the owned-string path: the SDK allocates, we read and free.
    let presence = Client::default_presence_scopes();
    let communication = Client::default_communication_scopes();

    // Both are OAuth2 scope strings, and both need an identity scope.
    assert!(presence.contains("openid"), "unexpected presence scopes: {presence}");
    assert!(
        communication.contains("openid"),
        "unexpected communication scopes: {communication}"
    );

    // Presence is the narrower, presence-only grant; communication is the broader
    // social grant. Note the broader one is the *shorter* string, so length is not
    // a proxy for how much access a scope set confers.
    assert!(presence.contains("sdk.social_layer_presence"));
    assert!(communication.contains("sdk.social_layer"));
    assert!(!communication.contains("sdk.social_layer_presence"));
}

#[test]
fn client_lifecycle_without_connecting() {
    let _guard = exclusive();
    let mut client = Client::new();

    client.set_application_id(1234567890);
    assert_eq!(client.application_id(), 1234567890);

    // A client that has never connected must not claim to be ready.
    assert_ne!(client.status(), ClientStatus::Ready);
    assert!(!client.is_authenticated());

    // Debug must not panic even in this partially-initialised state.
    assert!(format!("{client:?}").contains("Client"));
}

#[test]
fn strings_round_trip_through_the_sdk() {
    let mut activity = Activity::new();

    activity.set_name("Test Game");
    assert_eq!(activity.name(), "Test Game");

    // Non-ASCII proves the boundary is length-delimited, not NUL-terminated.
    activity.set_name("🎮 テスト");
    assert_eq!(activity.name(), "🎮 テスト");

    // An embedded NUL must survive, since Discord_String carries an explicit size.
    activity.set_name("a\0b");
    assert_eq!(activity.name(), "a\0b");
}

#[test]
fn optional_fields_distinguish_absent_from_empty() {
    let mut activity = Activity::new();

    // Never set: the SDK reports absent, not empty.
    assert_eq!(activity.state(), None);

    activity.set_state(Some("in a match"));
    assert_eq!(activity.state(), Some("in a match".to_string()));

    // An empty string is a value, distinct from clearing the field.
    activity.set_state(Some(""));
    assert_eq!(activity.state(), Some(String::new()));

    activity.set_state(None);
    assert_eq!(activity.state(), None);
}

#[test]
fn enums_round_trip() {
    let mut activity = Activity::new();

    activity.set_activity_type(ActivityType::Playing);
    assert_eq!(activity.activity_type(), ActivityType::Playing);

    activity.set_activity_type(ActivityType::Competing);
    assert_eq!(activity.activity_type(), ActivityType::Competing);
}

#[test]
fn spans_transfer_ownership_of_every_element() {
    let mut activity = Activity::new();

    activity.add_button(&ActivityButton::with_label_and_url("Play", "https://example.com/1"));
    activity.add_button(&ActivityButton::with_label_and_url("Watch", "https://example.com/2"));

    let buttons = activity.buttons();
    assert_eq!(buttons.len(), 2);
    assert_eq!(buttons[0].label(), "Play");
    assert_eq!(buttons[1].url(), "https://example.com/2");

    // Dropping the owned Vec must not disturb the activity that produced it.
    drop(buttons);
    assert_eq!(activity.buttons().len(), 2);
}

#[test]
fn clone_produces_an_independent_handle() {
    let mut original = Activity::new();
    original.set_name("original");

    let mut copy = original.clone();
    copy.set_name("copy");

    // Mutating the clone must not reach through to the source.
    assert_eq!(original.name(), "original");
    assert_eq!(copy.name(), "copy");

    // Dropping one must leave the other usable, proving the handles are distinct.
    drop(copy);
    assert_eq!(original.name(), "original");
}

#[test]
fn nested_handles_are_owned_by_the_caller() {
    let mut activity = Activity::new();

    let mut party = ActivityParty::new();
    party.set_id("party-1");
    party.set_current_size(2);
    party.set_max_size(4);

    // The setter copies, so the local party stays ours to drop.
    activity.set_party(Some(&party));
    drop(party);

    let fetched = activity.party().expect("party was just set");
    assert_eq!(fetched.id(), "party-1");
    assert_eq!(fetched.current_size(), 2);
    assert_eq!(fetched.max_size(), 4);
}

#[test]
fn callbacks_are_registered_and_their_closures_released() {
    let _guard = exclusive();
    // The Rc's strong count tells us whether the SDK actually dropped the boxed
    // closure: if the userdata leaks, the count never returns to 1.
    let witness = Rc::new(RefCell::new(Vec::<String>::new()));
    let mut client = Client::new();

    {
        let witness = Rc::clone(&witness);
        client.on_status_changed(move |status, _error, _detail| {
            witness.borrow_mut().push(format!("{status:?}"));
        });
    }
    assert_eq!(Rc::strong_count(&witness), 2, "closure should hold the Rc");

    // Replacing the handler hands the old userdata back to the SDK, but the SDK
    // defers running the free function until the next callback pump rather than
    // freeing inline — so the count does not drop until `run_callbacks`.
    client.on_status_changed(|_, _, _| {});
    assert_eq!(
        Rc::strong_count(&witness),
        2,
        "the SDK should not have released the closure before the queue is pumped"
    );

    run_callbacks();
    assert_eq!(
        Rc::strong_count(&witness),
        1,
        "pumping callbacks must release the replaced closure"
    );

    drop(client);
    assert_eq!(Rc::strong_count(&witness), 1);
}

/// Pins down what is actually guaranteed about log sink lifetime.
///
/// Teardown here is deliberately *not* asserted: the debug SDK never frees a log
/// sink while the release SDK frees it on client drop, so asserting either one
/// fails under the other profile. What must hold in both is that the closure
/// stays alive for as long as the SDK might still invoke it — releasing it early
/// would be a use-after-free — and that it is never released more than once.
#[test]
fn log_callbacks_stay_alive_while_registered() {
    let _guard = exclusive();
    let witness = Rc::new(RefCell::new(0u32));
    let mut client = Client::new();

    {
        let witness = Rc::clone(&witness);
        client.add_log_callback(LoggingSeverity::Info, move |_message, _severity| {
            *witness.borrow_mut() += 1;
        });
    }
    assert_eq!(
        Rc::strong_count(&witness),
        2,
        "the SDK must hold the boxed closure while it is registered"
    );

    // Pumping while the sink is still installed must not release it.
    run_callbacks();
    assert_eq!(
        Rc::strong_count(&witness),
        2,
        "a live log sink must survive a callback pump"
    );

    drop(client);
    run_callbacks();
    discord_social_sdk::reset_callbacks();
    run_callbacks();

    // Either 1 (release SDK freed it) or 2 (debug SDK retained it) is acceptable;
    // anything else would mean a double free or a corrupted refcount.
    let remaining = Rc::strong_count(&witness);
    assert!(
        remaining == 1 || remaining == 2,
        "unexpected refcount {remaining} after teardown"
    );
}

#[test]
fn many_handles_can_be_created_and_dropped() {
    // A crude check that nothing accumulates per handle; a leak here shows up as
    // memory growth rather than a failed assertion, so pair it with a leak checker.
    for i in 0..1000 {
        let mut activity = Activity::new();
        activity.set_name(&format!("activity {i}"));
        activity.set_details(Some("details"));
        let _ = activity.name();
        let _ = activity.clone();
    }
}
