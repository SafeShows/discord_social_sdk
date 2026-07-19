//! Users known to the SDK, and their per-application external profiles.
//!
//! A [`User`] is a single Discord account the SDK knows about: basic account
//! information such as id, name and avatar, plus "status" information covering
//! both whether they are online and whether they are playing this game.
//!
//! A [`UserApplicationProfile`] is the same person as seen through an external
//! identity provider such as Steam or Epic Online Services.
//!
//! # Handle lifetime
//!
//! Handles hold a reference to both the underlying data and the SDK instance.
//! Changes to the underlying data are generally visible through an existing
//! handle without re-creating it. If the SDK instance is destroyed while a
//! handle is still alive, every accessor returns a default value instead —
//! an empty [`String`], a zero id, and so on — rather than failing.

use crate::activity::Activity;
use crate::enums::{AvatarType, ExternalIdentityProviderType, StatusType};
use crate::handle::handle;
use crate::relationship::Relationship;
use crate::{span, string};
use discord_social_sdk_sys as sys;
use std::mem::MaybeUninit;

handle! {
    /// A single Discord user the SDK knows about.
    ///
    /// Obtained from the client, from a [`Relationship`], or from lobby and
    /// message payloads — never constructed directly, which is why there is no
    /// constructor here.
    ///
    /// See the [handle lifetime](self#handle-lifetime) notes for what happens to
    /// a handle that outlives the SDK instance.
    User(sys::Discord_UserHandle) {
        drop: sys::Discord_UserHandle_Drop,
        clone: sys::Discord_UserHandle_Clone,
    }
}

impl User {
    /// The hash of the user's Discord profile avatar, if one is set.
    ///
    /// This is the raw hash; use [`avatar_url`](Self::avatar_url) to get
    /// something directly loadable.
    pub fn avatar(&self) -> Option<String> {
        // SAFETY: read-only getter on a live handle; `out_opt` only adopts the
        // out-parameter when the SDK reports the field is present.
        unsafe { string::out_opt(|out| sys::Discord_UserHandle_Avatar(self.raw_ptr(), out)) }
    }

    /// A CDN url for the user's Discord profile avatar.
    ///
    /// `animated_type` is used when the user's avatar is animated and
    /// `static_type` when it is not. If the user has no avatar set, a url to one
    /// of Discord's default avatars is returned instead, so this never yields an
    /// empty result for a valid handle.
    pub fn avatar_url(&self, animated_type: AvatarType, static_type: AvatarType) -> String {
        // SAFETY: read-only getter; the returned string is freshly allocated and
        // ownership transfers to us, which `string::out` takes care of.
        unsafe {
            string::out(|out| {
                sys::Discord_UserHandle_AvatarUrl(
                    self.raw_ptr(),
                    animated_type.into_raw(),
                    static_type.into_raw(),
                    out,
                )
            })
        }
    }

    /// The name to show for this user: their preferred name if one is set,
    /// otherwise their unique username.
    pub fn display_name(&self) -> String {
        // SAFETY: read-only getter transferring ownership of the out-parameter.
        unsafe { string::out(|out| sys::Discord_UserHandle_DisplayName(self.raw_ptr(), out)) }
    }

