# Building Kestrel

## Prerequisites (Windows 10/11 x64)

| Tool | Why | Installed by `setup.ps1` |
|---|---|---|
| Visual Studio 2022 Build Tools, C++ workload | Rust's MSVC linker; compiles `ring` (TLS) | `winget Microsoft.VisualStudio.2022.BuildTools` |
| Rust stable (MSVC toolchain) | launcher | `winget Rustlang.Rustup` |
| JDK 17–21 | runs Gradle 8.8 | `winget EclipseAdoptium.Temurin.17.JDK` |

Gradle itself downloads Java 8 (via the foojay toolchain resolver), Minecraft 1.8.9,
MCP mappings and Forge 11.15.1.2318 on the first build. Expect the first mod build to take
several minutes; later builds are fast.

## Commands

```powershell
scripts\setup.ps1 [-Yes]      # check / install prerequisites
scripts\build.ps1 [-Run]      # dist\Kestrel\Kestrel.exe + dist\Kestrel\client\kestrel-<ver>.jar
scripts\test.ps1              # cargo fmt/clippy/test + gradle test
scripts\package.ps1           # tests + build + dist\Kestrel-<ver>-windows-x64.zip (+ .sha256)
```

(Prefix each with `powershell -ExecutionPolicy Bypass -File` if script execution is disabled.)

Manual equivalents:

```powershell
cd client;   .\gradlew.bat build          # mod -> client\build\libs\kestrel-<ver>.jar
cd launcher; cargo build --release -p kestrel-launcher
```

The launcher looks for the mod in `<launcher exe dir>\client\kestrel-*.jar`
(or `%KESTREL_MOD_JAR%`), and copies it into the game's `mods` folder on Play.

### Developing the mod

`cd client; .\gradlew.bat runClient` starts Minecraft with the mod from source.
DevAuth is included for signing in during development (see its README; disabled unless
configured).

## Standalone release (Kestrel.exe)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release.ps1
```

Output: `release\Kestrel.exe` (plus `Kestrel.exe.sha256.txt`). The script runs the tests,
builds the mod, builds the launcher for `x86_64-pc-windows-msvc` with the mod jar, icon,
fonts and cosmetic previews embedded and the C runtime linked statically, then copies the
exe to a temp folder outside the project and runs `Kestrel.exe --self-check` there against a
throw-away data directory, checks the DLL imports, and prints the final size and location.
`-SkipTests` skips step 1. Full output: `dist\release-log.txt`.

`Kestrel.exe` needs nothing else: no project folder, no .NET, no Java, no VC++ redistributable.
On first Play it downloads Minecraft 1.8.9, Forge and Mojang's Java 8 from the official servers
into `%LOCALAPPDATA%\Kestrel` (these are never bundled, for licensing reasons). Settings,
game files, logs and uploaded cosmetics live there too, so moving or replacing the exe (an
upgrade) keeps everything. A newer exe replaces only the client mod jar in `instance\mods`.

You can run the self-check yourself: `Kestrel.exe --self-check report.txt`.

## First launch

1. Start `Kestrel.exe`. Data goes to `%LOCALAPPDATA%\Kestrel` (override with
   `Kestrel.exe --root D:\Games\Kestrel` or `%KESTREL_HOME%`).
2. **Offline mode** works immediately for singleplayer and offline-mode servers.
3. Press **Play**. The first run downloads a few hundred MB (Minecraft, libraries, assets, Forge,
   Mojang's Java 8 runtime), verifying SHA-1 hashes where Mojang/Forge publish them.
4. The Home page shows "Forge loaded" and "Kestrel loaded" once the game has started.
5. In game, press **Right Shift** for the Kestrel menu.
6. **Cosmetics** (left sidebar of the launcher): equip kill effects, skies and capes, upload
   your own PNGs. Changes apply on the next Play, or within about a second if the game is
   already running.

## Microsoft sign-in (required for online servers such as Hypixel)

Kestrel uses Microsoft's OAuth device-code flow: you sign in on microsoft.com in your
browser and enter a short code. Kestrel never sees your password. Tokens are stored
encrypted with Windows DPAPI (bound to your Windows account) in `accounts.dat`.

Every third-party launcher needs its own Azure application ID:

1. Sign in at <https://portal.azure.com> → **Microsoft Entra ID** → **App registrations** →
   **New registration**.
2. Name: anything (e.g. "Kestrel Launcher"). Supported account types:
   **Personal Microsoft accounts only**. No redirect URI is needed.
3. Open the new app → **Authentication** → enable **Allow public client flows** → Save.
4. Copy the **Application (client) ID** into Kestrel → Settings → Advanced.
5. **Mojang approval:** new app IDs must be approved before Minecraft's services accept
   them. Submit the app ID at <https://aka.ms/mce-reviewappid>. Until approved, sign-in
   fails at the last step with "Minecraft services rejected this application ID (HTTP 403)".

## What has and has not been verified

See the "Verification status" section of ARCHITECTURE.md.
