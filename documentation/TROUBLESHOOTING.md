# Troubleshooting

Logs:
- Launcher: `%LOCALAPPDATA%\Kestrel\logs\launcher.log` (previous run: `launcher.prev.log`;
  crashes: `launcher-crash-*.txt`).
- Game: `%LOCALAPPDATA%\Kestrel\logs\latest.log` and `game-*.log` (tokens are redacted).
- Forge: `%LOCALAPPDATA%\Kestrel\instance\logs\fml-client-latest.log`.
- Kestrel → Logs page shows both live and has "Copy all".

| Symptom | Cause / fix |
|---|---|
| `setup.ps1` cannot run | `powershell -ExecutionPolicy Bypass -File scripts\setup.ps1` |
| Rust build: `link.exe not found` | Visual Studio C++ Build Tools missing → `setup.ps1` |
| Gradle: "Unsupported class file major version" | Gradle running on a JDK newer than 21; install JDK 17 (`setup.ps1`) or set `JAVA_HOME` |
| Gradle: "Could not find tools.jar … jre1.8…" | Gradle picked an installed Java 8 JRE. Fixed in `client/gradle.properties` (`org.gradle.java.installations.auto-detect=false`) so Gradle downloads its own JDK 8 |
| Gradle cannot resolve `gg.essential.loom` / Forge | Network/proxy blocking `repo.essential.gg`, `maven.minecraftforge.net`, `maven.architectury.dev`, `maven.fabricmc.net` |
| Home page: "Client mod: Not bundled" | Run `scripts\build.ps1` (it copies the jar to `dist\Kestrel\client`), or set `KESTREL_MOD_JAR` |
| Install: "refusing to download from untrusted host" | Metadata pointed outside the allowlist; this is a safety stop. Report it with `launcher.log` |
| Install: hash/size mismatch | Interrupted or corrupted download; press Play again or Repair (only bad files are re-fetched) |
| "custom Java is version 17…" | 1.8.9 + Forge needs Java 8. Use the managed runtime (Settings → Java) |
| Sign-in: "no Microsoft application (client) ID" | Follow BUILD.md → Microsoft sign-in |
| Sign-in: HTTP 403 from Minecraft services | Your Azure app ID is not approved by Mojang yet (<https://aka.ms/mce-reviewappid>) |
| Sign-in: "does not own Minecraft: Java Edition" | The Microsoft account has no Java Edition licence |
| Sign-in: XErr child/adult/region messages | Fix the account at xbox.com / Microsoft Family as the message says |
| Game exits, "Kestrel loaded" never appears | Check `latest.log` for the first exception. Try Repair. Remove other mods from `instance\mods` |
| Cape not visible | You have an official cape (vanilla draws it), "Show cape" is off, or the cape layer is disabled in Skin Customization |
| Custom cape/sky rejected | Kestrel menu shows "Last error"; see size/format rules in the menu or SKIES.md |
| Custom sky does nothing in Nether/End | By design: Overworld only |
| No kill effects on a server | Detection needs you to have hit the victim recently and either a 0-health update or a kill-feed line naming both of you. Some servers do neither; try toggling detection modes |
| Low FPS | Try the Potato preset (Performance), disable custom sky/kill effects, enable "Also apply video settings" |
| Cosmetics page: "Upload rejected: …" | The message says why (not a PNG, wrong size/aspect, too large). Cape: 64x32 or multiples, ≤ 2 MB. Sky: 3:2, 384–3072 px wide, ≤ 16 MB |
| Cosmetics page stays "Pending" | Normal until the game starts; the client applies it on launch (or within ~1 s while running). Check `latest.log` for "applied cosmetics from launcher" |
| Card shows "invalid image" | A file in the capes/skies folder is not a valid image; remove it with the card's Remove button |
| Launcher window never opens (VMs, old drivers) | Kestrel tries Direct3D 12 (incl. Windows' software renderer), then OpenGL, and shows an error box if both fail. In a VM enable 3D acceleration and install guest tools — Minecraft itself still needs OpenGL 2.0+. Force a renderer with `Kestrel.exe --renderer opengl` or `--renderer d3d`; the working choice is remembered in `%LOCALAPPDATA%\Kestrel\renderer.txt` |
| Settings → Updates: "Not configured in this build" | The exe was built before `scripts\setup-updates.ps1` was run. Run it, rebuild with `release.ps1` and install that exe once |
| Update check: "no release found" | Nothing published yet, the repo is private, or the release lacks `kestrel-update.json`. See UPDATES.md |
| Update: "signature check failed" | The release was signed with a different key than this launcher trusts (lost/regenerated key). Install the new build manually once |
| Update: "can't update itself in its current folder" | Kestrel.exe is in a protected folder (Program Files). Move it to Desktop/Documents, or use "Start new version" |
| Update rolled back | The new version crashed on start; see `launcher.log` and `launcher.prev.log` |
