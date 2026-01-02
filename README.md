# 🚀 CF-Workers-Raw: access private GitHub files with Cloudflare Workers

🔐 This project lets you access raw files from private GitHub repositories through Cloudflare Workers, without exposing your GitHub token in URLs.

- 📁 You have important files stored in a private GitHub repository
- 🔗 You want to access those files directly via a URL (config files, data files, etc.)
- 🛡️ You do not want to expose your GitHub token in URLs where it could be abused
- 🎯 You want different access rules per path

💡 The worker acts as a secure proxy that handles authentication for you.

Assume your Worker is deployed at `raw.090227.xyz`, and the private file you want is `https://raw.githubusercontent.com/cmliu/CF-Workers-Raw/main/_worker.js`.

## 🔑 Method 1: supply the token in the URL

The most direct approach is to pass your GitHub token as a query parameter:

```url
https://raw.090227.xyz/cmliu/CF-Workers-Raw/main/_worker.js?token=your-github-token
```

Or with the full raw URL:

```url
https://raw.090227.xyz/https://raw.githubusercontent.com/cmliu/CF-Workers-Raw/main/_worker.js?token=your-github-token
```

## 🌐 Method 2: set a global token in the Worker

If you often access the same repository, set a `GH_TOKEN` variable in your Worker. Then you can access files without passing the token each time:

```url
https://raw.090227.xyz/cmliu/CF-Workers-Raw/main/_worker.js
```

Or with the full raw URL:

```url
https://raw.090227.xyz/https://raw.githubusercontent.com/cmliu/CF-Workers-Raw/main/_worker.js
```

## 🔒 Method 3: add an extra access key (recommended)

For extra security, set two variables:

- `GH_TOKEN`: your GitHub token
- `TOKEN`: a custom access key (for example, `mysecretkey`)

Then use:

```url
https://raw.090227.xyz/cmliu/CF-Workers-Raw/main/_worker.js?token=mysecretkey
```

Or with the full raw URL:

```url
https://raw.090227.xyz/https://raw.githubusercontent.com/cmliu/CF-Workers-Raw/main/_worker.js?token=mysecretkey
```

This adds a second layer of protection: even if someone guesses your access key, they still cannot access the GitHub file without the Worker-side token.

## 🎯 Method 4: path-specific tokens (✨ new)

For finer-grained access control, you can configure per-path tokens:

Set the `TOKEN_PATH` variable using the format `token@path`, multiple entries separated by commas:

```
TOKEN_PATH="123456@sh,abcdef@admin,xyz789@private"
```

Usage:

```url
https://raw.090227.xyz/sh/script.py?token=123456
https://raw.090227.xyz/admin/config.json?token=abcdef
https://raw.090227.xyz/private/data.txt?token=xyz789
```

🛡️ **Security features:**

- ✅ Each path has its own token
- ✅ Token validation uses `GH_TOKEN` to access GitHub
- ✅ Case-insensitive matching (paths are compared in lowercase)
- ✅ URL decoding to prevent encoding bypasses
- ✅ Exact/segment path matching to prevent partial bypasses

## 🔍 Method 5: hide GitHub path information

To keep the GitHub path private, set these variables:

- 🧑‍💻 `GH_NAME`: your GitHub username (for example, **cmliu**)

```url
https://raw.090227.xyz/CF-Workers-Raw/main/_worker.js?token=sd123123
```

- 📦 `GH_REPO`: your GitHub repository (requires `GH_NAME`)

```url
https://raw.090227.xyz/main/_worker.js?token=sd123123
```

- 🌿 `GH_BRANCH`: your GitHub branch (requires `GH_NAME` and `GH_REPO`)

```url
https://raw.090227.xyz/_worker.js?token=sd123123
```

⚠️ **Note:** if you use the full raw URL, these variables are ignored.

```url
https://raw.090227.xyz/https://raw.githubusercontent.com/cmliu/CF-Workers-Raw/main/_worker.js?token=sd123123
```

## ⚙️ Setting environment variables

In the Cloudflare Workers dashboard:

