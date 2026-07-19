//! The relationship between the current user and another user.
//!
//! Relationships cover friends, blocked users and pending friend invites.
//!
//! # Discord relationships versus game relationships
//!
//! The SDK supports two kinds of relationship, and a [`Relationship`] carries a
//! type field for each:
//!
//! - **Discord** relationships persist across games and on the Discord client
//!   itself. Both users can see whether the other is online regardless of
//!   whether they are playing the same game.
//! - **Game** relationships are per-game and do not carry over to other games.
//!   The two users can only see each other's online state while playing a game
//!   in which they are friends.
//!
//! A game friend can later "upgrade" to a full Discord friend, at which point
//! both relationships exist simultaneously — which is why there are two type
//! fields. During such an upgrade
//! [`discord_relationship_type`](Relationship::discord_relationship_type) is
//! [`PendingIncoming`](crate::enums::RelationshipType::PendingIncoming) or
//! [`PendingOutgoing`](crate::enums::RelationshipType::PendingOutgoing)
//! depending on which side sent the invite, while
//! [`game_relationship_type`](Relationship::game_relationship_type) stays
//! [`Friend`](crate::enums::RelationshipType::Friend).
//!
//! Blocking is always recorded on the Discord relationship and persists across
//! games; it is not possible to block a user in only one game.
//!
//! # Consent
//!
//! The SDK lets a game manage relationships, but no such action should ever be
//! taken without the user's explicit consent. Never send or accept friend
//! requests automatically — only in response to a deliberate user action such as
//! clicking a "Send Friend Request" button.

use crate::enums::RelationshipType;
use crate::handle::handle;
use crate::user::User;
use discord_social_sdk_sys as sys;
use std::mem::MaybeUninit;

handle! {
    /// The relationship between the current user and a target user.
    ///
    /// Obtained from the client's relationship list or from
    /// [`User::relationship`] — never constructed directly, which is why there
    /// is no constructor here.
    ///
    /// See the [module documentation](self) for how the Discord and game
    /// relationship types interact.
    Relationship(sys::Discord_RelationshipHandle) {
        drop: sys::Discord_RelationshipHandle_Drop,
        clone: sys::Discord_RelationshipHandle_Clone,
    }
}

impl Relationship {
    /// The Discord-wide relationship type.
    ///
    /// This is the relationship that persists across games and on the Discord
    /// client, and it is where blocking is always recorded.
    pub fn discord_relationship_type(&self) -> RelationshipType {
        // SAFETY: read-only getter on a live handle.
        RelationshipType::from_raw(unsafe {
            sys::Discord_RelationshipHandle_DiscordRelationshipType(self.raw_ptr())
        })
    }

    /// The per-game relationship type.
    ///
    /// This relationship applies only to this game and does not carry over to
    /// others.
    pub fn game_relationship_type(&self) -> RelationshipType {
        // SAFETY: read-only getter on a live handle.
        RelationshipType::from_raw(unsafe {
            sys::Discord_RelationshipHandle_GameRelationshipType(self.raw_ptr())
        })
    }

    /// The id of the target user in this relationship.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only getter on a live handle.
        unsafe { sys::Discord_RelationshipHandle_Id(self.raw_ptr()) }
    }

    /// Whether this relationship is a spam request.
    pub fn is_spam_request(&self) -> bool {
        // SAFETY: read-only getter on a live handle.
        unsafe { sys::Discord_RelationshipHandle_IsSpamRequest(self.raw_ptr()) }
    }

    /// A handle to the target user in this relationship, if one is available.
    ///
    /// This is the user whose id [`id`](Self::id) returns.
    pub fn user(&self) -> Option<User> {
        let mut raw = MaybeUninit::<sys::Discord_UserHandle>::uninit();
        // SAFETY: read-only getter. It only writes the out-parameter when it
        // returns true, so `assume_init` is confined to that branch; the handle
        // it writes is owned by us and dropped by `User`.
        unsafe {
            if sys::Discord_RelationshipHandle_User(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(User::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }
}

impl std::fmt::Debug for Relationship {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relationship")
            .field("id", &self.id())
            .field("discord_relationship_type", &self.discord_relationship_type())
            .field("game_relationship_type", &self.game_relationship_type())
            .field("is_spam_request", &self.is_spam_request())
            .finish()
    }
}
