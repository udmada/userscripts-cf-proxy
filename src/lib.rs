use std::borrow::Cow;
use worker::{
    event, js_sys::Math, Env, Headers, Method, Request, RequestInit, Response, Result, Url,
};

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    let url = req.url()?;
    if url.path() != "/" {
        let github_api_url = build_github_api_url(&url, &env);
        let headers = Headers::new();
        headers.append("Accept", "application/vnd.github.raw")?;
        headers.append("User-Agent", "cloudflare-worker")?;
        let mut auth_token_set = false;

        if let Some(token_path_raw) = get_env_var(&env, "TOKEN_PATH") {
            let path_configs = parse_env_list(&token_path_raw);
            let normalized_pathname = normalize_path(url.path());

            for path_config in path_configs {
                let parts: Vec<&str> = path_config.split('@').collect();
                if parts.len() != 2 {
                    continue;
                }

                let required_token = parts[0].trim();
                let path_part = parts[1];
                let normalized_path = format!("/{}", path_part.trim().to_lowercase());

                let path_matches = normalized_pathname == normalized_path
                    || normalized_pathname.starts_with(&format!("{}/", normalized_path));

                if path_matches {
                    let provided_token = get_query_param(&url, "token");
                    let provided_token = match provided_token {
                        Some(token) if !token.is_empty() => token,
                        _ => return Response::error("TOKEN must not be empty", 400),
                    };

                    if provided_token != required_token {
                        return Response::error("TOKEN is invalid", 403);
                    }

                    let gh_token = match get_env_var(&env, "GH_TOKEN") {
                        Some(token) if !token.is_empty() => token,
                        _ => {
                            return Response::error(
                                "Server GitHub TOKEN configuration error",
                                500,
                            )
                        }
                    };
                    headers.append("Authorization", &format!("token {}", gh_token))?;
                    auth_token_set = true;
                    break;
                }
            }
        }

        if !auth_token_set {
            let gh_token = get_env_var(&env, "GH_TOKEN");
            let token_env = get_env_var(&env, "TOKEN");
            let query_token = get_query_param(&url, "token");

            let github_token = if gh_token.is_some() && token_env.is_some() {
                // Both GH_TOKEN and TOKEN are configured - validate query token.
                match &query_token {
                    Some(qt) if qt == token_env.as_ref().unwrap() => gh_token.unwrap(),
                    Some(_) => return Response::error("TOKEN is invalid", 403),
                    None => return Response::error("TOKEN must not be empty", 400),
                }
            } else if let Some(token) = gh_token {
                // Prefer GH_TOKEN if configured, even when a query token is present.
                token
            } else if let Some(token) = token_env {
                // TOKEN alone behaves like a GitHub token per docs.
                token
            } else {
                // Fall back to a query token only when no server-side tokens exist.
                query_token.unwrap_or_default()
            };

            if github_token.is_empty() {
                return Response::error("TOKEN must not be empty", 400);
            }

            headers.append("Authorization", &format!("token {}", github_token))?;
        }

        let mut init = RequestInit::new();
        init.with_method(Method::Get);
        init.with_headers(headers);

        let github_request = Request::new_with_init(&github_api_url, &init)?;
        let response = worker::Fetch::Request(github_request).send().await?;

        let status = response.status_code();
        if (200..300).contains(&status) {
            return Ok(response);
        }

        let error_text = get_env_var(&env, "ERROR")
            .unwrap_or_else(|| "Unable to fetch the file. Check the path or TOKEN.".to_string());
        return Response::error(error_text, response.status_code());
    }

    let url302 = get_env_var(&env, "URL302");
    let url_plain = get_env_var(&env, "URL");
    if let Some(urls_raw) = url302.as_ref().or(url_plain.as_ref()) {
        let urls = parse_env_list(urls_raw);
        if !urls.is_empty() {
            let idx = (Math::random() * urls.len() as f64).floor() as usize;
            let target_url = urls.get(idx).cloned().unwrap_or_default();
            if url302.is_some() {
                let redirect_url = Url::parse(&target_url)?;
                return Response::redirect(redirect_url);
            }

            let forward_req = Request::new(&target_url, Method::Get)?;
            return worker::Fetch::Request(forward_req).send().await;
        }
    }

    let mut response = Response::ok(nginx_html())?;
    response
        .headers_mut()
        .set("Content-Type", "text/html; charset=UTF-8")?;
    Ok(response)
}

