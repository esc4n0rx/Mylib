//! Remote media sources: M3U playlists (by URL or uploaded file) and Google
//! Drive. Discovery and normalization feed the shared catalog resolver
//! (`features::catalog::resolver`); playback is proxied so origin credentials
//! never reach the client.

pub mod api;
pub mod cache;
pub mod google_drive;
pub mod m3u;
pub mod models;
pub mod normalize;
pub mod scheduler;
pub mod sync;

/// Removes embedded userinfo (`user:pass@host`) and common secret query
/// parameters so a URL is safe to log or return in an API response.
pub fn sanitize_url(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return strip_secret_query(raw);
    };
    let (authority, path_and_query) = match rest.split_once('/') {
        Some((authority, tail)) => (authority, format!("/{tail}")),
        None => (rest, String::new()),
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{}", strip_secret_query(&path_and_query))
}

fn strip_secret_query(input: &str) -> String {
    let Some((path, query)) = input.split_once('?') else {
        return input.to_string();
    };
    let filtered: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair
                .split_once('=')
                .map_or(*pair, |(key, _)| key)
                .to_ascii_lowercase();
            !matches!(
                key.as_str(),
                "token"
                    | "password"
                    | "pwd"
                    | "pass"
                    | "auth"
                    | "key"
                    | "api_key"
                    | "apikey"
                    | "access_token"
                    | "refresh_token"
                    | "secret"
                    | "sig"
                    | "signature"
            )
        })
        .collect();
    if filtered.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", filtered.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_url;

    #[test]
    fn strips_userinfo_and_secret_params() {
        assert_eq!(
            sanitize_url("https://bob:hunter2@example.com/list.m3u?token=abc&type=m3u"),
            "https://example.com/list.m3u?type=m3u"
        );
        assert_eq!(
            sanitize_url("http://example.com/a.m3u8"),
            "http://example.com/a.m3u8"
        );
    }
}