1. 🏠 Open your Worker project
2. ⚙️ Select **Settings**
3. 📋 Scroll to **Environment variables**
4. ➕ Add the following:
   - 🔑 `GH_TOKEN`: your GitHub personal access token
   - 🔐 `TOKEN` (optional): your custom access key
   - 🎯 `TOKEN_PATH` (optional): path-specific tokens (format: `token@path`)

💡 You can create a GitHub personal access token under "Developer settings" > "Personal access tokens (classic)" in GitHub.

## 🧰 Rust/WASM build and deploy (workers-rs)

This project now uses **workers-rs (Rust/WASM)**. Wrangler runs `worker-build` to produce the `wasm` module and `shim.mjs`.

### Using mise to manage tooling and tasks

Install dependencies and run tasks with [mise](https://mise.jdx.dev/):

```bash
mise install
```

Build locally:

```bash
mise run build-wasm
```

Deploy:

```bash
mise run deploy
```

### Manual setup (if you do not use mise)

1. Install Rust and the wasm target:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```
2. Build locally (optional):
   ```bash
   wrangler build
   ```
3. Deploy:
   ```bash
   wrangler deploy
   ```

## ❌ Errors

If something goes wrong, you may see:

- 🚫 **TOKEN is invalid**: the access key is incorrect
- ⚠️ **TOKEN must not be empty**: a token is required
- 📂 **Unable to fetch the file. Check the path or TOKEN.**: the file path is wrong or the token does not have access
- 🔧 **Server GitHub TOKEN configuration error**: the server-side GitHub token is missing or invalid

# 📊 Variables

| Variable   | Example                                                       | Required | Notes                                                                                                                                |
| ---------- | ------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| GH_TOKEN   | `ghp_CgmlL2b5J8Z1soNUquc0bZblkbO3gKxhn13t`                    | ❌       | Your GitHub token                                                                                                                    |
| TOKEN      | `nicaibudaowo`                                                | ❌       | When `GH_TOKEN` and `TOKEN` are both set, `TOKEN` is used for access control. When `TOKEN` is set alone, it behaves like `GH_TOKEN`. |
| TOKEN_PATH | `sh@123456`,`admin@abcdef`                                    | ❌       | Path-specific tokens in `path@token` format; multiple entries separated by new lines or commas.                                      |
| GH_NAME    | `cmliu`                                                       | ❌       | Your GitHub username                                                                                                                 |
| GH_REPO    | `CF-Workers-Raw`                                              | ❌       | Your GitHub repo (requires `GH_NAME`)                                                                                                |
| GH_BRANCH  | `main`                                                        | ❌       | Your GitHub branch (requires `GH_NAME` and `GH_REPO`)                                                                                |
| URL302     | `https://t.me/CMLiussss`                                      | ❌       | Home page 302 redirect                                                                                                               |
| URL        | `https://github.com/cmliu/CF-Workers-Raw/blob/main/README.md` | ❌       | Home page disguise                                                                                                                   |
| ERROR      | `Unable to fetch the file. Check the path or TOKEN.`          | ❌       | Custom error message                                                                                                                 |

## 🎯 TOKEN_PATH details

`TOKEN_PATH` lets you set a dedicated token per path:

### 📝 Format

```
TOKEN_PATH="token1@path1,token2@path2,token3@path3"
```

### 💡 Example

Configuration:

```
TOKEN_PATH="123456@sh,abcdef@admin,xyz789@private"
GH_TOKEN="ghp_your_github_token"
```

Access:

- ✅ `/sh/script.py?token=123456` - use token `123456` for the `sh` path
- ✅ `/admin/config?token=abcdef` - use token `abcdef` for the `admin` path
- ✅ `/private/data?token=xyz789` - use token `xyz789` for the `private` path
- ❌ `/sh/script.py?token=wrong` - TOKEN is invalid
- ❌ `/sh/script.py` - TOKEN must not be empty

### 🛡️ Security properties

- 🔒 **Token isolation**: user access tokens are separate from the GitHub API token
- 🎯 **Exact path matching**: prevents path-injection bypasses
- 📝 **Case-insensitive matching**: normalises case to prevent bypasses
- 🔓 **URL decoding**: prevents encoding bypasses
- ⚡ **Automatic switching**: uses `GH_TOKEN` after validation

# 🙏 Thanks

My own idea, ChatGPT
