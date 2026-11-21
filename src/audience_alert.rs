//! Presenter-facing recovery guidance. Never render untrusted server text.
#[derive(Debug, PartialEq)]
pub struct AudienceAlert {
    pub title: &'static str,
    pub guidance: &'static str,
}

pub fn for_failure(error: &str) -> AudienceAlert {
    match error {
        "Relay creation credential was rejected."
        | "Relay authentication failed." | "Relay access was denied." => AudienceAlert {
            title: "Audience Live access was not allowed",
            guidance: "Check the relay credentials and session access with your server administrator, then turn Audience Live ON again. PDF playback is still available.",
        },
        "Relay denied room creation." => AudienceAlert {
            title: "Audience Live could not start a new session",
            guidance: "Another session may still be active, or new sessions may be restricted. If your relay provides a dashboard, check its status and complete any required activation there first. Stop Audience Live on the previous presenter, or ask your relay administrator to check session access and end the old session if needed. Then turn Audience Live ON again. PDF playback is still available.",
        },
        "Relay room expired. Start again to create a new room." => AudienceAlert {
            title: "Audience Live session has ended",
            guidance: "Check whether your relay allows another session within the original service period. If a dashboard is provided, check its status before retrying. When a new session is available, turn Audience Live ON and share the new QR code. PDF playback is still available.",
        },
        "Cannot connect to relay." | "Relay connection timed out."
        | "Relay authentication timed out."
        | "Relay startup send timed out." | "Relay connection closed during startup."
        | "Relay room creation is unavailable. Retry to recover the same room."
        | "Relay room creation timed out. Retry to recover the same room." => AudienceAlert {
            title: "Audience Live could not connect",
            guidance: "Check your network and relay server, then turn Audience Live ON again. PDF playback is still available.",
        },
        "Cannot confirm relay room termination." => AudienceAlert {
            title: "Audience Live room end is not confirmed",
            guidance: "Do not assume the previous room has ended. Check that same room through your relay administrator or dashboard before starting another session. PDF playback is still available.",
        },
        "Cannot confirm session termination. Retry to check the same session." => AudienceAlert {
            title: "Audience Live end is not confirmed",
            guidance: "Check your network and relay server. Turn Audience Live ON to retry the pending end before any connection is attempted. If your relay provides a dashboard, check the same session there. PDF playback is still available.",
        },
        "Relay heartbeat timed out." | "Relay connection closed."
        | "Relay send timed out."
        | "Relay disconnected. Start again to resume; events are not replayed." => AudienceAlert {
            title: "Audience Live connection was lost",
            guidance: "Check your network and relay server, then turn Audience Live ON again. If a new session starts, share its new QR code. Previous comments will not be resent. PDF playback is still available.",
        },
        _ => AudienceAlert {
            title: "Audience Live could not continue",
            guidance: "Check your Audience Live configuration and relay server, then turn Audience Live ON again. PDF playback is still available.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn room_end_failure_requires_previous_room_confirmation() {
        let alert = for_failure("Cannot confirm relay room termination.");
        assert_eq!(alert.title, "Audience Live room end is not confirmed");
        assert!(alert.guidance.contains("previous room"));
        assert!(alert.guidance.contains("before starting another session"));
        assert!(alert.guidance.contains("PDF playback is still available"));
        assert_ne!(
            alert.title,
            for_failure("Cannot confirm session termination. Retry to check the same session.")
                .title
        );
    }
    #[test]
    fn pending_end_guidance_preserves_same_session_recovery() {
        let alert =
            for_failure("Cannot confirm session termination. Retry to check the same session.");
        assert_eq!(alert.title, "Audience Live end is not confirmed");
        assert!(alert.guidance.contains("retry the pending end"));
        assert!(alert.guidance.contains("same session"));
        assert!(alert.guidance.contains("PDF playback is still available"));
    }
    #[test]
    fn denied_access_has_actionable_guidance() {
        for error in [
            "Relay creation credential was rejected.",
            "Relay authentication failed.",
            "Relay access was denied.",
        ] {
            let alert = for_failure(error);
            assert_eq!(alert.title, "Audience Live access was not allowed");
            assert!(alert.guidance.contains("server administrator"));
        }
    }
    #[test]
    fn denied_creation_explains_possible_previous_session_without_assuming_its_cause() {
        let alert = for_failure("Relay denied room creation.");
        assert_eq!(alert.title, "Audience Live could not start a new session");
        assert!(alert.guidance.contains("may still be active"));
        assert!(alert.guidance.contains("previous presenter"));
        assert!(alert.guidance.contains("required activation"));
        assert!(alert.guidance.contains("dashboard"));
        assert!(alert.guidance.contains("check session access"));
        assert!(alert.guidance.contains("PDF playback is still available"));
    }
    #[test]
    fn expired_session_requires_a_new_join_link() {
        let alert = for_failure("Relay room expired. Start again to create a new room.");
        assert_eq!(alert.title, "Audience Live session has ended");
        assert!(alert.guidance.contains("new QR code"));
        assert!(alert.guidance.contains("original service period"));
        assert!(alert.guidance.contains("dashboard"));
    }
    #[test]
    fn transport_failure_does_not_claim_access_revocation() {
        for error in [
            "Relay heartbeat timed out.",
            "Relay connection closed.",
            "Relay disconnected. Start again to resume; events are not replayed.",
        ] {
            let alert = for_failure(error);
            assert_eq!(alert.title, "Audience Live connection was lost");
            assert!(alert.guidance.contains("PDF playback is still available"));
        }
    }
    #[test]
    fn a_failed_start_is_not_reported_as_a_lost_connection() {
        for error in [
            "Relay room creation is unavailable. Retry to recover the same room.",
            "Relay startup send timed out.",
            "Relay connection closed during startup.",
        ] {
            let alert = for_failure(error);
            assert_eq!(alert.title, "Audience Live could not connect");
            assert!(alert.guidance.contains("Check your network"));
        }
    }
    #[test]
    fn untrusted_details_are_not_displayed() {
        let alert = for_failure("https://secret.example/?token=secret");
        assert!(!alert.guidance.contains("secret"));
        assert_eq!(alert.title, "Audience Live could not continue");
    }
}