    /// The user's rich presence activity associated with the current game, if
    /// one is set.
    ///
    /// A user may have several rich presence activities at once on Discord, but
    /// the SDK only exposes the one belonging to your application. Use it to
    /// learn about the party the user is in, if any, and what they are doing in
    /// the game.
    ///
    /// See the [rich presence overview] for more information.
    ///
    /// [rich presence overview]: https://discord.com/developers/docs/rich-presence/overview
    pub fn game_activity(&self) -> Option<Activity> {
        let mut raw = MaybeUninit::<sys::Discord_Activity>::uninit();
        // SAFETY: read-only getter. It only writes the out-parameter when it
        // returns true, so `assume_init` is confined to that branch, and the
        // activity it writes is owned by us from then on.
        unsafe {
            if sys::Discord_UserHandle_GameActivity(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(Activity::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The user's preferred display name, if one is set.
    ///
    /// Discord's public API calls this the "global name" rather than the display
    /// name. Users can set it to almost any string. See the [user resource
    /// documentation] for more information.
    ///
    /// [user resource documentation]: https://discord.com/developers/docs/resources/user
    pub fn global_name(&self) -> Option<String> {
        // SAFETY: read-only getter; ownership of the out-parameter transfers to
        // us only when it reports the field is present.
        unsafe { string::out_opt(|out| sys::Discord_UserHandle_GlobalName(self.raw_ptr(), out)) }
    }

    /// The id of this user.
    ///
    /// A return value of `0` means the handle is no longer valid — for example
    /// because the SDK instance it referenced has been destroyed.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only getter on a live handle.
        unsafe { sys::Discord_UserHandle_Id(self.raw_ptr()) }
    }

    /// Whether this user is a provisional account.
    ///
    /// Provisional accounts are created for players who have not linked a real
    /// Discord account; their [`username`](Self::username) is auto-generated.
    pub fn is_provisional(&self) -> bool {
        // SAFETY: read-only getter on a live handle.
        unsafe { sys::Discord_UserHandle_IsProvisional(self.raw_ptr()) }
    }

    /// The relationship between the currently authenticated user and this user.
    ///
    /// Always returns a handle; when there is no relationship its
    /// [`discord_relationship_type`](Relationship::discord_relationship_type) is
    /// [`RelationshipType::None`](crate::enums::RelationshipType::None).
    pub fn relationship(&self) -> Relationship {
        let mut raw = MaybeUninit::<sys::Discord_RelationshipHandle>::uninit();
        // SAFETY: read-only getter that always initialises the out-parameter and
        // transfers ownership of the resulting handle to us.
        unsafe {
            sys::Discord_UserHandle_Relationship(self.raw_ptr(), raw.as_mut_ptr());
            Relationship::from_raw(raw.assume_init())
        }
    }

    /// The user's online, offline or idle status.
    pub fn status(&self) -> StatusType {
        // SAFETY: read-only getter on a live handle.
        StatusType::from_raw(unsafe { sys::Discord_UserHandle_Status(self.raw_ptr()) })
    }

    /// The external identity provider profiles for this user.
    ///
    /// A user can currently have at most one profile per application, so this
    /// contains either zero or one element.
    pub fn user_application_profiles(&self) -> Vec<UserApplicationProfile> {
        // SAFETY: read-only span getter. Both the array and every element are
        // transferred to us; `span::out` adopts each element and frees the array.
        unsafe {
            span::out(
                |out| sys::Discord_UserHandle_UserApplicationProfiles(self.raw_ptr(), out),
                |raw| UserApplicationProfile::from_raw(raw),
            )
        }
    }

    /// The globally unique username of this user.
    ///
    /// For provisional accounts this is an auto-generated string. See the [user
    /// resource documentation] for more information.
    ///
    /// [user resource documentation]: https://discord.com/developers/docs/resources/user
    pub fn username(&self) -> String {
        // SAFETY: read-only getter transferring ownership of the out-parameter.
        unsafe { string::out(|out| sys::Discord_UserHandle_Username(self.raw_ptr(), out)) }
    }

    /// The SDK's own name for an [`AvatarType`], such as `"png"`.
    ///
    /// Free-standing in the C API rather than an accessor, so it takes no user.
    pub fn avatar_type_to_string(avatar_type: AvatarType) -> String {
        // SAFETY: pure conversion; the returned string is ours to free.
        unsafe {
            string::out(|out| {
                sys::Discord_UserHandle_AvatarTypeToString(avatar_type.into_raw(), out)
            })
        }
    }
}

impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id())
            .field("username", &self.username())
            .field("status", &self.status())
            .field("is_provisional", &self.is_provisional())
            .finish()
    }
}

handle! {
    /// A user's profile as reported by an external identity provider.
    ///
    /// External identity providers include Steam and Epic Online Services; which
    /// one this profile came from is given by
    /// [`provider_type`](UserApplicationProfile::provider_type).
    ///
    /// Obtained from [`User::user_application_profiles`].
    UserApplicationProfile(sys::Discord_UserApplicationProfileHandle) {
        drop: sys::Discord_UserApplicationProfileHandle_Drop,
        clone: sys::Discord_UserApplicationProfileHandle_Clone,
    }
}

impl UserApplicationProfile {
    /// The user's in-game avatar hash.
    pub fn avatar_hash(&self) -> String {
        // SAFETY: read-only getter transferring ownership of the out-parameter.
        unsafe {
            string::out(|out| {
                sys::Discord_UserApplicationProfileHandle_AvatarHash(self.raw_ptr(), out)
            })
        }
    }

    /// Any metadata set by the developer.
    pub fn metadata(&self) -> String {
        // SAFETY: read-only getter transferring ownership of the out-parameter.
        unsafe {
            string::out(|out| {
                sys::Discord_UserApplicationProfileHandle_Metadata(self.raw_ptr(), out)
            })
        }
    }

    /// The user's external identity provider id, if it exists.
    pub fn provider_id(&self) -> Option<String> {
        // SAFETY: read-only getter; ownership of the out-parameter transfers to
        // us only when it reports the field is present.
        unsafe {
            string::out_opt(|out| {
                sys::Discord_UserApplicationProfileHandle_ProviderId(self.raw_ptr(), out)
            })
        }
    }

    /// The user id issued by the external identity provider.
    pub fn provider_issued_user_id(&self) -> String {
        // SAFETY: read-only getter transferring ownership of the out-parameter.
        unsafe {
            string::out(|out| {
                sys::Discord_UserApplicationProfileHandle_ProviderIssuedUserId(self.raw_ptr(), out)
            })
        }
    }

    /// Which external identity provider this profile came from.
    pub fn provider_type(&self) -> ExternalIdentityProviderType {
        // SAFETY: read-only getter on a live handle.
        ExternalIdentityProviderType::from_raw(unsafe {
            sys::Discord_UserApplicationProfileHandle_ProviderType(self.raw_ptr())
        })
    }

    /// The user's in-game username.
    pub fn username(&self) -> String {
        // SAFETY: read-only getter transferring ownership of the out-parameter.
        unsafe {
            string::out(|out| {
                sys::Discord_UserApplicationProfileHandle_Username(self.raw_ptr(), out)
            })
        }
    }
}

impl std::fmt::Debug for UserApplicationProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserApplicationProfile")
            .field("username", &self.username())
            .field("provider_type", &self.provider_type())
            .field("provider_issued_user_id", &self.provider_issued_user_id())
            .finish()
    }
}
