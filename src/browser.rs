//! Delegate browser activation to the desktop rather than inheriting app restrictions.

pub fn open_join_url(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        match linux::open_portal(url) {
            Ok(()) => return Ok(()),
            Err(linux::PortalError::Unavailable) => {}
            Err(linux::PortalError::Failed(error)) => return Err(error),
        }
    }
    webbrowser::open(url).map_err(Into::into)
}

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::{anyhow, Context};
    use futures_lite::StreamExt;
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

    pub(super) enum PortalError {
        Unavailable,
        Failed(anyhow::Error),
    }

    fn classify(error: zbus::Error) -> PortalError {
        match &error {
            zbus::Error::FDO(error)
                if matches!(
                    error.as_ref(),
                    zbus::fdo::Error::ServiceUnknown(_)
                        | zbus::fdo::Error::NameHasNoOwner(_)
                        | zbus::fdo::Error::UnknownMethod(_)
                        | zbus::fdo::Error::UnknownInterface(_)
                ) =>
            {
                PortalError::Unavailable
            }
            zbus::Error::MethodError(name, _, _)
                if matches!(
                    name.as_str(),
                    "org.freedesktop.DBus.Error.ServiceUnknown"
                        | "org.freedesktop.DBus.Error.NameHasNoOwner"
                        | "org.freedesktop.DBus.Error.UnknownMethod"
                        | "org.freedesktop.DBus.Error.UnknownInterface"
                ) =>
            {
                PortalError::Unavailable
            }
            _ => PortalError::Failed(error.into()),
        }
    }

    fn response_result(code: u32) -> anyhow::Result<()> {
        match code {
            0 => Ok(()),
            1 => Err(anyhow!("Browser activation was cancelled")),
            _ => Err(anyhow!(
                "Desktop browser activation failed (response {code})"
            )),
        }
    }

    pub(super) fn open_portal(url: &str) -> Result<(), PortalError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| PortalError::Failed(error.into()))?;
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(30), activate(url))
                .await
                .map_err(|_| PortalError::Failed(anyhow!("Desktop browser activation timed out")))?
        })
    }

    async fn activate(url: &str) -> Result<(), PortalError> {
        let connection = zbus::Connection::session()
            .await
            .map_err(|_| PortalError::Unavailable)?;
        let portal = zbus::Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.OpenURI",
        )
        .await
        .map_err(classify)?;
        static NEXT_REQUEST: AtomicU64 = AtomicU64::new(0);
        let token = format!(
            "quick_presenter_{}",
            NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
        );
        let sender = connection
            .unique_name()
            .ok_or_else(|| PortalError::Failed(anyhow!("Missing D-Bus sender")))?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
        let request = zbus::Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            path.as_str(),
            "org.freedesktop.portal.Request",
        )
        .await
        .map_err(classify)?;
        // Subscribe before OpenURI: its response can arrive before the method reply.
        let mut responses = request.receive_signal("Response").await.map_err(classify)?;
        let options = HashMap::from([("handle_token", Value::from(token.as_str()))]);
        let handle: OwnedObjectPath = portal
            .call("OpenURI", &("", url, options))
            .await
            .map_err(classify)?;
        if handle.as_str() != path {
            return Err(PortalError::Failed(anyhow!(
                "Unexpected desktop request handle"
            )));
        }
        let response = responses
            .next()
            .await
            .ok_or_else(|| PortalError::Failed(anyhow!("Desktop response stream closed")))?;
        let (code, _results): (u32, HashMap<String, OwnedValue>) =
            response.body().deserialize().map_err(classify)?;
        response_result(code)
            .context("Cannot open the join URL")
            .map_err(PortalError::Failed)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn portal_acknowledgement_requires_a_successful_response() {
            assert!(response_result(0).is_ok());
            assert!(response_result(1)
                .unwrap_err()
                .to_string()
                .contains("cancelled"));
            assert!(response_result(2).is_err());
            assert!(response_result(u32::MAX).is_err());
        }

        #[test]
        fn rejected_activation_does_not_fall_back_to_a_second_launch() {
            assert!(matches!(
                classify(zbus::Error::Failure("denied".into())),
                PortalError::Failed(_)
            ));
            assert!(matches!(
                classify(zbus::fdo::Error::AccessDenied("denied".into()).into()),
                PortalError::Failed(_)
            ));
        }

        #[test]
        fn missing_desktop_portal_allows_the_legacy_launcher() {
            for error in [
                zbus::fdo::Error::ServiceUnknown("missing".into()),
                zbus::fdo::Error::NameHasNoOwner("missing".into()),
                zbus::fdo::Error::UnknownMethod("missing".into()),
                zbus::fdo::Error::UnknownInterface("missing".into()),
            ] {
                assert!(matches!(classify(error.into()), PortalError::Unavailable));
            }
        }
    }
}