fn build_github_api_url(url: &Url, env: &Env) -> String {
    let path = url.path();
    let path_trimmed = path.trim_start_matches('/');

    // Check if the path contains a full raw.githubusercontent.com URL
    let raw_base = "raw.githubusercontent.com/";
    let path_lower = path.to_lowercase();
    if let Some(index) = path_lower.find(raw_base) {
        // Extract: owner/repo/branch/filepath from the embedded URL
        let suffix_start = index + raw_base.len();
        let suffix = path.get(suffix_start..).unwrap_or("");
        let parts: Vec<&str> = suffix.splitn(4, '/').collect();
        if parts.len() >= 4 {
            let owner = parts[0];
            let repo = parts[1];
            let branch = parts[2];
            let filepath = parts[3];
            return format!(
                "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
                owner, repo, filepath, branch
            );
        }
    }

    // Build URL from environment variables
    let owner = get_env_var(env, "GH_NAME").unwrap_or_default();
    let repo = get_env_var(env, "GH_REPO").unwrap_or_default();
    let branch = get_env_var(env, "GH_BRANCH").unwrap_or_else(|| "master".to_string());

    format!(
        "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
        owner, repo, path_trimmed, branch
    )
}

fn get_env_var(env: &Env, key: &str) -> Option<String> {
    env.var(key).ok().map(|value| value.to_string())
}

fn get_query_param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

fn normalize_path(path: &str) -> String {
    let decoded = urlencoding::decode(path).unwrap_or_else(|_| Cow::Borrowed(path));
    decoded.to_lowercase()
}

fn parse_env_list(env_value: &str) -> Vec<String> {
    let mut output = String::new();
    let mut last_was_comma = false;

    for ch in env_value.chars() {
        let is_separator = matches!(ch, '\t' | '"' | '\'' | '\r' | '\n' | '|');
        let is_comma = ch == ',';
        if is_separator || is_comma {
            if !last_was_comma {
                output.push(',');
                last_was_comma = true;
            }
        } else {
            output.push(ch);
            last_was_comma = false;
        }
    }

    let trimmed = output.trim_matches(',');
    if trimmed.is_empty() {
        return Vec::new();
    }

    trimmed
        .split(',')
        .map(|item| item.to_string())
        .collect()
}

fn nginx_html() -> &'static str {
    r#"
	<!DOCTYPE html>
	<html>
	<head>
	<title>Welcome to nginx!</title>
	<style>
		body {
			width: 35em;
			margin: 0 auto;
			font-family: Tahoma, Verdana, Arial, sans-serif;
		}
	</style>
	</head>
	<body>
	<h1>Welcome to nginx!</h1>
	<p>If you see this page, the nginx web server is successfully installed and
	working. Further configuration is required.</p>

	<p>For online documentation and support please refer to
	<a href="http://nginx.org/">nginx.org</a>.<br/>
	Commercial support is available at
	<a href="http://nginx.com">nginx.com</a>.</p>

	<p><em>Thank you for using nginx.</em></p>
	</body>
	</html>
	"#
}

#[cfg(test)]
mod tests {
    use super::{nginx_html, normalize_path, parse_env_list};

    #[test]
    fn parse_env_list_handles_separators() {
        let value = "one,two|three\nfour\tfive";
        let parsed = parse_env_list(value);
        assert_eq!(parsed, vec!["one", "two", "three", "four", "five"]);
    }

    #[test]
    fn parse_env_list_collapses_consecutive_separators() {
        assert_eq!(parse_env_list("a,,,,b|||c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_env_list_trims_leading_trailing() {
        assert_eq!(parse_env_list(",,,a,b,,,"), vec!["a", "b"]);
    }

    #[test]
    fn parse_env_list_empty_input() {
        assert!(parse_env_list("").is_empty());
        assert!(parse_env_list(",,,|||").is_empty());
    }

    #[test]
    fn normalize_path_decodes_and_lowercases() {
        let normalized = normalize_path("/Some%20Path/File.TXT");
        assert_eq!(normalized, "/some path/file.txt");
    }

    #[test]
    fn normalize_path_invalid_encoding_passthrough() {
        assert_eq!(normalize_path("/foo%ZZbar"), "/foo%zzbar");
    }

    #[test]
    fn nginx_html_contains_expected_content() {
        let html = nginx_html();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Welcome to nginx!"));
    }
}
