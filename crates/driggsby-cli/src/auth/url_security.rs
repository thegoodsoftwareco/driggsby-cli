use anyhow::{Result, bail};

pub fn assert_broker_remote_url(raw_url: &str, context: &str) -> Result<()> {
    let url = url::Url::parse(raw_url)?;
    if url.scheme() == "https" || (url.scheme() == "http" && is_loopback_url(&url)) {
        return Ok(());
    }

    bail!("{context} must use https, except for local loopback development.")
}

fn is_loopback_url(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(hostname)) => hostname.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::assert_broker_remote_url;

    #[test]
    fn allows_https_and_loopback_http() {
        for url in [
            "https://example.com/mcp",
            "http://localhost:8080/mcp",
            "http://127.0.0.1:8080/mcp",
            "http://[::1]:8080/mcp",
        ] {
            assert!(assert_broker_remote_url(url, "test URL").is_ok(), "{url}");
        }
    }

    #[test]
    fn rejects_non_loopback_http() {
        for url in [
            "http://example.com/mcp",
            "http://127.evil.example/mcp",
            "http://127.0.0.1.nip.io/mcp",
            "http://[2001:db8::1]/mcp",
        ] {
            assert!(assert_broker_remote_url(url, "test URL").is_err(), "{url}");
        }
    }
}
