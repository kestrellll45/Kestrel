# Launcher updates

Kestrel updates itself from **GitHub Releases**. When `Kestrel.exe` starts it
checks, in the background, whether a newer signed release exists. If one does,
it shows **A Kestrel update is available** with the current and new versions,
the release notes, **Update Now** and **Later**. Update Now downloads the new
`Kestrel.exe`, verifies it, swaps it in, and restarts into the new version.
Your settings, Minecraft files, accounts, logs and cosmetics in
`%LOCALAPPDATA%\Kestrel` are never touched.

| Situation | Behaviour |
|---|---|
| Up to date | Nothing is shown. |
| Offline / GitHub unreachable / no release yet | Logged as a warning; the launcher works normally. Settings → Updates shows the error and a **Check for updates** button. |
| You chose **Later** | Not offered again automatically for *that* version (stored as `update_dismissed_version` in `launcher.json`). A newer version is offered again. **Settings → Updates → Check for updates** always shows it. |
| Minecraft is running | **Update Now** is disabled until you close the game. |
| Download interrupted, checksum wrong, self-test fails | Nothing is installed; the error is shown with **Retry**. The current version keeps working. |
| New version crashes on start-up | The old launcher puts itself back automatically and shows what happened. |
| Kestrel is in a folder you can't write to (e.g. `C:\Program Files`) | The original is left untouched. The verified new version is installed to `%LOCALAPPDATA%\Kestrel\app\Kestrel.exe` (which keeps updating itself from then on), with **Start new version** / **Open folder** buttons. No admin rights are requested. |
| Release marked as required (`minimum_supported` newer than your version) | Kestrel 0.3.0 and newer: the prompt says "required", **Later** only closes the window, and **Play is blocked** (button shows *UPDATE REQUIRED*, Home shows an *Update now* banner) until the update is installed. The required minimum is saved in `launcher.json` (`update_required_minimum`), so it stays enforced offline; it clears itself once the launcher is new enough or a check finds no required release. Required updates are checked at every start, even with *Show new versions automatically* turned off. Kestrel 0.2.x and older only show the "required" prompt at every start; their Play button was never blocked. |

Settings → **Updates**: installed version, update source, *Show new versions
automatically when Kestrel starts* (optional updates only; required ones are always
checked), **Check for updates**, and **Retry update** after a
failure. There is a single channel (`stable`), so there is no channel picker.

## Configuration of this project

| | |
|---|---|
| Release repository | https://github.com/kestrellll45/Kestrel (public: launchers download anonymously) |
| Manifest URL (compiled into `Kestrel.exe`) | `https://github.com/kestrellll45/Kestrel/releases/latest/download/kestrel-update.json` |
| Artifact URL | `https://github.com/kestrellll45/Kestrel/releases/download/v<version>/Kestrel.exe` |
| Signature | Ed25519 over `"kestrel-update-v1\0" + payload` |
| Trusted public key | `launcher/update-public-key.txt` (key id `592de470`), set by `release.json` + this file at build time |
| Private signing key | `%APPDATA%\Kestrel-signing\update-signing-key.txt` on the publishing PC only (folder ACL: your Windows account only). Never in the repo, CI, or a release. |

`scripts\release.ps1` refuses to produce an exe whose built-in self-check does not
report exactly this repository and key id; the summary line reads
`Updates: enabled (kestrellll45/Kestrel, key 592de470)`.

Signing happens **locally** on purpose: there is no GitHub Actions workflow holding
the key, so a compromised workflow, dependency or GitHub token cannot sign updates.

### Backing up and recovering the signing key

The private key is the root of trust. Anyone with it can publish an update that
every installed launcher accepts; without it, no launcher can be updated again.

* **Back up now:** copy `%APPDATA%\Kestrel-signing\update-signing-key.txt` (one line
  of base64) into a password manager entry or onto an offline USB stick. Do not
  put it in cloud-synced folders, chats, the repository, or GitHub secrets.
* **New PC / restore:** put the file back at the same path (or set
  `KESTREL_SIGNING_KEY` to its location). `publish-release.ps1` checks that it
  matches `launcher\update-public-key.txt` before building.
* **Lost:** create a new key with `setup-updates.ps1` (move the old folder away
  first), build, and every user installs that build manually once.
