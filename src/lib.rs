use regex::Regex;
use worker::*;

/// Parse a comma/whitespace/quote separated string into a vector of strings.
fn parse_add_list(input: &str) -> Vec<String> {
    let re = Regex::new(r#"[\t|"'\r\n]+"#).unwrap();
    let mut text = re.replace_all(input, ",").to_string();

    // Collapse multiple commas
    let multi_comma = Regex::new(r",+").unwrap();
    text = multi_comma.replace_all(&text, ",").to_string();

    // Trim leading/trailing commas
    text = text.trim_matches(',').to_string();

    text.split(',')
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Generate the fake nginx welcome page.
fn nginx_page() -> String {
    r#"<!DOCTYPE html>
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
<a href="http://nginx.com/">nginx.com</a>.</p>

<p><em>Thank you for using nginx.</em></p>
</body>
</html>"#
        .to_string()
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let pathname = url.path();

    if pathname != "/" {
        // Build the GitHub raw URL
        let github_raw_base = "https://raw.githubusercontent.com";
        let github_raw_url = if pathname.to_lowercase().contains(github_raw_base) {
            // Full GitHub URL embedded in path
            let parts: Vec<&str> = pathname.splitn(2, github_raw_base).collect();
            if parts.len() > 1 {
                format!("{}{}", github_raw_base, parts[1])
            } else {
                format!("{}{}", github_raw_base, pathname)
            }
        } else {
            // Build URL from env vars
            let mut path_parts = String::new();

            if let Ok(gh_name) = env.var("GH_NAME") {
                path_parts.push('/');
                path_parts.push_str(&gh_name.to_string());

                if let Ok(gh_repo) = env.var("GH_REPO") {
                    path_parts.push('/');
                    path_parts.push_str(&gh_repo.to_string());

                    if let Ok(gh_branch) = env.var("GH_BRANCH") {
                        path_parts.push('/');
                        path_parts.push_str(&gh_branch.to_string());
                    }
                }
            }

            format!("{}{}{}", github_raw_base, path_parts, pathname)
        };

        // Initialise headers
        let headers = Headers::new();
        let mut auth_token_set = false;

        // Check TOKEN_PATH special path authentication
        if let Ok(token_path_var) = env.var("TOKEN_PATH") {
            let path_configs = parse_add_list(&token_path_var.to_string());

            // Normalise pathname for comparison (lowercase, URL decoded)
            let normalized_pathname = urlencoding::decode(pathname)
                .unwrap_or_else(|_| pathname.into())
                .to_lowercase();

            for path_config in &path_configs {
                let config_parts: Vec<&str> = path_config.splitn(2, '@').collect();
                if config_parts.len() != 2 {
                    continue;
                }

                let required_token = config_parts[0].trim();
                let path_part = format!("/{}", config_parts[1].to_lowercase().trim());

                // Exact match or prefix match with /
                let path_matches = normalized_pathname == path_part
                    || normalized_pathname.starts_with(&format!("{}/", path_part));

                if path_matches {
                    // Get the token from query params
                    let provided_token = url
                        .query_pairs()
                        .find(|(k, _)| k == "token")
                        .map(|(_, v)| v.to_string());

                    match provided_token {
                        None => {
                            return Response::error("Token is required", 400);
                        }
                        Some(token) if token != required_token => {
                            return Response::error("Invalid token", 403);
                        }
                        Some(_) => {
                            // Token validated, use GH_TOKEN for GitHub request
                            match env.var("GH_TOKEN") {
                                Ok(gh_token) => {
                                    headers.set("Authorization", &format!("token {}", gh_token))?;
                                    auth_token_set = true;
                                }
                                Err(_) => {
                                    return Response::error(
                                        "Server GitHub token misconfigured",
                                        500,
                                    );
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }

        // If TOKEN_PATH didn't set auth, use default token logic
        if !auth_token_set {
            let query_token = url
                .query_pairs()
                .find(|(k, _)| k == "token")
                .map(|(_, v)| v.to_string());

            let gh_token = env.var("GH_TOKEN").ok().map(|v| v.to_string());
            let env_token = env.var("TOKEN").ok().map(|v| v.to_string());

            let final_token = if gh_token.is_some() && env_token.is_some() {
                // Both GH_TOKEN and TOKEN are set
                if query_token.as_ref() == env_token.as_ref() {
                    gh_token
                } else {
                    query_token.clone().or(gh_token)
                }
            } else {
                // Use whichever is available: query_token > GH_TOKEN > TOKEN
                query_token.or(gh_token).or(env_token)
            };

            match final_token {
                Some(token) if !token.is_empty() => {
                    headers.set("Authorization", &format!("token {}", token))?;
                }
                _ => {
                    return Response::error("Token is required", 400);
                }
            }
        }

        // Make the request to GitHub
        let mut init = RequestInit::new();
        init.with_headers(headers);

        let github_req = Request::new_with_init(&github_raw_url, &init)?;
        let response = Fetch::Request(github_req).send().await?;

        if response.status_code() >= 200 && response.status_code() < 300 {
            // Success - return the response as-is
            Ok(response)
        } else {
            // Error - return custom error message
            let error_text = env
                .var("ERROR")
                .map(|v| v.to_string())
                .unwrap_or_else(|_| {
                    "Unable to fetch file. Check that the path and token are correct.".to_string()
                });
            Response::error(error_text, response.status_code())
        }
    } else {
        // Root path handling
        let url302 = env.var("URL302").ok().map(|v| v.to_string());
        let url_var = env.var("URL").ok().map(|v| v.to_string());
        let is_redirect = url302.is_some();

        if let Some(urls_str) = url302.or(url_var) {
            let urls = parse_add_list(&urls_str);
            if !urls.is_empty() {
                // Pick a random URL
                let random_index = (js_sys::Math::random() * urls.len() as f64) as usize;
                let target_url = &urls[random_index.min(urls.len() - 1)];

                if is_redirect {
                    // 302 redirect
                    Response::redirect_with_status(Url::parse(target_url)?, 302)
                } else {
                    // Proxy the request
                    let proxy_req = Request::new(target_url, req.method())?;
                    Fetch::Request(proxy_req).send().await
                }
            } else {
                // No URLs configured, show nginx page
                Response::from_html(nginx_page())
            }
        } else {
            // No redirect configured, show fake nginx page
            Response::from_html(nginx_page())
        }
    }
}
