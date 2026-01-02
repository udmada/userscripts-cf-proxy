let token = "";
export default {
	async fetch(request, env) {
		const url = new URL(request.url);
		if (url.pathname !== '/') {
			let githubRawUrl = 'https://raw.githubusercontent.com';
			if (new RegExp(githubRawUrl, 'i').test(url.pathname)) {
				githubRawUrl += url.pathname.split(githubRawUrl)[1];
			} else {
				if (env.GH_NAME) {
					githubRawUrl += '/' + env.GH_NAME;
					if (env.GH_REPO) {
						githubRawUrl += '/' + env.GH_REPO;
						if (env.GH_BRANCH) githubRawUrl += '/' + env.GH_BRANCH;
					}
				}
				githubRawUrl += url.pathname;
			}
			//console.log(githubRawUrl);
			
			// Initialise request headers
			const headers = new Headers();
			let authTokenSet = false; // Track whether the auth token has been set
			
			// Check TOKEN_PATH scoped authorisation
			if (env.TOKEN_PATH) {
				const requiredAuthPaths = await ADD(env.TOKEN_PATH);
				// Compare lowercased paths to prevent case-based bypasses
				const normalizedPathname = decodeURIComponent(url.pathname.toLowerCase());

				// Check whether the request path requires authorisation
				for (const pathConfig of requiredAuthPaths) {
					const configParts = pathConfig.split('@');
					if (configParts.length !== 2) {
						// Skip malformed configuration entries
						continue;
					}

					const [requiredToken, pathPart] = configParts;
					const normalizedPath = '/' + pathPart.toLowerCase().trim();

					// Match path segments exactly to prevent partial bypasses
					const pathMatches = normalizedPathname === normalizedPath ||
						normalizedPathname.startsWith(normalizedPath + '/');

					if (pathMatches) {
						const providedToken = url.searchParams.get('token');
						if (!providedToken) {
							return new Response('TOKEN must not be empty', { status: 400 });
						}

						if (providedToken !== requiredToken.trim()) {
							return new Response('TOKEN is invalid', { status: 403 });
						}

						// Token validated; use GH_TOKEN for the GitHub request
						if (!env.GH_TOKEN) {
							return new Response('Server GitHub TOKEN configuration error', { status: 500 });
						}
						headers.append('Authorization', `token ${env.GH_TOKEN}`);
						authTokenSet = true;
						break; // Stop after the first matching path configuration
					}
				}
			}
			
			// If TOKEN_PATH did not set auth, fall back to default token logic
			if (!authTokenSet) {
				if (env.GH_TOKEN && env.TOKEN) {
					if (env.TOKEN == url.searchParams.get('token')) token = env.GH_TOKEN || token;
					else token = url.searchParams.get('token') || token;
				} else token = url.searchParams.get('token') || env.GH_TOKEN || env.TOKEN || token;
				
				const githubToken = token;
				//console.log(githubToken);
				if (!githubToken || githubToken == '') {
					return new Response('TOKEN must not be empty', { status: 400 });
				}
				headers.append('Authorization', `token ${githubToken}`);
			}

			// Issue request
			const response = await fetch(githubRawUrl, { headers });

			// Check whether the request succeeded (status 200-299)
			if (response.ok) {
				return new Response(response.body, {
					status: response.status,
					headers: response.headers
				});
			} else {
				const errorText = env.ERROR || 'Unable to fetch the file. Check the path or TOKEN.';
				// Return a suitable error response
				return new Response(errorText, { status: response.status });
			}

		} else {
			const envKey = env.URL302 ? 'URL302' : (env.URL ? 'URL' : null);
			if (envKey) {
				const URLs = await ADD(env[envKey]);
				const URL = URLs[Math.floor(Math.random() * URLs.length)];
				return envKey === 'URL302' ? Response.redirect(URL, 302) : fetch(new Request(URL, request));
			}
			// Home page uses a fake nginx page
			return new Response(await nginx(), {
				headers: {
					'Content-Type': 'text/html; charset=UTF-8',
				},
			});
		}
	}
};

async function nginx() {
	const text = `
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
	<a href="http://nginx.com/">nginx.com</a>.</p>
	
	<p><em>Thank you for using nginx.</em></p>
	</body>
	</html>
	`
	return text;
}

async function ADD(envadd) {
	var addtext = envadd.replace(/[	|"'\r\n]+/g, ',').replace(/,+/g, ',');	// Replace separators with commas
	//console.log(addtext);
	if (addtext.charAt(0) == ',') addtext = addtext.slice(1);
	if (addtext.charAt(addtext.length - 1) == ',') addtext = addtext.slice(0, addtext.length - 1);
	const add = addtext.split(',');
	//console.log(add);
	return add;
}
