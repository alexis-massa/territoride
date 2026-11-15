//! The HTTP client for Strava and Overpass.

use std::time::Duration;

use reqwest::{Certificate, Client};

/// A client that trusts the bundled Mozilla roots, not the system's: the container is `FROM scratch` and has
/// no certificate store.
pub fn client(timeout: Duration, user_agent: &str) -> Client {
    let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|der| Certificate::from_der(der.as_ref()).ok());
    Client::builder()
        .tls_certs_only(roots)
        .timeout(timeout)
        .user_agent(user_agent)
        .build()
        .expect("the HTTP client builds")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_with_the_bundled_roots() {
        assert!(webpki_root_certs::TLS_SERVER_ROOT_CERTS.len() > 100);
        let _ = client(Duration::from_secs(1), "test");
    }
}