* **Leaked:** same as lost, and tell users to reinstall manually; there is no
  remote key revocation.

### Setting it up from scratch (another repository)

1. Create a **public** GitHub repository.
2. `powershell -ExecutionPolicy Bypass -File scripts\setup-updates.ps1 -Repo owner/repo`
   (creates or reuses the private key, writes `launcher\update-public-key.txt` and
   `release.json`).
3. Install Git and the GitHub CLI (`dist\tasks\2-install-github-tools.cmd` does a
   per-user install, or `winget install Git.Git GitHub.cli`) and run `gh auth login`.
4. `scripts\release.ps1`, then replace your `Kestrel.exe` with `release\Kestrel.exe`
   once. Later versions arrive through the launcher.

## Publishing a new version

```powershell
# 1. Pick the new version (updates launcher/Cargo.toml, client/gradle.properties, Cargo.lock)
powershell -ExecutionPolicy Bypass -File scripts\bump-version.ps1 -Version 0.2.2

# 2. Write release notes (plain text / Markdown shown in the update window)
notepad notes\0.2.2.md

# 3. Commit and push to main
git add -A; git commit -m "Kestrel 0.2.2"; git push

# 4. Build, test, sign and publish
powershell -ExecutionPolicy Bypass -File scripts\publish-release.ps1 -Notes notes\0.2.2.md
```

`publish-release.ps1` stops before uploading anything unless:

1. the private signing key matches the trusted public key;
2. the checkout is on `main`, has no uncommitted changes, and equals `origin/main`
   (checked again after the build);
3. release `v<version>` does not exist yet;
4. `release.ps1` passes: Rust fmt/clippy/tests, Gradle tests, mod build, launcher
   build, self-check (version, embedded mod, repository, **key id**, no test hooks);
5. the signed manifest verifies against the public key and the exe's SHA-256.

It then creates GitHub release `v<version>` **at that exact commit**, marked
*Latest*, with **two assets: `Kestrel.exe` and `kestrel-update.json`**. Options:
`-Draft` (review on GitHub first; launchers ignore drafts), `-MinimumSupported x.y.z`
(mark as required for older launchers), `-SkipTests`.

The release must be the repository's **Latest** release (not a pre-release), because
launchers read `releases/latest/download/kestrel-update.json`.

The client mod is embedded in `Kestrel.exe`, so a launcher update also delivers
the matching client mod: on the next Play, the launcher replaces the old
`kestrel-*.jar` in `instance\mods` (and only that file).

## How it works

```
Kestrel.exe starts ─▶ background thread: GET releases/latest/download/kestrel-update.json
                      (HTTPS, redirects checked hop by hop, 256 KB limit)
                    ─▶ verify Ed25519 signature with the public key compiled into the exe
                    ─▶ validate: product, channel, semver, HTTPS URL on an allowed host,
                       .exe, 64-hex SHA-256, plausible size
                    ─▶ newer than this version and not dismissed? show the window
Update Now ─▶ download to %LOCALAPPDATA%\Kestrel\updates\Kestrel-x.y.z.exe.part
           ─▶ exact size + SHA-256 from the signed manifest, then rename
           ─▶ PE header check, then run  <new> --self-check  in a scratch folder:
              must exit 0, embed the client mod, and report version x.y.z
           ─▶ re-hash, then swap next to the running exe:
                Kestrel.exe ─rename▶ Kestrel.exe.old   (Windows allows renaming a running exe)
                Kestrel.exe.new ─rename▶ Kestrel.exe   (undone if it fails)
           ─▶ start the new exe with --post-update/--update-health and wait up to 45 s
                health marker appears ─▶ old launcher closes
                new exe exits early   ─▶ rollback: Kestrel.exe.old renamed back
New exe ─▶ writes the health marker once its window is up, deletes Kestrel.exe.old
           (retrying until the old process has exited) and the downloaded copy
```

Code: `launcher/crates/kestrel-core/src/update/` (manifest, fetch, install) and
`launcher/crates/kestrel-launcher/src/updater.rs` (window, Settings card, restart).
Publisher tool: `launcher/crates/kestrel-release` (`keygen`, `pubkey`, `sign`,
`verify`). It is never shipped inside `Kestrel.exe`.

### Testing the updater

