use std::time::Duration;

use axum::http::{HeaderMap, HeaderValue, header};

pub(crate) fn read(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned())
}

pub(crate) fn set(
    name: &str,
    value: &str,
    path: &str,
    max_age: Duration,
    secure: bool,
) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "{name}={value}; Path={path}; Max-Age={}; HttpOnly; SameSite=Lax{secure}",
        max_age.as_secs()
    );
    HeaderValue::from_str(&cookie).expect("cookie values are URL-safe base64")
}

pub(crate) fn clear(name: &str, path: &str, secure: bool) -> HeaderValue {
    set(name, "", path, Duration::ZERO, secure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_among_several_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; wp_session=abc; b=2"),
        );
        assert_eq!(read(&headers, "wp_session").as_deref(), Some("abc"));
        assert_eq!(read(&headers, "missing"), None);
    }

    #[test]
    fn secure_flag_follows_scheme() {
        let value = set("wp_session", "v", "/", Duration::from_secs(60), true);
        assert_eq!(
            value,
            "wp_session=v; Path=/; Max-Age=60; HttpOnly; SameSite=Lax; Secure"
        );
    }
}
