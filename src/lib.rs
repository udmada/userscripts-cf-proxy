use once_cell::sync::Lazy;
use std::borrow::Cow;
use std::sync::Mutex;
use worker::{
    event, js_sys::Math, Env, Headers, Method, Request, RequestInit, Response, Result, Url,
};

static TOKEN_CACHE: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    let url = req.url()?;
    if url.path() != "/" {
        let github_raw_url = build_github_raw_url(&url, &env);
        let headers = Headers::new();
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
            let mut cached_token = TOKEN_CACHE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();

            let github_token = if gh_token.is_some() && token_env.is_some() {
                if query_token.as_deref() == token_env.as_deref() {
                    gh_token.clone().unwrap_or_else(|| cached_token.clone())
                } else {
                    query_token.clone().unwrap_or_else(|| cached_token.clone())
                }
            } else {
                query_token
                    .clone()
                    .or(gh_token.clone())
                    .or(token_env.clone())
                    .unwrap_or_else(|| cached_token.clone())
            };

            if github_token.is_empty() {
                return Response::error("TOKEN must not be empty", 400);
            }

            cached_token = github_token.clone();
            if let Ok(mut token_lock) = TOKEN_CACHE.lock() {
                *token_lock = cached_token;
            }

            headers.append("Authorization", &format!("token {}", github_token))?;
        }

        let mut init = RequestInit::new();
        init.with_method(Method::Get);
        init.with_headers(headers);

        let github_request = Request::new_with_init(&github_raw_url, &init)?;
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

fn build_github_raw_url(url: &Url, env: &Env) -> String {
    let base = "https://raw.githubusercontent.com";
    let path = url.path();
    let base_lower = base.to_lowercase();
    let path_lower = path.to_lowercase();

    if let Some(index) = path_lower.find(&base_lower) {
        let suffix_start = index + base.len();
        let suffix = path.get(suffix_start..).unwrap_or("");
        return format!("{}{}", base, suffix);
    }

    let mut github_raw_url = String::from(base);
    if let Some(name) = get_env_var(env, "GH_NAME") {
        github_raw_url.push('/');
        github_raw_url.push_str(&name);
        if let Some(repo) = get_env_var(env, "GH_REPO") {
            github_raw_url.push('/');
            github_raw_url.push_str(&repo);
            if let Some(branch) = get_env_var(env, "GH_BRANCH") {
                github_raw_url.push('/');
                github_raw_url.push_str(&branch);
            }
        }
    }

    github_raw_url.push_str(path);
    github_raw_url
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