Automated tests (`scripts\test.ps1`, also run by `release.ps1`) cover version
comparison, up-to-date / update-available / dismissed ("Later") decisions, signed
and tampered manifests, malformed metadata, offline and server errors, unsafe
redirects, checksum mismatches, interrupted and over-long downloads, the exe swap
(including a Windows-only test that swaps out a *running* exe), rollback, a
read-only install folder, cleanup, and that `%LOCALAPPDATA%\Kestrel` user data is
left untouched. The release service is mocked; no public release is needed.

End-to-end with real Windows executables (developer test, nothing is published):

```powershell
scripts\update-e2e-prepare.cmd     # builds Kestrel 0.2.0 + 0.2.1 test builds, signs with a throw-away key
dist\update-e2e\run-update.cmd        # click Update Now: download, verify, self-test, swap, restart
dist\update-e2e\run-update-crash.cmd  # the "new" version crashes on start: the old one must roll back
scripts\update-e2e-verify.cmd      # checks results; prints E2E RESULT: PASS/FAIL
```

The test builds are compiled with the `e2e-update-test` cargo feature, which lets
`KESTREL_E2E_UPDATE_DIR` serve `kestrel-update.json` and `Kestrel.exe` from a local
folder (URL checks, signature, hash and self-test still apply) and adds a simulated
start-up crash. `release.ps1` refuses to publish any exe whose self-check reports
these hooks (`update test hooks: none` is required).

### Manifest format

```json
{ "format": "kestrel-update-v1", "key_id": "1a2b3c4d",
  "payload": "<base64 of the JSON below>", "signature": "<base64 Ed25519>" }
```
```json
{ "schema": 1, "product": "kestrel-launcher", "channel": "stable",
  "version": "0.2.1", "published": "2026-10-25T12:00:00Z", "minimum_supported": null,
  "notes": "…",
  "windows_x86_64": { "url": "https://github.com/<repo>/releases/download/v0.2.1/Kestrel.exe",
                      "sha256": "<64 hex>", "size": 22642688 } }
```
The signature covers the bytes `"kestrel-update-v1\0" || payload`, so there is no
JSON canonicalisation.

## Security: what is and isn't guaranteed

**Guaranteed** (enforced in code and covered by tests):
- An update is installed only if its manifest is signed by **your** private key.
  Someone who takes over the GitHub repository, the CDN or the network can't make
  launchers install a different file, because they can't produce a valid signature.
  A checksum alone wouldn't give this, since it comes from the same place as the file.
- The installed file is byte-for-byte the one you signed (exact size + SHA-256,
  checked after download and again right before the swap).
- HTTPS only, with certificate validation (rustls + Mozilla roots). Each redirect is
  re-validated: HTTPS, no credentials or ports in the URL, host limited to
  `github.com`, `objects.githubusercontent.com`, `release-assets.githubusercontent.com`
  (and the manifest's own host, if you use a custom one).
- The manifest can't run commands or choose a file path. It supplies one download URL;
  the file name and location are fixed by the launcher, and the new exe is started
  only after signature, hash, PE and self-test checks.
- No downgrades: only versions newer than the running one are offered, so replaying
  an old signed manifest does nothing.
- The working launcher is never deleted before the new one is verified and in place,
  and a new version that crashes on start-up is rolled back.
- Update requests send no user data: a plain GET with `User-Agent: Kestrel-Launcher/<version>`.

**Not guaranteed / your responsibility:**
- **Your private key is the root of trust.** Anyone who has it can publish an update
  that every launcher accepts. Keep it off shared machines, out of the repo, and backed up.
- **If you lose the key**, existing launchers can't accept further updates. Generate a
  new key (delete the old file and re-run `setup-updates.ps1`) and have users install
  the next build manually once. Key rotation without a manual install isn't implemented.
- **Windows code signing (Authenticode) isn't implemented.** Downloads aren't
  Authenticode-signed, so SmartScreen may warn the first time a new `Kestrel.exe` runs,
  and some antivirus products are more suspicious of unsigned executables. That needs a
  paid code-signing certificate.
- A user who installs a Kestrel.exe from somewhere else gets whatever key *that* build
  trusts. The updater protects the update path, not the first download.
- If GitHub is down or blocked, updates are unavailable (the launcher still works).
